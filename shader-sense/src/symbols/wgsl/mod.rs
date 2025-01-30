mod wgsl_filter;
mod wgsl_parser;
use tree_sitter::Parser;
use wgsl_filter::get_wgsl_filters;
use wgsl_parser::get_wgsl_parsers;

use crate::shader_error::ShaderError;

use super::{
    symbol_parser::SymbolParser,
    symbol_provider::SymbolProvider,
    symbol_tree::SymbolTree,
    symbols::{ShaderPosition, ShaderRange, ShaderSymbolList},
};

pub struct WgslSymbolProvider {
    parser: Parser,
    symbol_parser: SymbolParser,
    shader_intrinsics: ShaderSymbolList,
}

impl WgslSymbolProvider {
    pub fn new() -> Self {
        let lang = tree_sitter_wgsl_bevy::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading WGSL grammar");
        Self {
            parser,
            symbol_parser: SymbolParser::new(
                lang.clone(),
                "",
                get_wgsl_parsers(),
                get_wgsl_filters(),
            ),
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "wgsl-intrinsics.json"
            ))),
        }
    }
}

impl SymbolProvider for WgslSymbolProvider {
    fn get_parser(&mut self) -> &mut Parser {
        &mut self.parser
    }

    fn get_intrinsics_symbol(&self) -> &ShaderSymbolList {
        &self.shader_intrinsics
    }

    fn query_file_symbols(&self, symbol_tree: &SymbolTree) -> ShaderSymbolList {
        self.symbol_parser.query_file_symbols(symbol_tree)
    }

    fn get_word_range_at_position(
        &self,
        _symbol_tree: &SymbolTree,
        _position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        Err(ShaderError::NoSymbol)
    }

    fn get_word_chain_range_at_position(
        &mut self,
        _symbol_tree: &SymbolTree,
        _position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        Ok(vec![])
    }

    fn query_inactive_regions(
        &self,
        _symbol_tree: &SymbolTree,
    ) -> Result<Vec<ShaderRange>, ShaderError> {
        Ok(vec![])
    }
}
