use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use log::{debug, error, info};
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, PublishDiagnosticsParams, Url};

use shader_sense::shader_error::{ShaderDiagnosticSeverity, ShaderError};

use crate::server::common::shader_range_to_lsp_range;

use super::{ServerConnection, ServerFileCacheHandle, ServerLanguageData};

impl ServerLanguageData {
    pub fn publish_diagnostic(
        &mut self,
        connection: &ServerConnection,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        version: Option<i32>,
    ) {
        if self.config.validate {
            match self.recolt_diagnostic(uri, cached_file) {
                Ok(diagnostics) => {
                    info!(
                        "Publishing diagnostic for file {} ({} diags)",
                        uri.path(),
                        diagnostics.len()
                    );
                    for diagnostic in diagnostics {
                        let publish_diagnostics_params = PublishDiagnosticsParams {
                            uri: diagnostic.0,
                            diagnostics: diagnostic.1,
                            version: version,
                        };
                        connection
                            .send_notification::<lsp_types::notification::PublishDiagnostics>(
                                publish_diagnostics_params,
                            );
                    }
                }
                Err(err) => connection.send_notification_error(format!(
                    "Failed to compute diagnostic for file {}: {}",
                    uri, err
                )),
            }
        } else {
            debug!("Diagnostic disabled. {:?}", self.config);
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
        cached_file: &ServerFileCacheHandle,
    ) -> Result<HashMap<Url, Vec<Diagnostic>>, ShaderError> {
        let file_path = uri.to_file_path().unwrap();
        let validation_params = self.config.into_validation_params();
        let shading_language = RefCell::borrow(&cached_file).shading_language;
        let content = RefCell::borrow(&cached_file).symbol_tree.content.clone();
        debug!("Validating file {}", file_path.display());
        match self.validator.validate_shader(
            content,
            file_path.as_path(),
            validation_params.clone(),
            &mut |deps_path: &Path| -> Option<String> {
                let deps_uri = Url::from_file_path(deps_path).unwrap();
                let deps_file = match self.watched_files.get_dependency(&deps_uri) {
                    Some(deps_file) => deps_file,
                    None => {
                        // If include does not exist, add it to watched files.
                        match self.watched_files.watch_dependency(
                            &deps_uri,
                            shading_language,
                            self.symbol_provider.as_mut(),
                            &self.config,
                        ) {
                            Ok(deps_file) => deps_file,
                            Err(err) => {
                                error!("Failed to watch file {} : {:?}", file_path.display(), err);
                                return None;
                            }
                        }
                    }
                };
                let content = RefCell::borrow(&deps_file).symbol_tree.content.clone();
                RefCell::borrow_mut(&cached_file)
                    .dependencies
                    .insert(PathBuf::from(deps_path), deps_file);
                Some(content)
            },
        ) {
            Ok((diagnostic_list, dependencies)) => {
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                for diagnostic in diagnostic_list.diagnostics {
                    let uri = match diagnostic.file_path {
                        Some(diagnostic_file_path) => {
                            Url::from_file_path(&diagnostic_file_path).unwrap()
                        }
                        None => uri.clone(),
                    };
                    if diagnostic
                        .severity
                        .is_required(ShaderDiagnosticSeverity::from(self.config.severity.clone()))
                    {
                        let diagnostic = Diagnostic {
                            range: lsp_types::Range::new(
                                lsp_types::Position::new(diagnostic.line - 1, diagnostic.pos),
                                lsp_types::Position::new(diagnostic.line - 1, diagnostic.pos),
                            ),
                            severity: Some(match diagnostic.severity {
                                ShaderDiagnosticSeverity::Hint => {
                                    lsp_types::DiagnosticSeverity::HINT
                                }
                                ShaderDiagnosticSeverity::Information => {
                                    lsp_types::DiagnosticSeverity::INFORMATION
                                }
                                ShaderDiagnosticSeverity::Warning => {
                                    lsp_types::DiagnosticSeverity::WARNING
                                }
                                ShaderDiagnosticSeverity::Error => {
                                    lsp_types::DiagnosticSeverity::ERROR
                                }
                            }),
                            message: diagnostic.error,
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
                dependencies.visit_dependencies(&mut |dep| {
                    let uri = Url::from_file_path(&dep).unwrap();
                    if diagnostics.get(&uri).is_none() {
                        info!("Clearing diagnostic for deps file {}", uri);
                        diagnostics.insert(uri.clone(), vec![]);
                    }
                });
                // Add inactive regions to diag
                let mut inactive_diagnostics = RefCell::borrow(cached_file)
                    .preprocessor_cache
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
                    Some(diagnostics) => diagnostics.append(&mut inactive_diagnostics),
                    None => {
                        diagnostics.insert(uri.clone(), inactive_diagnostics);
                    }
                }
                Ok(diagnostics)
            }
            Err(err) => Err(err),
        }
    }
}
