use crate::shader::ShaderCompilationParams;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::ShaderSymbol;

pub fn get_glsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(GlslStageFilter {}), Box::new(GlslVersionFilter {})]
}

struct GlslVersionFilter {}

impl SymbolTreeFilter for GlslVersionFilter {
    fn filter_symbol(
        &self,
        _shader_symbol: &ShaderSymbol,
        _shader_compilation_params: &ShaderCompilationParams,
    ) -> bool {
        // TODO: filter version
        // Need to have correct & verified intrinsics data
        true
    }
}
struct GlslStageFilter {}

impl SymbolTreeFilter for GlslStageFilter {
    fn filter_symbol(
        &self,
        shader_symbol: &ShaderSymbol,
        shader_compilation_params: &ShaderCompilationParams,
    ) -> bool {
        match shader_compilation_params.shader_stage {
            Some(shader_stage) => {
                shader_symbol.stages.contains(&shader_stage) || shader_symbol.stages.is_empty()
            }
            None => true, // Not filtered
        }
    }
}
