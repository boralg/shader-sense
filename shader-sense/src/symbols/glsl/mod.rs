use glsl_filter::{GlslStageFilter, GlslVersionFilter};
use tree_sitter::Parser;

use super::{
    parser::create_symbol_parser, symbol_provider::SymbolProvider, symbols::ShaderSymbolList,
};

mod glsl_filter;
mod glsl_parser;

impl SymbolProvider {
    pub fn glsl() -> Self {
        let lang = tree_sitter_glsl::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading GLSL grammar");
        Self {
            parser,
            symbol_parsers: glsl_parser::get_glsl_parsers()
                .into_iter()
                .map(|symbol_parser| create_symbol_parser(symbol_parser, &lang))
                .collect(),
            scope_query: tree_sitter::Query::new(lang.clone(), r#"(compound_statement) @scope"#)
                .unwrap(),
            filters: vec![Box::new(GlslVersionFilter {}), Box::new(GlslStageFilter {})],
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "../intrinsics/glsl-intrinsics.json"
            ))),
        }
    }
}
