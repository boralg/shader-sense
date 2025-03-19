mod wgsl_filter;
mod wgsl_parser;
mod wgsl_regions;
use wgsl_filter::get_wgsl_filters;
use wgsl_parser::get_wgsl_parsers;
use wgsl_regions::WgslRegionFinder;

use crate::shader_error::ShaderError;

use super::{
    symbol_parser::{SymbolLabelChainProvider, SymbolLabelProvider},
    symbol_provider::SymbolProvider,
    symbol_tree::SymbolTree,
    symbols::{ShaderPosition, ShaderRange},
};

struct WgslSymbolLabelChainProvider {}

impl SymbolLabelChainProvider for WgslSymbolLabelChainProvider {
    fn find_label_chain_at_position_in_node(
        &self,
        _symbol_tree: &SymbolTree,
        _node: tree_sitter::Node,
        _position: &ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        return Err(ShaderError::NoSymbol);
    }
}

struct WgslSymbolLabelProvider {}

impl SymbolLabelProvider for WgslSymbolLabelProvider {
    fn find_label_at_position_in_node(
        &self,
        _symbol_tree: &SymbolTree,
        _node: tree_sitter::Node,
        _position: &ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        return Err(ShaderError::NoSymbol);
    }
}

pub fn create_wgsl_symbol_provider(tree_sitter_language: tree_sitter::Language) -> SymbolProvider {
    SymbolProvider::new(
        tree_sitter_language.clone(),
        get_wgsl_parsers(),
        get_wgsl_filters(),
        vec![],
        Box::new(WgslRegionFinder {}),
        Box::new(WgslSymbolLabelChainProvider {}),
        Box::new(WgslSymbolLabelProvider {}),
    )
}
