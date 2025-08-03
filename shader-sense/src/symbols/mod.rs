mod glsl;
mod hlsl;
pub mod shader_language;
mod symbol_parser;
pub mod symbol_provider;
pub mod symbol_tree;
pub mod symbols;
mod wgsl;

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
    };

    use regex::Regex;

    use crate::{
        include::IncludeHandler,
        shader::{
            GlslShadingLanguageTag, HlslShadingLanguageTag, ShadingLanguage, ShadingLanguageTag,
            WgslShadingLanguageTag,
        },
        shader_error::ShaderError,
        symbols::{
            shader_language::ShaderLanguage,
            symbol_provider::ShaderSymbolParams,
            symbols::{ShaderPosition, ShaderRange, ShaderSymbolData},
        },
    };

    use super::{
        symbol_provider::{default_include_callback, SymbolProvider},
        symbols::ShaderSymbolList,
    };

    pub fn find_file_dependencies(
        include_handler: &mut IncludeHandler,
        shader_content: &String,
    ) -> Vec<PathBuf> {
        let include_regex = Regex::new("\\#include\\s+\"([\\w\\s\\\\/\\.\\-]+)\"").unwrap();
        let dependencies_paths: Vec<&str> = include_regex
            .captures_iter(&shader_content)
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        dependencies_paths
            .iter()
            .filter_map(|dependency| include_handler.search_path_in_includes(Path::new(dependency)))
            .collect::<Vec<PathBuf>>()
    }
    pub fn find_dependencies(
        include_handler: &mut IncludeHandler,
        shader_content: &String,
    ) -> HashSet<(String, PathBuf)> {
        let dependencies_path = find_file_dependencies(include_handler, shader_content);
        let dependencies = dependencies_path
            .into_iter()
            .map(|e| (std::fs::read_to_string(&e).unwrap(), e))
            .collect::<Vec<(String, PathBuf)>>();

        // Use hashset to avoid computing dependencies twice.
        let mut recursed_dependencies = HashSet::new();
        for dependency in &dependencies {
            recursed_dependencies.extend(find_dependencies(include_handler, &dependency.0));
        }
        recursed_dependencies.extend(dependencies);

        recursed_dependencies
    }

    fn get_all_preprocessed_symbols<T: ShadingLanguageTag>(
        language: &mut ShaderLanguage,
        symbol_provider: &SymbolProvider,
        file_path: &Path,
        shader_content: &String,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let mut include_handler = IncludeHandler::main_without_config(&file_path);
        let deps = find_dependencies(&mut include_handler, &shader_content);
        let mut all_symbols = language.get_intrinsics_symbol().clone();
        let symbol_tree = language.create_module(file_path, shader_content).unwrap();
        let symbols = symbol_provider
            .query_symbols(
                &symbol_tree,
                ShaderSymbolParams::default(),
                &mut default_include_callback::<T>,
                None,
            )
            .unwrap();
        let symbols = symbols.get_all_symbols();
        all_symbols.append(symbols.into());
        for dep in deps {
            let symbol_tree = language.create_module(&dep.1, &dep.0).unwrap();
            let symbols = symbol_provider
                .query_symbols(
                    &symbol_tree,
                    ShaderSymbolParams::default(),
                    &mut default_include_callback::<T>,
                    None,
                )
                .unwrap();
            let symbols = symbols.get_all_symbols();
            all_symbols.append(symbols.into());
        }
        Ok(all_symbols)
    }

    #[test]
    fn intrinsics_glsl_ok() {
        // Ensure parsing of intrinsics is OK
        let _ = ShaderSymbolList::parse_from_json(String::from(include_str!(
            "glsl/glsl-intrinsics.json"
        )));
    }
    #[test]
    fn intrinsics_hlsl_ok() {
        // Ensure parsing of intrinsics is OK
        let _ = ShaderSymbolList::parse_from_json(String::from(include_str!(
            "hlsl/hlsl-intrinsics.json"
        )));
    }
    #[test]
    fn intrinsics_wgsl_ok() {
        // Ensure parsing of intrinsics is OK
        let _ = ShaderSymbolList::parse_from_json(String::from(include_str!(
            "wgsl/wgsl-intrinsics.json"
        )));
    }
    #[test]
    fn symbols_glsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/glsl/include-level.comp.glsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut language = ShaderLanguage::new(ShadingLanguage::Glsl);
        let symbol_provider = language.create_symbol_provider();
        let symbol_tree = language.create_module(file_path, &shader_content).unwrap();
        let symbols = symbol_provider
            .query_symbols(
                &symbol_tree,
                ShaderSymbolParams::default(),
                &mut default_include_callback::<GlslShadingLanguageTag>,
                None,
            )
            .unwrap();
        let symbols = symbols.get_all_symbols();
        assert!(!symbols.functions.is_empty());
    }
    #[test]
    fn symbols_hlsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/hlsl/include-level.hlsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut language = ShaderLanguage::new(ShadingLanguage::Hlsl);
        let symbol_provider = language.create_symbol_provider();
        let symbol_tree = language.create_module(file_path, &shader_content).unwrap();
        let symbols = symbol_provider
            .query_symbols(
                &symbol_tree,
                ShaderSymbolParams::default(),
                &mut default_include_callback::<HlslShadingLanguageTag>,
                None,
            )
            .unwrap();
        let symbols = symbols.get_all_symbols();
        assert!(!symbols.functions.is_empty());
    }
    #[test]
    fn symbols_wgsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/wgsl/ok.wgsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut language = ShaderLanguage::new(ShadingLanguage::Wgsl);
        let symbol_provider = language.create_symbol_provider();
        let symbol_tree = language.create_module(file_path, &shader_content).unwrap();
        let symbols = symbol_provider
            .query_symbols(
                &symbol_tree,
                ShaderSymbolParams::default(),
                &mut default_include_callback::<WgslShadingLanguageTag>,
                None,
            )
            .unwrap();
        let symbols = symbols.get_all_symbols();
        assert!(symbols.functions.is_empty());
    }
    #[test]
    fn symbol_scope_glsl_ok() {
        let file_path = Path::new("./test/glsl/scopes.frag.glsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut language = ShaderLanguage::new(ShadingLanguage::Glsl);
        let symbol_provider = language.create_symbol_provider();
        let preprocessed_symbol_list = get_all_preprocessed_symbols::<GlslShadingLanguageTag>(
            &mut language,
            &symbol_provider,
            file_path,
            &shader_content,
        )
        .unwrap();
        let symbol_list = preprocessed_symbol_list.as_ref();
        let symbols = symbol_list.filter_scoped_symbol(&ShaderPosition {
            file_path: PathBuf::from(file_path),
            line: 16,
            pos: 0,
        });
        let variables_visibles: Vec<String> = vec![
            "scopeRoot".into(),
            "scope1".into(),
            "scopeGlobal".into(),
            "level1".into(),
        ];
        let variables_not_visibles: Vec<String> = vec!["scope2".into(), "testData".into()];
        for variable_visible in variables_visibles {
            assert!(
                symbols
                    .variables
                    .iter()
                    .any(|e| e.label == variable_visible),
                "Failed to find variable {} {:#?}",
                variable_visible,
                symbols.variables
            );
        }
        for variable_not_visible in variables_not_visibles {
            assert!(
                !symbols
                    .variables
                    .iter()
                    .any(|e| e.label == variable_not_visible),
                "Found variable {}",
                variable_not_visible
            );
        }
    }
    #[test]
    fn uniform_glsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/glsl/uniforms.frag.glsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut language = ShaderLanguage::new(ShadingLanguage::Glsl);
        let symbol_provider = language.create_symbol_provider();
        let symbol_tree = language.create_module(file_path, &shader_content).unwrap();
        let symbols = symbol_provider
            .query_symbols(
                &symbol_tree,
                ShaderSymbolParams::default(),
                &mut default_include_callback::<GlslShadingLanguageTag>,
                None,
            )
            .unwrap();
        let symbols = symbols.get_all_symbols();
        assert!(symbols
            .types
            .iter()
            .find(|e| e.label == "MatrixHidden")
            .is_some());
        assert!(symbols
            .variables
            .iter()
            .find(|e| e.label == "u_accessor"
                && match &e.data {
                    ShaderSymbolData::Variables { ty, count: _ } => ty == "MatrixHidden",
                    _ => false,
                })
            .is_some());
        assert!(symbols
            .variables
            .iter()
            .find(|e| e.label == "u_modelviewGlobal")
            .is_some());
        assert!(symbols
            .variables
            .iter()
            .find(|e| e.label == "u_modelviewHidden")
            .is_none());
    }
    #[test]
    fn test_position_conversion() {
        fn test_to_byte_offset(
            shader_content: &str,
            expected_content: &str,
            position: &ShaderPosition,
        ) -> usize {
            let byte_offset = position.to_byte_offset(&shader_content).unwrap();
            if expected_content.len() > 0 {
                let content_from_offset = &shader_content[byte_offset..];
                assert!(content_from_offset.len() >= expected_content.len());
                assert!(
                    content_from_offset == expected_content,
                    "Offseted content {:?} with offset {} is incorrect.",
                    &shader_content[byte_offset..],
                    byte_offset
                );
            } else {
                assert!(byte_offset == shader_content.len());
            }
            byte_offset
        }
        fn test_back_to_position(
            shader_content: &str,
            expected_position: &ShaderPosition,
            byte_offset: usize,
        ) {
            let converted_position = ShaderPosition::from_byte_offset(
                &shader_content,
                byte_offset,
                &expected_position.file_path,
            )
            .unwrap();
            let converted_byte_offset = converted_position.to_byte_offset(&shader_content).unwrap();
            assert!(converted_position == *expected_position, "Position {:#?} with byte offset {} is different from converted position: {:#?} with byte offset {}", expected_position, byte_offset, converted_position, converted_byte_offset);
        }

        // Testing file
        let utf8_file_path = Path::new("./test/hlsl/utf8.hlsl");
        let utf8_shader_content = std::fs::read_to_string(utf8_file_path).unwrap();

        #[cfg(windows)]
        const EOL: &str = "\r\n";
        #[cfg(not(windows))]
        const EOL: &str = "\n";
        let test_data = vec![
            (
                "\n}".replace("\n", EOL),
                ShaderPosition::new(utf8_file_path.into(), 5, 0),
                &utf8_shader_content,
            ),
            (
                "".replace("\n", EOL),
                ShaderPosition::new(utf8_file_path.into(), 6, 1),
                &utf8_shader_content,
            ),
            (
                "id main() {\n\n}".replace("\n", EOL),
                ShaderPosition::new(utf8_file_path.into(), 4, 2),
                &utf8_shader_content,
            ),
            (
                "にちは世界!\n\nvoid main() {\n\n}".replace("\n", EOL),
                ShaderPosition::new(utf8_file_path.into(), 2, 5),
                &utf8_shader_content,
            ),
        ];
        for (index, (expected_content, position, shader_content)) in test_data.iter().enumerate() {
            println!("Testing conversion {} for {:?}", index, position);
            println!(
                "Content: {:?} (len {})",
                shader_content,
                shader_content.len()
            );
            let byte_offset = test_to_byte_offset(&shader_content, expected_content, &position);
            println!("Found byte_offset {}", byte_offset);
            test_back_to_position(&shader_content, &position, byte_offset);
        }
    }
    #[test]
    fn test_end_range() {
        let file_path = Path::new("./test/hlsl/utf8.hlsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let range = ShaderRange::whole(file_path.into(), &shader_content);
        println!("File range: {:#?}", range);
        let end_byte_offset = range.end.to_byte_offset(&shader_content).unwrap();
        assert!(end_byte_offset == shader_content.len());
    }
}
