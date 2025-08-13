use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;

mod common;
mod completion;
mod debug;
mod diagnostic;
mod document_symbol;
mod folding_range;
mod formatting;
mod goto;
mod hover;
mod inlay_hint;
mod semantic_token;
pub mod shader_variant; // pub for test.
mod signature;
mod workspace_symbol;

mod profile;
pub mod server_config; // pub for test.
mod server_connection;
mod server_file_cache;
mod server_language_data;

use debug::{DumpAstParams, DumpAstRequest, DumpDependencyParams, DumpDependencyRequest};
use log::{debug, error, info, warn};
use lsp_types::notification::{
    DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification,
};
use lsp_types::request::{
    Completion, DocumentDiagnosticRequest, DocumentSymbolRequest, FoldingRangeRequest, Formatting,
    GotoDefinition, HoverRequest, InlayHintRequest, RangeFormatting, Request,
    SemanticTokensFullRequest, SignatureHelpRequest, WorkspaceSymbolRequest,
};
use lsp_types::{
    CompletionOptionsCompletionItem, CompletionParams, CompletionResponse,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentDiagnosticParams,
    DocumentFormattingParams, DocumentRangeFormattingParams, DocumentSymbolOptions,
    DocumentSymbolParams, DocumentSymbolResponse, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, HoverParams, HoverProviderCapability,
    InlayHintParams, OneOf, SemanticTokenType, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensServerCapabilities,
    ServerCapabilities, SignatureHelpOptions, SignatureHelpParams, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions, WorkspaceSymbolOptions, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
};
use shader_sense::shader::ShadingLanguage;

use lsp_server::{ErrorCode, Message};

use server_config::ServerConfig;
use server_connection::ServerConnection;
use server_file_cache::ServerLanguageFileCache;
use server_language_data::ServerLanguageData;
use shader_sense::shader_error::ShaderError;
use shader_variant::DidChangeShaderVariant;

use crate::profile_scope;
use crate::server::common::lsp_range_to_shader_range;
use crate::server::server_file_cache::ServerFileCache;

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

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Transport {
    #[default]
    Stdio,
    Tcp,    // TODO: supported by lsp_server
    Memory, // TODO: supported by lsp_server
}

