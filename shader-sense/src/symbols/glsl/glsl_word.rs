use tree_sitter::Node;

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::{get_name, ShaderWordRange, SymbolWordProvider},
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderRange},
    },
};

pub struct GlslSymbolWordProvider {}

impl SymbolWordProvider for GlslSymbolWordProvider {
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
                "identifier" | "type_identifier" | "primitive_type" => {
                    return Ok(ShaderWordRange::new(
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), &symbol_tree.file_path),
                        None,
                    ));
                }
                // TODO: should use string_content instead
                "string_literal" => {
                    let path = get_name(&symbol_tree.content, node);
                    return Ok(ShaderWordRange::new(
                        path[1..path.len() - 1].into(),
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
                    let mut current_node = match node.prev_named_sibling() {
                        Some(prev_sibling) => prev_sibling,
                        None => return Err(ShaderError::NoSymbol),
                    };
                    loop {
                        let field = current_node.next_named_sibling().unwrap();
                        match field.kind() {
                            "field_identifier" => set_parent(
                                &mut word,
                                ShaderWordRange::new(
                                    get_name(&symbol_tree.content, field).into(),
                                    ShaderRange::from_range(field.range(), &symbol_tree.file_path),
                                    None,
                                ),
                            ),
                            _ => {
                                return Err(ShaderError::InternalErr(format!(
                                    "Unknown word field {}",
                                    field.kind()
                                )))
                            }
                        }
                        let mut cursor = current_node.walk();
                        match cursor.node().kind() {
                            "field_expression" => {
                                cursor.goto_first_child();
                                current_node = cursor.node();
                            }
                            "call_expression" => {
                                cursor.goto_first_child();
                                match cursor.node().kind() {
                                    "field_expression" => {
                                        cursor.goto_first_child();
                                        current_node = cursor.node();
                                    }
                                    "identifier" => {
                                        let identifier = cursor.node();
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
                                    }
                                    _ => {
                                        return Err(ShaderError::InternalErr(format!(
                                            "Failed to get word from call_expression {}",
                                            field.kind()
                                        )))
                                    }
                                }
                            }
                            "identifier" => {
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
                            }
                            _ => {
                                return Err(ShaderError::InternalErr(format!(
                                    "Failed to get word from cursor {}",
                                    current_node.kind()
                                )));
                            }
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
