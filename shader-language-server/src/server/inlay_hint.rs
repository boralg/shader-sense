use std::cell::RefCell;

use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Range, Url};

use shader_sense::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderSymbolData, ShaderSymbolType},
};

use super::{
    common::{lsp_range_to_shader_range, shader_position_to_lsp_position},
    ServerFileCacheHandle, ServerLanguage,
};

impl ServerLanguage {
    pub fn recolt_inlay_hint(
        &mut self,
        uri: &Url,
        cached_file: ServerFileCacheHandle,
        lsp_range: &Range,
    ) -> Result<Vec<InlayHint>, ShaderError> {
        let language_data = self
            .language_data
            .get_mut(&RefCell::borrow(&cached_file).shading_language)
            .unwrap();
        let symbols =
            self.watched_files
                .get_all_symbols(uri, &cached_file, &language_data.language);
        let inlay_hints = symbols
            .iter()
            .filter(|sl| sl.1 == ShaderSymbolType::CallExpression)
            .map(|sl| {
                sl.0.iter()
                    .filter(|s| match &s.range {
                        Some(range) => {
                            let valid_range =
                                lsp_range_to_shader_range(lsp_range, &range.start.file_path);
                            valid_range.contain_bounds(&range)
                        }
                        None => false, // Should not happen with local symbols
                    })
                    .map(|s| match &s.data {
                        ShaderSymbolData::CallExpression {
                            label,
                            range,
                            parameters,
                        } => {
                            // Find label from expression.
                            let symbols = symbols.find_symbols_before(&label, &range.start);
                            if symbols.len() == 0 {
                                vec![]
                            } else {
                                // NOTE: inlay hints have a limit of 43 char per line in vscode, after which, they are truncated.
                                // https://github.com/microsoft/vscode/pull/201190
                                // TODO: could solve parameter type to pick correct signature.
                                let symbol = symbols[0]; // Just pick first one now.
                                let functions = match &symbol.data {
                                    ShaderSymbolData::Functions { signatures } => signatures,
                                    ShaderSymbolData::Struct {
                                        constructors,
                                        members: _,
                                        methods: _,
                                    } => constructors,
                                    ShaderSymbolData::Types { constructors } => constructors,
                                    _ => &vec![],
                                };
                                match functions
                                    .iter()
                                    .find(|s| s.parameters.len() == parameters.len())
                                {
                                    Some(signature) => parameters
                                        .iter()
                                        .enumerate()
                                        .map(|(i, (_, range))| InlayHint {
                                            position: shader_position_to_lsp_position(&range.start),
                                            label: InlayHintLabel::String(format!(
                                                "{}:",
                                                signature.parameters[i].label
                                            )),
                                            kind: Some(InlayHintKind::PARAMETER),
                                            text_edits: None,
                                            tooltip: None,
                                            padding_left: None,
                                            padding_right: Some(true),
                                            data: None,
                                        })
                                        .collect::<Vec<InlayHint>>(),
                                    None => vec![],
                                }
                            }
                        }
                        _ => unreachable!("Should be filtered out"),
                    })
                    .collect::<Vec<Vec<InlayHint>>>()
                    .concat()
            })
            .collect::<Vec<Vec<InlayHint>>>()
            .concat();
        Ok(inlay_hints)
    }
}
