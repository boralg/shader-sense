use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;

mod common;
mod completion;
mod debug;
mod diagnostic;
mod goto;
mod hover;
mod shader_variant;
mod signature;

mod server_config;
mod server_connection;
mod server_file_cache;
mod server_language_data;

use common::shader_range_to_lsp_range;
use debug::{DumpAstParams, DumpAstRequest};
use log::{debug, error, info, warn};
use lsp_types::notification::{
    DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification,
};
use lsp_types::request::{
    Completion, DocumentDiagnosticRequest, DocumentSymbolRequest, FoldingRangeRequest,
    GotoDefinition, HoverRequest, Request, SignatureHelpRequest, WorkspaceConfiguration,
    WorkspaceSymbolRequest,
};
use lsp_types::{
    CompletionOptionsCompletionItem, CompletionParams, CompletionResponse, ConfigurationParams,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentDiagnosticParams,
    DocumentDiagnosticReport, DocumentDiagnosticReportKind, DocumentDiagnosticReportResult,
    DocumentSymbolOptions, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeKind, FoldingRangeParams, FullDocumentDiagnosticReport, GotoDefinitionParams,
    HoverParams, HoverProviderCapability, Location, OneOf, RelatedFullDocumentDiagnosticReport,
    ServerCapabilities, SignatureHelpOptions, SignatureHelpParams, SymbolInformation, SymbolKind,
    TextDocumentSyncKind, Url, WorkDoneProgressOptions, WorkspaceSymbolOptions,
    WorkspaceSymbolParams,
};
use shader_sense::shader::ShadingLanguage;

use lsp_server::{ErrorCode, Message};

use serde_json::Value;
use server_config::ServerConfig;
use server_connection::ServerConnection;
use server_file_cache::{ServerFileCacheHandle, ServerLanguageFileCache};
use server_language_data::ServerLanguageData;
use shader_sense::symbols::symbols::ShaderSymbolType;
use shader_variant::{DidChangeShaderVariant, DidChangeShaderVariantParams};

pub struct ServerLanguage {
    connection: ServerConnection,
    config: ServerConfig,
    // Cache
    watched_files: ServerLanguageFileCache,
    language_data: HashMap<ShadingLanguage, ServerLanguageData>,
}

fn clean_url(url: &Url) -> Url {
    // Workaround issue with url encoded as &3a that break key comparison.
    // Clean it by converting back & forth.
    #[cfg(not(target_os = "wasi"))]
    {
        Url::from_file_path(
            url.to_file_path()
                .expect(format!("Failed to convert {} to a valid path.", url).as_str()),
        )
        .unwrap()
    }
    // This method of cleaning URL fail on WASI due to path format. Removing it.
    #[cfg(target_os = "wasi")]
    {
        url.clone()
    }
}

