use std::cell::RefCell;

use lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult, Url};

use shader_sense::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange},
};

use super::{ServerFileCacheHandle, ServerLanguage};

impl ServerLanguage {
    pub fn recolt_semantic_tokens(
        &mut self,
        uri: &Url,
        cached_file: ServerFileCacheHandle,
    ) -> Result<SemanticTokensResult, ShaderError> {
        // For now, only handle macros as we cant resolve them with textmate.
        let shading_language = RefCell::borrow(&cached_file).shading_language;
        let symbols = self.watched_files.get_all_symbols(
            &uri,
            &cached_file,
            self.language_data
                .get(&shading_language)
                .unwrap()
                .symbol_provider
                .as_ref(),
        );
        // Find occurences of macros to paint them.
        let tokens = symbols
            .macros
            .iter()
            .map(|symbol| {
                let content = &RefCell::borrow(&cached_file).symbol_tree.content;
                let occurences: Vec<(usize, &str)> = content.match_indices(&symbol.label).collect();
                occurences
                    .iter()
                    .map(|(offset, label)| {
                        let position = ShaderPosition::from_byte_offset(
                            &content,
                            *offset,
                            &uri.to_file_path().unwrap(),
                        );
                        SemanticToken {
                            delta_line: position.line,
                            delta_start: position.pos,
                            length: label.len() as u32,
                            token_type: 0, // SemanticTokenType::MACRO, view registration
                            token_modifiers_bitset: 0,
                        }
                    })
                    .collect()
            })
            .collect::<Vec<Vec<SemanticToken>>>()
            .concat();

        Ok(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        }))
    }
}
