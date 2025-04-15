pub mod include;
pub mod shader;
pub mod shader_error;
pub mod symbols;
pub mod validator;

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
    };

    use crate::include::IncludeHandler;

    fn validate_include(path: &Path) -> bool {
        let file_path = Path::new("./test/hlsl/dontcare.hlsl");
        let mut include_handler = IncludeHandler::new(
            file_path,
            vec![],
            HashMap::from([
                (
                    PathBuf::from("/Packages"),
                    PathBuf::from("./test/hlsl/inc0/inc1"),
                ),
                (
                    PathBuf::from("Packages"),
                    PathBuf::from("./test/hlsl/inc0/inc1"),
                ),
                (
                    PathBuf::from("Using\\Backslashes"),
                    PathBuf::from("./test/hlsl/inc0/inc1"),
                ),
            ]),
        );
        include_handler.search_path_in_includes(path).is_some()
    }

    #[test]
    fn test_virtual_path() {
        assert!(
            validate_include(Path::new("/Packages/level1.hlsl")),
            "Virtual path with prefix failed."
        );
        assert!(
            validate_include(Path::new("Packages/level1.hlsl")),
            "Virtual path without prefix failed."
        );
        #[cfg(target_os = "windows")] // Only windows support backslashes.
        assert!(
            validate_include(Path::new("Using/Backslashes/level1.hlsl")),
            "Virtual path with backslash failed."
        );
    }

    #[test]
    fn test_directory_stack() {
        let file_path = Path::new("./test/hlsl/include-level.hlsl");
        let mut include_handler = IncludeHandler::new(file_path, vec![], HashMap::new());
        assert!(include_handler
            .search_path_in_includes(Path::new("./inc0/level0.hlsl"))
            .is_some());
        assert!(include_handler
            .search_path_in_includes(Path::new("./inc1/level1.hlsl"))
            .is_some());
    }
}
