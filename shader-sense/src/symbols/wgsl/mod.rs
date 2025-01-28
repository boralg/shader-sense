mod wgsl_filter;
mod wgsl_parser;
use tree_sitter::Parser;
use wgsl_parser::get_wgsl_parsers;

use super::parser::{create_symbol_parser, SymbolParser};

impl SymbolParser {
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
        }
    }
}
