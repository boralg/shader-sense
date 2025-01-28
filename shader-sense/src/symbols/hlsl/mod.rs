mod hlsl_filter;
mod hlsl_parser;
use hlsl_parser::get_hlsl_parsers;
use tree_sitter::Parser;

use super::{
    parser::create_symbol_parser, symbol_provider::SymbolProvider, symbols::ShaderSymbolList,
};

impl SymbolProvider {
    pub fn hlsl() -> Self {
        let lang = tree_sitter_hlsl::language();
        let mut parser = Parser::new();
        parser
            .set_language(lang.clone())
            .expect("Error loading HLSL grammar");
        Self {
            parser,
            symbol_parsers: get_hlsl_parsers()
                .into_iter()
                .map(|symbol_parser| create_symbol_parser(symbol_parser, &lang))
                .collect(),
            scope_query: tree_sitter::Query::new(lang.clone(), r#"(compound_statement) @scope"#)
                .unwrap(),
            filters: vec![],
            shader_intrinsics: ShaderSymbolList::parse_from_json(String::from(include_str!(
                "../intrinsics/hlsl-intrinsics.json"
            ))),
        }
    }
}
