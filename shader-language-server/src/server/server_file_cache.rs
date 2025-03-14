use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
};

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
    include::IncludeHandler,
    shader::ShadingLanguage,
    shader_error::{ShaderDiagnostic, ShaderDiagnosticList, ShaderDiagnosticSeverity, ShaderError},
    symbols::{
        symbol_provider::SymbolProvider,
        symbol_tree::SymbolTree,
        symbols::{
            ShaderPosition, ShaderPreprocessor, ShaderPreprocessorContext,
            ShaderPreprocessorDefine, ShaderPreprocessorInclude, ShaderSymbol, ShaderSymbolData,
            ShaderSymbolList, ShaderSymbolParams,
        },
    },
    validator::validator::Validator,
};

use super::{server_config::ServerConfig, shader_variant::ShaderVariant};

pub type ServerFileCacheHandle = Rc<RefCell<ServerFileCache>>;

#[derive(Debug, Clone, Default)]
pub struct ServerFileCacheData {
    pub preprocessor_cache: ShaderPreprocessor, // Store preprocessor to avoid computing them at every change.
    symbol_cache: ShaderSymbolList, // Store symbol to avoid computing them at every change.
    pub diagnostic_cache: ShaderDiagnosticList, // Cached diagnostic
    pub dependencies: HashMap<Url, ServerFileCacheHandle>, // Store all direct dependencies of this file.
}

#[derive(Debug, Clone)]
pub struct ServerFileCache {
    pub shading_language: ShadingLanguage,
    pub symbol_tree: SymbolTree, // Store content on change as its not on disk.
    pub data: ServerFileCacheData, // Data for file opened and edited.
    pub included_data: HashMap<Url, ServerFileCacheData>, // Data per entry point for context, data might change depending on it, and file might be included multiple times.
}

impl ServerFileCacheData {
    pub fn get_symbols(&self) -> ShaderSymbolList {
        let mut symbols = self.symbol_cache.clone();
        self.preprocessor_cache.preprocess_symbols(&mut symbols);
        symbols
    }
    fn dump_dependency_node(
        &self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        includer_uri: &Url,
        header: String,
        is_last: bool,
        unique_deps: &mut HashSet<Url>,
    ) -> String {
        unique_deps.insert(uri.clone());
        let mut dependency_tree =
            format!("{}{} {}\n", header, if is_last { "└─" } else { "├─" }, uri);
        let childs_header = format!("{}{}", header, if is_last { "  " } else { "|  " });
        match RefCell::borrow(&cached_file)
            .included_data
            .get(includer_uri)
        {
            Some(data) => {
                let mut deps_iter = data.dependencies.iter().peekable();
                while let Some((deps_url, deps_file)) = deps_iter.next() {
                    if !unique_deps.contains(deps_url) {
                        dependency_tree.push_str(
                            self.dump_dependency_node(
                                deps_url,
                                deps_file,
                                includer_uri,
                                childs_header.clone(),
                                deps_iter.peek().is_none(),
                                unique_deps,
                            )
                            .as_str(),
                        );
                    } else {
                        dependency_tree
                            .push_str(format!("{}└─ {}\n", childs_header, deps_url).as_str());
                        dependency_tree
                            .push_str(format!("{}  └─ [...cyclic...]\n", childs_header).as_str());
                    }
                }
            }
            None => dependency_tree
                .push_str(format!("{}└─ Missing included_data\n", childs_header).as_str()),
        }
        dependency_tree
    }
    pub fn dump_dependency_tree(&self, uri: &Url) -> String {
        let mut dependency_tree = format!("{}\n", uri);

        let mut deps_iter = self.dependencies.iter().peekable();
        while let Some((deps_url, deps_file)) = deps_iter.next() {
            dependency_tree.push_str(
                self.dump_dependency_node(
                    deps_url,
                    deps_file,
                    uri,
                    "   ".into(),
                    deps_iter.peek().is_none(),
                    &mut HashSet::new(),
                )
                .as_str(),
            );
        }
        dependency_tree
    }
}

pub struct ServerLanguageFileCache {
    pub files: HashMap<Url, ServerFileCacheHandle>,
    pub dependencies: HashMap<Url, ServerFileCacheHandle>,
    pub variants: HashMap<Url, ShaderVariant>,
}

