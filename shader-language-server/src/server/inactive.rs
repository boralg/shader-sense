use std::cell::RefCell;

use lsp_types::Url;
use shader_sense::symbols::symbols::{ShaderRange, SymbolError};

use super::{server_file_cache::ServerFileCacheHandle, server_language_data::ServerLanguageData};

impl ServerLanguageData {
    pub fn recolt_inactive(
        &mut self,
        _uri: &Url,
        cached_file: &ServerFileCacheHandle,
    ) -> Result<Vec<ShaderRange>, SymbolError> {
        // https://github.com/microsoft/language-server-protocol/issues/1938
        let cached_file = RefCell::borrow(cached_file);
        match self
            .symbol_provider
            .get_inactive_regions(&cached_file.symbol_tree)
        {
            Ok(ranges) => Ok(ranges),
            Err(error) => Err(error),
        }
    }
}
