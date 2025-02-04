use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use crate::server::{
    clean_url,
    common::{lsp_range_to_shader_range, read_string_lossy},
};
use log::debug;
use lsp_types::Url;
use shader_sense::{
    shader::ShadingLanguage,
    shader_error::ShaderError,
    symbols::{
        symbol_provider::SymbolProvider,
        symbol_tree::SymbolTree,
        symbols::{ShaderPreprocessor, ShaderSymbolList},
    },
};

use super::server_config::ServerConfig;

pub type ServerFileCacheHandle = Rc<RefCell<ServerFileCache>>;

#[derive(Debug, Clone)]
pub struct ServerFileCache {
    pub shading_language: ShadingLanguage,
    pub symbol_tree: SymbolTree, // Store content on change as its not on disk.
    pub preprocessor_cache: ShaderPreprocessor, // Store preprocessor to avoid computing them at every change.
    pub symbol_cache: ShaderSymbolList, // Store symbol to avoid computing them at every change.
    pub dependencies: HashMap<PathBuf, ServerFileCacheHandle>, // Store all dependencies of this file.
}

pub struct ServerLanguageFileCache {
    pub files: HashMap<Url, ServerFileCacheHandle>,
    pub dependencies: HashMap<Url, ServerFileCacheHandle>,
}

impl ServerFileCache {
    pub fn update(
        &mut self,
        uri: &Url,
        symbol_provider: &mut dyn SymbolProvider,
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
            self.symbol_tree
                .update_partial(symbol_provider, &shader_range, &partial_content)?;
        } else if let Some(whole_content) = partial_content {
            self.symbol_tree.update(symbol_provider, &whole_content)?;
        } else {
            // No update on content to perform.
        }
        debug!(
            "{}:timing:update:ast           {}ms",
            file_path.display(),
            now_update_ast.elapsed().as_millis()
        );

        let now_get_preproc = std::time::Instant::now();
        // Cache symbols
        self.preprocessor_cache =
            symbol_provider.query_preprocessor(&self.symbol_tree, &config.into_symbol_params())?;
        debug!(
            "{}:timing:update:get_preproc   {}ms",
            file_path.display(),
            now_get_preproc.elapsed().as_millis()
        );
        let now_get_symbol = std::time::Instant::now();
        self.symbol_cache = if config.symbols {
            symbol_provider.query_file_symbols(&self.symbol_tree, Some(&self.preprocessor_cache))?
        } else {
            ShaderSymbolList::default()
        };
        debug!(
            "{}:timing:update:get_all_symb  {}ms",
            file_path.display(),
            now_get_symbol.elapsed().as_millis()
        );
        debug!(
            "{}:timing:update:              {}ms",
            file_path.display(),
            now_start.elapsed().as_millis()
        );
        Ok(())
    }
}
impl ServerLanguageFileCache {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }
    pub fn watch_file(
        &mut self,
        uri: &Url,
        lang: ShadingLanguage,
        text: &String,
        symbol_provider: &mut dyn SymbolProvider,
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
                let preprocessor_cache = symbol_provider
                    .query_preprocessor(&symbol_tree, &config.into_symbol_params())?;
                let symbol_list =
                    symbol_provider.query_file_symbols(&symbol_tree, Some(&preprocessor_cache))?;
                let cached_file = Rc::new(RefCell::new(ServerFileCache {
                    shading_language: lang,
                    symbol_tree: symbol_tree,
                    preprocessor_cache,
                    symbol_cache: if config.symbols {
                        symbol_list
                    } else {
                        ShaderSymbolList::default()
                    },
                    dependencies: HashMap::new(), // Will be filled by validator.
                }));
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
        config: &ServerConfig,
    ) -> Result<ServerFileCacheHandle, ShaderError> {
        assert!(*uri == clean_url(&uri));
        let file_path = uri.to_file_path().unwrap();

        // Check watched file already watched as deps
        match self.files.get(&uri) {
            Some(cached_file) => {
                // Watched as main, copy it.
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
                    let preprocessor_cache = symbol_provider
                        .query_preprocessor(&symbol_tree, &config.into_symbol_params())?;
                    let symbol_list = symbol_provider
                        .query_file_symbols(&symbol_tree, Some(&preprocessor_cache))?;
                    let cached_file = Rc::new(RefCell::new(ServerFileCache {
                        shading_language: lang,
                        preprocessor_cache,
                        symbol_tree: symbol_tree,
                        symbol_cache: if config.symbols {
                            symbol_list
                        } else {
                            ShaderSymbolList::default()
                        },
                        dependencies: HashMap::new(), // Will be filled by validator.
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
