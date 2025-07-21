use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Clone)]
pub struct IncludeHandler {
    includes: HashSet<PathBuf>, // Dont store it stack to compute them before.
    directory_stack: Vec<PathBuf>, // Vec for keeping insertion order. Might own duplicate.
    visited_dependencies: HashMap<PathBuf, usize>,
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
                // read_link here is heavy.
                // Is it heavier than std::fs::canonicalize though ?
                // Check Dunce aswell.
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

impl IncludeHandler {
    pub const DEPTH_LIMIT: usize = 30;

    pub fn main_without_config(file: &Path) -> Self {
        Self::main(file, Vec::new(), HashMap::new())
    }
    pub fn main(
        file_path: &Path,
        includes: Vec<String>,
        path_remapping: HashMap<PathBuf, PathBuf>,
    ) -> Self {
        // Add local path to directory stack
        let cwd = file_path.parent().unwrap();
        let mut directory_stack = Vec::new();
        directory_stack.push(cwd.into());
        let mut visited_dependencies = HashMap::new();
        visited_dependencies.insert(file_path.into(), 1);
        Self {
            includes: includes
                .into_iter()
                .map(|s| canonicalize(Path::new(&s)).unwrap())
                .collect(),
            directory_stack: directory_stack,
            visited_dependencies: visited_dependencies,
            path_remapping: path_remapping,
        }
    }
    pub fn get_visited_count(&self, path: &Path) -> usize {
        self.visited_dependencies.get(path).cloned().unwrap_or(0)
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
            .map(|e| {
                // Canonicalize path.
                let path = canonicalize(&e).unwrap();
                // Add the parent to the stack
                match self.visited_dependencies.get_mut(&path) {
                    Some(visited_dependency_count) => *visited_dependency_count += 1,
                    None => {
                        self.visited_dependencies.insert(path.clone(), 1);
                        if let Some(parent) = path.parent() {
                            self.directory_stack.push(parent.into());
                        }
                    }
                }
                path
            })
    }
    fn search_path_in_includes_relative(&self, relative_path: &Path) -> Option<PathBuf> {
        // Checking for file existence is a bit costly.
        // Some options are available and have been tested
        // - path.exists(): approximatively 100us
        // - path.is_file(): approximatively 40us
        // - std::fs::exists(&path).unwrap_or(false): approximatively 40us but only stable with Rust>1.81
        if relative_path.is_file() {
            Some(PathBuf::from(relative_path))
        } else {
            // Check directory stack.
            // Reverse order to check first the latest added folders.
            // Might own duplicate, should use an ordered hashset instead.
            for directory_stack in self.directory_stack.iter().rev() {
                let path = directory_stack.join(&relative_path);
                if path.is_file() {
                    return Some(path);
                }
            }
            // Check include paths
            for include_path in &self.includes {
                let path = include_path.join(&relative_path);
                if path.is_file() {
                    return Some(path);
                }
            }
            // Check virtual paths
            if let Some(target_path) =
                Self::resolve_virtual_path(relative_path, &self.path_remapping)
            {
                if target_path.is_file() {
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
    }
}
