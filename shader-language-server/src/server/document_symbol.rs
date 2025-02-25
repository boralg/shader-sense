use std::cell::RefCell;

use lsp_types::{SymbolInformation, SymbolKind, Url};
use shader_sense::{shader_error::ShaderError, symbols::symbols::ShaderSymbolType};

use super::{
    common::shader_range_to_location, server_file_cache::ServerFileCacheHandle, ServerLanguage,
};

impl ServerLanguage {
    pub fn recolt_document_symbol(
        &mut self,
        _uri: &Url,
        cached_file: &ServerFileCacheHandle,
    ) -> Result<Vec<SymbolInformation>, ShaderError> {
        let symbols = RefCell::borrow(&cached_file)
            .data
            .symbol_cache
            .iter()
            .map(|(symbols, ty)| {
                symbols
                    .iter()
                    .filter(|symbol| {
                        // Dont publish keywords.
                        ty != ShaderSymbolType::Keyword && symbol.range.is_some()
                    })
                    .map(|symbol| {
                        #[allow(deprecated)]
                        // https://github.com/rust-lang/rust/issues/102777
                        SymbolInformation {
                            name: symbol.label.clone(),
                            kind: match ty {
                                ShaderSymbolType::Types => SymbolKind::TYPE_PARAMETER,
                                ShaderSymbolType::Constants => SymbolKind::CONSTANT,
                                ShaderSymbolType::Variables => SymbolKind::VARIABLE,
                                ShaderSymbolType::Functions => SymbolKind::FUNCTION,
                                ShaderSymbolType::Macros => SymbolKind::CONSTANT,
                                ShaderSymbolType::Include => SymbolKind::FILE,
                                ShaderSymbolType::Keyword => {
                                    unreachable!("Should be filtered out")
                                }
                            },
                            tags: None,
                            deprecated: None,
                            location: shader_range_to_location(
                                symbol.range.as_ref().expect("Should be filtered out"),
                            ),
                            container_name: None,
                        }
                    })
                    .collect()
            })
            .collect::<Vec<Vec<SymbolInformation>>>()
            .concat();
        Ok(symbols)
    }
}
