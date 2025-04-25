use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    shader::{GlslSpirvVersion, GlslTargetClient, HlslShaderModel, HlslVersion, ShaderStage},
    shader_error::{ShaderDiagnosticList, ShaderError},
};

#[derive(Debug, Default, Clone)]
pub struct ValidationParams {
    pub entry_point: Option<String>,
    pub shader_stage: Option<ShaderStage>,
    pub includes: Vec<String>,
    pub defines: HashMap<String, String>,
    pub path_remapping: HashMap<PathBuf, PathBuf>,
    pub hlsl_shader_model: HlslShaderModel,
    pub hlsl_version: HlslVersion,
    pub hlsl_enable16bit_types: bool,
    pub glsl_client: GlslTargetClient,
    pub glsl_spirv: GlslSpirvVersion,
}

pub fn default_include_callback(path: &Path) -> Option<String> {
    Some(std::fs::read_to_string(path).unwrap())
}
pub trait Validator {
    fn validate_shader(
        &mut self,
        shader_content: &String,
        file_path: &Path,
        params: &ValidationParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<ShaderDiagnosticList, ShaderError>;

    fn get_file_name(&self, path: &Path) -> String {
        String::from(path.file_name().unwrap().to_string_lossy())
    }
}
