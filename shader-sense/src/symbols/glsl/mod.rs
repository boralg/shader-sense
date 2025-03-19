mod glsl_filter;
mod glsl_parser;
mod glsl_preprocessor;
mod glsl_regions;
mod glsl_word;
mod glsl_word_chain;

use glsl_filter::get_glsl_filters;
use glsl_parser::get_glsl_parsers;
use glsl_preprocessor::get_glsl_preprocessor_parser;
use glsl_regions::GlslRegionFinder;
use glsl_word::GlslSymbolLabelProvider;
use glsl_word_chain::GlslSymbolLabelChainProvider;

use super::symbol_provider::SymbolProvider;

pub fn create_glsl_symbol_provider(tree_sitter_language: tree_sitter::Language) -> SymbolProvider {
    SymbolProvider::new(
        tree_sitter_language.clone(),
        get_glsl_parsers(),
        get_glsl_filters(),
        get_glsl_preprocessor_parser(),
        Box::new(GlslRegionFinder::new()),
        Box::new(GlslSymbolLabelChainProvider {}),
        Box::new(GlslSymbolLabelProvider {}),
    )
}
