use std::path::PathBuf;

use tree_sitter::{Tree, TreeCursor};

#[derive(Debug, Clone)]
pub struct SymbolTree {
    pub file_path: PathBuf,
    pub content: String,
    pub tree: Tree,
}

impl SymbolTree {
    pub fn dump_ast(&self) -> String {
        fn format_debug_cursor(cursor: &mut TreeCursor, depth: usize) -> String {
            let mut debug_tree = String::new();
            loop {
                debug_tree.push_str(&format!(
                    "{}\"{}\": \"{}\"\n",
                    " ".repeat(depth * 2),
                    cursor.field_name().unwrap_or("None"),
                    cursor.node().kind()
                ));
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
        format_debug_cursor(&mut self.tree.root_node().walk(), 0)
    }
}
