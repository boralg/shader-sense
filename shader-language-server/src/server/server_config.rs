use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use shader_sense::{
    shader::{GlslSpirvVersion, GlslTargetClient, HlslShaderModel, HlslVersion},
    shader_error::ShaderDiagnosticSeverity,
    symbols::symbols::ShaderSymbolParams,
    validator::validator::ValidationParams,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub includes: Vec<String>,
    pub defines: HashMap<String, String>,
    pub path_remapping: HashMap<String, String>,
    pub validate: bool,
    pub symbols: bool,
    pub severity: String,
    pub hlsl: ServerHlslConfig,
    pub glsl: ServerGlslConfig,
}

impl ServerConfig {
    pub fn into_validation_params(&self) -> ValidationParams {
        ValidationParams {
            includes: self.includes.clone(),
            defines: self.defines.clone(),
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
    pub fn into_symbol_params(&self) -> ShaderSymbolParams {
        ShaderSymbolParams {
            defines: self.defines.clone(),
            includes: self.includes.clone(),
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
            severity: ShaderDiagnosticSeverity::Hint.to_string(),
            hlsl: ServerHlslConfig::default(),
            glsl: ServerGlslConfig::default(),
        }
    }
}
