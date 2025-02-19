use std::{collections::HashMap, path::PathBuf};

use log::debug;
use lsp_types::{notification::Notification, request::Request, TextDocumentIdentifier, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shader_sense::shader::ShaderStage;

use super::ServerLanguage;

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
#[allow(dead_code)]
pub enum DidChangeShaderVariant {}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeShaderVariantParams {
    pub text_document: TextDocumentIdentifier,
    pub shader_variant: Option<ShaderVariant>,
}

impl Notification for DidChangeShaderVariant {
    type Params = DidChangeShaderVariantParams;
    const METHOD: &'static str = "textDocument/didChangeShaderVariant";
}

#[derive(Debug)]
pub enum ShaderVariantRequest {}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderVariantParams {
    #[serde(flatten)]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderVariantResponse {
    shader_variant: Option<ShaderVariant>,
}

impl Request for ShaderVariantRequest {
    type Params = ShaderVariantParams;
    type Result = ShaderVariantResponse;
    const METHOD: &'static str = "textDocument/shaderVariant";
}

impl ServerLanguage {
    pub fn request_variants(&mut self, uri: &Url) {
        self.connection.send_request::<ShaderVariantRequest>(
            ShaderVariantParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            },
            |_server: &mut ServerLanguage, value: Value| {
                let params: ShaderVariantResponse = serde_json::from_value(value).unwrap();
                // This seems to be received after textDocument notification, this might be an issue...
                debug!("Received variant {:?}", params);

                /*match server.watched_files.get(&uri) {
                    Some(cached_file) => {
                    }
                    None => server.connection.send_notification_error(format!(
                        "Trying to visit file that is not watched : {}",
                        uri
                    )),
                }*/
            },
        );
    }
}
