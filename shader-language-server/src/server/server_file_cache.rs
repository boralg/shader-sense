use std::{cell::RefCell, collections::HashMap, path::Path, rc::Rc};

use crate::{
    profile_scope,
    server::{
        clean_url,
        common::{lsp_range_to_shader_range, read_string_lossy},
    },
};
use log::{debug, info, warn};
use lsp_types::Url;
use shader_sense::{
    shader::ShadingLanguage,
    shader_error::{ShaderDiagnostic, ShaderDiagnosticList, ShaderDiagnosticSeverity, ShaderError},
    symbols::{
        shader_language::ShaderLanguage,
        symbol_provider::SymbolProvider,
        symbol_tree::{ShaderModuleHandle, ShaderSymbols},
        symbols::{ShaderPreprocessorContext, ShaderRange, ShaderSymbolListRef},
    },
    validator::validator::Validator,
};

use super::{server_config::ServerConfig, shader_variant::ShaderVariant};

#[derive(Debug, Clone, Default)]
pub struct ServerFileCacheData {
    pub symbol_cache: ShaderSymbols, // Store symbols to avoid computing them at every change.
    pub diagnostic_cache: ShaderDiagnosticList, // Cached diagnostic
}

#[derive(Debug, Clone)]
pub struct ServerFileCache {
    pub shading_language: ShadingLanguage,
    pub shader_module: ShaderModuleHandle, // Store content on change as its not on disk.
    pub data: Option<ServerFileCacheData>, // Data for file opened and edited.
}

impl ServerFileCache {
    pub fn is_main_file(&self) -> bool {
        self.data.is_some()
    }
    pub fn get_data(&self) -> &ServerFileCacheData {
        assert!(
            self.is_main_file(),
            "Trying to get data from file {} which does not have cache.",
            RefCell::borrow(&self.shader_module).file_path.display()
        );
        self.data.as_ref().unwrap()
    }
}

pub struct ServerLanguageFileCache {
    pub files: HashMap<Url, ServerFileCache>,
    pub variants: HashMap<Url, ShaderVariant>,
}

