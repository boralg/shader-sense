use std::path::{Path, PathBuf};

use tree_sitter::{Node, Query, QueryCursor, QueryMatch};

use crate::symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolList};

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

pub struct SymbolParser {
    symbol_parsers: Vec<(Box<dyn SymbolTreeParser>, tree_sitter::Query)>,
    symbol_filters: Vec<Box<dyn SymbolTreeFilter>>,
    scope_query: Query,
}

impl SymbolParser {
    pub fn new(
        language: tree_sitter::Language,
        scope_query: &str,
        parsers: Vec<Box<dyn SymbolTreeParser>>,
        filters: Vec<Box<dyn SymbolTreeFilter>>,
    ) -> Self {
        Self {
            symbol_parsers: parsers
                .into_iter()
                .map(|e| {
                    // Cache query
                    let query = Query::new(language, e.get_query().as_str()).unwrap();
                    (e, query)
                })
                .collect(),
            symbol_filters: filters,
            scope_query: tree_sitter::Query::new(language.clone(), scope_query).unwrap(),
        }
    }
    fn query_file_scopes(&self, symbol_tree: &SymbolTree) -> Vec<ShaderScope> {
        // TODO: look for namespace aswell
        let mut query_cursor = QueryCursor::new();
        let mut scopes = Vec::new();
        for matche in query_cursor.matches(
            &self.scope_query,
            symbol_tree.tree.root_node(),
            symbol_tree.content.as_bytes(),
        ) {
            scopes.push(ShaderScope::from_range(
                matche.captures[0].node.range(),
                symbol_tree.file_path.clone(),
            ));
        }
        scopes
    }
    pub fn query_file_symbols(&self, symbol_tree: &SymbolTree) -> ShaderSymbolList {
        let scopes = self.query_file_scopes(symbol_tree);
        let mut symbols = ShaderSymbolList::default();
        for parser in &self.symbol_parsers {
            let mut query_cursor = QueryCursor::new();
            for matches in query_cursor.matches(
                &parser.1,
                symbol_tree.tree.root_node(),
                symbol_tree.content.as_bytes(),
            ) {
                parser.0.process_match(
                    matches,
                    &symbol_tree.file_path,
                    &symbol_tree.content,
                    &scopes,
                    &mut symbols,
                );
            }
        }
        // TODO: Should be run directly on symbol add.
        // TODO: Should use something else than name...
        let file_name = symbol_tree
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        for filter in &self.symbol_filters {
            filter.filter_symbols(&mut symbols, &file_name);
        }
        symbols
    }
}
