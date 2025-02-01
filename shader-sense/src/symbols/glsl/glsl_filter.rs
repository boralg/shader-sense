use crate::shader::ShaderStage;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::ShaderSymbolList;

pub fn get_glsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(GlslStageFilter {}), Box::new(GlslVersionFilter {})]
}

struct GlslVersionFilter {}

impl SymbolTreeFilter for GlslVersionFilter {
    fn filter_symbols(&self, _shader_symbols: &mut ShaderSymbolList, _file_name: &String) {
        // TODO: filter version
        // Need to have correct & verified intrinsics data
    }
}
struct GlslStageFilter {}

impl SymbolTreeFilter for GlslStageFilter {
    fn filter_symbols(&self, shader_symbols: &mut ShaderSymbolList, file_name: &String) {
        match ShaderStage::from_file_name(file_name) {
            Some(shader_stage) => {
                shader_symbols.retain(|symbol| {
                    symbol.stages.contains(&shader_stage) || symbol.stages.is_empty()
                });
            }
            None => {
                // No filtering
            }
        }
    }
}
