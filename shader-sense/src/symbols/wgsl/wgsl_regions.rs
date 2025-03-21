use crate::{
    include::IncludeHandler,
    shader_error::ShaderError,
    symbols::{
        symbol_parser::SymbolRegionFinder,
        symbol_provider::SymbolIncludeCallback,
        symbol_tree::SymbolTree,
        symbols::{ShaderPreprocessor, ShaderPreprocessorContext, ShaderRegion},
    },
};

pub struct WgslRegionFinder {}

impl SymbolRegionFinder for WgslRegionFinder {
    fn query_regions_in_node<'a>(
        &self,
        _symbol_tree: &SymbolTree,
        _node: tree_sitter::Node,
        _preprocessor: &mut ShaderPreprocessor,
        _context: &'a mut ShaderPreprocessorContext,
        _include_handler: &'a mut IncludeHandler,
        _include_callback: &'a mut SymbolIncludeCallback<'a>,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        Ok(vec![])
    }
}
