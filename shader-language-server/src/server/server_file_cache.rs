use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use crate::server::{
    clean_url,
    common::{lsp_range_to_shader_range, read_string_lossy},
};
use log::{debug, warn};
use lsp_types::Url;
use shader_sense::{
    shader::ShadingLanguage,
    shader_error::{ShaderDiagnostic, ShaderDiagnosticList, ShaderDiagnosticSeverity, ShaderError},
    symbols::{
        symbol_provider::SymbolProvider,
        symbol_tree::SymbolTree,
        symbols::{ShaderPreprocessor, ShaderSymbolList, ShaderSymbolParams},
    },
    validator::validator::Validator,
};

use super::{server_config::ServerConfig, shader_variant::ShaderVariant};

pub type ServerFileCacheHandle = Rc<RefCell<ServerFileCache>>;

#[derive(Debug, Clone)]
pub struct ServerFileCache {
    pub shading_language: ShadingLanguage,
    pub symbol_tree: SymbolTree, // Store content on change as its not on disk.
    pub preprocessor_cache: ShaderPreprocessor, // Store preprocessor to avoid computing them at every change.
    pub symbol_cache: ShaderSymbolList, // Store symbol to avoid computing them at every change.
    pub dependencies: HashMap<PathBuf, ServerFileCacheHandle>, // Store all direct dependencies of this file.
    pub diagnostic_cache: ShaderDiagnosticList,                // Cached diagnostic
}

pub struct ServerLanguageFileCache {
    pub files: HashMap<Url, ServerFileCacheHandle>,
    pub dependencies: HashMap<Url, ServerFileCacheHandle>,
    pub variants: HashMap<Url, ShaderVariant>,
}

