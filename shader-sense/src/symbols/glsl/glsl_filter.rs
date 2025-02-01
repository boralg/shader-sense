use crate::shader::ShaderStage;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::ShaderSymbol;

pub fn get_glsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(GlslStageFilter {}), Box::new(GlslVersionFilter {})]
}

struct GlslVersionFilter {}

impl SymbolTreeFilter for GlslVersionFilter {
    fn filter_symbol(&self, _shader_symbol: &ShaderSymbol, _file_name: &String) -> bool {
        // TODO: filter version
        // Need to have correct & verified intrinsics data
        true
    }
}
struct GlslStageFilter {}

impl SymbolTreeFilter for GlslStageFilter {
    fn filter_symbol(&self, shader_symbol: &ShaderSymbol, file_name: &String) -> bool {
        match ShaderStage::from_file_name(file_name) {
            Some(shader_stage) => {
                shader_symbol.stages.contains(&shader_stage) || shader_symbol.stages.is_empty()
            }
            None => true, // Not filtered
        }
    }
}
