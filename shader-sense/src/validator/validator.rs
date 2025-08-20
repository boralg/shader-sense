use std::path::Path;

use crate::{
    shader::ShaderParams,
    shader_error::{ShaderDiagnosticList, ShaderError},
};

pub fn default_include_callback(path: &Path) -> Option<String> {
    Some(std::fs::read_to_string(path).unwrap())
}
pub trait Validator {
    fn validate_shader(
        &self,
        shader_content: &str,
        file_path: &Path,
        params: &ShaderParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<ShaderDiagnosticList, ShaderError>;

    fn get_file_name(&self, path: &Path) -> String {
        String::from(path.file_name().unwrap().to_string_lossy())
    }
}
