mod glsl_parser;
mod glsl_preprocessor;
mod glsl_regions;
mod glsl_word;

use glsl_parser::get_glsl_parsers;
use glsl_preprocessor::get_glsl_preprocessor_parser;
use glsl_regions::GlslRegionFinder;
use glsl_word::GlslSymbolWordProvider;

use super::symbol_provider::SymbolProvider;

pub(super) fn create_glsl_symbol_provider(
    tree_sitter_language: tree_sitter::Language,
) -> SymbolProvider {
    SymbolProvider::new(
        tree_sitter_language.clone(),
        get_glsl_parsers(),
        get_glsl_preprocessor_parser(),
        Box::new(GlslRegionFinder::new()),
        Box::new(GlslSymbolWordProvider {}),
    )
}
