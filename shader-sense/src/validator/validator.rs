use std::path::Path;

#[cfg(not(target_os = "wasi"))]
use crate::validator::dxc::Dxc;
use crate::{
    shader::{ShaderParams, ShaderStage, ShadingLanguage},
    shader_error::{ShaderDiagnosticList, ShaderError},
    validator::{glslang::Glslang, naga::Naga},
};

pub fn default_include_callback(path: &Path) -> Option<String> {
    Some(std::fs::read_to_string(path).unwrap())
}
pub trait ValidatorImpl {
    fn validate_shader(
        &self,
        shader_content: &str,
        file_path: &Path,
        params: &ShaderParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<ShaderDiagnosticList, ShaderError>;

    fn support(&self, shader_stage: ShaderStage) -> bool;

    fn get_file_name(&self, path: &Path) -> String {
        String::from(path.file_name().unwrap().to_string_lossy())
    }
}

pub struct Validator {
    imp: Box<dyn ValidatorImpl>,
}
impl Validator {
    pub fn glsl() -> Self {
        Self {
            imp: Box::new(Glslang::glsl()),
        }
    }
    pub fn hlsl() -> Self {
        Self {
            #[cfg(not(target_os = "wasi"))]
            imp: match Dxc::new() {
                Ok(dxc) => Box::new(dxc),
                Err(_) => Box::new(Glslang::hlsl()), // Failed to instantiate dxc. Fallback to glslang.
            },
            #[cfg(target_os = "wasi")]
            imp: Box::new(Glslang::hlsl()),
        }
    }
    pub fn wgsl() -> Self {
        Self {
            imp: Box::new(Naga::new()),
        }
    }
    pub fn from_shading_language(shading_language: ShadingLanguage) -> Self {
        match shading_language {
            ShadingLanguage::Wgsl => Self::wgsl(),
            ShadingLanguage::Hlsl => Self::hlsl(),
            ShadingLanguage::Glsl => Self::glsl(),
        }
    }
    pub fn validate_shader(
        &self,
        shader_content: &str,
        file_path: &Path,
        params: &ShaderParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<ShaderDiagnosticList, ShaderError> {
        self.imp
            .validate_shader(shader_content, file_path, params, include_callback)
    }
}
