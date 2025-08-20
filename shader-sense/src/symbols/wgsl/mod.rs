mod wgsl_parser;
mod wgsl_regions;
use wgsl_parser::get_wgsl_parsers;
use wgsl_regions::WgslRegionFinder;

use crate::{shader_error::ShaderError, symbols::symbol_parser::ShaderWordRange};

use super::{
    shader_module::ShaderModule, symbol_parser::SymbolWordProvider,
    symbol_provider::SymbolProvider, symbols::ShaderPosition,
};

struct WgslSymbolWordProvider {}

impl SymbolWordProvider for WgslSymbolWordProvider {
    fn find_word_at_position_in_node(
        &self,
        _shader_module: &ShaderModule,
        _node: tree_sitter::Node,
        _position: &ShaderPosition,
    ) -> Result<ShaderWordRange, ShaderError> {
        return Err(ShaderError::NoSymbol);
    }
}

pub fn create_wgsl_symbol_provider(tree_sitter_language: tree_sitter::Language) -> SymbolProvider {
    SymbolProvider::new(
        tree_sitter_language.clone(),
        get_wgsl_parsers(),
        vec![],
        Box::new(WgslRegionFinder {}),
        Box::new(WgslSymbolWordProvider {}),
    )
}
