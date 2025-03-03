use std::path::Path;

use crate::symbols::symbol_parser::ShaderSymbolListBuilder;

use crate::symbols::{
    symbol_parser::{get_name, SymbolTreeParser},
    symbols::{
        ShaderParameter, ShaderRange, ShaderScope, ShaderSignature, ShaderSymbol, ShaderSymbolData,
    },
};

pub fn get_glsl_parsers() -> Vec<Box<dyn SymbolTreeParser>> {
    vec![
        Box::new(GlslFunctionTreeParser {}),
        Box::new(GlslStructTreeParser {}),
        Box::new(GlslVariableTreeParser {}),
        Box::new(GlslUniformBlock {}),
    ]
}

struct GlslFunctionTreeParser {}

impl SymbolTreeParser for GlslFunctionTreeParser {
    fn get_query(&self) -> String {
        // could use include_str! for scm file.
        r#"(function_definition
            type: (_) @function.return
            declarator: (function_declarator
                declarator: (identifier) @function.label
                parameters: (parameter_list 
                    ((parameter_declaration
                        type: (_) @function.param.type
                        declarator: (_) @function.param.decl
                    )(",")?)*
                )
            )
            body: (compound_statement) @function.scope
            )"#
        .into() // compound_statement is function scope.
    }
    fn process_match(
        &self,
        matches: tree_sitter::QueryMatch,
        file_path: &Path,
        shader_content: &str,
        scopes: &Vec<ShaderScope>,
        symbols: &mut ShaderSymbolListBuilder,
    ) {
        let label_node = matches.captures[1].node;
        let range = ShaderRange::from_range(label_node.range(), file_path.into());
        let scope_stack = self.compute_scope_stack(scopes, &range);
        // Query internal scopes variables
        /*let scope_node = matche.captures[matche.captures.len() - 1].node;
        let content_scope_stack = {
            let mut s = scope_stack.clone();
            s.push(range.clone());
            s
        };
        query_variables(file_path, &shader_content[scope_node.range().start_byte.. scope_node.range().end_byte], scope_node, {
            let mut s = scope_stack.clone();
            s.push(range.clone());
            s
        });*/
        symbols.add_function(ShaderSymbol {
            label: get_name(shader_content, matches.captures[1].node).into(),
            description: "".into(),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Functions {
                signatures: vec![ShaderSignature {
                    returnType: get_name(shader_content, matches.captures[0].node).into(),
                    description: "".into(),
                    parameters: matches.captures[2..matches.captures.len() - 1]
                        .chunks(2)
                        .map(|w| ShaderParameter {
                            ty: get_name(shader_content, w[0].node).into(),
                            label: get_name(shader_content, w[1].node).into(),
                            description: "".into(),
                        })
                        .collect::<Vec<ShaderParameter>>(),
                }],
            },
            range: Some(range),
            scope_stack: Some(scope_stack), // In GLSL, all function are global scope.
        });
    }
}

struct GlslUniformBlock {}

