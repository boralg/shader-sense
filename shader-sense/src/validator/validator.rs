use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    include::Dependencies,
    shader::{GlslSpirvVersion, GlslTargetClient, HlslShaderModel, HlslVersion},
    shader_error::{ShaderDiagnosticList, ShaderError},
};

#[derive(Debug, Default, Clone)]
pub struct ValidationParams {
    pub includes: Vec<String>,
    pub defines: HashMap<String, String>,
    pub path_remapping: HashMap<PathBuf, PathBuf>,
    pub hlsl_shader_model: HlslShaderModel,
    pub hlsl_version: HlslVersion,
    pub hlsl_enable16bit_types: bool,
    pub glsl_client: GlslTargetClient,
    pub glsl_spirv: GlslSpirvVersion,
}

pub trait Validator {
    fn validate_shader(
        &mut self,
        shader_content: &String,
        file_path: &Path,
        params: &ValidationParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<(ShaderDiagnosticList, Dependencies), ShaderError>;

    fn get_file_name(&self, path: &Path) -> String {
        String::from(path.file_name().unwrap().to_string_lossy())
    }
}
