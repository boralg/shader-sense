use std::{collections::HashMap, path::PathBuf};

use log::info;
use lsp_types::{notification::Notification, request::Request, TextDocumentIdentifier, Url};
use serde::{Deserialize, Serialize};
use shader_sense::{shader::ShaderStage, shader_error::ShaderError};

use crate::server::ServerLanguage;

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderVariant {
    pub url: Url,
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
    pub shader_variant: Option<ShaderVariant>,
}

impl Notification for DidChangeShaderVariant {
    type Params = DidChangeShaderVariantParams;
    const METHOD: &'static str = "textDocument/didChangeShaderVariant";
}

#[derive(Debug)]
#[allow(dead_code)]
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
    pub fn update_variant(
        &mut self,
        new_variant: Option<ShaderVariant>,
    ) -> Result<Vec<Url>, ShaderError> {
        // Store it in cache.
        let old_variant = self.watched_files.variant.take();
        if old_variant != new_variant {
            info!("Detected changes, updating variant");
            // Set the new variant.
            self.watched_files.variant = new_variant;

            // Get old relying files before update.
            let mut old_relying_files = if let Some(old_variant) = &old_variant {
                self.watched_files.get_relying_files(&old_variant.url)
            } else {
                vec![]
            };

            // Cache new variant
            let mut all_updated_files = match &self.watched_files.variant {
                Some(new_variant) => match self.watched_files.get_file(&new_variant.url) {
                    Some(new_variant_file) => {
                        if new_variant_file.is_main_file() {
                            let new_variant_url = new_variant.url.clone();
                            let shading_language = new_variant_file.shading_language;
                            info!("Updating new variant {}", new_variant.url);
                            let language_data =
                                self.language_data.get_mut(&shading_language).unwrap();
                            let mut updated_files = self.watched_files.cache_file_data(
                                &new_variant_url,
                                language_data.validator.as_mut(),
                                &mut language_data.language,
                                &language_data.symbol_provider,
                                &self.config,
                                None,
                            )?;
                            updated_files.push(new_variant_url);
                            updated_files
                        } else {
                            vec![] // TODO: Not a main file. ignore for now.
                        }
                    }
                    None => vec![], // Not a watched file. ignore for now.
                },
                None => vec![], // No new variant to cache.
            };

            // Get new relying files after update.
            let new_relying_files = if let Some(new_variant) = &self.watched_files.variant {
                self.watched_files.get_relying_files(&new_variant.url)
            } else {
                vec![]
            };

            // Check if we need to update old variant or its already done.
            let mut files_to_update = match old_variant {
                Some(old_variant) => {
                    if new_relying_files
                        .iter()
                        .find(|url| **url == old_variant.url)
                        .is_none()
                        && if let Some(new_variant) = &self.watched_files.variant {
                            old_variant.url != new_variant.url
                        } else {
                            true
                        }
                    {
                        vec![old_variant.url.clone()] // Not already updated.
                    } else {
                        vec![] // Already updated.
                    }
                }
                None => vec![],
            };

            // Keep only relying file that are not in the other one.
            old_relying_files.retain(|old_url| {
                new_relying_files
                    .iter()
                    .find(|new_url| *new_url == old_url)
                    .is_none()
            });
            files_to_update.extend(old_relying_files);

            // Update all of them
            for file_to_update in files_to_update {
                match self.watched_files.get_file(&file_to_update) {
                    Some(cached_file) => {
                        if cached_file.is_main_file() {
                            let shading_language = cached_file.shading_language;
                            info!("Updating old relying file {}", file_to_update);
                            let language_data =
                                self.language_data.get_mut(&shading_language).unwrap();
                            let updated_files = self.watched_files.cache_file_data(
                                &file_to_update,
                                language_data.validator.as_mut(),
                                &mut language_data.language,
                                &language_data.symbol_provider,
                                &self.config,
                                None,
                            )?;
                            all_updated_files.push(file_to_update);
                            all_updated_files.extend(updated_files)
                        } // TODO: Not a main file. ignore for now.
                    }
                    None => {} // Not a watched file. ignore for now.
                };
            }
            Ok(all_updated_files)
        } else {
            info!("Variant unchanged.");
            Ok(vec![]) // Nothing changed
        }
    }
}
