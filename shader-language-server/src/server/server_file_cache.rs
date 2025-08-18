use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{
    profile_scope,
    server::{
        async_message::AsyncCacheRequest,
        clean_url,
        common::{lsp_range_to_shader_range, read_string_lossy},
        server_language_data::ServerLanguageData,
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
        symbols::{ShaderPreprocessorContext, ShaderRange, ShaderSymbolList, ShaderSymbolListRef},
    },
    validator::validator::Validator,
};

use super::{server_config::ServerConfig, shader_variant::ShaderVariant};

#[derive(Debug, Clone, Default)]
pub struct ServerFileCacheData {
    pub symbol_cache: ShaderSymbols, // Store symbols to avoid computing them at every change.
    pub intrinsics: ShaderSymbolList, // Cached intrinsics to not recompute them everytime
    pub diagnostic_cache: ShaderDiagnosticList, // Cached diagnostic
}

#[derive(Debug, Clone)]
pub struct ServerFileCache {
    pub shading_language: ShadingLanguage,
    pub shader_module: ShaderModuleHandle, // Store content on change as its not on disk.
    pub data: Option<ServerFileCacheData>, // Data for file opened and edited.
    // A file can be dependency, main, dependent variant or main variant.
    is_main_file: bool,    // main file are opened file in editor.
    is_variant_file: bool, // variant are set through variant window.
}

impl ServerFileCache {
    pub fn is_main_file(&self) -> bool {
        self.is_main_file
    }
    pub fn is_cachable_file(&self) -> bool {
        self.is_main_file || self.is_variant_file
    }
    pub fn get_data(&self) -> &ServerFileCacheData {
        assert!(
            self.data.is_some(),
            "Trying to get data from file {} which does not have cache.",
            RefCell::borrow(&self.shader_module).file_path.display()
        );
        self.data.as_ref().unwrap()
    }
}

pub struct ServerLanguageFileCache {
    pub files: HashMap<Url, ServerFileCache>,
    pub variant: Option<ShaderVariant>,
}

