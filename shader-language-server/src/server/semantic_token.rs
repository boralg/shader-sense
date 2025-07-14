use std::cell::RefCell;

use lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult, Url};
use shader_sense::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderSymbolData, ShaderSymbolList},
};

use crate::server::server_file_cache::ServerFileCache;

use super::ServerLanguage;

impl ServerLanguage {
    fn find_macros(
        uri: &Url,
        cached_file: &ServerFileCache,
        symbols: &ShaderSymbolList,
    ) -> Vec<SemanticToken> {
        let file_path = uri.to_file_path().unwrap();
        let content = &RefCell::borrow(&cached_file.shader_module).content;
        symbols
            .macros
            .iter()
            .map(|symbol| {
                let byte_offset_start = match &symbol.range {
                    Some(range) => {
                        if range.start.file_path == file_path {
                            range.start.to_byte_offset(content).unwrap()
                        } else {
                            match cached_file
                                .data
                                .as_ref()
                                .unwrap()
                                .symbol_cache
                                .find_direct_includer(&range.start.file_path)
                            {
                                Some(include) => {
                                    include.range.start.to_byte_offset(content).unwrap()
                                }
                                None => 0, // Included from another file, but not found...
                            }
                        }
                    }
                    None => 0, // No range means its everywhere
                };
                // Need to ignore comment aswell... Might need tree sitter instead.
                // Looking for preproc_arg & identifier might be enough.
                // Need to check for regions too...
                let reg = regex::Regex::new(format!("\\b({})\\b", symbol.label).as_str()).unwrap();
                let word_byte_offsets: Vec<usize> = reg
                    .captures_iter(&content)
                    .map(|e| e.get(0).unwrap().range().start)
                    .collect();
                word_byte_offsets
                    .iter()
                    .filter_map(|byte_offset| {
                        match ShaderPosition::from_byte_offset(&content, *byte_offset, &file_path) {
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
        symbols: &ShaderSymbolList,
    ) -> Vec<SemanticToken> {
        let file_path = uri.to_file_path().unwrap();
        let content = &RefCell::borrow(&cached_file.shader_module).content;
        symbols
            .functions
            .iter()
            .map(|symbol| {
                let mut tokens = Vec::new();
                // If we own a scope and have a range.
                if let (Some(scope), Some(range)) = (&symbol.scope, &symbol.range) {
                    if range.start.file_path == file_path {
                        // DIRTY_HACK: Start to range instead of scope to include parameters, because we dont have range stored for them.
                        //let content_start = scope.start.to_byte_offset(&content).unwrap();
                        let content_start = range.start.to_byte_offset(&content).unwrap();
                        let content_end = scope.end.to_byte_offset(&content).unwrap();
                        match &symbol.data {
                            ShaderSymbolData::Functions { signatures } => {
                                assert!(signatures.len() == 1, "Should have only one signature");
                                for parameter in &signatures[0].parameters {
                                    // TODO: Push parameter, but need range stored. Could be used elsewhere aswell
                                    /*tokens.push(SemanticToken {
                                        delta_line: parameter.,
                                        delta_start: (),
                                        length: parameter.label.len() as u32,
                                        token_type: 1, // SemanticTokenType::PARAMETERS, view registration
                                        token_modifiers_bitset: 0
                                    });*/
                                    // Push occurence in scope
                                    let reg = regex::Regex::new(
                                        format!("\\b({})\\b", parameter.label).as_str(),
                                    )
                                    .unwrap();
                                    let word_byte_offsets: Vec<usize> = reg
                                        .captures_iter(&content[content_start..content_end])
                                        .map(|e| e.get(0).unwrap().range().start + content_start)
                                        .collect();
                                    tokens.extend(
                                        word_byte_offsets
                                            .iter()
                                            .filter_map(|byte_offset| {
                                                match ShaderPosition::from_byte_offset(
                                                    &content,
                                                    *byte_offset,
                                                    &file_path,
                                                ) {
                                                    Ok(position) => {
                                                        Some(SemanticToken {
                                                            delta_line: position.line,
                                                            delta_start: position.pos,
                                                            length: parameter.label.len() as u32,
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
                tokens
            })
            .collect::<Vec<Vec<SemanticToken>>>()
            .concat()
    }
    pub fn recolt_semantic_tokens(
        &mut self,
        uri: &Url,
    ) -> Result<SemanticTokensResult, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        // For now, only handle macros as we cant resolve them with textmate.
        let shading_language = cached_file.shading_language;
        let symbols = self.watched_files.get_all_symbols(
            &uri,
            &self.language_data.get(&shading_language).unwrap().language,
        );
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