impl SymbolTreeParser for GlslUniformBlock {
    fn get_query(&self) -> String {
        r#"(declaration
            (identifier) @uniform.identifier
            (field_declaration_list
                (field_declaration 
                    type: (_) @uniform.param.type
                    declarator: (_) @uniform.param.decl
                )+
            )
            (identifier)? @uniform.name
        )"#
        .into()
    }
    fn process_match(
        &self,
        matches: tree_sitter::QueryMatch,
        file_path: &Path,
        shader_content: &str,
        _scopes: &Vec<ShaderScope>,
        symbols: &mut ShaderSymbolListBuilder,
    ) {
        let capture_count = matches.captures.len();
        if capture_count % 2 == 0 {
            // name
            let identifier_node = matches.captures[0].node;
            let identifier_range =
                ShaderRange::from_range(identifier_node.range(), file_path.into());
            symbols.add_type(ShaderSymbol {
                label: get_name(shader_content, identifier_node).into(),
                description: "".into(),
                version: "".into(),
                stages: vec![],
                link: None,
                data: ShaderSymbolData::Struct {
                    members: matches.captures[1..capture_count - 1]
                        .chunks(2)
                        .map(|w| ShaderParameter {
                            ty: get_name(shader_content, w[0].node).into(),
                            label: get_name(shader_content, w[1].node).into(),
                            description: "".into(),
                        })
                        .collect::<Vec<ShaderParameter>>(),
                    methods: vec![],
                },
                range: Some(identifier_range),
                scope_stack: None, // Uniform are global stack in GLSL.
            });
            // Add variable of type
            let variable_node = matches.captures.last().unwrap().node;
            let variable_range = ShaderRange::from_range(variable_node.range(), file_path.into());
            symbols.add_variable(ShaderSymbol {
                label: get_name(shader_content, variable_node).into(),
                description: "".into(),
                version: "".into(),
                stages: vec![],
                link: None,
                data: ShaderSymbolData::Variables {
                    ty: get_name(shader_content, identifier_node).into(),
                },
                range: Some(variable_range),
                scope_stack: None, // Uniform are global stack in GLSL.
            });
        } else {
            // no name, content global
            let _identifier_node = matches.captures[0].node;
            for uniform_value in matches.captures[1..].chunks(2) {
                let label_node = uniform_value[1].node;
                let range = ShaderRange::from_range(label_node.range(), file_path.into());
                symbols.add_variable(ShaderSymbol {
                    label: get_name(shader_content, uniform_value[1].node).into(),
                    description: "".into(),
                    version: "".into(),
                    stages: vec![],
                    link: None,
                    data: ShaderSymbolData::Variables {
                        ty: get_name(shader_content, uniform_value[0].node).into(),
                    },
                    range: Some(range),
                    scope_stack: None, // Uniform are global stack in GLSL.
                });
            }
        }
    }
}

struct GlslStructTreeParser {}

impl SymbolTreeParser for GlslStructTreeParser {
    fn get_query(&self) -> String {
        r#"(struct_specifier
            name: (type_identifier) @struct.type
            body: (field_declaration_list
                (field_declaration 
                    type: (_) @struct.param.type
                    declarator: (_) @struct.param.decl
                )+
            )
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
                members: matches.captures[1..]
                    .chunks(2)
                    .map(|w| ShaderParameter {
                        ty: get_name(shader_content, w[0].node).into(),
                        label: get_name(shader_content, w[1].node).into(),
                        description: "".into(),
                    })
                    .collect::<Vec<ShaderParameter>>(),
                methods: vec![],
            },
            range: Some(range),
            scope_stack: Some(scope_stack),
        });
    }
}
struct GlslVariableTreeParser {}

impl SymbolTreeParser for GlslVariableTreeParser {
    fn get_query(&self) -> String {
        r#"(declaration
            type: [
                (type_identifier) @variable.type
                (primitive_type) @variable.type
            ]
            declarator: [(init_declarator
                declarator: (identifier) @variable.label
                value: (_) @variable.value
            ) 
            (identifier) @variable.label
            ]
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
        let label_node = matches.captures[1].node;
        let range = ShaderRange::from_range(label_node.range(), file_path.into());
        let scope_stack = self.compute_scope_stack(&scopes, &range);
        // Check if its parameter or struct element.
        let _type_qualifier = get_name(shader_content, matches.captures[0].node);
        // TODO: handle values & qualifiers..
        //let _value = get_name(shader_content, matche.captures[2].node);
        symbols.add_variable(ShaderSymbol {
            label: get_name(shader_content, matches.captures[1].node).into(),
            description: "".into(),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Variables {
                ty: get_name(shader_content, matches.captures[0].node).into(),
            },
            range: Some(range),
            scope_stack: Some(scope_stack),
        });
    }
}