impl ServerLanguageFileCache {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            dependencies: HashMap::new(),
            variants: HashMap::new(),
        }
    }
    fn recurse_file_symbol(
        &mut self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        mut symbol_params: ShaderSymbolParams,
        symbol_provider: &mut dyn SymbolProvider,
    ) -> Result<(), ShaderError> {
        let file_path = uri.to_file_path().unwrap();
        let shading_language = RefCell::borrow(cached_file).shading_language;
        // We are recomputing every deps symbols here, but not really required, isnt it ?
        let now_symbols = Instant::now();
        let (preprocessor, symbol_list, diagnostics) = match symbol_provider
            .query_preprocessor(&RefCell::borrow(cached_file).symbol_tree, &symbol_params)
        {
            Ok(preprocessor) => {
                let symbol_list = symbol_provider.query_file_symbols(
                    &RefCell::borrow(cached_file).symbol_tree,
                    Some(&preprocessor),
                )?;
                (preprocessor, symbol_list, ShaderDiagnosticList::empty())
            }
            Err(error) => {
                // Return this error & store it to display it as a diagnostic & dont prevent linting.
                if let ShaderError::SymbolQueryError(message, range) = error {
                    (
                        ShaderPreprocessor::default(),
                        ShaderSymbolList::default(),
                        ShaderDiagnosticList {
                            diagnostics: vec![ShaderDiagnostic {
                                file_path: Some(uri.to_file_path().unwrap()),
                                severity: ShaderDiagnosticSeverity::Error,
                                error: message,
                                line: range.start.line,
                                pos: range.start.pos,
                            }],
                        },
                    )
                } else {
                    return Err(error);
                }
            }
        };
        debug!(
            "{}:timing:symbols:ast          {}ms",
            file_path.display(),
            now_symbols.elapsed().as_millis()
        );
        // Add context macro for next includes.
        for define in &preprocessor.defines {
            symbol_params.defines.insert(
                define.name.clone(),
                define.value.clone().unwrap_or_default(),
            );
        }
        // Recurse dependencies.
        for include in &preprocessor.includes {
            let include_uri = Url::from_file_path(&include.absolute_path).unwrap();
            let included_file =
                self.watch_dependency(&include_uri, shading_language, symbol_provider)?;
            self.recurse_file_symbol(
                &include_uri,
                &included_file,
                symbol_params.clone(),
                symbol_provider,
            )?;
            RefCell::borrow_mut(&cached_file)
                .dependencies
                .insert(include.absolute_path.clone(), included_file);
        }
        // Store cache.
        let mut cached_file = RefCell::borrow_mut(&cached_file);
        cached_file.preprocessor_cache = preprocessor;
        cached_file.symbol_cache = symbol_list;
        cached_file.diagnostic_cache = diagnostics;
        Ok(())
    }
    fn cache_file_data(
        &mut self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        validator: &mut dyn Validator,
        symbol_provider: &mut dyn SymbolProvider,
        shader_variant: Option<ShaderVariant>,
        config: &ServerConfig,
    ) -> Result<(), ShaderError> {
        {
            // Reset cache
            let mut cached_file = RefCell::borrow_mut(&cached_file);
            cached_file.preprocessor_cache = ShaderPreprocessor::default();
            cached_file.symbol_cache = ShaderSymbolList::default();
            cached_file.diagnostic_cache = ShaderDiagnosticList::default();
        }
        // Get symbols for main file.
        if config.symbols {
            let mut symbol_params = config.into_symbol_params();
            // Add variant data if some.
            if let Some(variant) = shader_variant {
                for (variable, value) in variant.defines {
                    symbol_params.defines.insert(variable, value);
                }
            }
            self.recurse_file_symbol(uri, cached_file, symbol_params, symbol_provider)?;
        }
        // Get diagnostics
        if config.validate {
            let shading_language = RefCell::borrow(cached_file).shading_language;
            let (mut diagnostic_list, _dependencies) = validator.validate_shader(
                &RefCell::borrow(cached_file).symbol_tree.content,
                RefCell::borrow(cached_file).symbol_tree.file_path.as_path(),
                &config.into_validation_params(),
                &mut |deps_path: &Path| -> Option<String> {
                    let deps_uri = Url::from_file_path(deps_path).unwrap();
                    let deps_file = match self.get_dependency(&deps_uri) {
                        Some(deps_file) => deps_file,
                        None => {
                            debug_assert!(
                                !config.symbols,
                                "Should only get there if symbols did not add deps."
                            );
                            // If include does not exist, add it to watched files.
                            match self.watch_dependency(
                                &deps_uri,
                                shading_language,
                                symbol_provider,
                            ) {
                                Ok(deps_file) => deps_file,
                                Err(err) => {
                                    warn!("Failed to watch deps {}", err);
                                    return None;
                                }
                            }
                        }
                    };
                    let content = RefCell::borrow(&deps_file).symbol_tree.content.clone();
                    // Add deps if they werent added by symbols.
                    if !config.symbols {
                        RefCell::borrow_mut(&cached_file)
                            .dependencies
                            .insert(PathBuf::from(deps_path), deps_file);
                    }
                    Some(content)
                },
            )?;
            // Clear diagnostic if no errors.
            // TODO: Should add empty for main file & deps if none to clear them.

            // Filter by severity.
            let required_severity = ShaderDiagnosticSeverity::from(config.severity.clone());
            diagnostic_list
                .diagnostics
                .retain(|e| e.severity.is_required(required_severity.clone()));

            let mut cached_file = RefCell::borrow_mut(&cached_file);
            cached_file
                .diagnostic_cache
                .diagnostics
                .append(&mut diagnostic_list.diagnostics);
        }
        Ok(())
    }
    pub fn watch_file(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        text: &String,
        symbol_provider: &mut dyn SymbolProvider,
        validator: &mut dyn Validator,
        config: &ServerConfig,
    ) -> Result<ServerFileCacheHandle, ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();

        // Check watched file already watched as deps
        let cached_file = match self.dependencies.get(&uri) {
            Some(cached_file) => {
                // Watched as deps, promote it.
                RefCell::borrow_mut(&cached_file).symbol_tree.content = text.clone();
                self.files.insert(uri.clone(), Rc::clone(&cached_file));
                Rc::clone(&cached_file)
            }
            None => {
                assert!(self.files.get(&uri).is_none());
                let symbol_tree = SymbolTree::new(symbol_provider, &file_path, &text)?;
                let cached_file = Rc::new(RefCell::new(ServerFileCache {
                    shading_language: lang,
                    symbol_tree: symbol_tree,
                    preprocessor_cache: ShaderPreprocessor::default(),
                    symbol_cache: ShaderSymbolList::default(),
                    dependencies: HashMap::new(), // Will be filled by validator.
                    diagnostic_cache: ShaderDiagnosticList::default(),
                }));
                self.cache_file_data(
                    uri,
                    &cached_file,
                    validator,
                    symbol_provider,
                    self.variants.get(&uri).cloned(),
                    config,
                )?;
                let none = self.files.insert(uri.clone(), Rc::clone(&cached_file));
                assert!(none.is_none());
                cached_file
            }
        };
        debug!(
            "Starting watching {:#?} main file at {}. {} files in cache.",
            lang,
            file_path.display(),
            self.files.len(),
        );
        Ok(cached_file)
    }
    pub fn watch_dependency(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        symbol_provider: &mut dyn SymbolProvider,
    ) -> Result<ServerFileCacheHandle, ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();

        // Check watched file already watched as main file
        match self.files.get(&uri) {
            Some(cached_file) => match self.dependencies.get(&uri) {
                Some(_deps_file) => {
                    // Watched as main & deps already, copy it.
                    assert!(Rc::ptr_eq(_deps_file, cached_file));
                    debug!(
                        "File already watched as main and deps : {:#?} dependency file at {}. {} deps in cache.",
                        lang,
                        file_path.display(),
                        self.dependencies.len(),
                    );
                    Ok(Rc::clone(&cached_file))
                }
                None => {
                    // Watched as main only, copy it.
                    self.dependencies
                        .insert(uri.clone(), Rc::clone(cached_file));
                    debug!(
                        "File already watched as main : {:#?} dependency file at {}. {} deps in cache.",
                        lang,
                        file_path.display(),
                        self.dependencies.len(),
                    );
                    Ok(Rc::clone(&cached_file))
                }
            },
            None => match self.dependencies.get(&uri) {
                Some(cached_file) => {
                    debug!(
                        "File already watched as deps : {:#?} dependency file at {}. {} deps in cache.",
                        lang,
                        file_path.display(),
                        self.dependencies.len(),
                    );
                    Ok(Rc::clone(&cached_file))
                }
                None => {
                    let text = read_string_lossy(&file_path).unwrap();
                    let symbol_tree = SymbolTree::new(symbol_provider, &file_path, &text)?;
                    let cached_file = Rc::new(RefCell::new(ServerFileCache {
                        shading_language: lang,
                        symbol_tree: symbol_tree,
                        preprocessor_cache: ShaderPreprocessor::default(),
                        symbol_cache: ShaderSymbolList::default(),
                        dependencies: HashMap::new(), // Will be filled by validator.
                        diagnostic_cache: ShaderDiagnosticList::default(),
                    }));
                    let none = self
                        .dependencies
                        .insert(uri.clone(), Rc::clone(&cached_file));
                    assert!(none.is_none());
                    debug!(
                        "Starting watching {:#?} dependency file at {}. {} deps in cache.",
                        lang,
                        file_path.display(),
                        self.dependencies.len(),
                    );
                    Ok(cached_file)
                }
            },
        }
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
        cached_file: &ServerFileCacheHandle,
        symbol_provider: &mut dyn SymbolProvider,
        validator: &mut dyn Validator,
        config: &ServerConfig,
        range: Option<lsp_types::Range>,
        partial_content: Option<&String>,
    ) -> Result<(), ShaderError> {
        let now_start = std::time::Instant::now();
        let now_update_ast = std::time::Instant::now();
        // Update abstract syntax tree
        let file_path = uri.to_file_path().unwrap();
        if let (Some(range), Some(partial_content)) = (range, partial_content) {
            let shader_range = lsp_range_to_shader_range(&range, &file_path);
            RefCell::borrow_mut(cached_file)
                .symbol_tree
                .update_partial(symbol_provider, &shader_range, &partial_content)?;
        } else if let Some(whole_content) = partial_content {
            RefCell::borrow_mut(cached_file)
                .symbol_tree
                .update(symbol_provider, &whole_content)?;
        } else {
            // No update on content to perform.
        }
        debug!(
            "{}:timing:update:ast           {}ms",
            file_path.display(),
            now_update_ast.elapsed().as_millis()
        );

        let now_get_all_symbol = std::time::Instant::now();
        self.cache_file_data(
            uri,
            cached_file,
            validator,
            symbol_provider,
            self.variants.get(&uri).cloned(),
            config,
        )?;
        debug!(
            "{}:timing:update:get_all_symbol{}ms",
            file_path.display(),
            now_get_all_symbol.elapsed().as_millis()
        );
        debug!(
            "{}:timing:update:              {}ms",
            file_path.display(),
            now_start.elapsed().as_millis()
        );
        Ok(())
    }
    pub fn get(&self, uri: &Url) -> Option<ServerFileCacheHandle> {
        assert!(*uri == clean_url(&uri));
        match self.files.get(uri) {
            Some(cached_file) => Some(Rc::clone(&cached_file)),
            None => None,
        }
    }
    pub fn get_dependency(&self, uri: &Url) -> Option<ServerFileCacheHandle> {
        assert!(*uri == clean_url(&uri));
        match self.dependencies.get(uri) {
            Some(cached_file) => Some(Rc::clone(&cached_file)),
            None => None,
        }
    }
    pub fn remove_dependency(&mut self, uri: &Url) -> Result<(), ShaderError> {
        fn list_all_dependencies_count(
            file_cache: &HashMap<PathBuf, ServerFileCacheHandle>,
        ) -> HashMap<PathBuf, usize> {
            let list = HashMap::new();
            for dependency in file_cache {
                let mut list = HashMap::new();
                let cached_dependency = RefCell::borrow_mut(dependency.1);
                let deps = list_all_dependencies_count(&cached_dependency.dependencies);
                for dep in deps {
                    match list.get_mut(&dep.0) {
                        Some(count) => {
                            *count += 1;
                        }
                        None => {
                            list.insert(dep.0, 1);
                        }
                    }
                }
            }
            list
        }
        let file_path = uri.to_file_path().unwrap();
        match self.dependencies.get(uri) {
            Some(cached_file) => {
                // Check if strong_count are not reference to itself within deps.
                let dependencies_count =
                    list_all_dependencies_count(&RefCell::borrow(cached_file).dependencies);
                let is_last_ref = match dependencies_count.get(&file_path) {
                    Some(count) => {
                        let ref_count = Rc::strong_count(cached_file);
                        debug!("Found {} deps count with {} strong count", count, ref_count);
                        *count + 1 >= ref_count
                    }
                    None => Rc::strong_count(cached_file) == 1,
                };
                if is_last_ref {
                    // Remove dependency.
                    let cached_file = self.dependencies.remove(uri).unwrap();
                    drop(cached_file);
                    debug!(
                        "Removing dependency file at {}. {} deps in cache.",
                        file_path.display(),
                        self.dependencies.len(),
                    );
                    // Remove every dangling deps
                    for (dependency_path, dependency_count) in dependencies_count {
                        let dependency_url = Url::from_file_path(&dependency_path).unwrap();
                        match self.dependencies.get(&dependency_url) {
                            Some(dependency_file) => {
                                if dependency_count >= Rc::strong_count(dependency_file) {
                                    self.dependencies.remove(&dependency_url).unwrap();
                                    debug!(
                                        "Removed dangling dependency file at {}. {} deps in cache.",
                                        dependency_path.display(),
                                        self.dependencies.len(),
                                    );
                                }
                            }
                            None => {
                                return Err(ShaderError::InternalErr(format!(
                                    "Could not find dependency file {}",
                                    dependency_path.display()
                                )))
                            }
                        }
                    }
                }
                Ok(())
            }
            None => Err(ShaderError::InternalErr(format!(
                "Trying to remove dependency file {} that is not watched",
                uri.path()
            ))),
        }
    }
    pub fn remove_file(&mut self, uri: &Url) -> Result<bool, ShaderError> {
        match self.files.remove(uri) {
            Some(cached_file) => {
                let mut cached_file = RefCell::borrow_mut(&cached_file);
                let dependencies = std::mem::take(&mut cached_file.dependencies);
                drop(cached_file);
                debug!(
                    "Removing main file at {}. {} files in cache.",
                    uri.to_file_path().unwrap().display(),
                    self.files.len(),
                );
                for dependency in dependencies {
                    let deps_uri = Url::from_file_path(&dependency.0).unwrap();
                    drop(dependency.1); // Decrease ref count.
                    let _removed = self.remove_dependency(&deps_uri)?;
                }
                // Check if it was destroyed or we still have it in deps.
                Ok(self.dependencies.get(&uri).is_none())
            }
            None => Err(ShaderError::InternalErr(format!(
                "Trying to remove main file {} that is not watched",
                uri.path()
            ))),
        }
    }
}
