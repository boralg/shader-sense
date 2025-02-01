use std::path::Path;

use tree_sitter::Parser;

use crate::shader_error::ShaderError;

use super::{
    symbol_tree::SymbolTree,
    symbols::{ShaderPosition, ShaderPreprocessor, ShaderRange, ShaderSymbolList},
};
pub trait SymbolProvider {
    // Get underlying treesitter parser
    fn get_parser(&mut self) -> &mut Parser;
    // Get intrinsic symbols from language
    fn get_intrinsics_symbol(&self) -> &ShaderSymbolList;
    // Query preprocess information for file.
    fn query_preprocessor(&self, symbol_tree: &SymbolTree) -> ShaderPreprocessor;
    // Query file symbol from file tree.
    fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
        preprocessor: Option<&ShaderPreprocessor>,
    ) -> ShaderSymbolList;
    // Create symbol tree from file.
    fn create_tree(
        &mut self,
        file_path: &Path,
        shader_content: &str,
    ) -> Result<SymbolTree, ShaderError> {
        SymbolTree::new(self, file_path, shader_content)
    }
    // Get word at a given position.
    fn get_word_range_at_position(
        &self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<(String, ShaderRange), ShaderError>;
    // Get a struct word chain at a given position
    fn get_word_chain_range_at_position(
        &mut self,
        symbol_tree: &SymbolTree,
        position: ShaderPosition,
    ) -> Result<Vec<(String, ShaderRange)>, ShaderError>;
}
