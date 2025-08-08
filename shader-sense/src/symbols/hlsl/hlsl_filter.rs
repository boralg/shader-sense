use crate::shader::ShaderCompilationParams;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::ShaderSymbol;

pub fn get_hlsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(HlslStageFilter {})]
}

struct HlslStageFilter {}

impl SymbolTreeFilter for HlslStageFilter {
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
