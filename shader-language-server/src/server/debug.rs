use lsp_types::{request::Request, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum DumpAstRequest {}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpAstParams {
    #[serde(flatten)]
    pub text_document: TextDocumentIdentifier,
}

impl Request for DumpAstRequest {
    type Params = DumpAstParams;
    type Result = Option<String>;
    const METHOD: &'static str = "debug/dumpAst";
}

#[derive(Debug)]
pub enum DumpDependencyRequest {}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpDependencyParams {
    #[serde(flatten)]
    pub text_document: TextDocumentIdentifier,
}

impl Request for DumpDependencyRequest {
    type Params = DumpDependencyParams;
    type Result = Option<String>;
    const METHOD: &'static str = "debug/dumpDependency";
}
