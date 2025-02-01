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

use crate::shader_error::ShaderError;

use super::{
    symbol_parser::SymbolParser,
    symbol_provider::SymbolProvider,
    symbol_tree::SymbolTree,
    symbols::{ShaderPosition, ShaderPreprocessor, ShaderRange, ShaderSymbolList},
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
                hlsl_regions::query_regions_in_node,
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
    ) -> Result<ShaderPreprocessor, ShaderError> {
        self.symbol_parser.query_file_preprocessor(symbol_tree)
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
