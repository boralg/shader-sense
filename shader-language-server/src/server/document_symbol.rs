use lsp_types::{SymbolInformation, SymbolKind, Url};
use shader_sense::{shader_error::ShaderError, symbols::symbols::ShaderSymbolType};

use super::{common::shader_range_to_location, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_document_symbol(
        &mut self,
        uri: &Url,
    ) -> Result<Vec<SymbolInformation>, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        let symbols = cached_file
            .data
            .as_ref()
            .unwrap()
            .symbol_cache
            .get_local_symbols()
            .iter()
            .map(|(symbols, ty)| {
                symbols
                    .iter()
                    .filter(|symbol| {
                        // Dont publish keywords & transient.
                        ty != ShaderSymbolType::Keyword
                            && !ty.is_transient()
                            && symbol.range.is_some()
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
                                ShaderSymbolType::Keyword | ShaderSymbolType::CallExpression => {
                                    unreachable!("Field should be filtered out")
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
