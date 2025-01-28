use std::{
    path::{Path, PathBuf},
    vec,
};

use tree_sitter::{InputEdit, Node, Parser, QueryCursor, QueryMatch, Tree, TreeCursor};

use crate::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolList},
};

use super::{symbol_tree::SymbolTree, symbols::ShaderScope};

pub(super) fn get_name<'a>(shader_content: &'a str, node: Node) -> &'a str {
    let range = node.range();
    &shader_content[range.start_byte..range.end_byte]
}

impl ShaderRange {
    pub(super) fn from_range(value: tree_sitter::Range, file_path: PathBuf) -> Self {
        ShaderRange {
            start: ShaderPosition {
                file_path: file_path.clone(),
                line: value.start_point.row as u32,
                pos: value.start_point.column as u32,
            },
            end: ShaderPosition {
                file_path: file_path.clone(),
                line: value.end_point.row as u32,
                pos: value.end_point.column as u32,
            },
        }
    }
}

pub trait SymbolTreeParser {
    // The query to match tree node
    fn get_query(&self) -> String;
    // Process the match & convert it to symbol
    fn process_match(
        &self,
        matches: QueryMatch,
        file_path: &Path,
        shader_content: &str,
        scopes: &Vec<ShaderScope>,
        symbols: &mut ShaderSymbolList,
    );
    fn compute_scope_stack(
        &self,
        scopes: &Vec<ShaderScope>,
        range: &ShaderRange,
    ) -> Vec<ShaderScope> {
        scopes
            .iter()
            .filter_map(|e| {
                if e.contain_bounds(&range) {
                    Some(e.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<ShaderScope>>()
    }
}

pub trait SymbolTreeFilter {
    fn filter_symbols(&self, shader_symbols: &mut ShaderSymbolList, file_name: &String);
}

pub fn create_symbol_parser(
    symbol_parser: Box<dyn SymbolTreeParser>,
    language: &tree_sitter::Language,
) -> (Box<dyn SymbolTreeParser>, tree_sitter::Query) {
    let query =
        tree_sitter::Query::new(language.clone(), symbol_parser.get_query().as_str()).unwrap();
    (symbol_parser, query)
}
