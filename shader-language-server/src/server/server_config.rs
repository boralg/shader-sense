use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use shader_sense::{
    shader::{GlslSpirvVersion, GlslTargetClient, HlslShaderModel, HlslVersion},
    shader_error::ShaderDiagnosticSeverity,
    symbols::symbol_provider::ShaderSymbolParams,
    validator::validator::ValidationParams,
};

use super::shader_variant::ShaderVariant;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHlslConfig {
    pub shader_model: HlslShaderModel,
    pub version: HlslVersion,
    pub enable16bit_types: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerGlslConfig {
    pub target_client: GlslTargetClient,
    pub spirv_version: GlslSpirvVersion,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerTraceLevel {
    #[default]
    Off,
    Messages,
    Verbose,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTrace {
    server: ServerTraceLevel,
}

impl ServerTrace {
    pub fn is_verbose(&self) -> bool {
        self.server == ServerTraceLevel::Verbose
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub includes: Vec<String>,
    pub defines: HashMap<String, String>,
    pub path_remapping: HashMap<String, String>,
    pub validate: bool,
    pub symbols: bool,
    pub symbol_diagnostics: bool,
    pub trace: ServerTrace,
    pub severity: String,
    pub hlsl: ServerHlslConfig,
    pub glsl: ServerGlslConfig,
}

impl ServerConfig {
    pub fn into_validation_params(&self, variant: Option<ShaderVariant>) -> ValidationParams {
        let (mut defines, mut includes) = match variant {
            Some(variant) => (
                variant.defines.clone(),
                variant
                    .includes
                    .into_iter()
                    .map(|e| e.into_os_string().into_string().unwrap())
                    .collect::<Vec<String>>(),
            ),
            None => (HashMap::new(), Vec::new()),
        };
        defines.extend(self.defines.clone());
        includes.extend(self.includes.clone());
        ValidationParams {
            defines: defines,
            includes: includes,
            path_remapping: self
                .path_remapping
                .iter()
                .map(|(vp, p)| (vp.into(), p.into()))
                .collect(),
            hlsl_shader_model: self.hlsl.shader_model,
            hlsl_version: self.hlsl.version,
            hlsl_enable16bit_types: self.hlsl.enable16bit_types,
            glsl_client: self.glsl.target_client,
            glsl_spirv: self.glsl.spirv_version,
        }
    }
    pub fn into_symbol_params(&self, variant: Option<ShaderVariant>) -> ShaderSymbolParams {
        let (mut defines, mut includes) = match variant {
            Some(variant) => (
                variant.defines.clone(),
                variant
                    .includes
                    .into_iter()
                    .map(|e| e.into_os_string().into_string().unwrap())
                    .collect::<Vec<String>>(),
            ),
            None => (HashMap::new(), Vec::new()),
        };
        defines.extend(self.defines.clone());
        includes.extend(self.includes.clone());
        ShaderSymbolParams {
            defines: defines,
            includes: includes,
            path_remapping: self
                .path_remapping
                .iter()
                .map(|(vp, p)| (vp.into(), p.into()))
                .collect(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            includes: Vec::new(),
            defines: HashMap::new(),
            path_remapping: HashMap::new(),
            validate: true,
            symbols: true,
            symbol_diagnostics: false,
            trace: ServerTrace::default(),
            severity: ShaderDiagnosticSeverity::Hint.to_string(),
            hlsl: ServerHlslConfig::default(),
            glsl: ServerGlslConfig::default(),
        }
    }
}
