use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use log::info;
use lsp_types::{notification::Notification, request::Request, TextDocumentIdentifier, Url};
use serde::{Deserialize, Serialize};
use shader_sense::{
    shader::{ShaderStage, ShadingLanguage},
    shader_error::ShaderError,
};

use crate::server::{clean_url, ServerLanguage};

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderVariant {
    pub url: Url,
    pub language: ShadingLanguage,
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
    pub fn parse_variant_params(
        &self,
        value: serde_json::Value,
    ) -> Result<Option<ShaderVariant>, serde_json::Error> {
        // Keep compatibility with old client.
        #[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct OldShaderVariant {
            pub entry_point: String,
            pub stage: Option<ShaderStage>,
            pub defines: HashMap<String, String>,
            pub includes: Vec<PathBuf>,
        }
        #[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct OldDidChangeShaderVariantParams {
            pub text_document: TextDocumentIdentifier,
            pub shader_variant: Option<OldShaderVariant>,
        }
        match serde_json::from_value::<DidChangeShaderVariantParams>(value.clone()) {
            Ok(variant) => Ok(variant.shader_variant.map(|mut v| {
                v.url = clean_url(&v.url);
                v
            })),
            Err(new_err) => {
                match serde_json::from_value::<OldDidChangeShaderVariantParams>(value) {
                    Ok(new_variant) => {
                        log::warn!("Client sending old variant version. Should use the new variant version instead. Only the latest enabled variant sent to server will be used.");
                        match &self.watched_files.variant {
                            Some(current_variant) => {
                                if current_variant.url == new_variant.text_document.uri
                                    && new_variant.shader_variant.is_none()
                                {
                                    // If same variant and we removed it, mark it gone.
                                    Ok(None)
                                } else if new_variant.shader_variant.is_some() {
                                    // If we pass a variant with new URL, update (will pick the latest enabled.)
                                    let url = clean_url(&new_variant.text_document.uri);
                                    Ok(new_variant.shader_variant.map(|v| ShaderVariant {
                                        language: match self.watched_files.files.get(&url) {
                                            Some(file) => file.shading_language,
                                            None => ShadingLanguage::Hlsl, // Default to HLSL as we have no way to guess it.
                                        },
                                        url: url,
                                        entry_point: v.entry_point,
                                        stage: v.stage,
                                        defines: v.defines,
                                        includes: v.includes,
                                    }))
                                } else {
                                    Ok(None)
                                }
                            }
                            // If no variant, set new one.
                            None => {
                                let url = clean_url(&new_variant.text_document.uri);
                                Ok(new_variant.shader_variant.map(|v| ShaderVariant {
                                    language: match self.watched_files.files.get(&url) {
                                        Some(file) => file.shading_language,
                                        None => ShadingLanguage::Hlsl, // Default to HLSL as we have no way to guess it.
                                    },
                                    url: url,
                                    entry_point: v.entry_point,
                                    stage: v.stage,
                                    defines: v.defines,
                                    includes: v.includes,
                                }))
                            }
                        }
                    }
                    Err(_err) => Err(new_err),
                }
            }
        }
    }
    pub fn update_variant(
        &mut self,
        new_variant: Option<ShaderVariant>,
    ) -> Result<(HashSet<Url>, HashSet<Url>), ShaderError> {
        // Store it in cache.
        let old_variant = self.watched_files.variant.take();
        if old_variant != new_variant {
            info!(
                "Detected changes, updating variant {:#?} to {:#?}",
                old_variant, new_variant
            );
            let is_same_file = if let Some(old_variant) = &old_variant {
                if let Some(new_variant) = &new_variant {
                    new_variant.url == old_variant.url
                } else {
                    false
                }
            } else {
                false
            };
            // Set the new variant.
            self.watched_files.variant = new_variant;

            // Get old relying files before update.
            let mut old_relying_files = if let Some(old_variant) = &old_variant {
                let old_relying_files = self.watched_files.get_relying_main_files(&old_variant.url);
                if !is_same_file {
                    let removed_urls = self.watched_files.remove_variant_file(&old_variant.url)?;
                    for removed_url in removed_urls {
                        self.clear_diagnostic(&removed_url);
                    }
                }
                old_relying_files
            } else {
                HashSet::new() // No old variant.
            };

            // Cache new variant
            let mut all_removed_files = match &self.watched_files.variant {
                Some(new_variant) => {
                    let lang = new_variant.language;
                    let language_data = self.language_data.get_mut(&lang).unwrap();
                    let new_variant_url = new_variant.url.clone();
                    if is_same_file {
                        self.watched_files.cache_file_data(
                            &new_variant_url,
                            language_data.validator.as_mut(),
                            &mut language_data.language,
                            &language_data.symbol_provider,
                            &self.config,
                            HashSet::new(),
                        )?
                    } else {
                        self.watched_files.watch_variant_file(
                            &new_variant_url,
                            lang,
                            &mut language_data.language,
                        )?;
                        self.watched_files.cache_file_data(
                            &new_variant_url,
                            language_data.validator.as_mut(),
                            &mut language_data.language,
                            &language_data.symbol_provider,
                            &self.config,
                            HashSet::new(),
                        )?
                    }
                }
                None => HashSet::new(), // No new variant to cache.
            };

            // Get new relying files after update.
            let new_relying_files = if let Some(new_variant) = &self.watched_files.variant {
                if self.watched_files.files.get(&new_variant.url).is_some() {
                    self.watched_files.get_relying_main_files(&new_variant.url)
                } else {
                    HashSet::new() // File not watched.
                }
            } else {
                HashSet::new() // No new variant.
            };

            // Check if we need to update variant or its already done.
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
                        HashSet::from([old_variant.url.clone()]) // Not already updated.
                    } else {
                        HashSet::new() // Already updated.
                    }
                }
                None => match &self.watched_files.variant {
                    Some(new_variant) => HashSet::from([new_variant.url.clone()]),
                    None => HashSet::new(),
                },
            };

            // Keep only relying file that are not in the other one.
            old_relying_files.retain(|old_url| {
                new_relying_files
                    .iter()
                    .find(|new_url| *new_url == old_url)
                    .is_none()
            });
            files_to_update.extend(old_relying_files);

            if let Some(new_variant) = &self.watched_files.variant {
                files_to_update.insert(new_variant.url.clone());
            }

            // Update all of them
            for file_to_update in &files_to_update {
                match self.watched_files.get_file(&file_to_update) {
                    Some(cached_file) => {
                        if cached_file.is_main_file() {
                            let shading_language = cached_file.shading_language;
                            info!("Updating old relying file {}", file_to_update);
                            let language_data =
                                self.language_data.get_mut(&shading_language).unwrap();
                            let removed_files = self.watched_files.cache_file_data(
                                &file_to_update,
                                language_data.validator.as_mut(),
                                &mut language_data.language,
                                &language_data.symbol_provider,
                                &self.config,
                                HashSet::from([file_to_update.to_file_path().unwrap()]),
                            )?;
                            all_removed_files.extend(removed_files)
                        }
                    }
                    None => {} // Not a watched file. ignore for now.
                };
            }
            Ok((all_removed_files, files_to_update))
        } else {
            info!("Variant unchanged.");
            Ok((HashSet::new(), HashSet::new())) // Nothing changed
        }
    }
}
