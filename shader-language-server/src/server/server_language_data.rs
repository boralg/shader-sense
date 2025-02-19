use std::cell::RefCell;

use shader_sense::{
    symbols::{
        symbol_provider::SymbolProvider, symbols::ShaderSymbolList, GlslSymbolProvider,
        HlslSymbolProvider, WgslSymbolProvider,
    },
    validator::{glslang::Glslang, naga::Naga, validator::Validator},
};

#[cfg(not(target_os = "wasi"))]
use shader_sense::validator::dxc::Dxc;

use super::server_file_cache::ServerFileCacheHandle;

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
    pub fn get_all_symbols(&self, cached_file: ServerFileCacheHandle) -> ShaderSymbolList {
        let cached_file = RefCell::borrow(&cached_file);
        // Add current symbols
        let mut symbol_cache = cached_file.symbol_cache.clone();
        let preprocess_cache = cached_file.preprocessor_cache.clone();
        // Preprocess symbols.
        preprocess_cache.preprocess_symbols(&mut symbol_cache);
        // Add deps symbols
        for (_, deps_cached_file) in &cached_file.dependencies {
            let deps_cached_file = RefCell::borrow(&deps_cached_file);
            // TODO: Should not store all deps here if we want to preprocess correctly
            symbol_cache.append(deps_cached_file.symbol_cache.clone());
        }
        // Add intrinsics symbols
        symbol_cache.append(self.symbol_provider.get_intrinsics_symbol().clone());
        symbol_cache
    }
}
