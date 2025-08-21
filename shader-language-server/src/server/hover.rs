use std::cell::RefCell;

use lsp_types::{Hover, HoverContents, MarkupContent, Position, Url};

use shader_sense::{position::ShaderFilePosition, shader_error::ShaderError};

use super::{common::shader_range_to_lsp_range, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_hover(
        &mut self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Hover>, ShaderError> {
        let cached_file = self.get_main_file(&uri)?;
        let file_path = uri.to_file_path().unwrap();
        let shader_position = ShaderFilePosition::new(
            file_path.clone(),
            position.line as u32,
            position.character as u32,
        );
        let language_data = self
            .language_data
            .get(&cached_file.shading_language)
            .unwrap();
        match language_data.symbol_provider.get_word_range_at_position(
            &RefCell::borrow(&cached_file.shader_module),
            &shader_position,
        ) {
            // word_range should be the same as symbol range
            Ok(word) => {
                let symbol_list = self.watched_files.get_all_symbols(uri);
                let matching_symbols = word.find_symbol_from_parent(&symbol_list);
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
                    let location = match &symbol.runtime {
                        Some(runtime) => format!(
                            "Defined in {}, line {}",
                            if runtime.file_path.as_os_str() == file_path.as_os_str() {
                                "this file".into()
                            } else {
                                runtime.file_path.file_name().unwrap().to_string_lossy()
                            },
                            runtime.range.start.line + 1
                        ),
                        None => "".into(),
                    };

                    Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: lsp_types::MarkupKind::Markdown,
                            value: format!(
                                "```{}\n{}\n```\n{}{}\n{}\n\n{}",
                                cached_file.shading_language.to_string(),
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
                        // Range of hovered element.
                        range: if word.get_range().file_path.as_os_str() == file_path.as_os_str() {
                            Some(shader_range_to_lsp_range(&word.get_range().range))
                        } else {
                            None
                        },
                    }))
                }
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
