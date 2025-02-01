use crate::shader::ShaderStage;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::ShaderSymbol;

pub fn get_hlsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(HlslStageFilter {})]
}

struct HlslStageFilter {}

impl SymbolTreeFilter for HlslStageFilter {
    fn filter_symbol(&self, shader_symbol: &ShaderSymbol, file_name: &String) -> bool {
        match ShaderStage::from_file_name(file_name) {
            Some(shader_stage) => {
                shader_symbol.stages.contains(&shader_stage) || shader_symbol.stages.is_empty()
            }
            None => true, // Not filtered
        }
    }
}
