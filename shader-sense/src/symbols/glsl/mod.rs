mod glsl_filter;
mod glsl_parser;
mod glsl_preprocessor;
mod glsl_regions;
mod glsl_word;
mod glsl_word_chain;

use glsl_filter::get_glsl_filters;
use glsl_parser::get_glsl_parsers;
use glsl_preprocessor::get_glsl_preprocessor_parser;
use glsl_regions::GlslRegionFinder;
use tree_sitter::Parser;

use crate::{include::IncludeHandler, shader_error::ShaderError};

use super::{
    symbol_parser::SymbolParser,
    symbol_provider::SymbolProvider,
    symbol_tree::SymbolTree,
    symbols::{ShaderPreprocessor, ShaderSymbolList, ShaderSymbolParams},
};

pub struct GlslSymbolProvider {
    parser: Parser,
    symbol_parser: SymbolParser,
    shader_intrinsics: ShaderSymbolList,
}

impl GlslSymbolProvider {
    pub fn new() -> Self {
        let lang = tree_sitter_glsl::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading GLSL grammar");
        Self {
            parser,
            symbol_parser: SymbolParser::new(
                lang.clone(),
                r#"(compound_statement) @scope"#,
                get_glsl_parsers(),
                get_glsl_filters(),
                get_glsl_preprocessor_parser(),
                Box::new(GlslRegionFinder::new()),
            ),
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "glsl-intrinsics.json"
            ))),
        }
    }
}

impl SymbolProvider for GlslSymbolProvider {
    fn get_parser(&mut self) -> &mut Parser {
        &mut self.parser
    }

    fn get_intrinsics_symbol(&self) -> &ShaderSymbolList {
        &self.shader_intrinsics
    }

    fn query_preprocessor(
        &self,
        symbol_tree: &SymbolTree,
        symbol_params: &ShaderSymbolParams,
        include_handler: &mut IncludeHandler,
    ) -> Result<ShaderPreprocessor, ShaderError> {
        self.symbol_parser
            .query_file_preprocessor(symbol_tree, symbol_params, include_handler)
    }

    fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
        preprocessor: &ShaderPreprocessor,
    ) -> Result<ShaderSymbolList, ShaderError> {
        self.symbol_parser
            .query_file_symbols(symbol_tree, preprocessor)
    }

    fn get_word_range_at_position(
        &self,
        symbol_tree: &super::symbol_tree::SymbolTree,
        position: super::symbols::ShaderPosition,
    ) -> Result<(String, super::symbols::ShaderRange), crate::shader_error::ShaderError> {
        self.find_label_at_position_in_node(symbol_tree, symbol_tree.tree.root_node(), position)
    }

    fn get_word_chain_range_at_position(
        &mut self,
        symbol_tree: &super::symbol_tree::SymbolTree,
        position: super::symbols::ShaderPosition,
    ) -> Result<Vec<(String, super::symbols::ShaderRange)>, crate::shader_error::ShaderError> {
        self.find_label_chain_at_position_in_node(
            symbol_tree,
            symbol_tree.tree.root_node(),
            position,
        )
    }
}
