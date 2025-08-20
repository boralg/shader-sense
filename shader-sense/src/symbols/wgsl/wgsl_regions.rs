use crate::{
    shader::ShaderCompilationParams,
    shader_error::ShaderError,
    symbols::{
        shader_module::{ShaderModule, ShaderSymbols},
        symbol_parser::SymbolRegionFinder,
        symbol_provider::{SymbolIncludeCallback, SymbolProvider},
        symbols::{ShaderPreprocessor, ShaderPreprocessorContext, ShaderRegion},
    },
};

pub struct WgslRegionFinder {}

impl SymbolRegionFinder for WgslRegionFinder {
    fn query_regions_in_node<'a>(
        &self,
        _shader_module: &ShaderModule,
        _symbol_provider: &SymbolProvider,
        _shader_params: &ShaderCompilationParams,
        _node: tree_sitter::Node,
        _preprocessor: &mut ShaderPreprocessor,
        _context: &'a mut ShaderPreprocessorContext,
        _include_callback: &'a mut SymbolIncludeCallback<'a>,
        _old_symbols: Option<ShaderSymbols>,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        Ok(vec![])
    }
}
