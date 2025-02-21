use shader_sense::{
    symbols::{
        symbol_provider::SymbolProvider, GlslSymbolProvider, HlslSymbolProvider, WgslSymbolProvider,
    },
    validator::{glslang::Glslang, naga::Naga, validator::Validator},
};

#[cfg(not(target_os = "wasi"))]
use shader_sense::validator::dxc::Dxc;

pub struct ServerLanguageData {
    pub validator: Box<dyn Validator>,
    pub symbol_provider: Box<dyn SymbolProvider>,
}

impl ServerLanguageData {
    pub fn glsl() -> Self {
        Self {
            validator: Box::new(Glslang::glsl()),
            symbol_provider: Box::new(GlslSymbolProvider::new()),
        }
    }
    pub fn hlsl() -> Self {
        Self {
            #[cfg(target_os = "wasi")]
            validator: Box::new(Glslang::hlsl()),
            #[cfg(not(target_os = "wasi"))]
            validator: Box::new(Dxc::new().unwrap()),
            symbol_provider: Box::new(HlslSymbolProvider::new()),
        }
    }
    pub fn wgsl() -> Self {
        Self {
            validator: Box::new(Naga::new()),
            symbol_provider: Box::new(WgslSymbolProvider::new()),
        }
    }
}
