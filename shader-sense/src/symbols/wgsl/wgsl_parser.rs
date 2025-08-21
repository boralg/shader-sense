use std::path::Path;

use crate::{
    position::{ShaderFileRange, ShaderRange},
    symbols::{
        symbol_parser::{get_name, ShaderSymbolListBuilder, SymbolTreeParser},
        symbols::{ShaderMember, ShaderParameter, ShaderScope, ShaderSymbol, ShaderSymbolData},
    },
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
        let range = ShaderFileRange::from(file_path.into(), ShaderRange::from(label_node.range()));
        let scope_stack = self.compute_scope_stack(&scopes, &range);
        let struct_name: String = get_name(shader_content, matches.captures[0].node).into();
        symbols.add_type(ShaderSymbol {
            label: struct_name.clone(),
            description: "".into(),
            requirement: None,
            link: None,
            data: ShaderSymbolData::Struct {
                constructors: vec![], // Constructor in wgsl ?
                members: matches.captures[1..]
                    .chunks(2)
                    .map(|w| ShaderMember {
                        context: struct_name.clone(),
                        parameters: ShaderParameter {
                            ty: get_name(shader_content, w[1].node).into(),
                            label: get_name(shader_content, w[0].node).into(),
                            count: None,
                            description: "".into(),
                            range: Some(ShaderFileRange::from(
                                file_path.into(),
                                ShaderRange::from(w[0].node.range()),
                            )),
                        },
                    })
                    .collect(),
                methods: vec![],
            },
            scope: None, // TODO: compute
            range: Some(range),
            scope_stack: Some(scope_stack),
        });
    }
}
