use std::path::{Path, PathBuf};

use tree_sitter::{InputEdit, Tree, TreeCursor};

use crate::shader_error::ShaderError;

use super::{symbol_provider::SymbolProvider, symbols::ShaderRange};

#[derive(Debug, Clone)]
pub struct SymbolTree {
    pub file_path: PathBuf,
    pub content: String,
    pub tree: Tree,
}

impl SymbolTree {
    // Create a symbol tree
    pub fn new<T: SymbolProvider + ?Sized>(
        symbol_provider: &mut T,
        file_path: &Path,
        shader_content: &str,
    ) -> Result<Self, ShaderError> {
        match symbol_provider.get_parser().parse(shader_content, None) {
            Some(tree) => Ok(SymbolTree {
                file_path: file_path.into(),
                content: shader_content.into(),
                tree,
            }),
            None => Err(ShaderError::ParseSymbolError(format!(
                "Failed to parse AST for file {}",
                file_path.display()
            ))),
        }
    }
    // Update whole content of symbol tree
    pub fn update<T: SymbolProvider + ?Sized>(
        &mut self,
        symbol_provider: &mut T,
        new_text: &String,
    ) -> Result<(), ShaderError> {
        self.update_partial(
            symbol_provider,
            &ShaderRange::whole(&self.content),
            new_text,
        )
    }
    // Update partial content of symbol tree
    pub fn update_partial<T: SymbolProvider + ?Sized>(
        &mut self,
        symbol_provider: &mut T,
        old_range: &ShaderRange,
        new_text: &String,
    ) -> Result<(), ShaderError> {
        let mut new_shader_content = self.content.clone();
        new_shader_content.replace_range(
            old_range.start.to_byte_offset(&self.content)
                ..old_range.end.to_byte_offset(&self.content),
            &new_text,
        );

        let line_count = new_text.lines().count();
        let tree_sitter_range = tree_sitter::Range {
            start_byte: old_range.start.to_byte_offset(&self.content),
            end_byte: old_range.end.to_byte_offset(&self.content),
            start_point: tree_sitter::Point {
                row: old_range.start.line as usize,
                column: old_range.start.pos as usize,
            },
            end_point: tree_sitter::Point {
                row: old_range.end.line as usize,
                column: old_range.end.pos as usize,
            },
        };
        self.tree.edit(&InputEdit {
            start_byte: tree_sitter_range.start_byte,
            old_end_byte: tree_sitter_range.end_byte,
            new_end_byte: tree_sitter_range.start_byte + new_text.len(),
            start_position: tree_sitter_range.start_point,
            old_end_position: tree_sitter_range.end_point,
            new_end_position: tree_sitter::Point {
                row: if line_count == 0 {
                    tree_sitter_range.start_point.row + new_text.len()
                } else {
                    new_text.lines().last().as_slice().len()
                },
                column: tree_sitter_range.start_point.column + line_count,
            },
        });
        // Update the tree.
        match symbol_provider
            .get_parser()
            .parse(&new_shader_content, Some(&self.tree))
        {
            Some(new_tree) => {
                self.tree = new_tree;
                self.content = new_shader_content.clone();
                Ok(())
            }
            None => Err(ShaderError::ParseSymbolError(format!(
                "Failed to update AST for file {}.",
                self.file_path.display()
            ))),
        }
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
