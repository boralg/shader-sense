use crate::{
    shader_error::ShaderError,
    symbols::{
        hlsl::HlslSymbolRegionFinder,
        symbol_parser::SymbolRegionFinder,
        symbol_provider::{SymbolIncludeCallback, SymbolProvider},
        symbol_tree::{ShaderSymbols, SymbolTree},
        symbols::{ShaderPreprocessor, ShaderPreprocessorContext, ShaderRegion},
    },
};

pub struct GlslRegionFinder {
    region_finder: HlslSymbolRegionFinder,
}

impl GlslRegionFinder {
    pub fn new() -> Self {
        Self {
            region_finder: HlslSymbolRegionFinder::new(tree_sitter_glsl::language()),
        }
    }
}

impl SymbolRegionFinder for GlslRegionFinder {
    fn query_regions_in_node<'a>(
        &self,
        symbol_tree: &SymbolTree,
        symbol_provider: &SymbolProvider,
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
        context: &'a mut ShaderPreprocessorContext,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        old_symbols: Option<ShaderSymbols>,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        self.region_finder.query_regions_in_node(
            symbol_tree,
            symbol_provider,
            node,
            preprocessor,
            context,
            include_callback,
            old_symbols,
        )
    }
}