impl ServerLanguageFileCache {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            variant: None,
        }
    }
    // Get all main dependent files using the given file.
    pub fn get_dependent_main_files(&self, dependent_url: &Url) -> HashSet<Url> {
        let dependant_file_path = dependent_url.to_file_path().unwrap();
        self.files
            .iter()
            .filter(|(file_url, file)| {
                *file_url != dependent_url
                    && file.is_cachable_file()
                    && file
                        .get_data()
                        .symbol_cache
                        .has_dependency(&dependant_file_path)
            })
            .map(|(url, _file)| url.clone())
            .collect()
    }
    // Get all main files relying on the given file.
    pub fn get_relying_main_files(&self, url: &Url) -> HashSet<Url> {
        match self.files.get(url) {
            Some(file) => {
                let mut relying_on_files = HashSet::new();
                file.get_data().symbol_cache.visit_includes(&mut |include| {
                    let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                    let is_relying_on = match self.files.get(&include_uri) {
                        Some(deps) => deps.is_cachable_file(),
                        None => false,
                    };
                    if is_relying_on {
                        relying_on_files.insert(include_uri);
                    }
                });
                relying_on_files
            }
            None => {
                debug_assert!(
                    false,
                    "Trying to get relying main files for file {} that is not watched",
                    url
                );
                HashSet::new()
            }
        }
    }
    // Get all files relying on the given file.
    pub fn get_all_relying_files(&self, url: &Url) -> HashSet<Url> {
        match self.files.get(url) {
            Some(file) => {
                let mut relying_on_files = HashSet::new();
                file.get_data().symbol_cache.visit_includes(&mut |include| {
                    let include_uri = Url::from_file_path(&include.get_absolute_path()).unwrap();
                    match self.files.get(&include_uri) {
                        Some(_) => {
                            relying_on_files.insert(include_uri);
                        }
                        None => {}
                    };
                });
                relying_on_files
            }
            None => {
                debug_assert!(
                    false,
                    "Trying to get all relying files for file {} that is not watched",
                    url
                );
                HashSet::new()
            }
        }
    }
    pub fn get_relying_variant(&self, url: &Url) -> Option<Url> {
        let file_path = url.to_file_path().unwrap();
        match &self.variant {
            Some(variant) => {
                let is_relying_on = match self.files.get(&variant.url) {
                    Some(variant_cached_file) => variant_cached_file
                        .get_data()
                        .symbol_cache
                        .has_dependency(&file_path),
                    None => false,
                };
                is_relying_on.then_some(variant.url.clone())
            }
            None => None,
        }
    }
    fn __cache_file_data(
        &mut self,
        uri: &Url,
        validator: &mut dyn Validator,
        shader_language: &mut ShaderLanguage,
        symbol_provider: &SymbolProvider,
        config: &ServerConfig,
        dirty_deps: HashSet<PathBuf>,
    ) -> Result<(), ShaderError> {
        let file_path = uri.to_file_path().unwrap();

        // Get variant if its our URL.
        let variant = self.variant.clone().filter(|v| v.url == *uri);

        // Compute params
        let shader_params = config.into_shader_params(variant.clone());
        let mut context =
            ShaderPreprocessorContext::main(&file_path, shader_params.context.clone());

        // Do not recache & revalidate if not dirty.
        match &self.files.get(&uri).unwrap().data {
            Some(data) => {
                let is_dirty = data
                    .symbol_cache
                    .get_preprocessor()
                    .context
                    .is_dirty(&file_path, &context);
                let has_cache = self.files.get_mut(&uri).unwrap().data.is_some();
                let has_dirty = !dirty_deps.is_empty();
                if !is_dirty && !has_dirty && has_cache {
                    return Ok(());
                }
            }
            None => {}
        };
        for dirty_dep in dirty_deps {
            context.mark_dirty(dirty_dep);
        }

        // Get old data and replace it by dummy to avoid empty data on early exit.
        let old_data = self.files.get_mut(&uri).unwrap().data.take();
        self.files.get_mut(&uri).unwrap().data = Some(ServerFileCacheData::default());

        // Get symbols for main file.
        let (mut symbols, symbol_diagnostics) = if config.get_symbols() {
            profile_scope!("Querying symbols for file {}", uri);
            let shading_language = self.files.get(uri).unwrap().shading_language;
            let shader_module = Rc::clone(&self.files.get(uri).unwrap().shader_module);
            let shader_module = RefCell::borrow(&shader_module);
            match symbol_provider.query_symbols_with_context(
                &shader_module,
                &mut context,
                &shader_params.compilation,
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
                            ShaderSymbols::new(&file_path, shader_params.context.clone()),
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
        let diagnostics = if config.get_validate() {
            profile_scope!("Validating file {}", uri);
            let shading_language = self.files.get(uri).unwrap().shading_language;
            let shader_module = Rc::clone(&self.files.get(uri).unwrap().shader_module);

            let mut diagnostic_list = {
                // TODO: should print warning if validation is too long.
                profile_scope!("Raw validation");
                let variant_shader_module = match &variant {
                    Some(variant) => {
                        Rc::clone(&self.files.get(&variant.url).unwrap().shader_module)
                    }
                    None => shader_module,
                };
                let diagnostics = match validator.validate_shader(
                    &RefCell::borrow(&variant_shader_module).content,
                    RefCell::borrow(&variant_shader_module).file_path.as_path(),
                    &shader_params,
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
            {
                // Filter by severity.
                let required_severity = config.get_severity();
                diagnostic_list
                    .diagnostics
                    .retain(|e| e.severity.is_required(required_severity.clone()));
            }
            {
                // If includes have issues, diagnose them.
                let mut ascended_diagnostics: Vec<ShaderDiagnostic> =
                    symbols
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
                                match include.cache.as_ref().unwrap().find_include(&mut |i| {
                                    i.get_absolute_path() == *diagnostic_path
                                }) {
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

        symbols
            .get_preprocessor_mut()
            .diagnostics
            .extend(symbol_diagnostics.diagnostics);
        let intrinsics = shader_language
            .get_intrinsics_symbol(&shader_params.compilation)
            .to_owned();
        self.files.get_mut(uri).unwrap().data = Some(ServerFileCacheData {
            symbol_cache: symbols,
            intrinsics,
            diagnostic_cache: diagnostics,
        });
        Ok(())
    }

    pub fn cache_single_main_file(
        &mut self,
        url: &Url,
        shading_language: ShadingLanguage,
        language_data: &mut HashMap<ShadingLanguage, ServerLanguageData>,
        dirty_files: &HashSet<Url>,
        config: &ServerConfig,
    ) -> Result<(HashSet<Url>, HashSet<Url>), ShaderError> {
        let cached_file = self.files.get(&url).unwrap();
        let dirty_deps: HashSet<PathBuf> = if cached_file.data.is_some() {
            // Updating cache
            // TODO:ASYNC: wtf, need to use cache to find relying & co.
            dirty_files
                .iter()
                .map(|url| url.to_file_path().unwrap())
                .collect()
        } else {
            // New cache request.
            HashSet::new()
        };
        let language_data = language_data.get_mut(&shading_language).unwrap();
        let removed_files = self.cache_file_data(
            &url,
            language_data.validator.as_mut(),
            &mut language_data.language,
            &mut language_data.symbol_provider,
            &config,
            dirty_deps,
        )?;
        let url_to_republish = self.get_relying_variant(&url).unwrap_or(url.clone());
        Ok((removed_files, HashSet::from([url_to_republish])))
    }

    pub fn cache_batched_file_data(
        &mut self,
        requests: HashSet<AsyncCacheRequest>,
        language_data: &mut HashMap<ShadingLanguage, ServerLanguageData>,
        config: &ServerConfig,
    ) -> Result<(HashSet<Url>, HashSet<Url>), ShaderError> {
        // Get unique files to update in batch aswell as dirty ones.
        let mut unique_files: HashSet<Url> = requests.iter().map(|r| r.url.clone()).collect();
        let dirty_files: HashSet<Url> = requests
            .iter()
            .filter(|r| r.dirty)
            .map(|r| r.url.clone())
            .collect();

        let mut files_to_clear = HashSet::new();
        let mut files_to_publish = HashSet::new();
        // Check if variant need update (or it has dirty relying files).
        if let Some(variant) = &self.variant {
            // Compute dependent files first
            let variant_url = variant.url.clone();
            let variant_language = variant.language;
            let mut dependent_files = self.get_dependent_main_files(&variant_url);
            let relying_files = self.get_relying_main_files(&variant_url);
            dependent_files.retain(|f| !relying_files.contains(f));
            unique_files.retain(|url| dependent_files.contains(url));
            for dependent_file in dependent_files {
                // Update single file.
                let (removed_files, updated_files) = self.cache_single_main_file(
                    &dependent_file,
                    variant_language,
                    language_data,
                    &dirty_files,
                    config,
                )?;
                files_to_clear.extend(removed_files);
                files_to_publish.extend(updated_files);
            }

            // Now cache variant.
            let variant_update: HashSet<Url> =
                unique_files.extract_if(|url| *url == variant_url).collect();
            assert!(variant_update.len() <= 1, "More than one variant found...");
            let relying_files = self.get_relying_main_files(&variant_url);
            let dirty_relying_files: HashSet<&Url> = relying_files
                .iter()
                .filter(|f| dirty_files.contains(f))
                .collect();
            let is_variant_dirty = !variant_update.is_empty() || !dirty_relying_files.is_empty();
            if is_variant_dirty {
                let cached_file = self.files.get(&variant_url).unwrap();
                let dirty_deps: HashSet<PathBuf> = if cached_file.data.is_some() {
                    // Updating cache
                    dirty_files
                        .iter()
                        .map(|url| url.to_file_path().unwrap())
                        .collect()
                } else {
                    // New cache request.
                    HashSet::new()
                };
                let language_data = language_data.get_mut(&variant_language).unwrap();
                let removed_files = self.cache_file_data(
                    &variant_url,
                    language_data.validator.as_mut(),
                    &mut language_data.language,
                    &mut language_data.symbol_provider,
                    &config,
                    dirty_deps,
                )?;
                let mut files_to_publish = HashSet::new();
                let url_to_republish = self
                    .get_relying_variant(&variant_url)
                    .unwrap_or(variant_url.clone());
                files_to_clear.extend(removed_files);
                files_to_publish.insert(url_to_republish);
            }
            // Now cache dependencies of variant.
            unique_files.retain(|url| relying_files.contains(url));
            for relying_file in relying_files {
                // Update single file.
                let (removed_files, updated_files) = self.cache_single_main_file(
                    &relying_file,
                    variant_language,
                    language_data,
                    &dirty_files,
                    config,
                )?;
                files_to_clear.extend(removed_files);
                files_to_publish.extend(updated_files);
            }

            // Now cache remaining files.
            for remaining_file in unique_files {
                // Update single file.
                let (removed_files, updated_files) = self.cache_single_main_file(
                    &remaining_file,
                    variant_language,
                    language_data,
                    &dirty_files,
                    config,
                )?;
                files_to_clear.extend(removed_files);
                files_to_publish.extend(updated_files);
            }
        } else {
            // First in, first out, skip duplicates.
            // For dirty files, should update relying & dependent.
            for url in unique_files {
                let cached_file = self.files.get(&url).unwrap();
                let dirty_deps: HashSet<PathBuf> = if cached_file.data.is_some() {
                    // Updating cache
                    dirty_files
                        .iter()
                        .map(|url| url.to_file_path().unwrap())
                        .collect()
                } else {
                    // New cache request.
                    HashSet::new()
                };
                let language_data = language_data
                    .get_mut(&cached_file.shading_language)
                    .unwrap();
                let removed_files = self.cache_file_data(
                    &url,
                    language_data.validator.as_mut(),
                    &mut language_data.language,
                    &mut language_data.symbol_provider,
                    &config,
                    dirty_deps,
                )?;
                let mut files_to_publish = HashSet::new();
                let url_to_republish = self.get_relying_variant(&url).unwrap_or(url.clone());
                files_to_publish.insert(url_to_republish);
                files_to_clear.extend(removed_files);
                // TODO:ASYNC: dependencies update aswell. Should be done for all.
                // for every file, gather dependent ones and relying ones. Update in correct order.
            }
        }
        Ok((files_to_clear, files_to_publish))
    }
    pub fn cache_file_data(
        &mut self,
        uri: &Url,
        validator: &mut dyn Validator,
        shader_language: &mut ShaderLanguage,
        symbol_provider: &SymbolProvider,
        config: &ServerConfig,
        dirty_deps: HashSet<PathBuf>,
    ) -> Result<HashSet<Url>, ShaderError> {
        profile_scope!("Caching file data for file {}", uri);
        assert!(
            self.files.get(&uri).unwrap().is_cachable_file(),
            "Trying to cache data of dependency {}...",
            uri
        );
        // Check if we cache this file for the first time.
        // Fill it default to avoid early return and empty cache.
        let file_path = uri.to_file_path().unwrap();

        // Get variant if its our URL.
        let variant = self.variant.clone().filter(|v| v.url == *uri);

        info!("Caching file {} as variant: {:#?}", uri, variant);
        let mut old_relying_files = if self.files.get(&uri).unwrap().data.is_some() {
            self.get_all_relying_files(uri)
        } else {
            HashSet::new() // No old relying files as no cache
        };
        self.__cache_file_data(
            uri,
            validator,
            shader_language,
            symbol_provider,
            config,
            dirty_deps,
        )?;

        // Copy variant deps data to all its relying data.
        if let Some(variant) = &variant {
            let variant_file = self.files.get(&variant.url).unwrap();
            let mut file_to_cache = HashMap::new();
            variant_file
                .get_data()
                .symbol_cache
                .visit_includes(&mut |include| {
                    // Here, we could visit the same include twice, which will overwrite final cache.
                    let include_url = Url::from_file_path(include.get_absolute_path()).unwrap();
                    match self.files.get(&include_url) {
                        Some(cached_file) => {
                            // Ensure we did not already got cache for this file,
                            // second include might have way less symbols (because of include guard mostly)
                            if !file_to_cache.contains_key(&include_url) {
                                if cached_file.is_main_file() {
                                    let symbol_cache = include.cache.clone().unwrap();
                                    let diagnostic_cache = ShaderDiagnosticList {
                                        diagnostics: variant_file
                                            .get_data()
                                            .diagnostic_cache
                                            .diagnostics
                                            .iter()
                                            .filter(|d| {
                                                let deps_file_path = &d.range.start.file_path;
                                                *deps_file_path == include.get_absolute_path()
                                                    || symbol_cache.has_dependency(deps_file_path)
                                            })
                                            .cloned()
                                            .collect(),
                                    };
                                    let intrinsics = shader_language
                                        .get_intrinsics_symbol(
                                            &config
                                                .into_shader_params(Some(variant.clone()))
                                                .compilation,
                                        )
                                        .to_owned();
                                    file_to_cache.insert(
                                        include_url,
                                        ServerFileCacheData {
                                            symbol_cache,
                                            intrinsics,
                                            diagnostic_cache,
                                        },
                                    );
                                }
                            }
                        }
                        None => {}
                    }
                });
            for (include_url, mut include_data) in file_to_cache {
                // When copying variant cache, some file in tree might be at their second include,
                // which remove most of their symbols due to include guard.
                // To workaround this, try to find their first occurence in variant and copy it.
                let mut first_include: HashSet<PathBuf> = HashSet::new();
                let mut reached_include = false;
                let variant_file = self.files.get(&variant.url).unwrap();
                include_data
                    .symbol_cache
                    .visit_includes_mut(&mut |include| {
                        // We only need previously declared element. Stop once we reach it.
                        if !reached_include {
                            reached_include = include.get_absolute_path() == file_path;
                            match variant_file.get_data().symbol_cache.find_include(
                                &mut |variant_include| {
                                    include.get_absolute_path()
                                        == variant_include.get_absolute_path()
                                },
                            ) {
                                Some(variant_include) => {
                                    if first_include.insert(include.get_absolute_path().into()) {
                                        include.cache = variant_include.cache.clone();
                                    }
                                }
                                None => {} // Not found
                            }
                        }
                    });
                self.files.get_mut(&include_url).unwrap().data = Some(include_data);
                // Mark them for publishing diagnostics.
            }
        }
        // Get dangling dependencies that need to be removed.
        let new_relying_files = self.get_all_relying_files(uri);
        old_relying_files.retain(|f| {
            if f != uri {
                // Check if file was removed from include tree.
                if new_relying_files.iter().find(|n| *n == f).is_none() {
                    self.is_dangling_file(f)
                } else {
                    false // Keep it by removing it from update.
                }
            } else {
                false // Avoid removing main file twice.
            }
        });
        // Remove these deps from cache.
        for old_relying_file in &old_relying_files {
            info!("Removing dangling deps {}", old_relying_file);
            self.files.remove(old_relying_file);
        }

        debug_assert!(
            self.get_file(uri).unwrap().data.is_some(),
            "Failed to cache data for file {}",
            uri
        );
        Ok(old_relying_files)
    }
    pub fn watch_variant_file(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        shader_language: &mut ShaderLanguage,
    ) -> Result<(), ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();
        // Check if watched file already watched as deps or variant.
        match self.files.get_mut(&uri) {
            Some(cached_file) => {
                if !cached_file.is_variant_file {
                    cached_file.is_variant_file = true;
                    info!(
                        "Starting watching {:#?} file as variant file at {}. {} files in cache.",
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
                    data: None,
                    is_main_file: false,
                    is_variant_file: true,
                };
                let none = self.files.insert(uri.clone(), cached_file);
                assert!(none.is_none());
                info!(
                    "Starting watching {:#?} variant file at {}. {} files in cache.",
                    lang,
                    file_path.display(),
                    self.files.len(),
                );
            }
        };
        Ok(())
    }
    pub fn watch_main_file(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        text: &str,
        shader_language: &mut ShaderLanguage,
    ) -> Result<(), ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();

        // Check if watched file already watched as deps
        match self.files.get_mut(&uri) {
            Some(cached_file) => {
                assert!(
                    !cached_file.is_main_file,
                    "File {} already watched as main.",
                    uri
                );
                cached_file.is_main_file = true;
                // Replace its content from request to make sure content is correct.
                debug_assert!(
                    RefCell::borrow_mut(&cached_file.shader_module).content == *text,
                    "Server deps content different from client provided one."
                );
                RefCell::borrow_mut(&cached_file.shader_module).content = text.into();
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
                debug_assert!(self.variant.as_ref().map(|v| v.url != *uri).unwrap_or(true));
                let cached_file = ServerFileCache {
                    shading_language: lang,
                    shader_module: shader_module,
                    data: None,
                    is_main_file: true,
                    is_variant_file: false, // Cannot be a variant if its not watched.
                };
                let none = self.files.insert(uri.clone(), cached_file);
                debug_assert!(none.is_none());
                info!(
                    "Starting watching {:#?} main file at {}. {} files in cache.",
                    lang,
                    file_path.display(),
                    self.files.len(),
                );
            }
        };
        Ok(())
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
                } else if file.is_variant_file {
                    debug!(
                        "Already watched {:#?} deps file as variant at {}. {} files in cache.",
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
                    data: None,
                    is_main_file: false,
                    is_variant_file: self
                        .variant
                        .as_ref()
                        .map(|v| v.url == *uri)
                        .unwrap_or(false),
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
        let file_path = uri.to_file_path().unwrap();
        self.files.iter().find(|(file_url, file_cache)| {
            if *file_url != uri {
                file_cache.is_cachable_file()
                    && file_cache
                        .get_data()
                        .symbol_cache
                        .has_dependency(&file_path)
            } else {
                false
            }
        })
    }
    fn is_dangling_file(&self, uri: &Url) -> bool {
        match self.files.get(uri) {
            Some(cached_file) => {
                !cached_file.is_cachable_file() && self.is_used_as_dependency(uri).is_none()
            }
            None => {
                debug_assert!(
                    false,
                    "Checking if file {} is dangling but its not watched.",
                    uri
                );
                false
            }
        }
    }
    // Dependency removal are handled by remove_variant & remove_file.
    pub fn remove_variant_file(&mut self, uri: &Url) -> Result<Vec<Url>, ShaderError> {
        let used_as_deps = self.is_used_as_dependency(uri).is_some();
        let mut dangling_files = self.get_all_relying_files(uri);
        match self.files.get_mut(&uri) {
            Some(cached_file) => {
                if used_as_deps || cached_file.is_main_file() {
                    let shading_language = cached_file.shading_language;
                    // Used as deps. Reset cache only if not main.
                    if !cached_file.is_main_file {
                        cached_file.data = None;
                    }
                    debug_assert!(cached_file.is_variant_file);
                    cached_file.is_variant_file = false;
                    info!(
                        "Converted {:#?} variant file to {} at {}. {} files in cache.",
                        shading_language,
                        if cached_file.is_main_file {
                            "main file"
                        } else {
                            "deps file"
                        },
                        uri,
                        self.files.len()
                    );
                    Ok(vec![])
                } else {
                    match self.files.remove(uri) {
                        Some(mut cached_file) => {
                            let shading_language = cached_file.shading_language;
                            assert!(
                                cached_file.data.is_some(),
                                "Removing variant file without data"
                            );
                            // Get dangling dependencies that need to be removed.
                            dangling_files.retain(|f| {
                                if uri != f {
                                    self.is_dangling_file(f)
                                } else {
                                    false // Avoid removing main file twice.
                                }
                            });
                            // Remove main file before deps & drop cache for ref.
                            let data = cached_file.data.unwrap();
                            drop(data);
                            cached_file.is_variant_file = false; // Just to be sure.
                            info!(
                                "Removed {:#?} main file at {}. {} files in cache.",
                                cached_file.shading_language,
                                uri,
                                self.files.len()
                            );
                            // Remove these deps from cache.
                            for dangling_file in &dangling_files {
                                self.files.remove(dangling_file);
                                info!(
                                    "Removed {:#?} dangling deps {}. {} files in cache.",
                                    shading_language,
                                    dangling_file,
                                    self.files.len()
                                );
                            }
                            Ok(
                                vec![vec![uri.clone()], dangling_files.into_iter().collect()]
                                    .concat(),
                            )
                        }
                        None => Err(ShaderError::InternalErr(format!(
                            "Trying to remove variant file {} that is not watched",
                            uri.path()
                        ))),
                    }
                }
            }
            None => Err(ShaderError::InternalErr(format!(
                "Trying to remove variant file {} that is not watched",
                uri.path()
            ))),
        }
    }
    pub fn remove_main_file(&mut self, uri: &Url) -> Result<Vec<Url>, ShaderError> {
        let used_as_deps = self.is_used_as_dependency(uri).is_some();
        let mut dangling_files = self.get_all_relying_files(uri);
        match self.files.get_mut(&uri) {
            Some(cached_file) => {
                if used_as_deps || cached_file.is_variant_file {
                    let shading_language = cached_file.shading_language;
                    // Used as deps. Reset cache only if not main.
                    if !cached_file.is_variant_file {
                        cached_file.data = None;
                    }
                    debug_assert!(cached_file.is_main_file);
                    cached_file.is_main_file = false;
                    info!(
                        "Converted {:#?} main file to {} at {}. {} files in cache.",
                        shading_language,
                        if cached_file.is_main_file {
                            "variant file"
                        } else {
                            "deps file"
                        },
                        uri,
                        self.files.len()
                    );
                    Ok(vec![])
                } else {
                    match self.files.remove(uri) {
                        Some(mut cached_file) => {
                            let shading_language = cached_file.shading_language;
                            assert!(
                                cached_file.data.is_some(),
                                "Removing main file without data"
                            );
                            // Get dangling dependencies that need to be removed.
                            dangling_files.retain(|f| {
                                if uri != f {
                                    self.is_dangling_file(f)
                                } else {
                                    false // Avoid removing main file twice.
                                }
                            });
                            // Remove main file before deps & drop cache for ref.
                            let data = cached_file.data.unwrap();
                            drop(data);
                            cached_file.is_main_file = false; // Just to be sure.
                            info!(
                                "Removed {:#?} main file at {}. {} files in cache.",
                                cached_file.shading_language,
                                uri,
                                self.files.len()
                            );
                            // Remove these deps from cache.
                            for dangling_file in &dangling_files {
                                self.files.remove(dangling_file);
                                info!(
                                    "Removed {:#?} dangling deps {}. {} files in cache.",
                                    shading_language,
                                    dangling_file,
                                    self.files.len()
                                );
                            }
                            Ok(
                                vec![vec![uri.clone()], dangling_files.into_iter().collect()]
                                    .concat(),
                            )
                        }
                        None => Err(ShaderError::InternalErr(format!(
                            "Trying to remove main file {} that is not watched",
                            uri.path()
                        ))),
                    }
                }
            }
            None => Err(ShaderError::InternalErr(format!(
                "Trying to remove main file {} that is not watched",
                uri.path()
            ))),
        }
    }
    pub fn get_all_symbols<'a>(&'a self, uri: &Url) -> ShaderSymbolListRef<'a> {
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
        symbol_cache.append_as_reference(&data.intrinsics);
        symbol_cache
    }
}
