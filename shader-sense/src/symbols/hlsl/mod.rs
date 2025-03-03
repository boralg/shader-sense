mod hlsl_filter;
mod hlsl_parser;
mod hlsl_preprocessor;
mod hlsl_regions;
mod hlsl_word;
mod hlsl_word_chain;

use hlsl_filter::get_hlsl_filters;
use hlsl_parser::get_hlsl_parsers;
use hlsl_preprocessor::get_hlsl_preprocessor_parser;
use tree_sitter::Parser;

// For glsl
pub use hlsl_regions::HlslSymbolRegionFinder;

use crate::shader_error::ShaderError;

use super::{
    symbol_parser::SymbolParser,
    symbol_provider::SymbolProvider,
    symbol_tree::SymbolTree,
    symbols::{
        ShaderPosition, ShaderPreprocessor, ShaderRange, ShaderSymbolList, ShaderSymbolParams,
    },
};

pub struct HlslSymbolProvider {
    parser: Parser,
    symbol_parser: SymbolParser,
    shader_intrinsics: ShaderSymbolList,
}

impl HlslSymbolProvider {
    pub fn new() -> Self {
        let lang = tree_sitter_hlsl::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading HLSL grammar");
        let scope_query = r#"(compound_statement) @scope"#;
        Self {
            parser,
            symbol_parser: SymbolParser::new(
                lang.clone(),
                scope_query,
                get_hlsl_parsers(),
                get_hlsl_filters(),
                get_hlsl_preprocessor_parser(),
                Box::new(HlslSymbolRegionFinder::new(lang.clone())),
            ),
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "hlsl-intrinsics.json"
            ))),
        }
    }
}

impl SymbolProvider for HlslSymbolProvider {
    // Get intrinsic symbols from language
    fn get_intrinsics_symbol(&self) -> &ShaderSymbolList {
        &self.shader_intrinsics
    }
    fn get_parser(&mut self) -> &mut Parser {
        &mut self.parser
    }
    fn query_preprocessor(
        &self,
        symbol_tree: &SymbolTree,
        symbol_params: &ShaderSymbolParams,
    ) -> Result<ShaderPreprocessor, ShaderError> {
        self.symbol_parser
            .query_file_preprocessor(symbol_tree, symbol_params)
    }
    fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
        preprocessor: Option<&ShaderPreprocessor>,
    ) -> Result<ShaderSymbolList, ShaderError> {
        self.symbol_parser
            .query_file_symbols(symbol_tree, preprocessor)
    }
    // Get word at a given position.
    fn get_word_range_at_position(
        &self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        self.find_label_at_position_in_node(symbol_tree, symbol_tree.tree.root_node(), position)
    }
    // Get a struct word chain at a given position
    fn get_word_chain_range_at_position(
        &mut self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        self.find_label_chain_at_position_in_node(
            symbol_tree,
            symbol_tree.tree.root_node(),
            position,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        shader::ShadingLanguage,
        symbols::{
            create_symbol_provider,
            symbol_provider::SymbolProvider,
            symbol_tree::SymbolTree,
            symbols::{ShaderPosition, ShaderRange, ShaderRegion, ShaderSymbolParams},
        },
    };

    #[test]
    fn test_hlsl_regions() {
        test_regions(create_symbol_provider(ShadingLanguage::Hlsl));
    }
    #[test]
    fn test_glsl_regions() {
        test_regions(create_symbol_provider(ShadingLanguage::Glsl));
    }

    fn test_regions(mut symbol_provider: Box<dyn SymbolProvider>) {
        let file_path = Path::new("./test/hlsl/regions.hlsl");
        let shader_content = std::fs::read_to_string(file_path).unwrap();
        let symbol_tree =
            SymbolTree::new(symbol_provider.as_mut(), file_path, &shader_content).unwrap();
        let preprocessor = symbol_provider
            .query_preprocessor(&symbol_tree, &ShaderSymbolParams::default())
            .unwrap();
        //let symbols = symbol_provider.query_file_symbols(&symbol_tree, Some(&preprocessor));
        let set_region =
            |start_line: u32, start_pos: u32, end_line: u32, end_pos: u32, active: bool| {
                ShaderRegion {
                    range: ShaderRange::new(
                        ShaderPosition::new(file_path.into(), start_line, start_pos),
                        ShaderPosition::new(file_path.into(), end_line, end_pos),
                    ),
                    is_active: active,
                }
            };
        let expected_regions = vec![
            // elif
            set_region(4, 21, 6, 0, true),
            set_region(6, 32, 8, 0, false),
            set_region(8, 5, 9, 16, false),
            // ifdef true
            set_region(12, 24, 14, 0, true),
            set_region(14, 5, 15, 16, false),
            // ifndef
            set_region(18, 25, 20, 0, false),
            set_region(20, 5, 21, 16, true),
            // ifdef false
            set_region(24, 28, 26, 0, false),
            // if 0
            set_region(28, 5, 30, 0, false),
            // if parenthesized
            set_region(33, 50, 35, 0, false),
            // if binary
            set_region(38, 43, 40, 0, false),
            // if unary
            set_region(43, 22, 45, 0, false),
            // unary defined expression
            set_region(48, 66, 50, 0, false),
            // region depending on region
            set_region(53, 25, 55, 0, false),
            set_region(56, 28, 58, 0, false),
        ];
        assert!(preprocessor.regions.len() == expected_regions.len());
        for region_index in 0..preprocessor.regions.len() {
            println!(
                "region {}: {:#?}",
                region_index, preprocessor.regions[region_index]
            );
            assert!(
                preprocessor.regions[region_index].range.start
                    == expected_regions[region_index].range.start,
                "Failed start assert for region {}",
                region_index
            );
            assert!(
                preprocessor.regions[region_index].range.end
                    == expected_regions[region_index].range.end,
                "Failed end assert for region {}",
                region_index
            );
            assert!(
                preprocessor.regions[region_index].is_active
                    == expected_regions[region_index].is_active,
                "Failed active assert for region {}",
                region_index
            );
        }
    }
}
