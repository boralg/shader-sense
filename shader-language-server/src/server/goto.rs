use std::{cell::RefCell, rc::Rc};

use shader_sense::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolData},
};

use lsp_types::{GotoDefinitionResponse, Position, Url};

use super::{common::shader_range_to_lsp_range, ServerFileCacheHandle, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_goto(
        &mut self,
        uri: &Url,
        cached_file: ServerFileCacheHandle,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>, ShaderError> {
        let cached_file_borrowed = RefCell::borrow(&cached_file);
        let language_data = self
            .language_data
            .get(&cached_file_borrowed.shading_language)
            .unwrap();
        let file_path = uri.to_file_path().unwrap();
        let shader_position = ShaderPosition {
            file_path: file_path.clone(),
            line: position.line as u32,
            pos: position.character as u32,
        };
        let all_symbol_list = self.watched_files.get_all_symbols(
            uri,
            Rc::clone(&cached_file),
            language_data.symbol_provider.as_ref(),
        );
        match language_data
            .symbol_provider
            .get_word_range_at_position(&cached_file_borrowed.symbol_tree, shader_position.clone())
        {
            Ok((word, word_range)) => {
                let symbol_list = all_symbol_list.filter_scoped_symbol(shader_position);
                let matching_symbols = symbol_list.find_symbols(word);
                Ok(Some(GotoDefinitionResponse::Link(
                    matching_symbols
                        .iter()
                        .filter_map(|symbol| {
                            if let ShaderSymbolData::Link { target } = &symbol.data {
                                match &symbol.range {
                                    // _range here should be equal to selected_range.
                                    Some(_range) => Some(lsp_types::LocationLink {
                                        origin_selection_range: Some(shader_range_to_lsp_range(
                                            &word_range,
                                        )),
                                        target_uri: Url::from_file_path(&target.file_path).unwrap(),
                                        target_range: shader_range_to_lsp_range(&ShaderRange::new(
                                            target.clone(),
                                            target.clone(),
                                        )),
                                        target_selection_range: shader_range_to_lsp_range(
                                            &ShaderRange::new(target.clone(), target.clone()),
                                        ),
                                    }),
                                    None => None,
                                }
                            } else {
                                match &symbol.range {
                                    Some(range) => Some(lsp_types::LocationLink {
                                        origin_selection_range: Some(shader_range_to_lsp_range(
                                            &word_range,
                                        )),
                                        target_uri: Url::from_file_path(&range.start.file_path)
                                            .unwrap(),
                                        target_range: shader_range_to_lsp_range(range),
                                        target_selection_range: shader_range_to_lsp_range(range),
                                    }),
                                    None => None,
                                }
                            }
                        })
                        .collect(),
                )))
            }
            Err(err) => {
                if let ShaderError::NoSymbol = err {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}
