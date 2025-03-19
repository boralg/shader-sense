use std::path::{Path, PathBuf};

use tree_sitter::{Tree, TreeCursor};

use super::shader_language::ShaderLanguage;

#[derive(Debug, Clone)]
// TODO: shadermodule
pub struct SymbolTree {
    pub file_path: PathBuf,
    pub content: String,
    pub tree: Tree,
}

impl SymbolTree {
    pub fn new(language: &mut ShaderLanguage, path: &Path, content: &str) -> Self {
        language.create_module(path, &content).unwrap()
    }
    // Dump AST from tree
    pub fn dump_ast(&self) -> String {
        Self::dump_ast_node(self.tree.root_node())
    }
    pub fn dump_ast_node(node: tree_sitter::Node) -> String {
        fn format_debug_cursor(cursor: &mut TreeCursor, depth: usize) -> String {
            let mut debug_tree = String::new();
            loop {
                debug_tree.push_str(&match cursor.field_name() {
                    Some(field_name) => format!(
                        "{}{}: {} [{}, {}] - [{}, {}]\n",
                        " ".repeat(depth * 2),
                        field_name,
                        cursor.node().kind(),
                        cursor.node().range().start_point.row,
                        cursor.node().range().start_point.column,
                        cursor.node().range().end_point.row,
                        cursor.node().range().end_point.column,
                    ),
                    None => {
                        if cursor.node().is_named() {
                            format!(
                                "{}{} [{}, {}] - [{}, {}]\n",
                                " ".repeat(depth * 2),
                                cursor.node().kind(),
                                cursor.node().range().start_point.row,
                                cursor.node().range().start_point.column,
                                cursor.node().range().end_point.row,
                                cursor.node().range().end_point.column,
                            )
                        } else {
                            format!(
                                "{}{:?} [{}, {}] - [{}, {}]\n",
                                " ".repeat(depth * 2),
                                cursor.node().kind(),
                                cursor.node().range().start_point.row,
                                cursor.node().range().start_point.column,
                                cursor.node().range().end_point.row,
                                cursor.node().range().end_point.column,
                            )
                        }
                    }
                });
                if cursor.goto_first_child() {
                    debug_tree.push_str(format_debug_cursor(cursor, depth + 1).as_str());
                    cursor.goto_parent();
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            debug_tree
        }
        format_debug_cursor(&mut node.walk(), 0)
    }
}
