use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_tree::SymbolTree,
        symbols::{ShaderPreprocessor, ShaderRegion},
    },
};

pub fn query_regions_in_node(
    _symbol_tree: &SymbolTree,
    _node: tree_sitter::Node,
    _preprocessor: &ShaderPreprocessor,
) -> Result<Vec<ShaderRegion>, ShaderError> {
    Ok(vec![])
}
