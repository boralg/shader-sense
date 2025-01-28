mod wgsl_filter;
mod wgsl_parser;
use tree_sitter::Parser;
use wgsl_parser::get_wgsl_parsers;

use super::{
    parser::create_symbol_parser,
    symbol_provider::SymbolProvider,
    symbols::{parse_default_shader_intrinsics, ShaderSymbolList},
};

impl SymbolProvider {
    pub fn wgsl() -> Self {
        let lang = tree_sitter_wgsl_bevy::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading WGSL grammar");
        Self {
            parser,
            symbol_parsers: get_wgsl_parsers()
                .into_iter()
                .map(|symbol_parser| create_symbol_parser(symbol_parser, &lang))
                .collect(),
            scope_query: tree_sitter::Query::new(lang.clone(), r#"(compound_statement) @scope"#)
                .unwrap(),
            filters: vec![],
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "../intrinsics/wgsl-intrinsics.json"
            ))),
        }
    }
}