struct IncludeContext {
    includer_uri: Url,                  // The file from which this file is included.
    visited_dependencies: HashSet<Url>, // Already visited deps, needed for once.
    include_handler: IncludeHandler,    // Handler for includes.
    defines: HashMap<String, String>,   // Preprocessor macros defined in context.
}

impl IncludeContext {
    pub fn main(
        uri: &Url,
        includes: Vec<String>,
        defines: HashMap<String, String>,
        path_remapping: HashMap<String, String>,
    ) -> Self {
        Self {
            includer_uri: uri.clone(),
            visited_dependencies: HashSet::new(),
            include_handler: IncludeHandler::new(
                &uri.to_file_path().unwrap(),
                includes,
                path_remapping
                    .iter()
                    .map(|(vp, p)| (vp.into(), p.into()))
                    .collect(),
            ),
            defines,
        }
    }
    pub fn should_visit(&mut self, uri: &Url) -> bool {
        // For now, every include is included once, should cover 99% of case.
        if self.visited_dependencies.insert(uri.clone()) {
            true
        } else {
            false // !preprocessor.once
        }
    }
    pub fn add_context_for_include(
        &mut self,
        include: &ShaderPreprocessorInclude,
        context_preprocessor_define: &mut Vec<ShaderPreprocessorDefine>,
    ) {
        let is_defined_before_include =
            |include_position: &ShaderPosition, define: &ShaderPreprocessorDefine| -> bool {
                match &define.range {
                    // Check define is before include position.
                    Some(range) => range.start < *include_position,
                    None => false, // Should not happen with local symbols...
                }
            };
        // Get context defined before include & add them to current context.
        let include_define_context: Vec<&ShaderPreprocessorDefine> = context_preprocessor_define
            .iter()
            .filter(|define| is_defined_before_include(&include.range.start, define))
            .collect();
        for define in include_define_context {
            self.defines.insert(
                define.name.clone(),
                define.value.clone().unwrap_or("".into()),
            );
        }
        context_preprocessor_define
            .retain(|define| !is_defined_before_include(&include.range.start, define));
    }
    pub fn add_context(&mut self, context_preprocessor_define: Vec<ShaderPreprocessorDefine>) {
        for define in context_preprocessor_define {
            self.defines
                .insert(define.name, define.value.unwrap_or("".into()));
        }
    }
    pub fn visit_data<F: FnOnce(&mut ServerFileCacheData)>(
        &self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        visitor: F,
    ) {
        if *uri == self.includer_uri {
            visitor(&mut RefCell::borrow_mut(cached_file).data);
        } else {
            let cached_file = &mut RefCell::borrow_mut(cached_file);
            match cached_file.included_data.get_mut(&self.includer_uri) {
                Some(data) => visitor(data),
                None => {
                    cached_file
                        .included_data
                        .insert(self.includer_uri.clone(), ServerFileCacheData::default());
                    visitor(
                        cached_file
                            .included_data
                            .get_mut(&self.includer_uri)
                            .unwrap(),
                    );
                }
            }
        }
    }
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
        symbol_provider: &mut dyn SymbolProvider,
        include_context: &mut IncludeContext,
    ) -> Result<(), ShaderError> {
        profile_scope!("Recursing symbols for file {}", uri);
        let shading_language = RefCell::borrow(cached_file).shading_language;
        let context_symbol_params = ShaderSymbolParams {
            defines: include_context.defines.clone(),
        };
        // We are recomputing every deps symbols here, but not really required, isnt it ?
        let (preprocessor_cache, symbol_cache, diagnostic_cache) = match symbol_provider
            .query_preprocessor(
                &RefCell::borrow(cached_file).symbol_tree,
                &context_symbol_params,
                &mut include_context.include_handler,
            ) {
            Ok(preprocessor_cache) => {
                // Might not have included data if first visit.
                let symbol_cache = match RefCell::borrow(cached_file).included_data.get(&uri) {
                    // Only query new symbols for dirty files (main files).
                    // Deps do not need update as they are not edited.
                    // But preprocessor changes in main file might impact deps.
                    Some(data) => {
                        if *uri != include_context.includer_uri {
                            profile_scope!("Cloning symbols for file {}", uri);
                            data.symbol_cache.clone()
                        } else {
                            profile_scope!("Query symbols for file {}", uri);
                            symbol_provider
                                .query_file_symbols(&RefCell::borrow(cached_file).symbol_tree)?
                        }
                    }
                    None => {
                        profile_scope!("Query symbols for empty file {}", uri);
                        symbol_provider
                            .query_file_symbols(&RefCell::borrow(cached_file).symbol_tree)?
                    }
                };
                (
                    preprocessor_cache,
                    symbol_cache,
                    ShaderDiagnosticList::empty(),
                )
            }
            Err(error) => {
                // Return this error & store it to display it as a diagnostic & dont prevent linting.
                if let ShaderError::SymbolQueryError(message, range) = error {
                    (
                        ShaderPreprocessor::new(ShaderPreprocessorContext {
                            defines: include_context.defines.clone(),
                        }),
                        ShaderSymbolList::default(),
                        ShaderDiagnosticList {
                            diagnostics: vec![ShaderDiagnostic {
                                severity: ShaderDiagnosticSeverity::Warning,
                                error: message,
                                range: range,
                            }],
                        },
                    )
                } else {
                    return Err(error);
                }
            }
        };
        let mut context_define = preprocessor_cache.defines.clone();
        let includes = preprocessor_cache.includes.clone();
        // Store symbol cache.
        include_context.visit_data(uri, cached_file, |data: &mut ServerFileCacheData| {
            data.preprocessor_cache = preprocessor_cache;
            data.symbol_cache = symbol_cache;
            data.diagnostic_cache = diagnostic_cache;
        });
        // Recurse dependencies.
        for include in &includes {
            let include_uri = Url::from_file_path(&include.absolute_path).unwrap();
            let included_file =
                self.watch_dependency(&include_uri, shading_language, symbol_provider)?;
            // Skip already visited deps if once.
            // If we skip this, we will need to fill data before.
            if include_context.should_visit(&include_uri) {
                include_context.add_context_for_include(include, &mut context_define);
                RefCell::borrow_mut(&included_file).included_data.insert(
                    include_context.includer_uri.clone(),
                    ServerFileCacheData::default(),
                );
                self.recurse_file_symbol(
                    &include_uri,
                    &included_file,
                    symbol_provider,
                    include_context,
                )?;
            }
            // Add deps to current file
            include_context.visit_data(uri, cached_file, |data: &mut ServerFileCacheData| {
                data.dependencies.insert(include_uri.clone(), included_file);
            });
        }
        // Add remaining context
        include_context.add_context(context_define);
        Ok(())
    }
    pub fn cache_file_data(
        &mut self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        validator: &mut dyn Validator,
        symbol_provider: &mut dyn SymbolProvider,
        shader_variant: Option<ShaderVariant>,
        config: &ServerConfig,
    ) -> Result<(), ShaderError> {
        // TODO: remove include context & replace it by includeHandler.
        // But it does not support uri though... Or include handler in include context.
        let mut symbol_params = config.into_symbol_params();
        let mut include_context = IncludeContext::main(
            uri,
            config.includes.clone(),
            config.defines.clone(),
            config.path_remapping.clone(),
        );
        // Reset cache
        include_context.visit_data(uri, cached_file, |data: &mut ServerFileCacheData| {
            data.preprocessor_cache = ShaderPreprocessor::default();
            data.symbol_cache = ShaderSymbolList::default();
            data.diagnostic_cache = ShaderDiagnosticList::default();
        });
        // Get symbols for main file.
        if config.symbols {
            profile_scope!("Parsing symbols for file {}", uri);
            // Add variant data if some.
            if let Some(variant) = &shader_variant {
                for (variable, value) in &variant.defines {
                    symbol_params
                        .defines
                        .insert(variable.clone(), value.clone());
                }
            }
            self.recurse_file_symbol(uri, cached_file, symbol_provider, &mut include_context)?;
        }
        // Get diagnostics
        if config.validate {
            profile_scope!("Validating file {}", uri);
            let shading_language = RefCell::borrow(cached_file).shading_language;
            let mut validation_params = config.into_validation_params();
            if let Some(variant) = &shader_variant {
                for (variable, value) in &variant.defines {
                    validation_params
                        .defines
                        .insert(variable.clone(), value.clone());
                }
            }
            let (mut diagnostic_list, _dependencies) = {
                profile_scope!("Raw validation");
                validator.validate_shader(
                    &RefCell::borrow(cached_file).symbol_tree.content,
                    RefCell::borrow(cached_file).symbol_tree.file_path.as_path(),
                    &validation_params,
                    &mut |deps_path: &Path| -> Option<String> {
                        let deps_uri = Url::from_file_path(deps_path).unwrap();
                        let deps_file = match self.get_dependency(&deps_uri) {
                            Some(deps_file) => deps_file,
                            None => {
                                if config.symbols {
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
                        // Add deps as direct deps if they werent added by symbols.
                        if !config.symbols {
                            include_context.visit_data(
                                uri,
                                cached_file,
                                |data: &mut ServerFileCacheData| {
                                    data.dependencies
                                        .insert(Url::from_file_path(&deps_path).unwrap(), deps_file);
                                },
                            );
                        }
                        Some(content)
                    },
                )?
            };
            // Clear diagnostic if no errors.
            // TODO: Should add empty for main file & deps if none to clear them.

            // Filter by severity.
            let required_severity = ShaderDiagnosticSeverity::from(config.severity.clone());
            diagnostic_list
                .diagnostics
                .retain(|e| e.severity.is_required(required_severity.clone()));

            // If includes have issues, diagnose them.
            fn ascend_dependency_error(
                includer_uri: &Url,
                uri: &Url,
                cached_file: &ServerFileCacheHandle,
                included_diagnostics: &Vec<PathBuf>,
                unique_deps: &mut HashSet<Url>,
            ) -> bool {
                unique_deps.insert(uri.clone());
                if included_diagnostics.contains(&uri.to_file_path().unwrap()) {
                    true
                } else {
                    match RefCell::borrow(&cached_file)
                        .included_data
                        .get(&includer_uri)
                    {
                        Some(data) => {
                            for (deps_uri, deps_file) in &data.dependencies {
                                if !unique_deps.contains(&deps_uri) {
                                    if ascend_dependency_error(
                                        includer_uri,
                                        deps_uri,
                                        deps_file,
                                        included_diagnostics,
                                        unique_deps,
                                    ) {
                                        return true;
                                    }
                                }
                            }
                            false
                        }
                        None => false,
                    }
                }
            }
            let included_diagnostics: Vec<PathBuf> = diagnostic_list
                .diagnostics
                .iter()
                .filter(|diag| diag.severity == ShaderDiagnosticSeverity::Error)
                .map(|diag| diag.range.start.file_path.clone())
                .collect();
            let mut ascended_diagnostics: Vec<ShaderDiagnostic> = RefCell::borrow(&cached_file)
                .data
                .preprocessor_cache
                .includes
                .iter()
                .filter_map(|include| {
                    let include_uri = Url::from_file_path(&include.absolute_path).unwrap();
                    match self.get_dependency(&include_uri) {
                        Some(include_file) => {
                            if ascend_dependency_error(
                                &uri,
                                &include_uri,
                                &include_file,
                                &included_diagnostics,
                                &mut HashSet::new(),
                            ) {
                                Some(ShaderDiagnostic {
                                    severity: ShaderDiagnosticSeverity::Error,
                                    error: format!("File {} has issues", include.relative_path),
                                    range: include.range.clone(),
                                })
                            } else {
                                None
                            }
                        }
                        None => Some(ShaderDiagnostic {
                            severity: ShaderDiagnosticSeverity::Error,
                            error: format!("Failed to get dependency {}", include.relative_path),
                            range: include.range.clone(),
                        }),
                    }
                })
                .collect();
            diagnostic_list
                .diagnostics
                .append(&mut ascended_diagnostics);

            include_context.visit_data(uri, cached_file, |data: &mut ServerFileCacheData| {
                data.diagnostic_cache
                    .diagnostics
                    .append(&mut data.preprocessor_cache.diagnostics);
                data.diagnostic_cache
                    .diagnostics
                    .append(&mut diagnostic_list.diagnostics);
            });
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
                    data: ServerFileCacheData::default(),
                    included_data: HashMap::new(),
                }));
                let none = self.files.insert(uri.clone(), Rc::clone(&cached_file));
                assert!(none.is_none());
                cached_file
            }
        };
        // Cache file data from new context.
        self.cache_file_data(
            uri,
            &cached_file,
            validator,
            symbol_provider,
            self.variants.get(&uri).cloned(),
            config,
        )?;
        info!(
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
                        data: ServerFileCacheData::default(),
                        included_data: HashMap::new(),
                    }));
                    let none = self
                        .dependencies
                        .insert(uri.clone(), Rc::clone(&cached_file));
                    assert!(none.is_none());
                    info!(
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
        range: Option<lsp_types::Range>,
        partial_content: Option<&String>,
    ) -> Result<(), ShaderError> {
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
    pub fn remove_file(&mut self, uri: &Url) -> Result<Vec<Url>, ShaderError> {
        fn list_all_dependencies_count(
            uri: &Url,
            cached_file: &ServerFileCacheHandle,
            includer_uri: &Url,
            unique_deps: &mut HashSet<Url>,
        ) -> HashMap<Url, usize> {
            unique_deps.insert(uri.clone());
            let mut list = HashMap::new();
            match RefCell::borrow(cached_file).included_data.get(includer_uri) {
                Some(data) => {
                    for (deps_uri, deps_cached_file) in &data.dependencies {
                        match list.get_mut(deps_uri) {
                            Some(count) => {
                                *count += 1;
                            }
                            None => {
                                list.insert(deps_uri.clone(), 1);
                            }
                        }
                        // Avoid stack overflow
                        if !unique_deps.contains(deps_uri) {
                            debug!("Recurse {}", deps_uri);
                            let deps = list_all_dependencies_count(
                                deps_uri,
                                deps_cached_file,
                                includer_uri,
                                unique_deps,
                            );
                            for (deps_deps_uri, deps_deps_count) in deps {
                                match list.get_mut(&deps_deps_uri) {
                                    Some(count) => {
                                        *count += deps_deps_count;
                                    }
                                    None => {
                                        list.insert(deps_deps_uri.clone(), deps_deps_count);
                                    }
                                }
                            }
                        }
                    }
                }
                None => {
                    warn!(
                        "Dependency {} has no data for includer {}",
                        uri, includer_uri
                    )
                }
            }
            list
        }
        match self.files.remove(uri) {
            Some(cached_file) => {
                debug!(
                    "Dependency tree:\n{}",
                    RefCell::borrow(&cached_file).data.dump_dependency_tree(uri)
                );
                let mut dependency_list = HashMap::new();
                for (dependency_url, dependency_file) in
                    &RefCell::borrow(&cached_file).data.dependencies
                {
                    match dependency_list.get_mut(dependency_url) {
                        Some(count) => {
                            *count += 1;
                        }
                        None => {
                            dependency_list.insert(dependency_url.clone(), 1);
                        }
                    }
                    let list = list_all_dependencies_count(
                        dependency_url,
                        dependency_file,
                        uri,
                        &mut HashSet::new(),
                    );
                    for (deps_uri, deps_count) in list {
                        match dependency_list.get_mut(&deps_uri) {
                            Some(count) => {
                                *count += deps_count;
                            }
                            None => {
                                dependency_list.insert(deps_uri, deps_count);
                            }
                        }
                    }
                }
                // Drop Rc to decrease refcount
                drop(cached_file);
                debug!(
                    "Removing main file at {}. {} files in cache.",
                    uri,
                    self.files.len(),
                );
                let mut deleted_files = Vec::new();
                for (dependency_uri, dependency_count) in dependency_list {
                    match self.dependencies.get(&dependency_uri) {
                        Some(dependency_file) => {
                            // Dont forget ref in self.files & self.dependencies
                            let deps_ref_count = 1;
                            let file_ref_count = self.files.contains_key(&dependency_uri) as usize;
                            if Rc::strong_count(dependency_file)
                                <= dependency_count + file_ref_count + deps_ref_count
                            {
                                // Remove dependency.
                                debug!(
                                    "Removing dependency file at {}. {} deps in cache.",
                                    dependency_uri,
                                    self.dependencies.len()
                                );
                                let dependency_file =
                                    self.dependencies.remove(&dependency_uri).unwrap();
                                drop(dependency_file);
                                deleted_files.push(dependency_uri.clone());
                            }
                        }
                        None => {
                            return Err(ShaderError::InternalErr(format!(
                                "Trying to remove dependency file {} that is not watched",
                                dependency_uri
                            )))
                        }
                    }
                }
                // Clear leaking dependencies
                // This means there is issues with them.
                let mut leaking_deps = Vec::new();
                for (dependency_url, dependency_file) in &self.dependencies {
                    if Rc::strong_count(dependency_file) == 1 {
                        warn!("Dependency {} is leaking.", dependency_url);
                        leaking_deps.push(dependency_url.clone());
                    }
                }
                for leaking_deps in &leaking_deps {
                    self.dependencies.remove(leaking_deps);
                    deleted_files.push(leaking_deps.clone());
                }
                // Check removed file still used as deps.
                match self.dependencies.get(uri) {
                    Some(_) => Ok(deleted_files),
                    None => {
                        deleted_files.push(uri.clone());
                        Ok(deleted_files)
                    }
                }
            }
            None => Err(ShaderError::InternalErr(format!(
                "Trying to remove main file {} that is not watched",
                uri.path()
            ))),
        }
    }
    pub fn get_all_symbols(
        &self,
        uri: &Url,
        cached_file: &ServerFileCacheHandle,
        symbol_provider: &dyn SymbolProvider,
    ) -> ShaderSymbolList {
        let cached_file = RefCell::borrow(&cached_file);
        // Add main file symbols
        let mut symbol_cache = cached_file.data.symbol_cache.clone();
        cached_file
            .data
            .preprocessor_cache
            .preprocess_symbols(&mut symbol_cache);
        // Add deps symbols
        fn get_deps(
            uri: &Url,
            cached_file: &ServerFileCacheHandle,
            includer_uri: &Url,
            include_context: &mut IncludeContext,
        ) -> ShaderSymbolList {
            let cached_file = RefCell::borrow(&cached_file);
            match cached_file.included_data.get(includer_uri) {
                Some(data) => {
                    let mut symbol_cache = data.symbol_cache.clone();
                    data.preprocessor_cache
                        .preprocess_symbols(&mut symbol_cache);
                    for (deps_uri, deps_cached_file) in &data.dependencies {
                        if include_context.should_visit(deps_uri) {
                            // Dont need to add_context_for_include here, bcz we only care about should_visit.
                            symbol_cache.append(get_deps(
                                deps_uri,
                                deps_cached_file,
                                includer_uri,
                                include_context,
                            ));
                        }
                    }
                    symbol_cache
                }
                None => {
                    warn!(
                        "Deps {} get_all_symbols has no data for includer {}",
                        uri, includer_uri
                    );
                    ShaderSymbolList::default()
                }
            }
        }
        for (deps_uri, deps_cached_file) in &cached_file.data.dependencies {
            symbol_cache.append(get_deps(
                deps_uri,
                deps_cached_file,
                uri,
                &mut IncludeContext::main(uri, Vec::new(), HashMap::new(), HashMap::new()),
            ));
        }
        // Add config symbols
        for (key, value) in &cached_file.data.preprocessor_cache.context.defines {
            symbol_cache.macros.push(ShaderSymbol {
                label: key.clone(),
                description: format!(
                    "Config preprocessor macro. Expanding to \n```\n{}\n```",
                    value
                ),
                version: "".into(),
                stages: vec![],
                link: None,
                data: ShaderSymbolData::Macro {
                    value: value.clone(),
                },
                range: None,
                scope_stack: None,
            });
        }
        // Add intrinsics symbols
        symbol_cache.append(symbol_provider.get_intrinsics_symbol().clone());
        symbol_cache
    }
}