impl ServerLanguageFileCache {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            variants: HashMap::new(),
        }
    }
    pub fn create_data(
        &mut self,
        uri: &Url,
        symbol_cache: ShaderSymbols,
        diagnostic_cache: ShaderDiagnosticList,
    ) {
        self.files.get_mut(uri).unwrap().data = Some(ServerFileCacheData {
            symbol_cache,
            diagnostic_cache,
        })
    }
    pub fn get_dependent_files(&self, dependent_url: &Url) -> Vec<Url> {
        let dependant_file_path = dependent_url.to_file_path().unwrap();
        self.files
            .iter()
            .filter(|(file_url, file)| {
                *file_url != dependent_url
                    && file.is_main_file()
                    && file
                        .get_data()
                        .symbol_cache
                        .has_dependency(&dependant_file_path)
            })
            .map(|(url, _file)| url.clone())
            .collect()
    }
    pub fn get_relying_files(&self, url: &Url) -> Vec<Url> {
        match self.files.get(url) {
            Some(file) => {
                let mut relying_on_files = Vec::new();
                file.get_data().symbol_cache.visit_includes(&mut |include| {
                    let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                    let is_relying_on = match self.files.get(&include_uri) {
                        Some(deps) => deps.is_main_file(),
                        None => false,
                    };
                    if is_relying_on {
                        relying_on_files.push(include_uri);
                    }
                });
                relying_on_files
            }
            None => Vec::new(),
        }
    }
    pub fn get_relying_variant(&self, url: &Url) -> Option<Url> {
        let file_path = url.to_file_path().unwrap();
        self.variants
            .iter()
            .find(|(url, _variant)| match self.files.get(url) {
                Some(cached_file) => {
                    cached_file.is_main_file()
                        && cached_file
                            .get_data()
                            .symbol_cache
                            .has_dependency(&file_path)
                }
                None => false,
            })
            .map(|(url, _)| url.clone())
    }
    pub fn cache_file_data(
        &mut self,
        uri: &Url,
        validator: &mut dyn Validator,
        shader_language: &mut ShaderLanguage,
        symbol_provider: &SymbolProvider,
        config: &ServerConfig,
        dirty_deps: Option<&Path>,
    ) -> Result<Vec<Url>, ShaderError> {
        profile_scope!("Caching file data for file {}", uri);
        // Check if we cache this file for the first time.
        // Fill it default to avoid early return and empty cache.
        let file_path = uri.to_file_path().unwrap();
        // Propagate caching only if the requested file is marked as dirty.
        let should_propagate = match dirty_deps {
            Some(dirty_deps) => dirty_deps == file_path,
            None => false,
        };
        // Get old data and replace it by dummy to avoid empty data on early exit.
        let old_data = self.files.get_mut(&uri).unwrap().data.take();
        self.files.get_mut(&uri).unwrap().data = Some(ServerFileCacheData::default());
        // Check open files that depend on this file and require a recache.
        // Only needed if we changed the content. Not if we just opened the file.
        let updated_files = if should_propagate {
            // Here we ensure a possible variant is always recomputed first so that deps can copy their data.
            let dependent_files_uri = match self.get_relying_variant(&uri) {
                Some(variant_url) => {
                    let mut dependent_files_uri = self.get_dependent_files(&uri);
                    dependent_files_uri.retain(|dependent_url| *dependent_url != variant_url);
                    let mut files_to_update = Vec::new();
                    files_to_update.push(variant_url); // Update variant first
                    files_to_update.extend(dependent_files_uri); // Update dependent files
                    files_to_update
                }
                None => self.get_dependent_files(&uri),
            };
            let relying_files_uri = self.get_relying_files(&uri);
            // Update dependent file & relying files as context might have changed for them.
            let files_to_update = [dependent_files_uri, relying_files_uri].concat();

            let mut updated_files = files_to_update.clone();
            // We recompute relying before computing deps, but marking this file as dirty, so should be fine.
            for dependent_file_uri in &files_to_update {
                profile_scope!(
                    "Updating file {} as it depend or rely on {}",
                    dependent_file_uri,
                    uri
                );
                updated_files.extend(self.cache_file_data(
                    &dependent_file_uri,
                    validator,
                    shader_language,
                    symbol_provider,
                    config,
                    Some(&file_path),
                )?);
            }
            updated_files
        } else {
            vec![]
        };
        // Prepare context depending on variant.
        let mut context = if let Some(variant) = self.variants.get(uri) {
            // If we have an active variant for this file, use it.
            info!("Caching file {} as variant", uri);
            ShaderPreprocessorContext::main(
                &file_path,
                config.into_symbol_params(Some(variant.clone())),
            )
        } else {
            // Else, look for variant that include this file to get its context.
            if let Some(variant_url) = self.get_relying_variant(uri) {
                // Find variant cache & reuse it.
                info!("Caching file {} from variant {}", uri, variant_url);
                let variant_cached_file = self.files.get(&variant_url).unwrap();
                let cached_file_as_include = variant_cached_file
                    .get_data()
                    .symbol_cache
                    .find_include(&mut |i| i.get_absolute_path() == file_path)
                    .unwrap(); // Is expected.
                               // Copy all symbol cache & filter all diagnostic for the available files
                let symbol_cache = cached_file_as_include.get_cache().clone();
                let diagnostic_cache = ShaderDiagnosticList {
                    diagnostics: variant_cached_file
                        .get_data()
                        .diagnostic_cache
                        .diagnostics
                        .iter()
                        .filter(|d| {
                            let deps_file_path = &d.range.start.file_path;
                            *deps_file_path == file_path
                                || symbol_cache.has_dependency(deps_file_path)
                        })
                        .cloned()
                        .collect(),
                };
                self.files.get_mut(uri).unwrap().data = Some(ServerFileCacheData {
                    symbol_cache,
                    diagnostic_cache,
                });
                return Ok([updated_files, vec![uri.clone()]].concat());
            } else {
                info!("Caching file {} without variant", uri);
                ShaderPreprocessorContext::main(&file_path, config.into_symbol_params(None))
            }
        };
        if let Some(dirty_deps) = dirty_deps {
            context.mark_dirty(dirty_deps);
        }
        // Get symbols for main file.
        let (symbols, symbol_diagnostics) = if config.get_symbols() {
            profile_scope!("Querying symbols for file {}", uri);
            let shading_language = self.files.get(uri).unwrap().shading_language;
            let shader_module = Rc::clone(&self.files.get(uri).unwrap().shader_module);
            let shader_module = RefCell::borrow(&shader_module);
            match symbol_provider.query_symbols_with_context(
                &shader_module,
                &mut context,
                &mut |include| {
                    let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                    let included_file =
                        self.watch_dependency(&include_uri, shading_language, shader_language)?;
                    Ok(Some(Rc::clone(&included_file.shader_module)))
                },
                old_data.map(|e| e.symbol_cache),
            ) {
                Ok(symbols) => (symbols, ShaderDiagnosticList::default()),
                Err(error) => {
                    // Return this error & store it to display it as a diagnostic & dont prevent linting.
                    match error.into_diagnostic(ShaderDiagnosticSeverity::Warning) {
                        Some(diagnostic) => (
                            ShaderSymbols::new(
                                &file_path,
                                config.into_symbol_params(self.variants.get(uri).cloned()),
                            ),
                            ShaderDiagnosticList {
                                diagnostics: vec![diagnostic],
                            },
                        ),
                        None => return Err(error),
                    }
                }
            }
        } else {
            (ShaderSymbols::default(), ShaderDiagnosticList::default())
        };
        // Get diagnostics
        let mut diagnostics = if config.get_validate() {
            profile_scope!("Validating file {}", uri);
            let shading_language = self.files.get(uri).unwrap().shading_language;
            let shader_module = Rc::clone(&self.files.get(uri).unwrap().shader_module);

            let mut diagnostic_list = {
                // TODO: should print warning if validation is too long.
                profile_scope!("Raw validation");
                // Get variant & compute diagnostics from it.
                let variant = if let Some(variant) = self.variants.get(uri) {
                    Some((uri.clone(), variant.clone()))
                } else {
                    // Here if we copied data from includer variant, we should never reach. Simply return None
                    assert!(
                        self.get_relying_variant(uri).is_none(),
                        "Should not be reached"
                    );
                    None
                };
                let validation_params = config
                    .into_validation_params(variant.as_ref().map(|(_, variant)| variant.clone()));
                let variant_shader_module = match variant {
                    Some((variant_url, _)) => {
                        Rc::clone(&self.files.get(&variant_url).unwrap().shader_module)
                    }
                    None => shader_module,
                };
                let diagnostics = match validator.validate_shader(
                    &RefCell::borrow(&variant_shader_module).content,
                    RefCell::borrow(&variant_shader_module).file_path.as_path(),
                    &validation_params,
                    &mut |deps_path: &Path| -> Option<String> {
                        let deps_uri = Url::from_file_path(deps_path).unwrap();
                        let deps_file = match self.get_file(&deps_uri) {
                            Some(deps_file) => deps_file,
                            None => {
                                if config.get_symbols() {
                                    warn!(
                                        "Should only get there if symbols did not add deps: {} from includer {}.",
                                        deps_uri,
                                        uri, // This is includer as we dont recurse here.
                                    );
                                }
                                // If include does not exist, add it to watched files.
                                // Issue here: They will be considered as direct deps, while its not necessarly true, might break symbols.
                                match self.watch_dependency(
                                    &deps_uri,
                                    shading_language,
                                    shader_language,
                                ) {
                                    Ok(deps_file) => deps_file,
                                    Err(err) => {
                                        warn!("Failed to watch deps {}", err);
                                        return None;
                                    }
                                }
                            }
                        };
                        let content = RefCell::borrow(&deps_file.shader_module).content.clone();
                        Some(content)
                    },
                ) {
                    Ok(diagnostics) => diagnostics,
                    Err(err) => ShaderDiagnosticList { diagnostics: vec![
                        ShaderDiagnostic {
                            severity: ShaderDiagnosticSeverity::Error,
                            error: format!("Failed to validate shader: {:?}", err),
                            range: ShaderRange::zero(file_path.clone())
                        }
                    ]},
                };
                diagnostics
            };
            // Clear diagnostic if no errors.
            // TODO: Should add empty for main file & deps if none to clear them.

            {
                // Filter by severity.
                let required_severity = config.get_severity();
                diagnostic_list
                    .diagnostics
                    .retain(|e| e.severity.is_required(required_severity.clone()));
            }
            {
                // If includes have issues, diagnose them.
                let mut ascended_diagnostics: Vec<ShaderDiagnostic> = symbols
                    .get_preprocessor()
                    .includes
                    .iter()
                    .filter_map(|include| {
                        for diagnostic in &diagnostic_list.diagnostics {
                            if diagnostic.severity != ShaderDiagnosticSeverity::Error {
                                continue;
                            }
                            let diagnostic_path = &diagnostic.range.start.file_path;
                            if *diagnostic_path == file_path {
                                continue; // Main file diagnostics
                            }
                            if *diagnostic_path == include.get_absolute_path() {
                                return Some(ShaderDiagnostic {
                                    severity: ShaderDiagnosticSeverity::Error,
                                    error: format!(
                                        "File {} has issues:\n{}", // TODO: add command to file
                                        include.get_relative_path(),
                                        diagnostic.error
                                    ),
                                    range: include.get_range().clone(),
                                });
                            }
                            match symbols
                                .find_include(&mut |i| i.get_absolute_path() == *diagnostic_path)
                            {
                                Some(includer) => {
                                    return Some(ShaderDiagnostic {
                                        severity: ShaderDiagnosticSeverity::Error,
                                        error: format!(
                                            "File {} has issues:\n{}",
                                            includer.get_relative_path(),
                                            diagnostic.error
                                        ),
                                        range: include.get_range().clone(),
                                    })
                                }
                                None => {}
                            }
                        }
                        None
                    })
                    .collect();
                diagnostic_list
                    .diagnostics
                    .append(&mut ascended_diagnostics);
            }
            diagnostic_list
        } else {
            ShaderDiagnosticList::default()
        };

        let preprocessor_diagnotsics = symbols.get_preprocessor().diagnostics.clone();
        self.create_data(
            uri,
            symbols,
            if config.get_symbol_diagnostics() {
                diagnostics.diagnostics.extend(preprocessor_diagnotsics);
                diagnostics
                    .diagnostics
                    .extend(symbol_diagnostics.diagnostics);
                diagnostics
            } else {
                diagnostics
            },
        );
        Ok(updated_files)
    }
    pub fn watch_file(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        text: &String,
        shader_language: &mut ShaderLanguage,
        symbol_provider: &SymbolProvider,
        validator: &mut dyn Validator,
        config: &ServerConfig,
    ) -> Result<&ServerFileCache, ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();

        // Check if watched file already watched as deps
        match self.files.get_mut(&uri) {
            Some(cached_file) => {
                assert!(
                    !cached_file.is_main_file(),
                    "File {} already watched as main.",
                    uri
                );
                // Replace its content from request to make sure content is correct.
                debug_assert!(
                    RefCell::borrow_mut(&cached_file.shader_module).content == *text,
                    "Server deps content different from client provided one."
                );
                RefCell::borrow_mut(&cached_file.shader_module).content = text.clone();
                info!(
                    "Starting watching {:#?} dependency file as main file at {}. {} files in cache.",
                    lang,
                    file_path.display(),
                    self.files.len(),
                );
            }
            None => {
                let shader_module = Rc::new(RefCell::new(
                    shader_language.create_module(&file_path, &text)?,
                ));
                let cached_file = ServerFileCache {
                    shading_language: lang,
                    shader_module: shader_module,
                    data: None, // Will be filled by cache_file_data
                };
                let none = self.files.insert(uri.clone(), cached_file);
                assert!(none.is_none());
                info!(
                    "Starting watching {:#?} main file at {}. {} files in cache.",
                    lang,
                    file_path.display(),
                    self.files.len(),
                );
            }
        };
        // Cache file data from new context.
        // We dont update content here, so we can ignore updated_files.
        let _updated_files = self.cache_file_data(
            uri,
            validator,
            shader_language,
            symbol_provider,
            config,
            None, // We simply open the file. No change detected.
        )?;
        Ok(self.files.get(&uri).unwrap())
    }
    pub fn watch_dependency(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        shader_language: &mut ShaderLanguage,
    ) -> Result<&ServerFileCache, ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();
        // If file is not watched, add it as deps.
        match self.files.get(&uri) {
            Some(file) => {
                if file.is_main_file() {
                    debug!(
                        "Starting watching {:#?} main file as deps at {}. {} files in cache.",
                        lang,
                        file_path.display(),
                        self.files.len(),
                    );
                } else {
                    debug!(
                        "Already watched {:#?} deps file at {}. {} files in cache.",
                        lang,
                        file_path.display(),
                        self.files.len(),
                    );
                }
            }
            None => {
                let text = read_string_lossy(&file_path).unwrap();
                let shader_module = Rc::new(RefCell::new(
                    shader_language.create_module(&file_path, &text)?,
                ));
                let cached_file = ServerFileCache {
                    shading_language: lang,
                    shader_module: shader_module,
                    data: None, // No data means deps.
                };
                let none = self.files.insert(uri.clone(), cached_file);
                assert!(none.is_none());
                info!(
                    "Starting watching {:#?} dependency file at {}. {} files in cache.",
                    lang,
                    file_path.display(),
                    self.files.len(),
                );
            }
        }
        Ok(self.files.get(&uri).unwrap())
    }
    pub fn set_variant(&mut self, uri: Url, shader_variant: ShaderVariant) {
        self.variants.insert(uri, shader_variant);
    }
    pub fn remove_variant(&mut self, uri: Url) {
        self.variants.remove(&uri);
    }
    pub fn update_file(
        &mut self,
        uri: &Url,
        shader_language: &mut ShaderLanguage,
        range: Option<lsp_types::Range>,
        partial_content: Option<&String>,
    ) -> Result<(), ShaderError> {
        let cached_file = self.get_file(uri).unwrap();
        profile_scope!(
            "Updating file {} (Content {:?} at {:?})",
            uri,
            partial_content,
            range
        );
        // Update abstract syntax tree
        let file_path = uri.to_file_path().unwrap();
        if let (Some(range), Some(partial_content)) = (range, partial_content) {
            let shader_range = lsp_range_to_shader_range(&range, &file_path);
            shader_language.update_module_partial(
                &mut RefCell::borrow_mut(&cached_file.shader_module),
                &shader_range,
                &partial_content,
            )?;
        } else if let Some(whole_content) = partial_content {
            shader_language.update_module(
                &mut RefCell::borrow_mut(&cached_file.shader_module),
                &whole_content,
            )?;
        } else {
            // No update on content to perform.
            assert!(false, "Calling update_file unnecessarily");
        }
        Ok(())
    }
    pub fn get_file(&self, uri: &Url) -> Option<&ServerFileCache> {
        assert!(*uri == clean_url(&uri));
        match self.files.get(uri) {
            Some(cached_file) => Some(&cached_file),
            None => None,
        }
    }
    fn is_used_as_dependency(&self, uri: &Url) -> Option<(&Url, &ServerFileCache)> {
        self.files.iter().find(|(file_url, file_cache)| {
            if *file_url != uri {
                file_cache.is_main_file()
                    && file_cache
                        .get_data()
                        .symbol_cache
                        .has_dependency(&uri.to_file_path().unwrap())
            } else {
                false
            }
        })
    }
    pub fn remove_file(&mut self, uri: &Url) -> Result<Vec<Url>, ShaderError> {
        let used_as_deps = self.is_used_as_dependency(uri);
        let file_count = self.files.len();
        match used_as_deps {
            Some(_) => match self.files.get_mut(&uri) {
                Some(cached_file) => {
                    // Used as deps. Reset cache only.
                    cached_file.data = None;
                    info!(
                        "Converting {:#?} main file to deps at {}. {} files in cache.",
                        cached_file.shading_language, uri, file_count
                    );
                    Ok(Vec::new())
                }
                None => Err(ShaderError::InternalErr(format!(
                    "Trying to remove main file {} that is not watched",
                    uri.path()
                ))),
            },
            None => match self.files.remove(uri) {
                Some(cached_file) => {
                    assert!(
                        cached_file.data.is_some(),
                        "Removing main file without data"
                    );
                    let data = cached_file.data.unwrap();
                    info!(
                        "Removing {:#?} main file at {}. {} files in cache.",
                        cached_file.shading_language,
                        uri,
                        self.files.len()
                    );
                    let mut removed_files = Vec::new();
                    data.symbol_cache.visit_includes(&mut |include| {
                        let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                        let include_used_as_deps = self.is_used_as_dependency(&include_uri);
                        match include_used_as_deps {
                            Some(_) => {} // Still used, nothing to do here.
                            None => match self.files.get(&include_uri) {
                                Some(file) => if !file.is_main_file() {
                                    // Remove deps file to avoid dangling file only if not a main file.
                                    match self.files.remove(&include_uri) {
                                        Some(removed_file) => {
                                            info!(
                                                "Removing {:#?} deps file at {}. {} files in cache.",
                                                removed_file.shading_language,
                                                include_uri,
                                                self.files.len()
                                            );
                                            removed_files.push(include_uri);
                                        },
                                        None => {}, // File already removed.
                                    }
                                } else {
                                    // TODO: Mark as dirty as context is changing. Cant update here
                                    //self.cache_file_data(&include_uri, validator, shader_language, symbol_provider, config)
                                },
                                None => {}, // File already removed
                            }
                        }
                    });
                    removed_files.push(uri.clone());
                    Ok(removed_files)
                }
                None => Err(ShaderError::InternalErr(format!(
                    "Trying to remove main file {} that is not watched",
                    uri.path()
                ))),
            },
        }
    }
    pub fn get_all_symbols<'a>(
        &'a self,
        uri: &Url,
        shader_language: &'a ShaderLanguage,
    ) -> ShaderSymbolListRef<'a> {
        let cached_file = self.files.get(uri).unwrap();
        assert!(cached_file.data.is_some(), "File {} do not have cache", uri);
        let data = &cached_file.get_data();
        // Add main file symbols
        let mut symbol_cache = data.symbol_cache.get_all_symbols();
        // Add config symbols
        for symbol in data.symbol_cache.get_context().get_defines().iter() {
            symbol_cache.macros.push(&symbol);
        }
        // Add intrinsics symbols
        symbol_cache.append_as_reference(&shader_language.get_intrinsics_symbol());
        symbol_cache
    }
}
