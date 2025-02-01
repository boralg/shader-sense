mod glsl;
mod hlsl;
mod symbol_parser;
pub mod symbol_provider;
pub mod symbol_tree;
pub mod symbols;
mod wgsl;

pub use glsl::GlslSymbolProvider;
pub use hlsl::HlslSymbolProvider;
use symbol_provider::SymbolProvider;
pub use wgsl::WgslSymbolProvider;

use crate::shader::ShadingLanguage;

pub fn create_symbol_provider(shading_language: ShadingLanguage) -> Box<dyn SymbolProvider> {
    match shading_language {
        ShadingLanguage::Wgsl => Box::new(WgslSymbolProvider::new()),
        ShadingLanguage::Hlsl => Box::new(HlslSymbolProvider::new()),
        ShadingLanguage::Glsl => Box::new(GlslSymbolProvider::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
    };

    use regex::Regex;

    use crate::{
        include::IncludeHandler, shader::ShadingLanguage, shader_error::ShaderError, symbols::symbols::ShaderPosition
    };

    use super::{
        create_symbol_provider, symbol_provider::SymbolProvider, symbol_tree::SymbolTree,
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

    fn get_all_symbols(
        symbol_provider: &mut dyn SymbolProvider,
        file_path: &Path,
        shader_content: &String,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let mut include_handler = IncludeHandler::new(&file_path, vec![]);
        let deps = find_dependencies(&mut include_handler, &shader_content);
        let mut symbols = symbol_provider.get_intrinsics_symbol().clone();
        let symbol_tree = SymbolTree::new(symbol_provider, file_path, shader_content).unwrap();
        symbols.append(symbol_provider.query_file_symbols(&symbol_tree, None)?);
        for dep in deps {
            let symbol_tree = SymbolTree::new(symbol_provider, &dep.1, &dep.0).unwrap();
            symbols.append(symbol_provider.query_file_symbols(&symbol_tree, None)?);
        }
        Ok(symbols)
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
        let mut symbol_provider = create_symbol_provider(ShadingLanguage::Glsl);
        let symbol_tree =
            SymbolTree::new(symbol_provider.as_mut(), file_path, &shader_content).unwrap();
        let symbols = symbol_provider.query_file_symbols(&symbol_tree, None).unwrap();
        assert!(!symbols.functions.is_empty());
    }
    #[test]
    fn symbols_hlsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/hlsl/include-level.hlsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut symbol_provider = create_symbol_provider(ShadingLanguage::Hlsl);
        let symbol_tree =
            SymbolTree::new(symbol_provider.as_mut(), file_path, &shader_content).unwrap();
        let symbols = symbol_provider.query_file_symbols(&symbol_tree, None).unwrap();
        assert!(!symbols.functions.is_empty());
    }
    #[test]
    fn symbols_wgsl_ok() {
        // Ensure parsing of symbols is OK
        let file_path = Path::new("./test/wgsl/ok.wgsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut symbol_provider = create_symbol_provider(ShadingLanguage::Wgsl);
        let symbol_tree =
            SymbolTree::new(symbol_provider.as_mut(), file_path, &shader_content).unwrap();
        let symbols = symbol_provider.query_file_symbols(&symbol_tree, None).unwrap();
        assert!(symbols.functions.is_empty());
    }
    #[test]
    fn symbol_scope_glsl_ok() {
        let file_path = Path::new("./test/glsl/scopes.frag.glsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let mut symbol_provider = create_symbol_provider(ShadingLanguage::Glsl);
        let symbols = get_all_symbols(symbol_provider.as_mut(), file_path, &shader_content)
            .unwrap()
            .filter_scoped_symbol(ShaderPosition {
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
}
