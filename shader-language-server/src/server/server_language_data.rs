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

use super::{
    server_config::ServerConfig,
    server_file_cache::{ServerFileCacheHandle, ServerLanguageFileCache},
};

pub struct ServerLanguageData {
    pub watched_files: ServerLanguageFileCache,
    pub validator: Box<dyn Validator>,
    pub symbol_provider: Box<dyn SymbolProvider>,
    pub config: ServerConfig,
}

impl ServerLanguageData {
    pub fn glsl() -> Self {
        Self {
            watched_files: ServerLanguageFileCache::new(),
            validator: Box::new(Glslang::glsl()),
            symbol_provider: Box::new(GlslSymbolProvider::new()),
            config: ServerConfig::default(),
        }
    }
    pub fn hlsl() -> Self {
        Self {
            watched_files: ServerLanguageFileCache::new(),
            #[cfg(target_os = "wasi")]
            validator: Box::new(Glslang::hlsl()),
            #[cfg(not(target_os = "wasi"))]
            validator: Box::new(Dxc::new().unwrap()),
            symbol_provider: Box::new(HlslSymbolProvider::new()),
            config: ServerConfig::default(),
        }
    }
    pub fn wgsl() -> Self {
        Self {
            watched_files: ServerLanguageFileCache::new(),
            validator: Box::new(Naga::new()),
            symbol_provider: Box::new(WgslSymbolProvider::new()),
            config: ServerConfig::default(),
        }
    }
    pub fn get_all_symbols(&self, cached_file: ServerFileCacheHandle) -> ShaderSymbolList {
        let cached_file = RefCell::borrow(&cached_file);
        // Add current symbols
        let mut symbol_cache = cached_file.symbol_cache.clone();
        // Add intrinsics symbols
        symbol_cache.append(self.symbol_provider.get_intrinsics_symbol().clone());
        // Add deps symbols
        for (_, deps_cached_file) in &cached_file.dependencies {
            let deps_cached_file = RefCell::borrow(&deps_cached_file);
            symbol_cache.append(deps_cached_file.symbol_cache.clone());
        }
        // Add macros
        self.config.append_custom_defines(&mut symbol_cache);
        symbol_cache
    }
}
