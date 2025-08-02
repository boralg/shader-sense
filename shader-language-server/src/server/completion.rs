use std::{cell::RefCell, ffi::OsStr};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, MarkupContent, Position, Url,
};

use shader_sense::{
    shader::ShadingLanguage,
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderSymbol, ShaderSymbolData, ShaderSymbolType},
};

use super::ServerLanguage;

impl ServerLanguage {
    pub fn recolt_completion(
        &mut self,
        uri: &Url,
        position: Position,
        trigger_character: Option<String>,
    ) -> Result<Vec<CompletionItem>, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        let language_data = self
            .language_data
            .get_mut(&cached_file.shading_language)
            .unwrap();
        let file_path = uri.to_file_path().unwrap();
        let symbol_list = self
            .watched_files
            .get_all_symbols(uri, &language_data.language);
        let shader_position = ShaderPosition {
            file_path: file_path.clone(),
            line: position.line as u32,
            // TODO: -1 should be able to go up a line.
            pos: if position.character == 0 {
                0
            } else {
                position.character - 1
            },
        };
        let symbol_list = symbol_list.filter_scoped_symbol(&shader_position);
        match trigger_character {
            Some(_) => {
                match language_data.symbol_provider.get_word_range_at_position(
                    &RefCell::borrow(&cached_file.shader_module),
                    &shader_position,
                ) {
                    Ok(word) => {
                        let symbols = word.find_symbol_from_parent(&symbol_list);
                        // TODO: should select right ones based on types and context
                        if symbols.is_empty() {
                            Ok(vec![])
                        } else {
                            let symbol_type = &symbols[0];
                            let ty = match &symbol_type.data {
                                ShaderSymbolData::Variables { ty, count: _ } => ty,
                                ShaderSymbolData::Functions { signatures } => {
                                    &signatures[0].returnType
                                }
                                ShaderSymbolData::Parameter {
                                    context: _,
                                    ty,
                                    count: _,
                                } => ty,
                                ShaderSymbolData::Method {
                                    context: _,
                                    signatures,
                                } => &signatures[0].returnType,
                                _ => return Ok(vec![]),
                            };
                            let completion_items = match symbol_list.find_type_symbol(ty) {
                                Some(ty) => match &ty.data {
                                    ShaderSymbolData::Struct {
                                        constructors: _,
                                        members,
                                        methods,
                                    } => {
                                        let mut members_and_methods: Vec<ShaderSymbol> = Vec::new();
                                        members_and_methods.extend(
                                            members
                                                .iter()
                                                .map(|m| m.as_symbol(None))
                                                .collect::<Vec<ShaderSymbol>>(),
                                        );
                                        members_and_methods.extend(
                                            methods
                                                .iter()
                                                .map(|m| m.as_symbol(None))
                                                .collect::<Vec<ShaderSymbol>>(),
                                        );
                                        members_and_methods
                                            .into_iter()
                                            .map(|s| {
                                                convert_completion_item(
                                                    cached_file.shading_language,
                                                    &s,
                                                )
                                            })
                                            .collect()
                                    }
                                    _ => vec![],
                                },
                                None => vec![],
                            };
                            Ok(completion_items)
                        }
                    }
                    Err(err) => {
                        if let ShaderError::NoSymbol = err {
                            Ok(vec![])
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            None => Ok(symbol_list
                .iter()
                .filter(|symbol| !symbol.is_type(ShaderSymbolType::CallExpression))
                .map(|symbol| convert_completion_item(cached_file.shading_language, symbol))
                .collect::<Vec<CompletionItem>>()),
        }
    }
}

fn convert_completion_item(
    shading_language: ShadingLanguage,
    shader_symbol: &ShaderSymbol,
) -> CompletionItem {
    let completion_kind = match shader_symbol.get_type().unwrap() {
        ShaderSymbolType::Types => CompletionItemKind::TYPE_PARAMETER,
        ShaderSymbolType::Constants => CompletionItemKind::CONSTANT,
        ShaderSymbolType::Variables => CompletionItemKind::VARIABLE,
        ShaderSymbolType::Functions => CompletionItemKind::FUNCTION,
        ShaderSymbolType::Keyword => CompletionItemKind::KEYWORD,
        ShaderSymbolType::Macros => CompletionItemKind::CONSTANT,
        ShaderSymbolType::Include => CompletionItemKind::FILE,
        ShaderSymbolType::CallExpression => {
            unreachable!("Field should be filtered out.")
        }
    };
    let doc_link = if let Some(link) = &shader_symbol.link {
        if !link.is_empty() {
            format!("\n[Online documentation]({})", link)
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };
    let doc_signature = if let ShaderSymbolData::Functions { signatures } = &shader_symbol.data {
        // TODO: should not hide variants
        let parameters = signatures[0]
            .parameters
            .iter()
            .map(|p| format!("- `{} {}` {}", p.ty, p.label, p.description))
            .collect::<Vec<String>>();
        let parameters_markdown = if parameters.is_empty() {
            "".into()
        } else {
            format!("**Parameters:**\n\n{}", parameters.join("\n\n"))
        };
        format!(
            "\n**Return type:**\n\n`{}` {}\n\n{}",
            signatures[0].returnType, signatures[0].description, parameters_markdown
        )
    } else {
        "".to_string()
    };
    let position = if let Some(range) = &shader_symbol.range {
        format!(
            "{}:{}:{}",
            range
                .start
                .file_path
                .file_name()
                .unwrap_or(OsStr::new("file"))
                .to_string_lossy(),
            range.start.line,
            range.start.pos
        )
    } else {
        "".to_string()
    };
    let shading_language = shading_language.to_string();
    let description = {
        let mut description = shader_symbol.description.clone();
        let max_len = 500;
        if description.len() > max_len {
            description.truncate(max_len);
            description.push_str("...");
        }
        description
    };

    let signature = shader_symbol.format();
    CompletionItem {
        kind: Some(completion_kind),
        label: shader_symbol.label.clone(),
        detail: None,
        label_details: Some(CompletionItemLabelDetails {
            detail: None,
            description: if let ShaderSymbolData::Functions { signatures } = &shader_symbol.data {
                Some(if signatures.len() > 1 {
                    format!("{} (+ {})", signatures[0].format(shader_symbol.label.as_str()), signatures.len() - 1)
                } else {
                    signatures[0].format(shader_symbol.label.as_str())
                })
            } else {
                None
            },
        }),
        filter_text: Some(shader_symbol.label.clone()),
        documentation: Some(lsp_types::Documentation::MarkupContent(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: format!("```{shading_language}\n{signature}\n```\n{description}\n\n{doc_signature}\n\n{position}\n{doc_link}"),
        })),
        ..Default::default()
    }
}
