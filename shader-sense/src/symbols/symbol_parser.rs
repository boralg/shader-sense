use std::path::Path;

use tree_sitter::{Node, Query, QueryCursor, QueryMatch};

use crate::{
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolList},
};

use super::{
    symbol_tree::SymbolTree,
    symbols::{
        ShaderPreprocessor, ShaderPreprocessorDefine, ShaderRegion, ShaderScope, ShaderSymbol,
        ShaderSymbolParams,
    },
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

pub struct ShaderSymbolListBuilder<'a> {
    shader_symbol_list: ShaderSymbolList,
    filter_callback: Box<&'a dyn Fn(&ShaderSymbol) -> bool>,
}
impl<'a> ShaderSymbolListBuilder<'a> {
    pub fn new(filter_callback: &'a dyn Fn(&ShaderSymbol) -> bool) -> Self {
        Self {
            shader_symbol_list: ShaderSymbolList::default(),
            filter_callback: Box::new(filter_callback),
        }
    }
    pub fn add_constant(&mut self, shader_symbol: ShaderSymbol) {
        if (self.filter_callback)(&shader_symbol) {
            self.shader_symbol_list.constants.push(shader_symbol);
        }
    }
    pub fn add_variable(&mut self, shader_symbol: ShaderSymbol) {
        if (self.filter_callback)(&shader_symbol) {
            self.shader_symbol_list.variables.push(shader_symbol);
        }
    }
    pub fn add_type(&mut self, shader_symbol: ShaderSymbol) {
        if (self.filter_callback)(&shader_symbol) {
            self.shader_symbol_list.types.push(shader_symbol);
        }
    }
    pub fn add_function(&mut self, shader_symbol: ShaderSymbol) {
        if (self.filter_callback)(&shader_symbol) {
            self.shader_symbol_list.functions.push(shader_symbol);
        }
    }
    pub fn get_shader_symbol_list(&mut self) -> ShaderSymbolList {
        std::mem::take(&mut self.shader_symbol_list)
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
        symbols: &mut ShaderSymbolListBuilder,
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
    // Filter symbol, keep them on true, remove them on false
    fn filter_symbol(&self, shader_symbol: &ShaderSymbol, file_name: &String) -> bool;
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
    pub fn query_file_preprocessor(
        &self,
        symbol_tree: &SymbolTree,
        symbol_params: &ShaderSymbolParams,
    ) -> Result<ShaderPreprocessor, ShaderError> {
        let mut preprocessor = ShaderPreprocessor::default();
        for parser in &self.preprocessor_parsers {
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
        // Add settings define
        preprocessor.defines.append(
            &mut symbol_params
                .defines
                .iter()
                .map(|(define, value)| ShaderPreprocessorDefine {
                    name: define.clone(),
                    range: None,
                    value: Some(value.clone()),
                })
                .collect(),
        );
        // Query regions.
        preprocessor.regions =
            (self.region_finder)(symbol_tree, symbol_tree.tree.root_node(), &preprocessor)?;
        // Filter out defines in inactive regions.
        preprocessor.defines.retain(|define| match &define.range {
            Some(range) => preprocessor
                .regions
                .iter()
                .find(|region| !region.is_active && region.range.contain_bounds(&range))
                .is_none(),
            None => true,
        });
        Ok(preprocessor)
    }
    pub fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
        preprocessor: Option<&ShaderPreprocessor>,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let file_preprocessor = match preprocessor {
            Some(preprocessor) => preprocessor.clone(),
            // This will not include external preprocessor symbols
            // (such as file included through another file, and having parent file preproc)
            None => self.query_file_preprocessor(symbol_tree, &ShaderSymbolParams::default())?,
        };
        // TODO: Should use something else than name...
        // Required for shader stage filtering...
        let file_name = symbol_tree
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let filter_symbol = |symbol: &ShaderSymbol| -> bool {
            // Filter inactive regions.
            let is_in_inactive_region = match &symbol.range {
                Some(range) => {
                    for region in &file_preprocessor.regions {
                        if !region.is_active && region.range.contain_bounds(&range) {
                            return false; // Symbol is in inactive region. Remove it.
                        }
                    }
                    true
                }
                None => true, // keep
            };
            let mut is_filtered = !is_in_inactive_region;
            for filter in &self.symbol_filters {
                is_filtered = is_filtered | filter.filter_symbol(symbol, &file_name);
            }
            is_filtered
        };
        let mut symbol_list_builder = ShaderSymbolListBuilder::new(&filter_symbol);
        let scopes = self.query_file_scopes(symbol_tree);
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
                    &mut symbol_list_builder,
                );
            }
        }
        let symbols = symbol_list_builder.get_shader_symbol_list();
        Ok(symbols)
    }
}
