use std::path::Path;

use crate::symbols::{
    symbol_parser::{get_name, ShaderSymbolListBuilder, SymbolTreeParser},
    symbols::{ShaderParameter, ShaderRange, ShaderScope, ShaderSymbol, ShaderSymbolData},
};

pub fn get_wgsl_parsers() -> Vec<Box<dyn SymbolTreeParser>> {
    vec![]
}

#[allow(dead_code)] // For now, dont pollute Wgsl as its not ready
struct WgslStructTreeParser {}

impl SymbolTreeParser for WgslStructTreeParser {
    fn get_query(&self) -> String {
        r#"(struct_declaration
            name: (identifier) @struct.type
            ((struct_member
                (variable_identifier_declaration
                    name: (identifier) @struct.param.type
                    type: (type_declaration) @struct.param.decl
                )
            )(",")?)*
        )"#
        .into()
    }
    fn process_match(
        &self,
        matches: tree_sitter::QueryMatch,
        file_path: &Path,
        shader_content: &str,
        scopes: &Vec<ShaderScope>,
        symbols: &mut ShaderSymbolListBuilder,
    ) {
        let label_node = matches.captures[0].node;
        let range = ShaderRange::from_range(label_node.range(), file_path.into());
        let scope_stack = self.compute_scope_stack(&scopes, &range);
        symbols.add_type(ShaderSymbol {
            label: get_name(shader_content, matches.captures[0].node).into(),
            description: "".into(),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Struct {
                constructors: vec![], // Constructor in wgsl ?
                members: matches.captures[1..]
                    .chunks(2)
                    .map(|w| ShaderParameter {
                        ty: get_name(shader_content, w[1].node).into(),
                        label: get_name(shader_content, w[0].node).into(),
                        count: None,
                        description: "".into(),
                    })
                    .collect::<Vec<ShaderParameter>>(),
                methods: vec![],
            },
            scope: None, // TODO: compute
            range: Some(range),
            scope_stack: Some(scope_stack),
        });
    }
}
