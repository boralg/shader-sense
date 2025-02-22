use std::cell::RefCell;

use lsp_types::{Location, SymbolInformation, SymbolKind};
use shader_sense::{shader_error::ShaderError, symbols::symbols::ShaderSymbolType};

use super::{common::shader_range_to_lsp_range, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_workspace_symbol(&mut self) -> Result<Vec<SymbolInformation>, ShaderError> {
        // Might want to take active variants instead...
        // We dont browse workspace here, only opened files...
        let symbols = self
            .watched_files
            .files
            .iter()
            .map(|(uri, cached_file)| {
                let shading_language = RefCell::borrow(&cached_file).shading_language;
                let symbols = self.watched_files.get_all_symbols(
                    uri,
                    cached_file,
                    self.language_data
                        .get(&shading_language)
                        .unwrap()
                        .symbol_provider
                        .as_ref(),
                );
                symbols
                    .iter()
                    .filter(|(_, ty)| {
                        // For workspace, only publish function & types
                        *ty == ShaderSymbolType::Functions || *ty == ShaderSymbolType::Types
                    })
                    .map(|(symbols, ty)| {
                        symbols
                            .iter()
                            .filter(|symbol| {
                                symbol.range.is_some()
                                    && (symbol.scope_stack.is_none()
                                        || symbol.scope_stack.as_ref().unwrap().is_empty())
                            })
                            .map(|symbol| {
                                #[allow(deprecated)]
                                // https://github.com/rust-lang/rust/issues/102777
                                SymbolInformation {
                                    name: symbol.label.clone(),
                                    kind: match ty {
                                        ShaderSymbolType::Types => SymbolKind::TYPE_PARAMETER,
                                        ShaderSymbolType::Functions => SymbolKind::FUNCTION,
                                        _ => unreachable!("Should be filtered out"),
                                    },
                                    tags: None,
                                    deprecated: None,
                                    location: Location::new(
                                        uri.clone(),
                                        shader_range_to_lsp_range(
                                            &symbol.range.as_ref().expect("Should be filtered out"),
                                        ),
                                    ),
                                    container_name: Some(shading_language.to_string()),
                                }
                            })
                            .collect()
                    })
                    .collect::<Vec<Vec<SymbolInformation>>>()
                    .concat()
            })
            .collect::<Vec<Vec<SymbolInformation>>>()
            .concat();
        Ok(symbols)
    }
}
