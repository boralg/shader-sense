use std::path::Path;

use crate::{
    shader::ShadingLanguage, shader_error::ShaderError, validator::validator::ValidationParams,
};

use super::{
    parser::SymbolParser,
    symbols::{
        parse_default_shader_intrinsics, ShaderPosition, ShaderRange, ShaderSymbol,
        ShaderSymbolData, ShaderSymbolList,
    },
    SymbolTree,
};

// This class should parse a file with a given position & return available symbols.
// It should even return all available symbols aswell as scopes, that are then recomputed
pub struct SymbolProvider {
    shader_intrinsics: ShaderSymbolList,
    symbol_parser: SymbolParser,
}

impl SymbolProvider {
    pub fn glsl() -> Self {
        Self {
            symbol_parser: SymbolParser::glsl(),
            shader_intrinsics: parse_default_shader_intrinsics(ShadingLanguage::Glsl),
        }
    }
    pub fn hlsl() -> Self {
        Self {
            symbol_parser: SymbolParser::hlsl(),
            shader_intrinsics: parse_default_shader_intrinsics(ShadingLanguage::Hlsl),
        }
    }
    pub fn wgsl() -> Self {
        Self {
            symbol_parser: SymbolParser::wgsl(),
            shader_intrinsics: parse_default_shader_intrinsics(ShadingLanguage::Wgsl),
        }
    }
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
        self.symbol_parser.create_ast(&file_path, &shader_content)
    }
    pub fn update_ast(
        &mut self,
        symbol_tree: &mut SymbolTree,
        old_shader_content: &str,
        new_shader_content: &str,
        old_range: &ShaderRange,
        new_text: &String,
    ) -> Result<(), ShaderError> {
        self.symbol_parser.update_ast(
            symbol_tree,
            new_shader_content,
            tree_sitter::Range {
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
            },
            new_text,
        )
    }

    // Get all symbols including dependencies.
    pub fn get_all_symbols(
        &self,
        symbol_tree: &SymbolTree,
        params: &ValidationParams,
    ) -> Result<ShaderSymbolList, ShaderError> {
        let mut shader_symbols = self.symbol_parser.query_local_symbols(&symbol_tree)?;
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
        for filter in &self.symbol_parser.filters {
            filter.filter_symbols(&mut shader_symbols, &file_name);
        }
        Ok(shader_symbols)
    }
    pub fn get_word_range_at_position(
        &self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError> {
        self.symbol_parser
            .find_label_at_position(symbol_tree, position)
    }
    pub fn get_word_chain_range_at_position(
        &mut self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError> {
        self.symbol_parser
            .find_label_chain_at_position(symbol_tree, position)
    }
    pub fn get_inactive_regions(
        &self,
        symbol_tree: &SymbolTree,
    ) -> Result<Vec<ShaderRange>, ShaderError> {
        self.symbol_parser.find_inactive_regions(symbol_tree)
    }
}
