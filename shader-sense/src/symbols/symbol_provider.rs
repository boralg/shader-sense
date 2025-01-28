use std::path::Path;

use tree_sitter::{InputEdit, Node, Parser, QueryCursor, Tree};

use crate::{
    shader::ShadingLanguage, shader_error::ShaderError, symbols::parser::get_name,
    validator::validator::ValidationParams,
};

use super::{
    parser::{SymbolTreeFilter, SymbolTreeParser},
    symbol_tree::SymbolTree,
    symbols::{
        ShaderPosition, ShaderRange, ShaderScope, ShaderSymbol, ShaderSymbolData, ShaderSymbolList,
    },
};

// This class should parse a file with a given position & return available symbols.
// It should even return all available symbols aswell as scopes, that are then recomputed
pub struct SymbolProvider {
    pub shader_intrinsics: ShaderSymbolList,
    pub parser: Parser,
    pub symbol_parsers: Vec<(Box<dyn SymbolTreeParser>, tree_sitter::Query)>,
    pub scope_query: tree_sitter::Query,
    pub filters: Vec<Box<dyn SymbolTreeFilter>>,
}

impl SymbolProvider {
    pub fn from(shading_language: ShadingLanguage) -> Self {
        match shading_language {
            ShadingLanguage::Wgsl => Self::wgsl(),
            ShadingLanguage::Hlsl => Self::hlsl(),
            ShadingLanguage::Glsl => Self::glsl(),
        }
    }
    pub fn get_intrinsics_symbol(&self) -> &ShaderSymbolList {
        &self.shader_intrinsics
    }
    pub fn create_ast(
        &mut self,
        file_path: &Path,
        shader_content: &str,
    ) -> Result<SymbolTree, ShaderError> {
        match self.parser.parse(shader_content, None) {
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
    pub fn update_ast(
        &mut self,
        symbol_tree: &mut SymbolTree,
        old_shader_content: &str,
        new_shader_content: &str,
        old_range: &ShaderRange,
        new_text: &String,
    ) -> Result<(), ShaderError> {
        let line_count = new_text.lines().count();
        let tree_sitter_range = tree_sitter::Range {
            start_byte: old_range.start.to_byte_offset(old_shader_content),
            end_byte: old_range.end.to_byte_offset(old_shader_content),
            start_point: tree_sitter::Point {
                row: old_range.start.line as usize,
                column: old_range.start.pos as usize,
            },
            end_point: tree_sitter::Point {
                row: old_range.end.line as usize,
                column: old_range.end.pos as usize,
            },
        };
        symbol_tree.tree.edit(&InputEdit {
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
        match self
            .parser
            .parse(new_shader_content, Some(&symbol_tree.tree))
        {
            Some(new_tree) => {
                symbol_tree.tree = new_tree;
                symbol_tree.content = new_shader_content.into();
                Ok(())
            }
            None => Err(ShaderError::ParseSymbolError(format!(
                "Failed to update AST for file {}.",
                symbol_tree.file_path.display()
            ))),
        }
    }

    // Get all symbols including dependencies.
    pub fn get_all_symbols(
        &self,
        symbol_tree: &SymbolTree,
        params: &ValidationParams,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let mut shader_symbols = self.query_local_symbols(&symbol_tree)?;
        // Add custom macros to symbol list.
        for define in &params.defines {
            shader_symbols.constants.push(ShaderSymbol {
                label: define.0.clone(),
                description: format!("Preprocessor macro (value: {})", define.1),
                version: "".into(),
                stages: Vec::new(),
                link: None,
                data: ShaderSymbolData::Constants {
                    ty: "".into(),
                    qualifier: "".into(),
                    value: define.1.clone(),
                },
                range: None,
                scope_stack: None,
            });
        }
        // Should be run directly on symbol add.
        let file_name = symbol_tree
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        for filter in &self.filters {
            filter.filter_symbols(&mut shader_symbols, &file_name);
        }
        Ok(shader_symbols)
    }
    pub fn get_word_range_at_position(
        &self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        self.find_label_at_position_in_node(symbol_tree, symbol_tree.tree.root_node(), position)
    }
    pub fn get_word_chain_range_at_position(
        &mut self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        self.find_label_chain_at_position_in_node(
            symbol_tree,
            symbol_tree.tree.root_node(),
            position,
        )
    }
    pub fn get_inactive_regions(
        &self,
        symbol_tree: &SymbolTree,
    ) -> Result<Vec<ShaderRange>, ShaderError> {
        self.find_inactive_regions_in_node(symbol_tree, symbol_tree.tree.root_node())
    }

    fn query_scopes(
        &self,
        file_path: &Path,
        shader_content: &str,
        tree: &Tree,
    ) -> Vec<ShaderScope> {
        // TODO: look for namespace aswell
        let mut query_cursor = QueryCursor::new();
        let mut scopes = Vec::new();
        for matche in query_cursor.matches(
            &self.scope_query,
            tree.root_node(),
            shader_content.as_bytes(),
        ) {
            scopes.push(ShaderScope::from_range(
                matche.captures[0].node.range(),
                file_path.into(),
            ));
        }
        scopes
    }
    pub fn query_local_symbols(
        &self,
        symbol_tree: &SymbolTree,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let scopes = self.query_scopes(
            &symbol_tree.file_path,
            &symbol_tree.content,
            &symbol_tree.tree,
        );
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
        Ok(symbols)
    }
    fn find_label_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        fn range_contain(including_range: tree_sitter::Range, position: ShaderPosition) -> bool {
            let including_range =
                ShaderRange::from_range(including_range, position.file_path.clone());
            including_range.contain(&position)
        }
        if range_contain(node.range(), position.clone()) {
            match node.kind() {
                // identifier = function name, variable...
                // type_identifier = struct name, class name...
                // primitive_type = float, uint...
                // string_content = include, should check preproc_include as parent.
                // TODO: should depend on language...
                "identifier" | "type_identifier" | "primitive_type" => {
                    return Ok((
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), symbol_tree.file_path.clone()),
                    ))
                }
                // TODO: should use string_content instead
                "string_literal" => {
                    let path = get_name(&symbol_tree.content, node);
                    return Ok((
                        path[1..path.len() - 1].into(),
                        ShaderRange::from_range(node.range(), symbol_tree.file_path.clone()),
                    ));
                }
                _ => {
                    for child in node.children(&mut node.walk()) {
                        match self.find_label_at_position_in_node(
                            symbol_tree,
                            child,
                            position.clone(),
                        ) {
                            Ok(label) => return Ok(label),
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
    fn find_label_chain_at_position_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: Node,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        fn range_contain(including_range: tree_sitter::Range, position: ShaderPosition) -> bool {
            let including_range =
                ShaderRange::from_range(including_range, position.file_path.clone());
            including_range.contain(&position)
        }
        if range_contain(node.range(), position.clone()) {
            match node.kind() {
                "identifier" => {
                    return Ok(vec![(
                        get_name(&symbol_tree.content, node).into(),
                        ShaderRange::from_range(node.range(), symbol_tree.file_path.clone()),
                    )])
                }
                "field_identifier" => {
                    let mut chain = Vec::new();
                    let mut current_node = node.prev_named_sibling().unwrap();
                    loop {
                        let field = current_node.next_named_sibling().unwrap();
                        if field.kind() == "field_identifier" {
                            chain.push((
                                get_name(&symbol_tree.content, field).into(),
                                ShaderRange::from_range(
                                    field.range(),
                                    symbol_tree.file_path.clone(),
                                ),
                            ));
                        } else {
                            return Err(ShaderError::InternalErr(format!(
                                "Unhandled case in find_label_chain_at_position_in_node: {}",
                                field.kind()
                            )));
                        }
                        match current_node.child_by_field_name("argument") {
                            Some(child) => {
                                current_node = child;
                            }
                            None => {
                                let identifier = current_node;
                                chain.push((
                                    get_name(&symbol_tree.content, identifier).into(),
                                    ShaderRange::from_range(
                                        identifier.range(),
                                        symbol_tree.file_path.clone(),
                                    ),
                                ));
                                break;
                            } // Should have already break here
                        }
                    }
                    return Ok(chain);
                }
                _ => {
                    for child in node.children(&mut node.walk()) {
                        match self.find_label_chain_at_position_in_node(
                            symbol_tree,
                            child,
                            position.clone(),
                        ) {
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
    fn find_inactive_regions_in_node(
        &self,
        _symbol_tree: &SymbolTree,
        _node: Node,
    ) -> Result<Vec<ShaderRange>, ShaderError> {
        // Query differ from lang. Need to be handled per lang
        Ok(vec![])
    }
}
