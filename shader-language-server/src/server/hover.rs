use std::cell::RefCell;

use lsp_types::{Hover, HoverContents, MarkupContent, Position, Url};

use shader_sense::{shader_error::ShaderError, symbols::symbols::ShaderPosition};

use super::{common::shader_range_to_lsp_range, ServerFileCacheHandle, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_hover(
        &mut self,
        uri: &Url,
        cached_file: ServerFileCacheHandle,
        position: Position,
    ) -> Result<Option<Hover>, ShaderError> {
        let file_path = uri.to_file_path().unwrap();
        let shader_position = ShaderPosition {
            file_path: file_path.clone(),
            line: position.line as u32,
            pos: position.character as u32,
        };
        let cached_file = RefCell::borrow(&cached_file);
        let language_data = self
            .language_data
            .get(&cached_file.shading_language)
            .unwrap();
        match language_data
            .symbol_provider
            .get_word_range_at_position(&cached_file.symbol_tree, shader_position.clone())
        {
            // word_range should be the same as symbol range
            Ok((word, _word_range)) => match self.watched_files.get(uri) {
                Some(target_cached_file) => {
                    let all_symbol_list = self.watched_files.get_all_symbols(
                        uri,
                        &target_cached_file,
                        language_data.symbol_provider.as_ref(),
                    );
                    let target_cached_file = target_cached_file.borrow();
                    let symbol_list = all_symbol_list.filter_scoped_symbol(&shader_position);
                    let matching_symbols = symbol_list.find_symbols_before(word, &shader_position);
                    if matching_symbols.len() == 0 {
                        Ok(None)
                    } else {
                        let symbol = &matching_symbols[0];
                        let label = symbol.format();
                        let description = symbol.description.clone();
                        let link = match &symbol.link {
                            Some(link) => format!("[Online documentation]({})", link),
                            None => "".into(),
                        };
                        let location = match &symbol.range {
                            Some(range) => format!(
                                "Defined in {}, line {}",
                                if range.start.file_path == file_path {
                                    "this file".into()
                                } else {
                                    range.start.file_path.file_name().unwrap().to_string_lossy()
                                },
                                range.start.line + 1
                            ),
                            None => "".into(),
                        };

                        Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: lsp_types::MarkupKind::Markdown,
                                value: format!(
                                    "```{}\n{}\n```\n{}{}\n{}\n\n{}",
                                    target_cached_file.shading_language.to_string(),
                                    label,
                                    if matching_symbols.len() > 1 {
                                        format!("(+{} symbol)\n\n", matching_symbols.len() - 1)
                                    } else {
                                        "".into()
                                    },
                                    description,
                                    location,
                                    link
                                ),
                            }),
                            range: match &symbol.range {
                                None => None,
                                Some(range) => {
                                    if range.start.file_path == *file_path {
                                        Some(shader_range_to_lsp_range(range))
                                    } else {
                                        None
                                    }
                                }
                            },
                        }))
                    }
                }
                None => Ok(None),
            },
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
