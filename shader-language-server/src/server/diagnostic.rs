use std::collections::HashMap;

use log::info;
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, PublishDiagnosticsParams, Url};

use shader_sense::shader_error::{ShaderDiagnosticSeverity, ShaderError};

use crate::server::common::shader_range_to_lsp_range;

use super::{ServerConnection, ServerLanguage};

impl ServerLanguage {
    pub fn publish_diagnostic(&mut self, uri: &Url, version: Option<i32>) {
        if self.config.get_validate() {
            match self.recolt_diagnostic(uri) {
                Ok(diagnostics) => {
                    info!(
                        "Publishing diagnostic for file {} ({} diags)",
                        uri,
                        diagnostics.len()
                    );
                    for diagnostic in diagnostics {
                        let publish_diagnostics_params = PublishDiagnosticsParams {
                            uri: diagnostic.0,
                            diagnostics: diagnostic.1,
                            version: version,
                        };
                        self.connection
                            .send_notification::<lsp_types::notification::PublishDiagnostics>(
                                publish_diagnostics_params,
                            );
                    }
                }
                Err(err) => self.connection.send_notification_error(format!(
                    "Failed to compute diagnostic for file {}: {}",
                    uri, err
                )),
            }
        } else {
            info!("Diagnostic disabled. {:?}", self.config);
        }
    }

    pub fn clear_diagnostic(&self, connection: &ServerConnection, uri: &Url) {
        // TODO: check it exist ?
        info!("Clearing diagnostic for file {}", uri);
        let publish_diagnostics_params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: Vec::new(),
            version: None,
        };
        connection.send_notification::<lsp_types::notification::PublishDiagnostics>(
            publish_diagnostics_params,
        );
    }

    pub fn recolt_diagnostic(
        &mut self,
        uri: &Url,
    ) -> Result<HashMap<Url, Vec<Diagnostic>>, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        let data = cached_file.get_data();
        // Diagnostic for included file stored in main cache.
        let diagnostic_cache = &data.diagnostic_cache;

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        for diagnostic in &diagnostic_cache.diagnostics {
            let uri = Url::from_file_path(&diagnostic.range.start.file_path).unwrap();
            if diagnostic.severity.is_required(self.config.get_severity()) {
                let diagnostic = Diagnostic {
                    range: shader_range_to_lsp_range(&diagnostic.range),
                    severity: Some(match diagnostic.severity {
                        ShaderDiagnosticSeverity::Hint => lsp_types::DiagnosticSeverity::HINT,
                        ShaderDiagnosticSeverity::Information => {
                            lsp_types::DiagnosticSeverity::INFORMATION
                        }
                        ShaderDiagnosticSeverity::Warning => lsp_types::DiagnosticSeverity::WARNING,
                        ShaderDiagnosticSeverity::Error => lsp_types::DiagnosticSeverity::ERROR,
                    }),
                    message: if diagnostic.error.is_empty() {
                        "No message.".into() // vscode extension send error when empty message.
                    } else {
                        diagnostic.error.clone()
                    },
                    source: Some("shader-validator".to_string()),
                    ..Default::default()
                };
                match diagnostics.get_mut(&uri) {
                    Some(value) => value.push(diagnostic),
                    None => {
                        diagnostics.insert(uri, vec![diagnostic]);
                    }
                };
            }
        }
        // Clear diagnostic if no errors.
        if diagnostics.get(&uri).is_none() {
            info!(
                "No issue found for main file. Clearing previous diagnostic {}",
                uri
            );
            diagnostics.insert(uri.clone(), vec![]);
        }
        // Add empty diagnostics to direct dependencies without errors to clear them.
        for include in &data.symbol_cache.get_preprocessor().includes {
            let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
            if diagnostics.get(&include_uri).is_none() {
                info!("Clearing diagnostic for deps file {}", include_uri);
                diagnostics.insert(include_uri.clone(), vec![]);
            }
        }
        // Add inactive regions to diag for open file.
        let mut inactive_diagnostics = data
            .symbol_cache
            .get_preprocessor()
            .regions
            .iter()
            .filter_map(|region| {
                (!region.is_active).then_some(Diagnostic {
                    range: shader_range_to_lsp_range(&region.range),
                    severity: Some(DiagnosticSeverity::HINT),
                    message: "Code disabled by currently used macros".into(),
                    source: Some("shader-validator".to_string()),
                    tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                    ..Default::default()
                })
            })
            .collect();
        match diagnostics.get_mut(&uri) {
            Some(diagnostics) => {
                diagnostics.append(&mut inactive_diagnostics);
            }
            None => {
                diagnostics.insert(uri.clone(), inactive_diagnostics);
            }
        }
        Ok(diagnostics)
    }
}
