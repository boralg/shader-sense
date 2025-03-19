use shader_sense::{
    shader::ShadingLanguage,
    symbols::{shader_language::ShaderLanguage, symbol_provider::SymbolProvider},
    validator::{glslang::Glslang, naga::Naga, validator::Validator},
};

#[cfg(not(target_os = "wasi"))]
use shader_sense::validator::dxc::Dxc;

pub struct ServerLanguageData {
    pub validator: Box<dyn Validator>,
    pub language: ShaderLanguage,
    pub symbol_provider: SymbolProvider,
}

impl ServerLanguageData {
    pub fn glsl() -> Self {
        let language = ShaderLanguage::new(ShadingLanguage::Glsl);
        let symbol_provider = language.create_symbol_provider();
        Self {
            validator: Box::new(Glslang::glsl()),
            language,
            symbol_provider,
        }
    }
    pub fn hlsl() -> Self {
        let language = ShaderLanguage::new(ShadingLanguage::Hlsl);
        let symbol_provider = language.create_symbol_provider();
        Self {
            #[cfg(target_os = "wasi")]
            validator: Box::new(Glslang::hlsl()),
            #[cfg(not(target_os = "wasi"))]
            validator: Box::new(Dxc::new().unwrap()),
            language,
            symbol_provider,
        }
    }
    pub fn wgsl() -> Self {
        let language = ShaderLanguage::new(ShadingLanguage::Wgsl);
        let symbol_provider = language.create_symbol_provider();
        Self {
            validator: Box::new(Naga::new()),
            language,
            symbol_provider,
        }
    }
}
