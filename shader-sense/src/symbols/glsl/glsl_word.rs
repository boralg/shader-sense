use tree_sitter::Node;

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::get_name,
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderRange},
    },
};

use super::GlslSymbolProvider;

impl GlslSymbolProvider {
    pub fn find_label_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        fn range_contain(including_range: tree_sitter::Range, position: ShaderPosition) -> bool {
            let including_range =
                ShaderRange::from_range(including_range, position.file_path.clone());
            including_range.contain(&position)
        }
        if range_contain(node.range(), position.clone()) {
            match node.kind() {
                // identifier = function name, variable...
                // type_identifier = struct name, class name...
                // primitive_type = float, uint...
                // string_content = include, should check preproc_include as parent.
                // TODO: should depend on language...
                "identifier" | "type_identifier" | "primitive_type" => {
                    return Ok((
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), symbol_tree.file_path.clone()),
                    ))
                }
                // TODO: should use string_content instead
                "string_literal" => {
                    let path = get_name(&symbol_tree.content, node);
                    return Ok((
                        path[1..path.len() - 1].into(),
                        ShaderRange::from_range(node.range(), symbol_tree.file_path.clone()),
                    ));
                }
                _ => {
                    for child in node.children(&mut node.walk()) {
                        match self.find_label_at_position_in_node(
                            symbol_tree,
                            child,
                            position.clone(),
                        ) {
                            Ok(label) => return Ok(label),
                            Err(err) => {
                                if let ShaderError::NoSymbol = err {
                                    // Skip.
                                } else {
                                    return Err(err);
                                }
                            }
                        }
                    }
                }
            }
            Err(ShaderError::NoSymbol)
        } else {
            Err(ShaderError::NoSymbol)
        }
    }
}
