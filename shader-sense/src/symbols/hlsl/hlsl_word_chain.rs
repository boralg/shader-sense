use tree_sitter::Node;

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::get_name,
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderRange},
    },
};

use super::HlslSymbolProvider;

impl HlslSymbolProvider {
    pub fn find_label_chain_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        fn range_contain(including_range: tree_sitter::Range, position: ShaderPosition) -> bool {
            let including_range = ShaderRange::from_range(including_range, &position.file_path);
            including_range.contain(&position)
        }
        if range_contain(node.range(), position.clone()) {
            match node.kind() {
                "identifier" => {
                    return Ok(vec![(
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), &symbol_tree.file_path),
                    )])
                }
                "field_identifier" => {
                    let mut chain = Vec::new();
                    let mut current_node = node.prev_named_sibling().unwrap();
                    loop {
                        let field = current_node.next_named_sibling().unwrap();
                        if field.kind() == "field_identifier" {
                            chain.push((
                                get_name(&symbol_tree.content, field).into(),
                                ShaderRange::from_range(field.range(), &symbol_tree.file_path),
                            ));
                        } else {
                            return Err(ShaderError::InternalErr(format!(
                                "Unhandled case in find_label_chain_at_position_in_node: {}",
                                field.kind()
                            )));
                        }
                        match current_node.child_by_field_name("argument") {
                            Some(child) => {
                                current_node = child;
                            }
                            None => {
                                let identifier = current_node;
                                chain.push((
                                    get_name(&symbol_tree.content, identifier).into(),
                                    ShaderRange::from_range(
                                        identifier.range(),
                                        &symbol_tree.file_path,
                                    ),
                                ));
                                break;
                            } // Should have already break here
                        }
                    }
                    return Ok(chain);
                }
                _ => {
                    for child in node.children(&mut node.walk()) {
                        match self.find_label_chain_at_position_in_node(
                            symbol_tree,
                            child,
                            position.clone(),
                        ) {
                            Ok(chain_list) => return Ok(chain_list),
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
