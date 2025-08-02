use tree_sitter::Node;

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::{get_name, ShaderWordRange, SymbolWordProvider},
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderRange},
    },
};

pub struct HlslSymbolWordProvider {}

impl SymbolWordProvider for HlslSymbolWordProvider {
    fn find_word_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: &ShaderPosition,
    ) -> Result<ShaderWordRange, ShaderError> {
        fn range_contain(including_range: tree_sitter::Range, position: ShaderPosition) -> bool {
            let including_range = ShaderRange::from_range(including_range, &position.file_path);
            including_range.contain(&position)
        }
        if range_contain(node.range(), position.clone()) {
            match node.kind() {
                // identifier = function name, variable...
                // type_identifier = struct name, class name...
                // primitive_type = float, uint...
                // string_content = include, should check preproc_include as parent.
                // TODO: handle function call for return type chaining aswell
                "identifier" | "type_identifier" | "primitive_type" => {
                    return Ok(ShaderWordRange::new(
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), &symbol_tree.file_path),
                        None,
                    ));
                }
                "field_identifier" => {
                    fn set_parent(
                        root: &mut Option<ShaderWordRange>,
                        root_parent: ShaderWordRange,
                    ) {
                        match root {
                            Some(root) => root.set_root_parent(root_parent),
                            None => *root = Some(root_parent),
                        }
                    }
                    let mut word: Option<ShaderWordRange> = None;
                    let mut current_node = node.prev_named_sibling().unwrap();
                    loop {
                        let field = current_node.next_named_sibling().unwrap();
                        if field.kind() == "field_identifier" {
                            set_parent(
                                &mut word,
                                ShaderWordRange::new(
                                    get_name(&symbol_tree.content, field).into(),
                                    ShaderRange::from_range(field.range(), &symbol_tree.file_path),
                                    None,
                                ),
                            );
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
                                set_parent(
                                    &mut word,
                                    ShaderWordRange::new(
                                        get_name(&symbol_tree.content, identifier).into(),
                                        ShaderRange::from_range(
                                            identifier.range(),
                                            &symbol_tree.file_path,
                                        ),
                                        None,
                                    ),
                                );
                                break;
                            } // Should have already break here
                        }
                    }
                    return word.ok_or(ShaderError::NoSymbol);
                }
                _ => {
                    for child in node.children(&mut node.walk()) {
                        match self.find_word_at_position_in_node(symbol_tree, child, position) {
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
