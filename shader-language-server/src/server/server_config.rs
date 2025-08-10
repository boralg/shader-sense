use std::collections::{HashMap, HashSet};

use log::info;
use lsp_types::{request::WorkspaceConfiguration, ConfigurationParams, Url};
use serde::{Deserialize, Serialize};

use serde_json::Value;
use shader_sense::{
    shader::{
        GlslCompilationParams, GlslSpirvVersion, GlslTargetClient, HlslCompilationParams,
        HlslShaderModel, HlslVersion, ShaderCompilationParams, ShaderContextParams, ShaderParams,
        ShadingLanguage, WgslCompilationParams,
    },
    shader_error::ShaderDiagnosticSeverity,
};

use crate::{profile_scope, server::ServerLanguage};

use super::shader_variant::ShaderVariant;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerHlslConfig {
    pub shader_model: Option<HlslShaderModel>,
    pub version: Option<HlslVersion>,
    pub enable16bit_types: Option<bool>,
    pub spirv: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerGlslConfig {
    pub target_client: Option<GlslTargetClient>,
    pub spirv_version: Option<GlslSpirvVersion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerTraceLevel {
    #[default]
    Off,
    Messages,
    Verbose,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerTrace {
    server: ServerTraceLevel,
}

impl ServerTrace {
    pub fn is_verbose(&self) -> bool {
        self.server == ServerTraceLevel::Verbose
    }
}

// Only use option to allow non defined values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    includes: Option<Vec<String>>,
    defines: Option<HashMap<String, String>>,
    path_remapping: Option<HashMap<String, String>>,
    validate: Option<bool>,
    symbols: Option<bool>,
    symbol_diagnostics: Option<bool>,
    trace: Option<ServerTrace>,
    severity: Option<String>,
    hlsl: Option<ServerHlslConfig>,
    glsl: Option<ServerGlslConfig>,
}

impl ServerConfig {
    pub const DEFAULT_SYMBOLS: bool = true;
    pub const DEFAULT_VALIDATE: bool = true;
    pub const DEFAULT_SYMBOL_DIAGNOSTIC: bool = false; // Mostly for debug
    pub const DEFAULT_SEVERITY: ShaderDiagnosticSeverity = ShaderDiagnosticSeverity::Error;
    pub const DEFAULT_TRACE: ServerTrace = ServerTrace {
        server: ServerTraceLevel::Off,
    };

    pub fn into_shader_params(&self, variant: Option<ShaderVariant>) -> ShaderParams {
        let (mut defines, mut includes, entry_point, shader_stage) = match variant {
            Some(variant) => (
                variant.defines.clone(),
                variant
                    .includes
                    .into_iter()
                    .map(|e| e.into_os_string().into_string().unwrap())
                    .collect::<Vec<String>>(),
                Some(variant.entry_point),
                variant.stage,
            ),
            None => (HashMap::new(), Vec::new(), None, None),
        };
        defines.extend(self.defines.clone().unwrap_or_default());
        includes.extend(self.includes.clone().unwrap_or_default());
        let hlsl = self.hlsl.clone().unwrap_or_default();
        let glsl = self.glsl.clone().unwrap_or_default();
        ShaderParams {
            context: ShaderContextParams {
                defines,
                includes,
                path_remapping: self
                    .path_remapping
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(vp, p)| (vp.into(), p.into()))
                    .collect(),
            },
            compilation: ShaderCompilationParams {
                entry_point,
                shader_stage,
                hlsl: HlslCompilationParams {
                    shader_model: hlsl.shader_model.unwrap_or_default(),
                    version: hlsl.version.unwrap_or_default(),
                    enable16bit_types: hlsl.enable16bit_types.unwrap_or_default(),
                    spirv: hlsl.spirv.unwrap_or_default(),
                },
                glsl: GlslCompilationParams {
                    client: glsl.target_client.unwrap_or_default(),
                    spirv: glsl.spirv_version.unwrap_or_default(),
                },
                wgsl: WgslCompilationParams {},
            },
        }
    }
    pub fn get_symbols(&self) -> bool {
        match &self.symbols {
            Some(symbols) => *symbols,
            None => Self::DEFAULT_SYMBOLS,
        }
    }
    pub fn get_validate(&self) -> bool {
        match &self.validate {
            Some(validate) => *validate,
            None => Self::DEFAULT_VALIDATE,
        }
    }
    pub fn get_symbol_diagnostics(&self) -> bool {
        match &self.symbol_diagnostics {
            Some(symbol_diagnostics) => *symbol_diagnostics,
            None => Self::DEFAULT_SYMBOL_DIAGNOSTIC,
        }
    }
    pub fn get_severity(&self) -> ShaderDiagnosticSeverity {
        match &self.severity {
            Some(severity) => ShaderDiagnosticSeverity::from(severity.as_str()),
            None => Self::DEFAULT_SEVERITY,
        }
    }
    pub fn is_verbose(&self) -> bool {
        match &self.trace {
            Some(trace) => trace.is_verbose(),
            None => Self::DEFAULT_TRACE.is_verbose(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            includes: None,
            defines: None,
            path_remapping: None,
            validate: None,
            symbols: None,
            symbol_diagnostics: None,
            trace: None,
            severity: None,
            hlsl: None,
            glsl: None,
        }
    }
}

impl ServerLanguage {
    pub fn request_configuration(&mut self) {
        let config = ConfigurationParams {
            items: vec![lsp_types::ConfigurationItem {
                scope_uri: None,
                section: Some("shader-validator".to_owned()),
            }],
        };
        self.connection.send_request::<WorkspaceConfiguration>(
            config,
            |server: &mut ServerLanguage, value: Value| {
                // Sent 1 item, received 1 in an array
                let mut parsed_config: Vec<Option<ServerConfig>> =
                    serde_json::from_value(value).expect("Failed to parse received config");
                let config = parsed_config.remove(0).unwrap_or_default();
                if server.config != config {
                    profile_scope!("Updating server config: {:#?}", config);
                    server.config = config.clone();
                    // Republish all diagnostics
                    let mut files_to_republish = HashSet::new();
                    let mut files_to_clear = HashSet::new();
                    let watched_urls: Vec<(Url, ShadingLanguage)> = server
                        .watched_files
                        .files
                        .iter()
                        .filter(|(_, file)| file.is_cachable_file())
                        .map(|(url, file)| (url.clone(), file.shading_language))
                        .collect();
                    for (url, shading_language) in watched_urls {
                        profile_scope!("Updating server config for file: {}", url);
                        let language_data =
                            server.language_data.get_mut(&shading_language).unwrap();
                        // Update symbols & republish diags.
                        match server.watched_files.cache_file_data(
                            &url,
                            language_data.validator.as_mut(),
                            &mut language_data.language,
                            &language_data.symbol_provider,
                            &server.config,
                            Some(&url.to_file_path().unwrap()),
                        ) {
                            Ok(removed_files) => {
                                let url_to_republish = server
                                    .watched_files
                                    .get_relying_variant(&url)
                                    .unwrap_or(url.clone());
                                files_to_republish.insert(url_to_republish);
                                files_to_clear.extend(removed_files);
                            }
                            Err(err) => server
                                .connection
                                .send_notification_error(format!("{}", err)),
                        }
                    }
                    // Republish all diagnostics with new settings.
                    for url in &files_to_clear {
                        server.clear_diagnostic(url);
                    }
                    for url in &files_to_republish {
                        server.publish_diagnostic(url, None);
                    }
                } else {
                    info!("Requested configuration has not changed.");
                }
            },
        );
    }
}