impl ServerLanguage {
    pub fn new() -> Self {
        // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
        Self {
            connection: ServerConnection::new(),
            watched_files: ServerLanguageFileCache::new(),
            config: ServerConfig::default(),
            language_data: HashMap::from([
                (ShadingLanguage::Glsl, ServerLanguageData::glsl()),
                (ShadingLanguage::Hlsl, ServerLanguageData::hlsl()),
                (ShadingLanguage::Wgsl, ServerLanguageData::wgsl()),
            ]),
        }
    }
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let server_capabilities = serde_json::to_value(&ServerCapabilities {
            text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            completion_provider: Some(lsp_types::CompletionOptions {
                resolve_provider: None, // For more detailed data
                completion_item: Some(CompletionOptionsCompletionItem {
                    label_details_support: Some(true),
                }),
                trigger_characters: Some(vec![".".into()]),
                ..Default::default()
            }),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_owned(), ",".to_owned()]),
                retrigger_characters: None,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            type_definition_provider: Some(lsp_types::TypeDefinitionProviderCapability::Simple(
                false, // Disable as definition_provider is doing it.
            )),
            // This seems to be done automatically by vscode, so not mandatory.
            //folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Right(DocumentSymbolOptions {
                label: None,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            })),
            workspace_symbol_provider: Some(OneOf::Right(WorkspaceSymbolOptions {
                resolve_provider: None,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            })),
            ..Default::default()
        })?;
        let client_initialization_params = self.connection.initialize(server_capabilities);
        debug!(
            "Received client params: {:#?}",
            client_initialization_params
        );
        // Request configuration as its not sent automatically (at least with vscode)
        self.request_configuration();

        return Ok(());
    }
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        loop {
            let msg_err = self.connection.connection.receiver.recv();
            match msg_err {
                Ok(msg) => match msg {
                    Message::Request(req) => {
                        if self.connection.connection.handle_shutdown(&req)? {
                            return Ok(());
                        }
                        self.on_request(req)?;
                    }
                    Message::Response(resp) => {
                        self.on_response(resp)?;
                    }
                    Message::Notification(not) => {
                        self.on_notification(not)?;
                    }
                },
                Err(_) => {
                    // Recv error means disconnected.
                    return Ok(());
                }
            }
        }
    }
    fn on_request(&mut self, req: lsp_server::Request) -> Result<(), serde_json::Error> {
        match req.method.as_str() {
            DocumentDiagnosticRequest::METHOD => {
                let params: DocumentDiagnosticParams = serde_json::from_value(req.params)?;
                debug!(
                    "Received document diagnostic request #{}: {:#?}",
                    req.id, params
                );
                let uri = clean_url(&params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        match self.recolt_diagnostic(&uri, &cached_file) {
                            Ok(mut diagnostics) => {
                                let main_diagnostic = match diagnostics.remove(&uri) {
                                    Some(diag) => diag,
                                    None => vec![],
                                };
                                self.connection.send_response::<DocumentDiagnosticRequest>(
                                    req.id.clone(),
                                    DocumentDiagnosticReportResult::Report(
                                        DocumentDiagnosticReport::Full(
                                            RelatedFullDocumentDiagnosticReport {
                                                related_documents: Some(
                                                    diagnostics
                                                        .into_iter()
                                                        .map(|diagnostic| {
                                                            (
                                                                diagnostic.0,
                                                                DocumentDiagnosticReportKind::Full(
                                                                    FullDocumentDiagnosticReport {
                                                                        result_id: Some(
                                                                            req.id.to_string(),
                                                                        ),
                                                                        items: diagnostic.1,
                                                                    },
                                                                ),
                                                            )
                                                        })
                                                        .collect(),
                                                ),
                                                full_document_diagnostic_report:
                                                    FullDocumentDiagnosticReport {
                                                        result_id: Some(req.id.to_string()),
                                                        items: main_diagnostic,
                                                    },
                                            },
                                        ),
                                    ),
                                )
                            }
                            // Send empty report.
                            Err(error) => self.connection.send_response_error(
                                req.id.clone(),
                                lsp_server::ErrorCode::InternalError,
                                error.to_string(),
                            ),
                        };
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                };
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = serde_json::from_value(req.params)?;
                debug!("Received gotoDefinition request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let position = params.text_document_position_params.position;
                        match self.recolt_goto(&uri, Rc::clone(&cached_file), position) {
                            Ok(value) => self
                                .connection
                                .send_response::<GotoDefinition>(req.id.clone(), value),
                            Err(err) => self.connection.send_response_error(
                                req.id.clone(),
                                ErrorCode::InvalidParams,
                                format!("Failed to recolt signature : {:#?}", err),
                            ),
                        }
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                };
            }
            Completion::METHOD => {
                let params: CompletionParams = serde_json::from_value(req.params)?;
                debug!("Received completion request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document_position.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        match self.recolt_completion(
                            &uri,
                            Rc::clone(&cached_file),
                            params.text_document_position.position,
                            match &params.context {
                                Some(context) => context.trigger_character.clone(),
                                None => None,
                            },
                        ) {
                            Ok(value) => self.connection.send_response::<Completion>(
                                req.id.clone(),
                                Some(CompletionResponse::Array(value)),
                            ),
                            Err(error) => self.connection.send_response_error(
                                req.id.clone(),
                                lsp_server::ErrorCode::InternalError,
                                error.to_string(),
                            ),
                        }
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                };
            }
            SignatureHelpRequest::METHOD => {
                let params: SignatureHelpParams = serde_json::from_value(req.params)?;
                debug!("Received completion request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        match self.recolt_signature(
                            &uri,
                            Rc::clone(&cached_file),
                            params.text_document_position_params.position,
                        ) {
                            Ok(value) => self
                                .connection
                                .send_response::<SignatureHelpRequest>(req.id.clone(), value),
                            Err(err) => self.connection.send_response_error(
                                req.id.clone(),
                                ErrorCode::InvalidParams,
                                format!("Failed to recolt signature : {:#?}", err),
                            ),
                        }
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                };
            }
            HoverRequest::METHOD => {
                let params: HoverParams = serde_json::from_value(req.params)?;
                debug!("Received hover request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let position = params.text_document_position_params.position;
                        match self.recolt_hover(&uri, Rc::clone(&cached_file), position) {
                            Ok(value) => self
                                .connection
                                .send_response::<HoverRequest>(req.id.clone(), value),
                            Err(err) => self.connection.send_response_error(
                                req.id.clone(),
                                ErrorCode::InvalidParams,
                                format!("Failed to recolt signature : {:#?}", err),
                            ),
                        }
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            // Provider not enabled as vscode already does this nicely with grammar files
            FoldingRangeRequest::METHOD => {
                let params: FoldingRangeParams = serde_json::from_value(req.params)?;
                debug!("Received folding range request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let folding_ranges = RefCell::borrow(&cached_file)
                            .preprocessor_cache
                            .regions
                            .iter()
                            .map(|region| FoldingRange {
                                start_line: region.range.start.line,
                                start_character: Some(region.range.start.pos),
                                end_line: region.range.end.line,
                                end_character: Some(region.range.end.pos),
                                kind: Some(FoldingRangeKind::Region),
                                collapsed_text: None,
                            })
                            .collect();
                        // TODO: should add scopes aswell to request
                        self.connection.send_response::<FoldingRangeRequest>(
                            req.id.clone(),
                            Some(folding_ranges),
                        );
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            WorkspaceSymbolRequest::METHOD => {
                let params: WorkspaceSymbolParams = serde_json::from_value(req.params)?;
                debug!(
                    "Received workspace symbol request #{}: {:#?}",
                    req.id, params
                );
                let _ = params.query; // Should we filter ?
                                      // TODO: add dependencies as well, should be iterating on all file of workspace instead,
                                      // but might be very costly on big codebase (implement light queries ? only query global func & struct)
                                      // Will need to cache symbols. As they are global to workspace, might be updated every
                                      // change of files, & stored somewhere as watched deps ?
                let symbols = self
                    .watched_files
                    .files
                    .iter()
                    .map(|(uri, cached_file)| {
                        let shading_language = RefCell::borrow(&cached_file).shading_language;
                        RefCell::borrow(&cached_file)
                            .symbol_cache
                            .iter()
                            .filter(|(_, ty)| {
                                // For workspace, only publish function & types
                                *ty == ShaderSymbolType::Functions || *ty == ShaderSymbolType::Types
                            })
                            .map(|(symbols, ty)| {
                                symbols
                                    .iter()
                                    .filter(|symbol| {
                                        symbol.range.is_some()
                                            && (symbol.scope_stack.is_none()
                                                || symbol.scope_stack.as_ref().unwrap().is_empty())
                                    })
                                    .map(|symbol| {
                                        #[allow(deprecated)]
                                        // https://github.com/rust-lang/rust/issues/102777
                                        SymbolInformation {
                                            name: symbol.label.clone(),
                                            kind: match ty {
                                                ShaderSymbolType::Types => {
                                                    SymbolKind::TYPE_PARAMETER
                                                }
                                                ShaderSymbolType::Functions => SymbolKind::FUNCTION,
                                                _ => unreachable!("Should be filtered out"),
                                            },
                                            tags: None,
                                            deprecated: None,
                                            location: Location::new(
                                                uri.clone(),
                                                shader_range_to_lsp_range(
                                                    &symbol
                                                        .range
                                                        .as_ref()
                                                        .expect("Should be filtered out"),
                                                ),
                                            ),
                                            container_name: Some(shading_language.to_string()),
                                        }
                                    })
                                    .collect()
                            })
                            .collect::<Vec<Vec<SymbolInformation>>>()
                            .concat()
                    })
                    .collect::<Vec<Vec<SymbolInformation>>>()
                    .concat();

                self.connection.send_response::<DocumentSymbolRequest>(
                    req.id.clone(),
                    Some(DocumentSymbolResponse::Flat(symbols)),
                );
            }
            DocumentSymbolRequest::METHOD => {
                let params: DocumentSymbolParams = serde_json::from_value(req.params)?;
                debug!(
                    "Received document symbol request #{}: {:#?}",
                    req.id, params
                );
                let uri = clean_url(&params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let symbols = RefCell::borrow(&cached_file)
                            .symbol_cache
                            .iter()
                            .map(|(symbols, ty)| {
                                symbols
                                    .iter()
                                    .filter(|symbol| {
                                        // Dont publish keywords.
                                        ty != ShaderSymbolType::Keyword && symbol.range.is_some()
                                    })
                                    .map(|symbol| {
                                        #[allow(deprecated)]
                                        // https://github.com/rust-lang/rust/issues/102777
                                        SymbolInformation {
                                            name: symbol.label.clone(),
                                            kind: match ty {
                                                ShaderSymbolType::Types => {
                                                    SymbolKind::TYPE_PARAMETER
                                                }
                                                ShaderSymbolType::Constants => SymbolKind::CONSTANT,
                                                ShaderSymbolType::Variables => SymbolKind::VARIABLE,
                                                ShaderSymbolType::Functions => SymbolKind::FUNCTION,
                                                ShaderSymbolType::Keyword => {
                                                    unreachable!("Should be filtered out")
                                                }
                                            },
                                            tags: None,
                                            deprecated: None,
                                            location: Location::new(
                                                uri.clone(),
                                                shader_range_to_lsp_range(
                                                    &symbol
                                                        .range
                                                        .as_ref()
                                                        .expect("Should be filtered out"),
                                                ),
                                            ),
                                            container_name: None,
                                        }
                                    })
                                    .collect()
                            })
                            .collect::<Vec<Vec<SymbolInformation>>>()
                            .concat();

                        self.connection.send_response::<DocumentSymbolRequest>(
                            req.id.clone(),
                            Some(DocumentSymbolResponse::Flat(symbols)),
                        );
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            // Debug request
            DumpAstRequest::METHOD => {
                let params: DumpAstParams = serde_json::from_value(req.params)?;
                debug!("Received dump ast request #{}: {:#?}", req.id, params);
                let uri = clean_url(&params.text_document.uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let ast = RefCell::borrow(&cached_file).symbol_tree.dump_ast();
                        self.connection
                            .send_response::<DumpAstRequest>(req.id.clone(), Some(ast));
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            _ => warn!("Received unhandled request: {:#?}", req),
        }
        Ok(())
    }
    fn on_response(&mut self, response: lsp_server::Response) -> Result<(), serde_json::Error> {
        match self.connection.remove_callback(&response.id) {
            Some(callback) => match response.result {
                Some(result) => callback(self, result),
                None => callback(self, serde_json::from_str("{}").unwrap()),
            },
            None => warn!("Received unhandled response: {:#?}", response),
        }
        Ok(())
    }
    fn on_notification(
        &mut self,
        notification: lsp_server::Notification,
    ) -> Result<(), serde_json::Error> {
        debug!("Received notification: {}", notification.method);
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);

                // Skip non file uri.
                if uri.scheme() != "file" {
                    self.connection.send_notification_error(format!(
                        "Trying to watch file with unsupported scheme : {}",
                        uri.scheme()
                    ));
                    return Ok(());
                }
                self.request_variants(&uri);
                match ShadingLanguage::from_str(params.text_document.language_id.as_str()) {
                    Ok(shading_language) => {
                        let language_data = self.language_data.get_mut(&shading_language).unwrap();
                        match self.watched_files.watch_file(
                            &uri,
                            shading_language.clone(),
                            &params.text_document.text,
                            language_data.symbol_provider.as_mut(),
                            &self.config,
                        ) {
                            Ok(cached_file) => {
                                // Should compute following after variant received.
                                // + it seems variant are coming too early on client and too late here...
                                self.publish_diagnostic(
                                    &uri,
                                    &cached_file,
                                    Some(params.text_document.version),
                                );
                            }
                            Err(error) => self.connection.send_notification_error(format!(
                                "Failed to watch file {}: {}",
                                uri.to_string(),
                                error.to_string()
                            )),
                        }
                    }
                    Err(_) => self.connection.send_notification_error(format!(
                        "Failed to parse language id : {}",
                        params.text_document.language_id
                    )),
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                debug!("got did save text document: {:#?}", uri);
                // File content is updated through DidChangeTextDocument.
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        assert!(
                            params.text.is_none()
                                || (params.text.is_some()
                                    && RefCell::borrow(&cached_file).symbol_tree.content
                                        == *params.text.as_ref().unwrap())
                        );
                        let mut cached_file_borrowed = RefCell::borrow_mut(&cached_file);
                        let language_data = self
                            .language_data
                            .get_mut(&cached_file_borrowed.shading_language)
                            .unwrap();
                        match cached_file_borrowed.update(
                            &uri,
                            language_data.symbol_provider.as_mut(),
                            &self.config,
                            None,
                            None,
                        ) {
                            Ok(_) => {}
                            Err(err) => self.connection.send_notification_error(format!("{}", err)),
                        };
                        self.publish_diagnostic(&uri, &cached_file, None);
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                debug!("got did close text document: {:#?}", uri);
                match self.watched_files.remove_file(&uri) {
                    Ok(was_removed) => {
                        if was_removed {
                            self.clear_diagnostic(&self.connection, &uri);
                        }
                    }
                    Err(err) => self.connection.send_notification_error(format!("{}", err)),
                }
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                debug!("got did change text document: {:#?}", uri);
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        let mut cached_file_borrowed = RefCell::borrow_mut(&cached_file);
                        let language_data = self
                            .language_data
                            .get_mut(&cached_file_borrowed.shading_language)
                            .unwrap();
                        for content in &params.content_changes {
                            match cached_file_borrowed.update(
                                &uri,
                                language_data.symbol_provider.as_mut(),
                                &self.config,
                                content.range,
                                Some(&content.text),
                            ) {
                                Ok(_) => {}
                                Err(err) => {
                                    self.connection.send_notification_error(format!("{}", err))
                                }
                            };
                        }
                        self.publish_diagnostic(
                            &uri,
                            &cached_file,
                            Some(params.text_document.version),
                        );
                    }
                    None => self.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }
            }
            DidChangeConfiguration::METHOD => {
                let params: DidChangeConfigurationParams =
                    serde_json::from_value(notification.params)?;
                debug!("Received did change configuration: {:#?}", params);
                // Here config received is empty. we need to request it to user.
                //let config : ServerConfig = serde_json::from_value(params.settings)?;
                self.request_configuration();
            }
            DidChangeShaderVariant::METHOD => {
                let params: DidChangeShaderVariantParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                debug!("Received did change shader variant: {:#?}", params);
                // Check if variant is watched.
                // Should request from client aswell when opening files.
                // Client should only send update if watched file.
                match self.watched_files.get(&uri) {
                    Some(cached_file) => {
                        RefCell::borrow_mut(&cached_file).shader_variant =
                            params.shader_variant.clone();
                        // TODO: relaunch diag for file (& symbols for deps aswell)

                        // If file has multiple variants, get the first one & ignore others entry points.
                        // OR better, from client, only send 0 or 1 variant per file.
                        // Still need to handle shared deps (first arrived, first served, or some hashmap caching both depending on entry point...)
                        // 1. Preprocess file, get deps & regions.
                        // 2. For each deps, preprocess with previous context (add macros).
                        // 3. Recurse until all deps reached.
                        // 4. Compute symbols.
                    }
                    None => {
                        // TODO: store it still somewhere else.
                    }
                }
            }
            _ => info!("Received unhandled notification: {:#?}", notification),
        }
        Ok(())
    }
    fn request_configuration(&mut self) {
        let config = ConfigurationParams {
            items: vec![lsp_types::ConfigurationItem {
                scope_uri: None,
                section: Some("shader-validator".to_owned()),
            }],
        };
        self.connection.send_request::<WorkspaceConfiguration>(
            config,
            |server: &mut ServerLanguage, value: Value| {
                // Sent 1 item, received 1 in an array
                let mut parsed_config: Vec<Option<ServerConfig>> =
                    serde_json::from_value(value).expect("Failed to parse received config");
                let config = parsed_config.remove(0).unwrap_or_default();
                info!("Updating server config: {:#?}", config);
                server.config = config.clone();
                // Republish all diagnostics
                let mut file_to_republish = Vec::new();
                for (url, cached_file) in &server.watched_files.files {
                    // Clear diags
                    server.clear_diagnostic(&server.connection, &url);
                    let mut cached_file_borrowed = RefCell::borrow_mut(&cached_file);
                    let language_data = server
                        .language_data
                        .get_mut(&cached_file_borrowed.shading_language)
                        .unwrap();
                    // Update symbols & republish diags.
                    match cached_file_borrowed.update(
                        &url,
                        language_data.symbol_provider.as_mut(),
                        &server.config,
                        None,
                        None,
                    ) {
                        Ok(_) => file_to_republish.push((url.clone(), Rc::clone(&cached_file))),
                        Err(err) => server
                            .connection
                            .send_notification_error(format!("{}", err)),
                    };
                }
                // Republish all diagnostics with new settings.
                for (url, cached_file) in &file_to_republish {
                    server.publish_diagnostic(url, cached_file, None);
                }
            },
        );
    }
}

pub fn run() {
    let mut server = ServerLanguage::new();

    match server.initialize() {
        Ok(_) => info!("Server initialization successfull"),
        Err(value) => error!("Failed initalization: {:#?}", value),
    }

    match server.run() {
        Ok(_) => info!("Client disconnected"),
        Err(value) => error!("Client disconnected: {:#?}", value),
    }

    match server.connection.join() {
        Ok(_) => info!("Server shutting down gracefully"),
        Err(value) => error!("Server failed to join threads: {:#?}", value),
    }
}
