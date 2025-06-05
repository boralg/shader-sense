use lsp_types::{DocumentSymbol, SymbolKind, Url};
use shader_sense::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderScope, ShaderSymbolType},
};

use super::{common::shader_range_to_location, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_document_symbol(
        &mut self,
        uri: &Url,
    ) -> Result<Vec<DocumentSymbol>, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        let symbols = cached_file
            .get_data()
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
                        let label_range = symbol.range.clone().expect("Should be filtered out");
                        // Content expected to englobe label.
                        let content_range = match &symbol.scope {
                            Some(scope) => ShaderScope::join(scope.clone(), label_range.clone()),
                            None => label_range.clone(),
                        };
                        #[allow(deprecated)]
                        // https://github.com/rust-lang/rust/issues/102777
                        DocumentSymbol {
                            name: symbol.label.clone(),
                            detail: Some(symbol.format()),
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
                            range: shader_range_to_location(&content_range).range,
                            selection_range: shader_range_to_location(&label_range).range,
                            children: None, // TODO: Should use a tree instead.
                        }
                    })
                    .collect()
            })
            .collect::<Vec<Vec<DocumentSymbol>>>()
            .concat();
        Ok(symbols)
    }
}
