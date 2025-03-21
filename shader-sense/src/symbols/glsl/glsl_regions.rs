use crate::{
    include::IncludeHandler,
    shader_error::ShaderError,
    symbols::{
        hlsl::HlslSymbolRegionFinder,
        symbol_parser::SymbolRegionFinder,
        symbol_provider::SymbolIncludeCallback,
        symbol_tree::SymbolTree,
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
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
        context: &'a mut ShaderPreprocessorContext,
        include_handler: &mut IncludeHandler,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        self.region_finder.query_regions_in_node(
            symbol_tree,
            node,
            preprocessor,
            context,
            include_handler,
            include_callback,
        )
    }
}
