use std::cell::RefCell;

use lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult, Url};
use shader_sense::symbols::symbols::ShaderSymbolMode;
use shader_sense::{
    position::ShaderPosition, shader_error::ShaderError, symbols::symbol_list::ShaderSymbolListRef,
    symbols::symbols::ShaderSymbolData,
};

use crate::server::server_file_cache::ServerFileCache;

use crate::server::ServerLanguage;

impl ServerLanguage {
    fn find_macros(
        uri: &Url,
        cached_file: &ServerFileCache,
        symbols: &ShaderSymbolListRef,
    ) -> Vec<SemanticToken> {
        let file_path = uri.to_file_path().unwrap();
        let content = &RefCell::borrow(&cached_file.shader_module).content;
        symbols
            .macros
            .iter()
            .map(|symbol| {
                let byte_offset_start = match &symbol.mode {
                    ShaderSymbolMode::Runtime(runtime) => {
                        if runtime.file_path.as_os_str() == file_path.as_os_str() {
                            runtime.range.start.to_byte_offset(content).unwrap()
                        } else {
                            match cached_file
                                .data
                                .as_ref()
                                .unwrap()
                                .symbol_cache
                                .find_direct_includer(&runtime.file_path)
                            {
                                Some(include) => {
                                    include.get_range().start.to_byte_offset(content).unwrap()
                                }
                                None => 0, // Included from another file, but not found...
                            }
                        }
                    }
                    _ => 0, // Not runtime means no range means its everywhere
                };
                // Need to ignore comment aswell... Might need tree sitter instead.
                // Looking for preproc_arg & identifier might be enough.
                // Need to check for regions too...
                let reg =
                    regex::Regex::new(format!("\\b({})\\b", regex::escape(&symbol.label)).as_str())
                        .unwrap();
                let word_byte_offsets: Vec<usize> = reg
                    .captures_iter(&content)
                    .map(|e| e.get(0).unwrap().range().start)
                    .collect();
                word_byte_offsets
                    .iter()
                    .filter_map(|byte_offset| {
                        match ShaderPosition::from_byte_offset(&content, *byte_offset) {
                            Ok(position) => {
                                if byte_offset_start > *byte_offset {
                                    None
                                } else {
                                    Some(SemanticToken {
                                        delta_line: position.line,
                                        delta_start: position.pos,
                                        length: symbol.label.len() as u32,
                                        token_type: 0, // SemanticTokenType::MACRO, view registration
                                        token_modifiers_bitset: 0,
                                    })
                                }
                            }
                            Err(_) => None,
                        }
                    })
                    .collect()
            })
            .collect::<Vec<Vec<SemanticToken>>>()
            .concat()
    }
    fn find_parameters_variables(
        uri: &Url,
        cached_file: &ServerFileCache,
        symbols: &ShaderSymbolListRef,
    ) -> Vec<SemanticToken> {
        let file_path = uri.to_file_path().unwrap();
        let content = &RefCell::borrow(&cached_file.shader_module).content;
        symbols
            .functions
            .iter()
            .map(|symbol| {
                let mut tokens = Vec::new();
                // If we own a scope and have a range.
                if let ShaderSymbolMode::Runtime(runtime) = &symbol.mode {
                    if let Some(scope) = &runtime.scope {
                        if runtime.file_path.as_os_str() == file_path.as_os_str() {
                            let content_start = scope.start.to_byte_offset(&content).unwrap();
                            let content_end = scope.end.to_byte_offset(&content).unwrap();
                            match &symbol.data {
                                ShaderSymbolData::Functions { signatures } => {
                                    assert!(
                                        signatures.len() == 1,
                                        "Should have only one signature"
                                    );
                                    for parameter in &signatures[0].parameters {
                                        match &parameter.range {
                                            Some(range) => tokens.push(SemanticToken {
                                                delta_line: range.start.line,
                                                delta_start: range.start.pos,
                                                length: parameter.label.len() as u32,
                                                token_type: 1, // SemanticTokenType::PARAMETERS, view registration
                                                token_modifiers_bitset: 0,
                                            }),
                                            None => continue, // Should not happen for local symbol, but skip it to be sure...
                                        }
                                        // Push occurences in scope
                                        // TODO: NOT dot at beginning of capture (as its a field.)
                                        let reg = regex::Regex::new(
                                            format!("\\b({})\\b", regex::escape(&parameter.label))
                                                .as_str(),
                                        )
                                        .unwrap();
                                        let word_byte_offsets: Vec<usize> = reg
                                            .captures_iter(&content[content_start..content_end])
                                            .map(|e| {
                                                e.get(0).unwrap().range().start + content_start
                                            })
                                            .collect();
                                        tokens.extend(
                                            word_byte_offsets
                                                .iter()
                                                .filter_map(|byte_offset| {
                                                    match ShaderPosition::from_byte_offset(
                                                        &content,
                                                        *byte_offset,
                                                    ) {
                                                        Ok(position) => {
                                                            Some(SemanticToken {
                                                                delta_line: position.line,
                                                                delta_start: position.pos,
                                                                length: parameter.label.len()
                                                                    as u32,
                                                                token_type: 1, // SemanticTokenType::PARAMETERS, view registration
                                                                token_modifiers_bitset: 0,
                                                            })
                                                        }
                                                        Err(_) => None,
                                                    }
                                                })
                                                .collect::<Vec<SemanticToken>>(),
                                        );
                                    }
                                }
                                _ => {} // Nothing to push
                            }
                        } else {
                            // Nothing to push
                        }
                    } else {
                        // Nothing to push
                    }
                } else {
                    // Nothing to push
                }
                tokens
            })
            .collect::<Vec<Vec<SemanticToken>>>()
            .concat()
    }
    pub fn recolt_semantic_tokens(
        &mut self,
        uri: &Url,
    ) -> Result<SemanticTokensResult, ShaderError> {
        let cached_file = self.get_main_file(&uri)?;
        // For now, only handle macros as we cant resolve them with textmate.
        let symbols = self.watched_files.get_all_symbols(&uri);
        // Find occurences of tokens to paint
        let mut tokens = Vec::new();
        tokens.extend(Self::find_macros(uri, cached_file, &symbols));
        tokens.extend(Self::find_parameters_variables(uri, cached_file, &symbols));

        // Sort by positions
        tokens.sort_by(|lhs, rhs| {
            (&lhs.delta_line, &lhs.delta_start).cmp(&(&rhs.delta_line, &rhs.delta_start))
        });
        // Compute delta from position
        let mut delta_line = 0;
        let mut delta_pos = 0;
        for token in &mut tokens {
            // Reset pos on new line.
            if token.delta_line != delta_line {
                delta_pos = 0;
            }
            let line = token.delta_line;
            let pos = token.delta_start;
            token.delta_line = line - delta_line;
            token.delta_start = pos - delta_pos;
            delta_line = line;
            delta_pos = pos;
        }
        Ok(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        }))
    }
}
