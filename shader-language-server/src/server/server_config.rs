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
    pub shader_model: Option<HlslShaderModel>,
    pub version: Option<HlslVersion>,
    pub enable16bit_types: Option<bool>,
    pub spirv: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

// Only use option to allow non defined values.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn into_validation_params(&self, variant: Option<ShaderVariant>) -> ValidationParams {
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
        ValidationParams {
            entry_point: entry_point,
            shader_stage: shader_stage,
            defines: defines,
            includes: includes,
            path_remapping: self
                .path_remapping
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(vp, p)| (vp.into(), p.into()))
                .collect(),
            hlsl_shader_model: hlsl.shader_model.unwrap_or_default(),
            hlsl_version: hlsl.version.unwrap_or_default(),
            hlsl_enable16bit_types: hlsl.enable16bit_types.unwrap_or_default(),
            hlsl_spirv: hlsl.spirv.unwrap_or_default(),
            glsl_client: glsl.target_client.unwrap_or_default(),
            glsl_spirv: glsl.spirv_version.unwrap_or_default(),
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
        defines.extend(self.defines.clone().unwrap_or_default());
        includes.extend(self.includes.clone().unwrap_or_default());
        ShaderSymbolParams {
            defines: defines,
            includes: includes,
            path_remapping: self
                .path_remapping
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(vp, p)| (vp.into(), p.into()))
                .collect(),
        }
    }
    pub fn get_symbols(&self) -> bool {
        match &self.symbols {
            Some(symbols) => *symbols,
            None => false,
        }
    }
    pub fn get_validate(&self) -> bool {
        match &self.validate {
            Some(validate) => *validate,
            None => false,
        }
    }
    pub fn get_symbol_diagnostics(&self) -> bool {
        match &self.symbol_diagnostics {
            Some(symbol_diagnostics) => *symbol_diagnostics,
            None => false,
        }
    }
    pub fn get_severity(&self) -> ShaderDiagnosticSeverity {
        match &self.severity {
            Some(severity) => ShaderDiagnosticSeverity::from(severity.as_str()),
            None => ShaderDiagnosticSeverity::Error,
        }
    }
    pub fn is_verbose(&self) -> bool {
        match &self.trace {
            Some(trace) => trace.is_verbose(),
            None => false,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            includes: Some(Vec::new()),
            defines: Some(HashMap::new()),
            path_remapping: Some(HashMap::new()),
            validate: Some(true),
            symbols: Some(true),
            symbol_diagnostics: Some(false),
            trace: Some(ServerTrace::default()),
            severity: Some(ShaderDiagnosticSeverity::Hint.to_string()),
            hlsl: Some(ServerHlslConfig::default()),
            glsl: Some(ServerGlslConfig::default()),
        }
    }
}
