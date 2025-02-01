use std::path::Path;

use tree_sitter::{Node, Query, QueryCursor, QueryMatch};

use crate::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolList},
};

use super::{
    symbol_tree::SymbolTree,
    symbols::{ShaderPreprocessor, ShaderRegion, ShaderScope},
};

pub(super) fn get_name<'a>(shader_content: &'a str, node: Node) -> &'a str {
    let range = node.range();
    &shader_content[range.start_byte..range.end_byte]
}

impl ShaderRange {
    pub(super) fn from_range(value: tree_sitter::Range, file_path: &Path) -> Self {
        ShaderRange {
            start: ShaderPosition {
                file_path: file_path.into(),
                line: value.start_point.row as u32,
                pos: value.start_point.column as u32,
            },
            end: ShaderPosition {
                file_path: file_path.into(),
                line: value.end_point.row as u32,
                pos: value.end_point.column as u32,
            },
        }
    }
}

impl ShaderPosition {
    pub(super) fn from_tree_sitter_point(point: tree_sitter::Point, file_path: &Path) -> Self {
        ShaderPosition {
            file_path: file_path.into(),
            line: point.row as u32,
            pos: point.column as u32,
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

pub trait SymbolTreePreprocessorParser {
    // The query to match tree node
    fn get_query(&self) -> String;
    // Process the match & convert it to preprocessor
    fn process_match(
        &self,
        matches: QueryMatch,
        file_path: &Path,
        shader_content: &str,
        preprocessor: &mut ShaderPreprocessor,
    );
}

pub type SymbolRegionCallback = fn(
    &SymbolTree,
    tree_sitter::Node,
    &ShaderPreprocessor,
) -> Result<Vec<ShaderRegion>, ShaderError>;

pub struct SymbolParser {
    symbol_parsers: Vec<(Box<dyn SymbolTreeParser>, tree_sitter::Query)>,
    symbol_filters: Vec<Box<dyn SymbolTreeFilter>>,
    scope_query: Query,

    preprocessor_parsers: Vec<(Box<dyn SymbolTreePreprocessorParser>, tree_sitter::Query)>,
    region_finder: SymbolRegionCallback,
}

impl SymbolParser {
    pub fn new(
        language: tree_sitter::Language,
        scope_query: &str,
        parsers: Vec<Box<dyn SymbolTreeParser>>,
        filters: Vec<Box<dyn SymbolTreeFilter>>,
        preprocessor_parsers: Vec<Box<dyn SymbolTreePreprocessorParser>>,
        region_finder: SymbolRegionCallback,
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
            preprocessor_parsers: preprocessor_parsers
                .into_iter()
                .map(|e| {
                    // Cache query
                    let query = Query::new(language, e.get_query().as_str()).unwrap();
                    (e, query)
                })
                .collect(),
            region_finder: region_finder,
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
                &symbol_tree.file_path,
            ));
        }
        scopes
    }
    pub fn query_file_preprocessor(&self, symbol_tree: &SymbolTree) -> ShaderPreprocessor {
        let mut preprocessor = ShaderPreprocessor::default();
        for parser in &self.preprocessor_parsers {
            // TODO: cache
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
                    &mut preprocessor,
                );
            }
        }
        match (self.region_finder)(symbol_tree, symbol_tree.tree.root_node(), &preprocessor) {
            Ok(regions) => preprocessor.regions = regions,
            Err(_) => unimplemented!(),
        }
        preprocessor
    }
    pub fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
        preprocessor: Option<&ShaderPreprocessor>,
    ) -> ShaderSymbolList {
        let file_preprocessor = match preprocessor {
            Some(preprocessor) => preprocessor.clone(),
            // This will not include external preprocessor symbols
            // (such as file included through another file, and having parent file preproc)
            None => self.query_file_preprocessor(symbol_tree),
        };
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
        // TODO: filter with file_preprocessor
        symbols
    }
}
