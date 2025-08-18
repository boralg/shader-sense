use lsp_server::RequestId;
use lsp_types::{
    request::{
        Completion, DocumentDiagnosticRequest, DocumentSymbolRequest, FoldingRangeRequest,
        Formatting, GotoDefinition, HoverRequest, InlayHintRequest, RangeFormatting, Request,
        SemanticTokensFullRequest, SignatureHelpRequest, WorkspaceSymbolRequest,
    },
    CompletionParams, DocumentDiagnosticParams, DocumentFormattingParams,
    DocumentRangeFormattingParams, DocumentSymbolParams, FoldingRangeParams, GotoDefinitionParams,
    HoverParams, InlayHintParams, SemanticTokensParams, SignatureHelpParams, Url,
    WorkspaceSymbolParams,
};
use shader_sense::shader::ShadingLanguage;

use crate::server::{
    clean_url,
    debug::{DumpAstParams, DumpAstRequest, DumpDependencyParams, DumpDependencyRequest},
};

pub struct AsyncRequest<R: Request> {
    pub req_id: RequestId,
    pub params: R::Params,
}
#[derive(PartialEq, Eq, Hash)]
pub struct AsyncCacheRequest {
    pub url: Url,
    pub lang: ShadingLanguage,
    pub dirty: bool,
    //version: u32,
}
pub enum AsyncMessage {
    None,
    // All notification and content changes are processed intantaneously.
    // Cache request are enqueued with version.
    UpdateCache(Vec<AsyncCacheRequest>),

    // All request available.
    DocumentSymbolRequest(AsyncRequest<DocumentSymbolRequest>),
    WorkspaceSymbolRequest(AsyncRequest<WorkspaceSymbolRequest>),
    RangeFormatting(AsyncRequest<RangeFormatting>),
    Formatting(AsyncRequest<Formatting>),
    SemanticTokensFullRequest(AsyncRequest<SemanticTokensFullRequest>),
    FoldingRangeRequest(AsyncRequest<FoldingRangeRequest>),
    InlayHintRequest(AsyncRequest<InlayHintRequest>),
    HoverRequest(AsyncRequest<HoverRequest>),
    SignatureHelpRequest(AsyncRequest<SignatureHelpRequest>),
    Completion(AsyncRequest<Completion>),
    GotoDefinition(AsyncRequest<GotoDefinition>),
    DocumentDiagnosticRequest(AsyncRequest<DocumentDiagnosticRequest>),
    // Debug
    DumpDependencyRequest(AsyncRequest<DumpDependencyRequest>),
    DumpAstRequest(AsyncRequest<DumpAstRequest>),
}

impl AsyncCacheRequest {
    pub fn new(url: Url, lang: ShadingLanguage, dirty: bool) -> Self {
        Self {
            url,
            lang,
            dirty,
            //version: 0,
        }
    }
}
impl AsyncMessage {
    pub fn is_update(&self) -> bool {
        matches!(self, Self::None | Self::UpdateCache(_))
    }
    pub fn get_request_id(&self) -> &RequestId {
        match self {
            AsyncMessage::DocumentSymbolRequest(async_request) => &async_request.req_id,
            AsyncMessage::WorkspaceSymbolRequest(async_request) => &async_request.req_id,
            AsyncMessage::RangeFormatting(async_request) => &async_request.req_id,
            AsyncMessage::Formatting(async_request) => &async_request.req_id,
            AsyncMessage::SemanticTokensFullRequest(async_request) => &async_request.req_id,
            AsyncMessage::FoldingRangeRequest(async_request) => &async_request.req_id,
            AsyncMessage::InlayHintRequest(async_request) => &async_request.req_id,
            AsyncMessage::HoverRequest(async_request) => &async_request.req_id,
            AsyncMessage::SignatureHelpRequest(async_request) => &async_request.req_id,
            AsyncMessage::Completion(async_request) => &async_request.req_id,
            AsyncMessage::GotoDefinition(async_request) => &async_request.req_id,
            AsyncMessage::DocumentDiagnosticRequest(async_request) => &async_request.req_id,
            AsyncMessage::DumpDependencyRequest(async_request) => &async_request.req_id,
            AsyncMessage::DumpAstRequest(async_request) => &async_request.req_id,
            // These variants do not have a RequestId
            AsyncMessage::None | AsyncMessage::UpdateCache(_) => {
                unreachable!("Should not be reached. Update AsyncMessage::is_update accordingly.");
            }
        }
    }
}

#[allow(private_bounds)] // Trait only used in this file.
impl<R: Request> AsyncRequest<R>
where
    R::Params: ParamsDeserialization,
{
    pub fn new(req_id: RequestId, mut params: R::Params) -> Self {
        params.clean();
        Self { req_id, params }
    }
}
trait ParamsDeserialization {
    fn clean(&mut self);
}
impl ParamsDeserialization for DocumentSymbolParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for WorkspaceSymbolParams {
    fn clean(&mut self) {}
}
impl ParamsDeserialization for DocumentRangeFormattingParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for DocumentFormattingParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for SemanticTokensParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for FoldingRangeParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for InlayHintParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for HoverParams {
    fn clean(&mut self) {
        self.text_document_position_params.text_document.uri =
            clean_url(&self.text_document_position_params.text_document.uri)
    }
}
impl ParamsDeserialization for SignatureHelpParams {
    fn clean(&mut self) {
        self.text_document_position_params.text_document.uri =
            clean_url(&self.text_document_position_params.text_document.uri)
    }
}
impl ParamsDeserialization for CompletionParams {
    fn clean(&mut self) {
        self.text_document_position.text_document.uri =
            clean_url(&self.text_document_position.text_document.uri)
    }
}
impl ParamsDeserialization for GotoDefinitionParams {
    fn clean(&mut self) {
        self.text_document_position_params.text_document.uri =
            clean_url(&self.text_document_position_params.text_document.uri)
    }
}
impl ParamsDeserialization for DocumentDiagnosticParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for DumpAstParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
impl ParamsDeserialization for DumpDependencyParams {
    fn clean(&mut self) {
        self.text_document.uri = clean_url(&self.text_document.uri)
    }
}
