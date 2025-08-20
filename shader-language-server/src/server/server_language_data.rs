use shader_sense::{
    shader::ShadingLanguage,
    symbols::{shader_language::ShaderLanguage, symbol_provider::SymbolProvider},
    validator::{glslang::Glslang, naga::Naga, validator::ValidatorImpl},
};

#[cfg(not(target_os = "wasi"))]
use shader_sense::validator::dxc::Dxc;

pub struct ServerLanguageData {
    pub validator: Box<dyn ValidatorImpl>,
    pub language: ShaderLanguage,
    pub symbol_provider: SymbolProvider,
}

impl ServerLanguageData {
    pub fn glsl() -> Self {
        let language = ShaderLanguage::from_shading_language(ShadingLanguage::Glsl);
        let symbol_provider = SymbolProvider::from_shading_language(ShadingLanguage::Glsl);
        Self {
            validator: Box::new(Glslang::glsl()),
            language,
            symbol_provider,
        }
    }
    pub fn hlsl() -> Self {
        let language = ShaderLanguage::from_shading_language(ShadingLanguage::Hlsl);
        let symbol_provider = SymbolProvider::from_shading_language(ShadingLanguage::Hlsl);
        Self {
            #[cfg(target_os = "wasi")]
            validator: {
                log::info!("Using glslang for HLSL validation as DXC is unsupported on WASI.");
                Box::new(Glslang::hlsl())
            },
            #[cfg(not(target_os = "wasi"))]
            validator: match Dxc::new() {
                Ok(dxc) => {
                    log::info!(
                        "Using Dxc for HLSL. DXIL validation {}",
                        if dxc.is_dxil_validation_available() {
                            "available"
                        } else {
                            "unavailable"
                        }
                    );
                    Box::new(dxc)
                }
                Err(err) => {
                    log::error!(
                        "Failed to instantiate DXC: {}\nFallback to glslang instead.",
                        err.to_string()
                    );
                    Box::new(Glslang::hlsl())
                }
            },
            language,
            symbol_provider,
        }
    }
    pub fn wgsl() -> Self {
        let language = ShaderLanguage::from_shading_language(ShadingLanguage::Wgsl);
        let symbol_provider = SymbolProvider::from_shading_language(ShadingLanguage::Wgsl);
        Self {
            validator: Box::new(Naga::new()),
            language,
            symbol_provider,
        }
    }
}
