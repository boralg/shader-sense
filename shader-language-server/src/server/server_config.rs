use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use shader_sense::{
    shader::{GlslSpirvVersion, GlslTargetClient, HlslShaderModel, HlslVersion},
    shader_error::ShaderDiagnosticSeverity,
    symbols::symbols::ShaderSymbolParams,
    validator::validator::ValidationParams,
};

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerHlslConfig {
    pub shaderModel: HlslShaderModel,
    pub version: HlslVersion,
    pub enable16bitTypes: bool,
}
#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerGlslConfig {
    pub targetClient: GlslTargetClient,
    pub spirvVersion: GlslSpirvVersion,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub includes: Vec<String>,
    pub defines: HashMap<String, String>,
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
            hlsl_shader_model: self.hlsl.shaderModel,
            hlsl_version: self.hlsl.version,
            hlsl_enable16bit_types: self.hlsl.enable16bitTypes,
            glsl_client: self.glsl.targetClient,
            glsl_spirv: self.glsl.spirvVersion,
        }
    }
    pub fn into_symbol_params(&self) -> ShaderSymbolParams {
        ShaderSymbolParams {
            defines: self.defines.clone(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            includes: Vec::new(),
            defines: HashMap::new(),
            validate: true,
            symbols: true,
            severity: ShaderDiagnosticSeverity::Hint.to_string(),
            hlsl: ServerHlslConfig::default(),
            glsl: ServerGlslConfig::default(),
        }
    }
}
