use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::SymbolRegionFinder,
        symbol_tree::SymbolTree,
        symbols::{ShaderPreprocessor, ShaderRegion},
    },
};

pub struct GgslRegionFinder {}

impl SymbolRegionFinder for GgslRegionFinder {
    fn query_regions_in_node(
        &self,
        _symbol_tree: &SymbolTree,
        _node: tree_sitter::Node,
        _preprocessor: &ShaderPreprocessor,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        Ok(vec![])
    }
}
