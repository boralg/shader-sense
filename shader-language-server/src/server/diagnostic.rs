use std::collections::HashMap;

use log::{info, warn};
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, PublishDiagnosticsParams, Url};

use shader_sense::shader_error::{ShaderDiagnosticSeverity, ShaderError};

use crate::server::common::shader_range_to_lsp_range;

use super::{ServerConnection, ServerLanguage};

impl ServerLanguage {
    pub fn publish_diagnostic(&mut self, uri: &Url, version: Option<i32>) {
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
    fn get_lsp_severity(severity: &ShaderDiagnosticSeverity) -> lsp_types::DiagnosticSeverity {
        match severity {
            ShaderDiagnosticSeverity::Hint => lsp_types::DiagnosticSeverity::HINT,
            ShaderDiagnosticSeverity::Information => lsp_types::DiagnosticSeverity::INFORMATION,
            ShaderDiagnosticSeverity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            ShaderDiagnosticSeverity::Error => lsp_types::DiagnosticSeverity::ERROR,
        }
    }
    pub fn recolt_diagnostic(
        &mut self,
        uri: &Url,
    ) -> Result<HashMap<Url, Vec<Diagnostic>>, ShaderError> {
        // If file not watched, send empty diagnostic.
        let cached_file = if let Some(cached_file) = self.watched_files.get_file(uri) {
            cached_file
        } else {
            warn!("Trying to recolt_diagnostic for file {} that is not watched. Sending empty diagnostic.", uri);
            return Ok(HashMap::from([(uri.clone(), Vec::new())]));
        };
        let data = cached_file.get_data();
        // Diagnostic for included file stored in main cache.
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        // Only take diagnostics if required
        if self.config.get_validate() {
            let diagnostic_cache = &data.diagnostic_cache;

            for diagnostic in &diagnostic_cache.diagnostics {
                let uri = Url::from_file_path(&diagnostic.range.start.file_path).unwrap();
                if diagnostic.severity.is_required(self.config.get_severity()) {
                    let diagnostic = Diagnostic {
                        range: shader_range_to_lsp_range(&diagnostic.range),
                        severity: Some(Self::get_lsp_severity(&diagnostic.severity)),
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
            // Add empty diagnostics to dependencies without errors to clear them.
            data.symbol_cache.visit_includes(&mut |include| {
                let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                if diagnostics.get(&include_uri).is_none() {
                    info!("Clearing diagnostic for deps file {}", include_uri);
                    diagnostics.insert(include_uri.clone(), vec![]);
                }
            });
        } else {
            info!("Diagnostic disabled.");
        }

        // Add inactive regions to diag for open file.
        let inactive_diagnostics = data
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
                if self.config.get_symbol_diagnostics() {
                    diagnostics.extend(
                        data.symbol_cache
                            .get_preprocessor()
                            .diagnostics
                            .iter()
                            .map(|d| Diagnostic {
                                range: shader_range_to_lsp_range(&d.range),
                                severity: Some(Self::get_lsp_severity(&d.severity)),
                                message: d.error.clone(),
                                source: Some("shader-validator".to_string()),
                                ..Default::default()
                            }),
                    );
                }
                diagnostics.extend(inactive_diagnostics);
            }
            None => {
                diagnostics.insert(uri.clone(), inactive_diagnostics);
            }
        }
        Ok(diagnostics)
    }
}
