use std::{collections::HashMap, path::PathBuf};

use lsp_types::{notification::Notification, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};
use shader_sense::shader::ShaderStage;

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderVariant {
    pub entry_point: String,
    pub stage: Option<ShaderStage>,
    pub defines: HashMap<String, String>,
    pub includes: Vec<PathBuf>,
}

// Could split with add / delete / update
#[derive(Debug)]
pub enum DidChangeShaderVariant {}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeShaderVariantParams {
    pub text_document: TextDocumentIdentifier,
    pub shader_variants: Vec<ShaderVariant>
}

impl Notification for DidChangeShaderVariant {
    type Params = DidChangeShaderVariantParams;
    const METHOD: &'static str = "textDocument/didChangeShaderVariant";
}
