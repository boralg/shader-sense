use std::path::Path;

use tree_sitter::{Node, QueryMatch};

use crate::{
    shader_error::ShaderError,
    symbols::symbols::{
        ShaderPosition, ShaderRange, ShaderSymbolData, ShaderSymbolList, ShaderSymbolListRef,
    },
};

use super::{
    symbol_provider::{SymbolIncludeCallback, SymbolProvider},
    symbol_tree::{ShaderSymbols, SymbolTree},
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
    pub fn add_call_expression(&mut self, shader_symbol: ShaderSymbol) {
        if (self.filter_callback)(&shader_symbol) {
            self.shader_symbol_list.call_expression.push(shader_symbol);
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
    pub fn get_shader_symbol_list(self) -> ShaderSymbolList {
        self.shader_symbol_list
    }
}

pub struct ShaderWordRange {
    parent: Option<Box<ShaderWordRange>>, // Box to avoid recursive struct
    word: String,
    range: ShaderRange,
}

impl ShaderWordRange {
    pub fn new(word: String, range: ShaderRange, parent: Option<ShaderWordRange>) -> Self {
        Self {
            parent: match parent {
                Some(parent) => Some(Box::new(parent)),
                None => None,
            },
            word,
            range,
        }
    }
    pub fn get_word(&self) -> &str {
        &self.word
    }
    pub fn get_range(&self) -> &ShaderRange {
        &self.range
    }
    pub fn get_parent(&self) -> Option<&ShaderWordRange> {
        self.parent.as_ref().map(|p| p.as_ref())
    }
    fn get_parent_mut(&mut self) -> Option<&mut ShaderWordRange> {
        self.parent.as_mut().map(|p| p.as_mut())
    }
    pub fn set_root_parent(&mut self, root_parent: ShaderWordRange) {
        // Use a raw pointer to traverse without holding a mutable borrow
        let mut parent: *mut ShaderWordRange = self;
        unsafe {
            while let Some(p) = (*parent).get_parent_mut() {
                parent = p;
            }
            // Now parent is the deepest node, safe to assign
            (*parent).parent = Some(Box::new(root_parent));
        }
    }
    pub fn get_word_stack(&self) -> Vec<&ShaderWordRange> {
        let mut current_word = self;
        let mut stack = Vec::new();
        stack.push(self);
        while let Some(parent) = &current_word.parent {
            stack.push(parent.as_ref());
            current_word = parent.as_ref();
        }
        stack
    }
    pub fn set_parent(&mut self, parent: ShaderWordRange) {
        self.parent = Some(Box::new(parent));
    }
    pub fn is_field(&self) -> bool {
        self.parent.is_some()
    }
    // Look for matching symbol in symbol_list
    pub fn find_symbol_from_parent<'a>(
        &self,
        symbol_list: &'a ShaderSymbolListRef,
    ) -> Vec<&'a ShaderSymbol> {
        if self.parent.is_none() {
            // Could be either a variable, a link, or a type.
            symbol_list.find_symbols_at(&self.word, &self.range.start)
        } else {
            // Will be a variable or a function if chained.
            let stack = self.get_word_stack();
            let mut rev_stack = stack.iter().rev();
            // TODO: should act on scoped symbols only.
            let mut current_symbol = match rev_stack.next() {
                Some(current_word) => match symbol_list.find_symbol(&current_word.word) {
                    Some(symbol) => {
                        match &symbol.data {
                            ShaderSymbolData::Functions { signatures } => {
                                match symbol_list.find_type_symbol(&signatures[0].returnType) {
                                    Some(ty_symbol) => ty_symbol,
                                    None => return vec![], // Did not found corresponding type symbol
                                }
                            }
                            ShaderSymbolData::Variables { ty, count: _ } => {
                                match symbol_list.find_type_symbol(ty) {
                                    Some(ty_symbol) => ty_symbol,
                                    None => return vec![], // Did not found corresponding type symbol
                                }
                            }
                            _ => return vec![], // Symbol found is not a variable nor a function.
                        }
                    }
                    None => {
                        return vec![]; // No variable found for main parent.
                    }
                },
                None => unreachable!("Should always have at least one "),
            };
            while let Some(next_item) = &rev_stack.next() {
                let symbol = match &current_symbol.data {
                    ShaderSymbolData::Struct {
                        constructors: _,
                        members,
                        methods,
                    } => {
                        if let Some(member) = members.iter().find(|m| m.label == next_item.word) {
                            match symbol_list.find_type_symbol(&member.ty) {
                                Some(ty_symbol) => ty_symbol,
                                None => return vec![], // parent is not a known type
                            }
                        } else if let Some(method) =
                            methods.iter().find(|m| m.label == next_item.word)
                        {
                            match symbol_list.find_type_symbol(&method.signature.returnType) {
                                Some(ty_symbol) => ty_symbol,
                                None => return vec![], // parent is not a known type
                            }
                        } else {
                            return vec![]; // No fit found.
                        }
                    }
                    ShaderSymbolData::Types { constructors: _ } => {
                        panic!("Should not be reached as this type cannot be accessed.")
                    }
                    ShaderSymbolData::Functions { signatures: _ } => {
                        panic!("Need to find correct signature.")
                    }
                    _ => return vec![], // Data useless.
                };
                current_symbol = symbol;
            }
            // TODO: This should return variable or function. Method and Members should be symbol for this to work.
            vec![current_symbol]
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
        symbol_provider: &SymbolProvider,
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
        context: &'a mut ShaderPreprocessorContext,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        old_symbols: Option<ShaderSymbols>,
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
        context: &mut ShaderPreprocessorContext,
    );
}

pub trait SymbolWordProvider {
    fn find_word_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: &ShaderPosition,
    ) -> Result<ShaderWordRange, ShaderError>;
}
