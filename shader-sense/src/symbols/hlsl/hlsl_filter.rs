use crate::shader::ShaderStage;

use crate::symbols::symbol_parser::SymbolTreeFilter;
use crate::symbols::symbols::{ShaderRange, ShaderSymbolList};

pub fn get_hlsl_filters() -> Vec<Box<dyn SymbolTreeFilter>> {
    vec![Box::new(HlslStageFilter {})]
}

struct HlslStageFilter {}

impl SymbolTreeFilter for HlslStageFilter {
    fn filter_symbols(&self, shader_symbols: &mut ShaderSymbolList, file_name: &String) {
        match ShaderStage::from_file_name(file_name) {
            Some(shader_stage) => {
                shader_symbols.filter(|symbol| {
                    symbol.stages.contains(&shader_stage) || symbol.stages.is_empty()
                });
            }
            None => {
                // No filtering
            }
        }
    }
}
