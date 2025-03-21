use std::path::Path;

use tree_sitter::{Node, QueryMatch};

use crate::{
    include::IncludeHandler,
    shader_error::ShaderError,
    symbols::symbols::{ShaderPosition, ShaderRange, ShaderSymbolList},
};

use super::{
    symbol_provider::SymbolIncludeCallback,
    symbol_tree::SymbolTree,
    symbols::{
        ShaderPreprocessor, ShaderPreprocessorContext, ShaderRegion, ShaderScope, ShaderSymbol,
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

pub trait SymbolRegionFinder {
    fn query_regions_in_node<'a>(
        &self,
        symbol_tree: &SymbolTree,
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
        context: &'a mut ShaderPreprocessorContext,
        include_handler: &'a mut IncludeHandler,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
    ) -> Result<Vec<ShaderRegion>, ShaderError>;
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
        include_handler: &mut IncludeHandler,
    );
}

pub trait SymbolLabelChainProvider {
    fn find_label_chain_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: &ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError>;
}

pub trait SymbolLabelProvider {
    fn find_label_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: &ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError>;
}
