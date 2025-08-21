use std::{cell::RefCell, ffi::OsStr};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, MarkupContent, Position, Url,
};

use shader_sense::{
    position::{ShaderFilePosition, ShaderPosition},
    shader::ShadingLanguage,
    shader_error::ShaderError,
    symbols::symbols::{ShaderSymbol, ShaderSymbolData, ShaderSymbolType},
};

use super::ServerLanguage;

impl ServerLanguage {
    pub fn recolt_completion(
        &mut self,
        uri: &Url,
        position: Position,
        trigger_character: Option<String>,
    ) -> Result<Vec<CompletionItem>, ShaderError> {
        let cached_file = self.get_main_file(&uri)?;
        let language_data = self
            .language_data
            .get(&cached_file.shading_language)
            .unwrap();
        let file_path = uri.to_file_path().unwrap();
        let symbol_list = self.watched_files.get_all_symbols(uri);
        let content = &RefCell::borrow(&cached_file.shader_module).content;
        let shader_position = {
            let position =
                ShaderFilePosition::new(file_path.clone(), position.line, position.character);

            // Get UTF8 offset of trigger character
            let trigger_offset = match &trigger_character {
                Some(trigger) => trigger.len(),
                None => 0,
            };
            // Remove offset
            let byte_offset = position.position.to_byte_offset(content).unwrap() - trigger_offset;
            assert!(content.is_char_boundary(byte_offset));
            if byte_offset == 0 {
                ShaderPosition::from_byte_offset(content, byte_offset).unwrap()
            } else {
                let mut new_byte_offset = byte_offset;
                // Check if the previous character is ')' for getting function call label position
                let prev = &content[..byte_offset];
                let mut chars = prev.char_indices().rev();
                if let Some((_, ')')) = chars.next() {
                    let mut depth = 1;
                    for (idx, ch) in chars {
                        match ch {
                            ')' => depth += 1,
                            '(' => {
                                depth -= 1;
                                if depth == 0 {
                                    new_byte_offset = idx;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ShaderPosition::from_byte_offset(content, new_byte_offset).unwrap()
            }
        };
        let shader_file_position = ShaderFilePosition::from(file_path.clone(), shader_position);
        let symbol_list = symbol_list.filter_scoped_symbol(&shader_file_position);
        match trigger_character {
            Some(_) => {
                match language_data.symbol_provider.get_word_range_at_position(
                    &RefCell::borrow(&cached_file.shader_module),
                    &shader_file_position,
                ) {
                    Ok(word) => {
                        let symbols = word.find_symbol_from_parent(file_path.clone(), &symbol_list);
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
                                                .map(|m| {
                                                    m.as_symbol(
                                                        ty.runtime
                                                            .as_ref()
                                                            .map(|s| s.file_path.clone()),
                                                    )
                                                })
                                                .collect::<Vec<ShaderSymbol>>(),
                                        );
                                        members_and_methods.extend(
                                            methods
                                                .iter()
                                                .map(|m| {
                                                    m.as_symbol(
                                                        ty.runtime
                                                            .as_ref()
                                                            .map(|s| s.file_path.clone()),
                                                    )
                                                })
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
    let position = if let Some(runtime) = &shader_symbol.runtime {
        format!(
            "{}:{}:{}",
            runtime
                .file_path
                .file_name()
                .unwrap_or(OsStr::new("file"))
                .to_string_lossy(),
            runtime.range.start.line,
            runtime.range.start.pos
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
            description: match &shader_symbol.data {
                ShaderSymbolData::Functions { signatures } => {
                    Some(if signatures.len() > 1 {
                        format!("{} (+ {})", signatures[0].format(shader_symbol.label.as_str()), signatures.len() - 1)
                    } else {
                        signatures[0].format(shader_symbol.label.as_str())
                    })
                },
                ShaderSymbolData::Method { context, signatures } => {
                    Some(if signatures.len() > 1 {
                        format!("{} (+ {})", signatures[0].format_with_context(shader_symbol.label.as_str(), context), signatures.len() - 1)
                    } else {
                        signatures[0].format(shader_symbol.label.as_str())
                    })
                },
                _ => Some(shader_symbol.format())
            }
        }),
        filter_text: Some(shader_symbol.label.clone()),
        documentation: Some(lsp_types::Documentation::MarkupContent(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: format!("```{shading_language}\n{signature}\n```\n{description}\n\n{doc_signature}\n\n{position}\n{doc_link}"),
        })),
        ..Default::default()
    }
}
