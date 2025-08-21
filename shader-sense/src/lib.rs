pub mod include;
pub mod position;
pub mod shader;
pub mod shader_error;
pub mod symbols;
pub mod validator;

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        collections::HashMap,
        path::{Path, PathBuf},
        rc::Rc,
    };

    use crate::{
        include::{canonicalize, IncludeHandler},
        shader::{ShaderParams, ShadingLanguage},
        symbols::{shader_module_parser::ShaderModuleParser, symbol_provider::SymbolProvider},
        validator::validator::Validator,
    };

    fn validate_include(path: &Path) -> bool {
        let file_path = Path::new("./test/hlsl/dontcare.hlsl");
        let mut include_handler = IncludeHandler::main(
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
        let mut include_handler = IncludeHandler::main(file_path, vec![], HashMap::new());
        let absolute_level0 =
            include_handler.search_path_in_includes(Path::new("./inc0/level0.hlsl"));
        assert!(absolute_level0.is_some());
        include_handler.push_directory_stack(&absolute_level0.unwrap());
        let absolute_level1 =
            include_handler.search_path_in_includes(Path::new("./inc1/level1.hlsl"));
        assert!(absolute_level1.is_some());
    }

    #[test]
    fn test_stack_overflow() {
        // Should handle include stack overflow gracefully.
        let file_path = Path::new("./test/hlsl/stack-overflow.hlsl");
        let mut shader_module_parser =
            ShaderModuleParser::from_shading_language(ShadingLanguage::Hlsl);
        let symbol_provider = SymbolProvider::from_shading_language(ShadingLanguage::Hlsl);
        let shader_module = shader_module_parser
            .create_module(file_path, &std::fs::read_to_string(file_path).unwrap())
            .unwrap();
        println!("Testing symbol overflow");
        let mut depth = 0;
        match symbol_provider.query_symbols(
            &shader_module,
            ShaderParams::default(),
            &mut |include| {
                depth += 1;
                println!(
                    "Including {} (depth {})",
                    include.get_absolute_path().display(),
                    depth
                );
                Ok(Some(Rc::new(RefCell::new(
                    shader_module_parser
                        .create_module(
                            &include.get_absolute_path(),
                            &std::fs::read_to_string(&include.get_absolute_path()).unwrap(),
                        )
                        .unwrap(),
                ))))
            },
            None,
        ) {
            Ok(_) => {}
            Err(err) => panic!("Failed to query symbols: {}", err),
        }
        println!("Testing validation overflow");
        let validator = Validator::from_shading_language(ShadingLanguage::Hlsl);
        match validator.validate_shader(
            &shader_module.content,
            file_path,
            &ShaderParams::default(),
            &mut |path| Some(std::fs::read_to_string(path).unwrap()),
        ) {
            Ok(diagnostics) => assert!(
                !diagnostics.is_empty(),
                "Diagnostics are empty but should not be."
            ),
            Err(err) => panic!("Failed to validate shader: {}", err),
        }
    }
    #[test]
    fn test_canonicalize_parent() {
        if cfg!(target_os = "windows") {
            let path = canonicalize(Path::new("D:\\test\\data")).unwrap();
            assert!(path == Path::new("D:\\test\\data"));
            assert!(path.parent().unwrap() == Path::new("D:\\test"));
        } else {
            let path = canonicalize(Path::new("/test/data")).unwrap();
            assert!(path == Path::new("/test/data"));
            assert!(path.parent().unwrap() == Path::new("/test"));
        }
    }
    #[test]
    fn test_canonicalize_join() {
        if cfg!(target_os = "windows") {
            let path = canonicalize(Path::new("D:\\test")).unwrap();
            assert!(path == Path::new("D:\\test"));
            assert!(path.join("data") == Path::new("D:\\test\\data"));
        } else {
            let path = canonicalize(Path::new("/test")).unwrap();
            assert!(path == Path::new("/test"));
            assert!(path.join("data") == Path::new("/test/data"));
        }
    }
}