impl ServerLanguage {
    pub fn new(config: ServerConfig, transport: Transport) -> Self {
        assert!(
            transport == Transport::Stdio,
            "Only stdio transport implemented for now"
        );
        // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
        info!(
            "Creating server with transport {:?} and config {:#?}",
            transport, config
        );
        Self {
            connection: ServerConnection::new(),
            watched_files: ServerLanguageFileCache::new(),
            config: config,
            language_data: HashMap::from([
                (ShadingLanguage::Glsl, ServerLanguageData::glsl()),
                (ShadingLanguage::Hlsl, ServerLanguageData::hlsl()),
                (ShadingLanguage::Wgsl, ServerLanguageData::wgsl()),
            ]),
        }
    }
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let is_clang_format_available = Self::is_clang_format_available();
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
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Right(DocumentSymbolOptions {
                label: Some("shader-validator".into()),
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
            inlay_hint_provider: Some(OneOf::Left(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                    legend: SemanticTokensLegend {
                        token_modifiers: vec![],
                        token_types: vec![SemanticTokenType::MACRO, SemanticTokenType::PARAMETER],
                    },
                    range: None,
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                }),
            ),
            document_formatting_provider: Some(OneOf::Left(is_clang_format_available)),
            document_range_formatting_provider: Some(OneOf::Left(is_clang_format_available)),
            ..Default::default()
        })?;
        let _client_initialization_params =
            self.connection.initialize(server_capabilities).unwrap();
        // TODO: Check features support from client params.
        debug!("Received client params");
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
                        let id = req.id.clone();
                        match self.on_request(req) {
                            Ok(_) => {}
                            Err(err) => self.connection.send_response_error(
                                id,
                                match &err {
                                    ShaderError::ValidationError(_)
                                    | ShaderError::ParseSymbolError(_)
                                    | ShaderError::IoErr(_)
                                    | ShaderError::InternalErr(_)
                                    | ShaderError::FileNotWatched(_) => ErrorCode::InternalError,
                                    ShaderError::InvalidParams(_) => ErrorCode::InvalidParams,
                                    ShaderError::SerializationError(_) => ErrorCode::InvalidParams,
                                    // Should have been caught before getting here.
                                    ShaderError::SymbolQueryError(_, _) | ShaderError::NoSymbol => {
                                        unreachable!()
                                    }
                                },
                                err.to_string(),
                            ),
                        }
                    }
                    Message::Response(resp) => match self.on_response(resp) {
                        Ok(_) => {}
                        Err(err) => self.connection.send_notification_error(err.to_string()),
                    },
                    Message::Notification(not) => match self.on_notification(not) {
                        Ok(_) => {}
                        Err(err) => self.connection.send_notification_error(err.to_string()),
                    },
                },
                Err(_) => {
                    // Recv error means disconnected.
                    return Ok(());
                }
            }
        }
    }
    fn debug(&self, dbg: &impl Debug) -> String {
        if self.config.is_verbose() {
            format!("{:#?}", dbg)
        } else {
            "{}".into()
        }
    }
    fn get_main_file(&self, uri: &Url) -> Result<&ServerFileCache, ShaderError> {
        let main_file = self
            .watched_files
            .get_file(&uri)
            .ok_or(ShaderError::FileNotWatched(uri.to_file_path().unwrap()))?;
        debug_assert!(main_file.is_main_file(), "File {} is not a main file.", uri);
        Ok(main_file)
    }
    fn on_request(&mut self, req: lsp_server::Request) -> Result<(), ShaderError> {
        match req.method.as_str() {
            DocumentDiagnosticRequest::METHOD => {
                let params: DocumentDiagnosticParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received document diagnostic request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let document_diagnostic = self.recolt_document_diagnostic(&uri)?;
                self.connection.send_response::<DocumentDiagnosticRequest>(
                    req.id.clone(),
                    document_diagnostic,
                );
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                profile_scope!(
                    "Received gotoDefinition request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let position = params.text_document_position_params.position;
                let value = self.recolt_goto(&uri, position)?;
                self.connection
                    .send_response::<GotoDefinition>(req.id.clone(), value);
            }
            Completion::METHOD => {
                let params: CompletionParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document_position.text_document.uri);
                profile_scope!(
                    "Received completion request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let value = self.recolt_completion(
                    &uri,
                    params.text_document_position.position,
                    match &params.context {
                        Some(context) => context.trigger_character.clone(),
                        None => None,
                    },
                )?;
                self.connection.send_response::<Completion>(
                    req.id.clone(),
                    Some(CompletionResponse::Array(value)),
                );
            }
            SignatureHelpRequest::METHOD => {
                let params: SignatureHelpParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                profile_scope!(
                    "Received completion request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let value =
                    self.recolt_signature(&uri, params.text_document_position_params.position)?;
                self.connection
                    .send_response::<SignatureHelpRequest>(req.id.clone(), value);
            }
            HoverRequest::METHOD => {
                let params: HoverParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document_position_params.text_document.uri);
                profile_scope!(
                    "Received hover request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let position = params.text_document_position_params.position;
                let value = self.recolt_hover(&uri, position)?;
                self.connection
                    .send_response::<HoverRequest>(req.id.clone(), value);
            }
            InlayHintRequest::METHOD => {
                let params: InlayHintParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received inlay hint request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let inlay_hints = self.recolt_inlay_hint(&uri, &params.range)?;
                self.connection
                    .send_response::<InlayHintRequest>(req.id.clone(), Some(inlay_hints));
            }
            FoldingRangeRequest::METHOD => {
                let params: FoldingRangeParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received folding range request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let folding_ranges = self.recolt_folding_range(&uri)?;
                self.connection
                    .send_response::<FoldingRangeRequest>(req.id.clone(), Some(folding_ranges));
            }
            WorkspaceSymbolRequest::METHOD => {
                let params: WorkspaceSymbolParams = serde_json::from_value(req.params)?;
                profile_scope!("Received workspace symbol request: {}", self.debug(&params));
                let _ = params.query; // TODO: Should we filter ?
                let symbols = self.recolt_workspace_symbol()?;
                self.connection.send_response::<WorkspaceSymbolRequest>(
                    req.id.clone(),
                    Some(WorkspaceSymbolResponse::Flat(symbols)),
                )
            }
            DocumentSymbolRequest::METHOD => {
                let params: DocumentSymbolParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received document symbol request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let symbols = self.recolt_document_symbol(&uri)?;
                self.connection.send_response::<DocumentSymbolRequest>(
                    req.id.clone(),
                    Some(DocumentSymbolResponse::Nested(symbols)),
                );
            }
            // Debug request
            DumpAstRequest::METHOD => {
                let params: DumpAstParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received dump ast request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let cached_file = self.get_main_file(&uri)?;
                let ast = RefCell::borrow(&cached_file.shader_module).dump_ast();
                self.connection
                    .send_response::<DumpAstRequest>(req.id.clone(), Some(ast));
            }
            DumpDependencyRequest::METHOD => {
                let params: DumpDependencyParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received dump dependency request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let cached_file = self.get_main_file(&uri)?;
                let deps_tree = cached_file
                    .data
                    .as_ref()
                    .unwrap()
                    .symbol_cache
                    .dump_dependency_tree(&uri.to_file_path().unwrap());
                self.connection
                    .send_response::<DumpDependencyRequest>(req.id.clone(), Some(deps_tree));
            }
            SemanticTokensFullRequest::METHOD => {
                let params: SemanticTokensParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received semantic token request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let semantic_tokens = self.recolt_semantic_tokens(&uri)?;
                self.connection.send_response::<SemanticTokensFullRequest>(
                    req.id.clone(),
                    Some(semantic_tokens),
                );
            }
            Formatting::METHOD => {
                let params: DocumentFormattingParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received formatting request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let formatting = self.recolt_formatting(&uri, None)?;
                self.connection
                    .send_response::<Formatting>(req.id.clone(), Some(formatting));
            }
            RangeFormatting::METHOD => {
                let params: DocumentRangeFormattingParams = serde_json::from_value(req.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received formatting range request for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let formatting = self.recolt_formatting(
                    &uri,
                    Some(lsp_range_to_shader_range(
                        &params.range,
                        &uri.to_file_path().unwrap(),
                    )),
                )?;
                self.connection
                    .send_response::<Formatting>(req.id.clone(), Some(formatting));
            }
            _ => warn!("Received unhandled request: {:#?}", req),
        }
        Ok(())
    }
    fn on_response(&mut self, response: lsp_server::Response) -> Result<(), ShaderError> {
        match self.connection.remove_callback(&response.id) {
            Some(callback) => match response.result {
                Some(result) => callback(self, result),
                None => Err(ShaderError::InternalErr(format!(
                    "Received response without result: {:#?}",
                    response
                ))),
            },
            None => Err(ShaderError::InternalErr(format!(
                "Received unhandled response: {:#?}",
                response
            ))),
        }
    }
    fn on_notification(
        &mut self,
        notification: lsp_server::Notification,
    ) -> Result<(), ShaderError> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received did open text document notification for {}:{}",
                    uri,
                    self.debug(&params)
                );

                // Skip non file uri.
                if uri.scheme() != "file" {
                    // Invalid Params
                    return Err(ShaderError::InvalidParams(format!(
                        "Trying to watch file with unsupported scheme : {}",
                        uri.scheme()
                    )));
                }
                let shading_language = ShadingLanguage::from_str(&params.text_document.language_id)
                    .map_err(|_| {
                        ShaderError::InvalidParams(format!(
                            "Trying to watch file with unsupported langID : {}",
                            params.text_document.language_id
                        ))
                    })?;
                let language_data = self.language_data.get_mut(&shading_language).unwrap();
                let _ = self.watched_files.watch_main_file(
                    &uri,
                    shading_language.clone(),
                    &params.text_document.text,
                    &mut language_data.language,
                    &language_data.symbol_provider,
                    language_data.validator.as_mut(),
                    &self.config,
                )?;
                let url_to_republish = self
                    .watched_files
                    .get_relying_variant(&uri)
                    .unwrap_or(uri.clone());
                self.publish_diagnostic(&url_to_republish, None);
            }
            DidSaveTextDocument::METHOD => {
                let params: DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received did save text document notification for file {}:{}",
                    uri,
                    self.debug(&params)
                );
                // File content is updated through DidChangeTextDocument.
                let cached_file = self.get_main_file(&uri)?;

                assert!(
                    params.text.is_none()
                        || (params.text.is_some()
                            && RefCell::borrow(&cached_file.shader_module).content
                                == *params.text.as_ref().unwrap())
                );
                // Only update cache if content changed.
                if let Some(text) = params.text {
                    if text != RefCell::borrow(&cached_file.shader_module).content {
                        let shading_language = cached_file.shading_language;
                        let language_data = self.language_data.get_mut(&shading_language).unwrap();
                        let _ = self.watched_files.update_file(
                            &uri,
                            &mut language_data.language,
                            None,
                            None,
                        )?;
                        // Cache once all changes have been applied.
                        let removed_files = self.watched_files.cache_file_data(
                            &uri,
                            language_data.validator.as_mut(),
                            &mut language_data.language,
                            &language_data.symbol_provider,
                            &self.config,
                            Some(&uri.to_file_path().unwrap()), // Force update
                        )?;
                        for removed_file in removed_files {
                            self.clear_diagnostic(&removed_file);
                        }
                        let url_to_republish = self
                            .watched_files
                            .get_relying_variant(&uri)
                            .unwrap_or(uri.clone());
                        self.publish_diagnostic(&url_to_republish, None);
                    }
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received did close text document notification for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let removed_urls = self.watched_files.remove_main_file(&uri)?;
                for removed_url in removed_urls {
                    self.clear_diagnostic(&removed_url);
                }
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = clean_url(&params.text_document.uri);
                profile_scope!(
                    "Received did change text document notification for file {}: {}",
                    uri,
                    self.debug(&params)
                );
                let cached_file = self.get_main_file(&uri)?;
                let shading_language = cached_file.shading_language;
                let language_data = self.language_data.get_mut(&shading_language).unwrap();
                // Update all content before caching data.
                for content in &params.content_changes {
                    match self.watched_files.update_file(
                        &uri,
                        &mut language_data.language,
                        content.range,
                        Some(&content.text),
                    ) {
                        Ok(_) => {}
                        Err(err) => self.connection.send_notification_error(format!("{}", err)),
                    };
                }
                // Cache once all changes have been applied.
                let removed_files = self.watched_files.cache_file_data(
                    &uri,
                    language_data.validator.as_mut(),
                    &mut language_data.language,
                    &language_data.symbol_provider,
                    &self.config,
                    Some(&uri.to_file_path().unwrap()),
                )?;
                for removed_file in removed_files {
                    self.clear_diagnostic(&removed_file);
                }
                let url_to_republish = self
                    .watched_files
                    .get_relying_variant(&uri)
                    .unwrap_or(uri.clone());
                self.publish_diagnostic(&url_to_republish, Some(params.text_document.version));
            }
            DidChangeConfiguration::METHOD => {
                let params: DidChangeConfigurationParams =
                    serde_json::from_value(notification.params)?;
                profile_scope!(
                    "Received did change configuration notification: {}",
                    self.debug(&params)
                );
                // Here config received is empty. we need to request it to user.
                //let config : ServerConfig = serde_json::from_value(params.settings)?;
                self.request_configuration();
            }
            DidChangeShaderVariant::METHOD => {
                let new_variant = self.parse_variant_params(notification.params)?;
                profile_scope!(
                    "Received did change shader variant notification for file {}: {}",
                    new_variant
                        .as_ref()
                        .map(|v| v.url.to_string())
                        .unwrap_or("None".into()),
                    self.debug(&new_variant)
                );
                let (removed_files, updated_files) = self.update_variant(new_variant)?;
                for removed_file in removed_files {
                    self.clear_diagnostic(&removed_file);
                }
                for file in updated_files {
                    self.publish_diagnostic(&file, None);
                }
            }
            _ => warn!(
                "Received unhandled notification {}: {}",
                notification.method,
                self.debug(&notification)
            ),
        }
        Ok(())
    }
}

pub fn run(config: ServerConfig, transport: Transport) {
    let mut server = ServerLanguage::new(config, transport);

    match server.initialize() {
        Ok(_) => info!("Server initialization successfull"),
        Err(value) => error!("Failed initalization: {:#?}", value),
    }

    match server.run() {
        Ok(_) => {
            info!("Client disconnected");
            match server.connection.join() {
                Ok(_) => info!("Server shutting down gracefully"),
                Err(value) => error!("Server failed to join threads: {:#?}", value),
            }
        }
        Err(value) => error!("Client disconnected: {:#?}", value),
    }
}
