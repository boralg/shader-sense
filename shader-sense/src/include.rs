use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Clone)]
pub struct Dependencies {
    dependencies: HashSet<PathBuf>,
}

pub struct IncludeHandler {
    includes: Vec<String>,
    directory_stack: HashSet<PathBuf>,
    dependencies: Dependencies,                // TODO: Remove
    path_remapping: HashMap<PathBuf, PathBuf>, // remapping of path / virtual path
}
// std::fs::canonicalize not supported on wasi target... Emulate it.
// On Windows, std::fs::canonicalize return a /? prefix that break hashmap.
// https://stackoverflow.com/questions/50322817/how-do-i-remove-the-prefix-from-a-canonical-windows-path
// Instead use a custom canonicalize.
pub fn canonicalize(p: &Path) -> std::io::Result<PathBuf> {
    // https://github.com/antmicro/wasi_ext_lib/blob/main/canonicalize.patch
    fn __canonicalize(path: &Path, buf: &mut PathBuf) {
        if path.is_absolute() {
            buf.clear();
        }
        for part in path {
            if part == ".." {
                buf.pop();
            } else if part != "." {
                buf.push(part);
                if let Ok(linkpath) = buf.read_link() {
                    buf.pop();
                    __canonicalize(&linkpath, buf);
                }
            }
        }
    }
    let mut path = if p.is_absolute() {
        PathBuf::new()
    } else {
        PathBuf::from(std::env::current_dir()?)
    };
    __canonicalize(p, &mut path);
    Ok(path)
}

impl Dependencies {
    pub fn new() -> Self {
        Self {
            dependencies: HashSet::new(),
        }
    }
    pub fn add_dependency(&mut self, relative_path: PathBuf) {
        self.dependencies.insert(
            canonicalize(&relative_path).expect("Failed to convert dependency path to absolute"),
        );
    }
    pub fn visit_dependencies<F: FnMut(&Path)>(&self, callback: &mut F) {
        for dependency in &self.dependencies {
            callback(&dependency);
        }
    }
}

impl IncludeHandler {
    pub fn new(
        file: &Path,
        includes: Vec<String>,
        path_remapping: HashMap<PathBuf, PathBuf>,
    ) -> Self {
        // Add local path to include path
        let mut includes_mut = includes;
        let cwd = file.parent().unwrap();
        let str = String::from(cwd.to_string_lossy());
        // TODO: push cwd in first. Or move it elsewhere
        includes_mut.push(str);
        Self {
            includes: includes_mut,
            directory_stack: HashSet::new(),
            dependencies: Dependencies::new(),
            path_remapping: path_remapping,
        }
    }
    pub fn search_in_includes(
        &mut self,
        relative_path: &Path,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Option<(String, PathBuf)> {
        match self.search_path_in_includes(relative_path) {
            Some(absolute_path) => include_callback(&absolute_path).map(|e| (e, absolute_path)),
            None => None,
        }
    }
    pub fn search_path_in_includes(&mut self, relative_path: &Path) -> Option<PathBuf> {
        self.search_path_in_includes_relative(relative_path)
            .map(|e| canonicalize(&e).expect("Failed to convert relative path to absolute"))
    }
    pub fn search_path_in_includes_relative(&mut self, relative_path: &Path) -> Option<PathBuf> {
        // Checking for file existence is a bit costly.
        // Some options are available and have been tested
        // - path.exists(): approximatively 100us
        // - path.is_file(): approximatively 40us
        // - std::fs::exists(&path).unwrap_or(false): approximatively 40us but only stable with Rust>1.81
        if relative_path.exists() {
            Some(PathBuf::from(relative_path))
        } else {
            // Check directory stack.
            for directory_stack in &self.directory_stack {
                let path = Path::new(directory_stack).join(&relative_path);
                if path.is_file() {
                    if let Some(parent) = path.parent() {
                        self.directory_stack.insert(PathBuf::from(parent));
                    }
                    self.dependencies.add_dependency(path.clone());
                    return Some(path);
                }
            }
            // Check include paths
            for include_path in &self.includes {
                let path = Path::new(include_path).join(&relative_path);
                if path.is_file() {
                    if let Some(parent) = path.parent() {
                        self.directory_stack.insert(PathBuf::from(parent));
                    }
                    self.dependencies.add_dependency(path.clone());
                    return Some(path);
                }
            }
            // Check virtual paths
            if let Some(target_path) =
                Self::resolve_virtual_path(relative_path, &self.path_remapping)
            {
                if target_path.is_file() {
                    if let Some(parent) = target_path.parent() {
                        self.directory_stack.insert(PathBuf::from(parent));
                    }
                    self.dependencies.add_dependency(target_path.clone());
                    return Some(target_path);
                }
            }
            return None;
        }
    }
    fn resolve_virtual_path(
        virtual_path: &Path,
        virtual_folders: &HashMap<PathBuf, PathBuf>,
    ) -> Option<PathBuf> {
        // Virtual path need to start with /
        // Dxc automatically insert .\ in front of path that are not absolute.
        // We should simply strip it, but how do we know its a virtual path or a real relative path ?
        // Instead dirty hack to remove it and try to load it, as its the last step of include, should be fine...
        let virtual_path = if virtual_path.starts_with("./") || virtual_path.starts_with(".\\") {
            let mut comp = virtual_path.components();
            comp.next();
            Path::new("/").join(comp.as_path())
        } else {
            PathBuf::from(virtual_path)
        };
        if virtual_path.starts_with("/") || virtual_path.starts_with("\\") {
            // Browse possible mapping & find a match.
            for (virtual_folder, target_path) in virtual_folders {
                let mut path_components = virtual_path.components();
                let mut found = true;
                for virtual_folder_component in virtual_folder.components() {
                    match path_components.next() {
                        Some(component) => {
                            if component != virtual_folder_component {
                                found = false;
                                break;
                            }
                        }
                        None => {
                            found = false;
                            break;
                        }
                    }
                }
                if found {
                    let resolved_path = target_path.join(path_components.as_path());
                    return Some(resolved_path.into());
                }
            }
            None
        } else {
            None
        }
    }
    pub fn get_dependencies(&self) -> &Dependencies {
        return &self.dependencies;
    }
}
