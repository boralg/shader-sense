use std::num::ParseIntError;

use tree_sitter::{Query, QueryCursor};

use crate::{
    shader_error::{ShaderDiagnostic, ShaderDiagnosticSeverity, ShaderError},
    symbols::{
        symbol_parser::{get_name, SymbolRegionFinder},
        symbol_provider::{SymbolIncludeCallback, SymbolProvider},
        symbol_tree::{ShaderSymbols, SymbolTree},
        symbols::{
            ShaderPosition, ShaderPreprocessor, ShaderPreprocessorContext,
            ShaderPreprocessorDefine, ShaderPreprocessorInclude, ShaderRange, ShaderRegion,
        },
    },
};

// Some nice query resources
// https://davisvaughan.github.io/r-tree-sitter/reference/query-matches-and-captures.html
// https://parsiya.net/blog/knee-deep-tree-sitter-queries/
// Check https://github.com/tree-sitter/tree-sitter-cpp/blob/master/src/grammar.json & grammar.json to inspect how node are layout
// All symbols details can be found here
// https://github.com/tree-sitter/tree-sitter-cpp/blob/master/src/grammar.json
// Here the goal is mostly to compute #if regions & mark them as active or not
// This try resolving condition using macros through tree sitter.

macro_rules! assert_tree_sitter {
    ($file_path:expr, $condition:expr) => {
        if !$condition {
            return Err(ShaderError::InternalErr(format!(
                "Unexpected query failure {}:{} at : {}",
                file!(),
                line!(),
                $file_path.display(),
            )));
        }
    };
}
macro_rules! assert_field_name {
    ($file_path:expr, $cursor:expr, $value:expr) => {
        match $cursor.field_name() {
            Some(field_name) => {
                if field_name != $value {
                    return Err(ShaderError::SymbolQueryError(
                        format!(
                            "Unexpected query field name \"{}\" (at {}:{})",
                            field_name,
                            file!(),
                            line!(),
                        ),
                        ShaderRange::from_range($cursor.node().range(), &$file_path),
                    ));
                }
            }
            None => {
                return Err(ShaderError::SymbolQueryError(
                    format!("Missing query field name (at {}:{})", file!(), line!(),),
                    ShaderRange::from_range($cursor.node().range(), &$file_path),
                ));
            }
        }
    };
}
macro_rules! assert_node_kind {
    ($file_path:expr, $cursor:expr, $value:expr) => {
        if $cursor.node().kind() != $value {
            return Err(ShaderError::SymbolQueryError(
                format!(
                    "Unexpected query node kind \"{}\" (at {}:{})",
                    $cursor.node().kind(),
                    file!(),
                    line!(),
                ),
                ShaderRange::from_range($cursor.node().range(), &$file_path),
            ));
        }
    };
}
// Better API design:
/*struct SymbolTreeCursor {}
struct SymbolTreeCursorIter {
    cursor: tree_sitter::TreeCursor,
}
impl SymbolTreeCursor {
    // Iterator for siblings.
    // Each iterator
    pub fn iter() -> SymbolTreeCursorIter {}
}*/

pub struct HlslSymbolRegionFinder {
    query_if: tree_sitter::Query,
}

impl HlslSymbolRegionFinder {
    pub fn new(lang: tree_sitter::Language) -> Self {
        let query_if = Query::new(
            lang,
            r#"
        [
            (preproc_if)
            (preproc_ifdef)
        ] @inactive
        "#,
        )
        .unwrap();
        Self { query_if }
    }
    fn parse_number(number: &str) -> Result<i32, ParseIntError> {
        if number.starts_with("0x") && number.len() > 2 {
            let prefixed_number = number
                .strip_prefix("0x")
                .unwrap_or(number.strip_prefix("0X").unwrap_or(number));
            i32::from_str_radix(prefixed_number, 16)
        } else if number.starts_with("0") && number.len() > 1 {
            let prefixed_number = number.strip_prefix("0").unwrap_or(number);
            i32::from_str_radix(prefixed_number, 8)
        } else {
            number.parse::<i32>()
        }
    }
    fn get_define_as_i32_depth(
        context: &ShaderPreprocessorContext,
        name: &str,
        position: &ShaderPosition,
        depth: u32,
    ) -> Result<i32, ShaderError> {
        if depth == 0 {
            Err(ShaderError::SymbolQueryError(
                format!("Failed to parse number_literal {}", name),
                ShaderRange::new(position.clone(), position.clone()),
            ))
        } else if name.contains(" ") {
            // Here we try to detect expression that cannot be parsed, such as expression (#define MACRO (MACRO0 + MACRO1)).
            // TODO: We should store a proxy tree that is used to query over it for solving them.
            Err(ShaderError::SymbolQueryError(
                format!("Macro expression solving not implemented ({}).", name),
                ShaderRange::new(position.clone(), position.clone()),
            ))
        } else {
            // Here we recurse define value cuz a define might just be an alias for another define.
            // If we dont manage to parse it as a number, parse it as another define.
            match context.get_define_value(name) {
                Some(value) => match Self::parse_number(value.as_str()) {
                    Ok(parsed_value) => Ok(parsed_value),
                    Err(_) => {
                        Self::get_define_as_i32_depth(&context, value.as_str(), position, depth - 1)
                    }
                },
                None => Ok(0), // Return false instead of error
            }
        }
    }
    fn get_define_as_i32(
        context: &ShaderPreprocessorContext,
        name: &str,
        position: &ShaderPosition,
    ) -> Result<i32, ShaderError> {
        match context.get_define_value(name) {
            Some(value) => match value.parse::<i32>() {
                Ok(parsed_value) => Ok(parsed_value),
                Err(_) => {
                    // Recurse result up to 10 times for macro that define other macro.
                    Self::get_define_as_i32_depth(&context, value.as_str(), &position, 10)
                }
            },
            None => Ok(0), // Return false instead of error
        }
    }
    fn is_define_defined(context: &ShaderPreprocessorContext, name: &str) -> i32 {
        context.get_define_value(name).is_some() as i32
    }
    fn resolve_condition(
        cursor: tree_sitter::TreeCursor,
        symbol_tree: &SymbolTree,
        context: &ShaderPreprocessorContext,
    ) -> Result<i32, ShaderError> {
        let mut cursor = cursor;
        match cursor.node().kind() {
            "preproc_defined" => {
                // check if macro is defined.
                let _ = r#"condition: (preproc_defined
                    "defined"
                    "("?
                    (identifier) @identifier
                    ")"?
                )"#;
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
                assert_node_kind!(symbol_tree.file_path, cursor, "defined");
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                // paranthesis not mandatory...
                if cursor.node().kind() == "(" {
                    assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                }
                assert_node_kind!(symbol_tree.file_path, cursor, "identifier");
                let condition_macro = get_name(&symbol_tree.content, cursor.node());
                Ok(Self::is_define_defined(context, condition_macro))
            }
            "number_literal" => {
                // As simple as it is, but need to be warry of hexa or octal values.
                let _ = r#"condition: (number_literal)"#;
                let number_str = get_name(&symbol_tree.content, cursor.node());
                let parsed_number = Self::parse_number(number_str);
                match parsed_number {
                    Ok(value) => Ok(value),
                    Err(_) => Err(ShaderError::SymbolQueryError(
                        format!("Failed to parse number_literal {}", number_str),
                        ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                    )),
                }
            }
            "identifier" => {
                // An identifier is simply a macro.
                let _ = r#"condition: (identifier)"#;
                let condition_macro = get_name(&symbol_tree.content, cursor.node());
                let value = Self::get_define_as_i32(
                    context,
                    condition_macro,
                    &ShaderPosition::from_tree_sitter_point(
                        cursor.node().start_position(),
                        &symbol_tree.file_path,
                    ),
                )?;
                Ok(value)
            }
            "binary_expression" => {
                // A binary expression might compare two expression or identifier. Recurse them.
                let _ = r#"condition: (binary_expression
                    left:(_) @identifier.or.expression
                    operator:(_) @binary.operator
                    right(_)  @identifier.or.expression
                )"#;
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
                assert_field_name!(symbol_tree.file_path, cursor, "left");
                let left_condition = Self::resolve_condition(cursor.clone(), symbol_tree, context)?;
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                assert_field_name!(symbol_tree.file_path, cursor, "operator");
                let operator = cursor.node().kind();
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                assert_field_name!(symbol_tree.file_path, cursor, "right");
                let right_condition =
                    Self::resolve_condition(cursor.clone(), symbol_tree, context)?;
                match operator {
                    "&&" => Ok(((left_condition != 0) && (right_condition != 0)) as i32),
                    "||" => Ok(((left_condition != 0) || (right_condition != 0)) as i32),
                    "|" => Ok(((left_condition != 0) | (right_condition != 0)) as i32),
                    "&" => Ok(((left_condition != 0) & (right_condition != 0)) as i32),
                    "^" => Ok(left_condition ^ right_condition),
                    "==" => Ok((left_condition == right_condition) as i32),
                    "!=" => Ok((left_condition != right_condition) as i32),
                    "<" => Ok((left_condition < right_condition) as i32),
                    ">" => Ok((left_condition > right_condition) as i32),
                    "<=" => Ok((left_condition <= right_condition) as i32),
                    ">=" => Ok((left_condition >= right_condition) as i32),
                    ">>" => Ok(left_condition >> right_condition),
                    "<<" => Ok(left_condition << right_condition),
                    "+" => Ok(left_condition + right_condition),
                    "-" => Ok(left_condition - right_condition),
                    "*" => Ok(left_condition * right_condition),
                    "/" => Ok(left_condition / right_condition),
                    "%" => Ok(left_condition % right_condition),
                    value => Err(ShaderError::SymbolQueryError(
                        format!("Binary operator unhandled for {}", value),
                        ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                    )),
                }
            }
            "unary_expression" => {
                let _ = r#"condition: (unary_expression 
                    operator: ["!" "-" "+" "~"] @operator
                    argument: (identifier) @identifier
                )"#;
                // Operator = !~-+
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
                assert_field_name!(symbol_tree.file_path, cursor, "operator");
                let operator = cursor.node().kind();
                assert_node_kind!(symbol_tree.file_path, cursor, "!");
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                assert_field_name!(symbol_tree.file_path, cursor, "argument");
                let value = Self::resolve_condition(cursor.clone(), symbol_tree, context)?;
                match operator {
                    "!" => Ok(!(value != 0) as i32), // Comparing as bool
                    "+" => Ok(value),
                    "-" => Ok(-value),
                    "~" => Ok(!value),
                    value => Err(ShaderError::SymbolQueryError(
                        format!("Unary operator unhandled for {}", value),
                        ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                    )),
                }
            }
            "parenthesized_expression" => {
                // This expression children is an anonymous condition.
                let _ = r#"condition: (parenthesized_expression 
                    "("
                    (_) @anykindofexpression
                    ")"
                )"#;
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                Self::resolve_condition(cursor, symbol_tree, context)
            }
            "call_expression" => {
                // This expression is a function call
                let _ = r#"condition: (call_expression 
                    function: (identifier) @function.name
                    arguments: (argument_list
                        "("
                        (
                            (identifier) @argument
                            (",")?
                        )*
                        ")"
                    )
                )"#;
                // TODO: should solve this complex expression, simply ignoring it for now.
                Err(ShaderError::SymbolQueryError(
                    format!(
                        "Call expression solving not implemented ({}).",
                        get_name(&symbol_tree.content, cursor.node())
                    ),
                    ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                ))
            }
            value => Err(ShaderError::SymbolQueryError(
                format!("Condition unhandled for {}", value),
                ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
            )),
        }
    }
}
impl SymbolRegionFinder for HlslSymbolRegionFinder {
    fn query_regions_in_node<'a>(
        &self,
        symbol_tree: &SymbolTree,
        symbol_provider: &SymbolProvider,
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
        context: &'a mut ShaderPreprocessorContext,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        mut old_symbols: Option<ShaderSymbols>,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        fn get_defined_macros_for_position(
            last_position: &ShaderPosition,
            position: &ShaderPosition,
            defines: &Vec<ShaderPreprocessorDefine>,
        ) -> Vec<ShaderPreprocessorDefine> {
            // TODO: avoid duplicated macros.
            defines
                .iter()
                .filter(|define| match define.get_range() {
                    Some(range) => range.start >= *last_position && range.end <= *position,
                    None => false, // Global define, already filled ?
                })
                .cloned()
                .collect::<Vec<ShaderPreprocessorDefine>>()
        }
        // Query regions
        let mut query_cursor = QueryCursor::new();
        let mut regions = Vec::new();
        let mut last_processed_position = ShaderPosition::zero(symbol_tree.file_path.clone());
        for region_match in
            query_cursor.matches(&self.query_if, node, symbol_tree.content.as_bytes())
        {
            fn parse_region<'a>(
                symbol_tree: &SymbolTree,
                symbol_provider: &SymbolProvider,
                preprocessor: &mut ShaderPreprocessor,
                cursor: &mut tree_sitter::TreeCursor,
                found_active_region: bool,
                context: &mut ShaderPreprocessorContext,
                include_callback: &mut SymbolIncludeCallback<'a>,
                old_symbols: &mut Option<ShaderSymbols>,
                last_processed_position: &mut ShaderPosition,
            ) -> Result<Vec<ShaderRegion>, ShaderError> {
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
                // Process includes here as they will impact defines which impact regions.
                let region_start = ShaderPosition::from_tree_sitter_point(
                    cursor.node().range().start_point,
                    &symbol_tree.file_path,
                );
                // Find include before this region that defines its context.
                // Need to filter already processed includes.
                let includes_before = preprocessor
                    .includes
                    .iter_mut()
                    .filter(|include| {
                        include.get_range().end < region_start
                            && include.get_range().start > *last_processed_position
                    })
                    .collect::<Vec<&mut ShaderPreprocessorInclude>>();
                for include_before in includes_before {
                    // Mark as processed before computing its child.
                    context.append_defines(get_defined_macros_for_position(
                        last_processed_position,
                        &include_before.get_range().start,
                        &preprocessor.defines,
                    ));
                    match symbol_provider.process_include(context, include_before, include_callback)
                    {
                        Ok(_) => {}
                        Err(err) => match err {
                            ShaderError::SymbolQueryError(message, shader_range) => {
                                // Include not found or limit reached.
                                preprocessor.diagnostics.push(ShaderDiagnostic {
                                    severity: ShaderDiagnosticSeverity::Warning,
                                    error: message,
                                    range: shader_range,
                                });
                            }
                            err => Err(err)?, // Propagate the error.
                        },
                    }
                    *last_processed_position = include_before.get_range().end.clone();
                }
                // Include should handle adding defines, but if no include or defines after last include, need to add remaining ones.
                context.append_defines(get_defined_macros_for_position(
                    last_processed_position,
                    &region_start,
                    &preprocessor.defines,
                ));
                *last_processed_position = region_start;
                // Process regions
                let (is_active_region, region_start) = match cursor.node().kind() {
                    "#ifdef" => {
                        assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                        assert_field_name!(symbol_tree.file_path, cursor, "name");
                        let condition_macro = get_name(&symbol_tree.content, cursor.node());
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            HlslSymbolRegionFinder::is_define_defined(context, condition_macro),
                            position,
                        )
                    }
                    "#ifndef" => {
                        assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                        assert_field_name!(symbol_tree.file_path, cursor, "name");
                        let condition_macro = get_name(&symbol_tree.content, cursor.node());
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            1 - HlslSymbolRegionFinder::is_define_defined(context, condition_macro),
                            position,
                        )
                    }
                    "#if" => {
                        assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                        assert_field_name!(symbol_tree.file_path, cursor, "condition");
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            match HlslSymbolRegionFinder::resolve_condition(
                                cursor.clone(),
                                symbol_tree,
                                context,
                            ) {
                                Ok(value) => value,
                                Err(err) => {
                                    match err.into_diagnostic(ShaderDiagnosticSeverity::Warning) {
                                        Some(diagnostic) => {
                                            preprocessor.diagnostics.push(diagnostic);
                                            0 // Return 0 as default
                                        }
                                        None => return Err(err),
                                    }
                                }
                            },
                            position,
                        )
                    }
                    "#else" => (
                        if found_active_region { 0 } else { 1 },
                        ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        ),
                    ),
                    "#elif" => {
                        assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                        assert_field_name!(symbol_tree.file_path, cursor, "condition");
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            if found_active_region {
                                0
                            } else {
                                match HlslSymbolRegionFinder::resolve_condition(
                                    cursor.clone(),
                                    symbol_tree,
                                    context,
                                ) {
                                    Ok(value) => value,
                                    Err(err) => match err
                                        .into_diagnostic(ShaderDiagnosticSeverity::Warning)
                                    {
                                        Some(diagnostic) => {
                                            preprocessor.diagnostics.push(diagnostic);
                                            0 // Return 0 as default
                                        }
                                        None => return Err(err),
                                    },
                                }
                            },
                            position,
                        )
                    }
                    //"#elifdef" => {}
                    child => {
                        return Err(ShaderError::SymbolQueryError(
                            format!("preproc operator not implemented: {}", child),
                            ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                        ));
                    }
                };
                // Now find alternative blocks & loop on it
                let mut regions = Vec::new();
                let mut previous_node = cursor.node();
                while cursor.goto_next_sibling() {
                    match cursor.field_name() {
                        Some(field_name) => {
                            if field_name == "alternative" {
                                let region_end = ShaderPosition::from_tree_sitter_point(
                                    previous_node.range().end_point,
                                    &symbol_tree.file_path,
                                );
                                let region_range = ShaderRange::new(region_start, region_end);
                                regions.push(ShaderRegion::new(
                                    region_range.clone(),
                                    is_active_region != 0,
                                ));
                                // Filter out local define & include in region as it may impact next region in file.
                                preprocessor
                                    .defines
                                    .retain(|define| match &define.get_range() {
                                        Some(range) => {
                                            is_active_region != 0
                                                || (is_active_region == 0
                                                    && !region_range.contain_bounds(&range))
                                        }
                                        None => true,
                                    });
                                preprocessor.includes.retain(|include| {
                                    is_active_region != 0
                                        || (is_active_region == 0
                                            && !region_range.contain_bounds(&include.get_range()))
                                });

                                let mut next_region = parse_region(
                                    symbol_tree,
                                    symbol_provider,
                                    preprocessor,
                                    cursor,
                                    (is_active_region != 0) || found_active_region,
                                    context,
                                    include_callback,
                                    old_symbols,
                                    last_processed_position,
                                )?;
                                regions.append(&mut next_region);
                                return Ok(regions);
                            } else {
                                previous_node = cursor.node();
                            }
                        }
                        None => {
                            match cursor.node().kind() {
                                // Don't store endif because we already reached the end.
                                "#endif" => {}
                                // Some node end their line too late vs their content bcs of new line node.
                                // Read their content last node instead and skip new line.
                                "preproc_call" | "preproc_def" => {
                                    let mut internal_cursor = cursor.clone();
                                    internal_cursor.goto_first_child();
                                    previous_node = internal_cursor.node();
                                    while internal_cursor.goto_next_sibling() {
                                        if internal_cursor.node().kind() != "\n" {
                                            previous_node = internal_cursor.node();
                                        }
                                    }
                                }
                                _ => previous_node = cursor.node(),
                            }
                        }
                    }
                }
                let end_position = ShaderPosition::from_tree_sitter_point(
                    previous_node.range().end_point,
                    &symbol_tree.file_path,
                );
                let region_range = ShaderRange::new(region_start, end_position);
                regions.push(ShaderRegion::new(
                    region_range.clone(),
                    is_active_region != 0,
                ));
                // Filter out local define & include in region as it may impact next region in file.
                preprocessor
                    .defines
                    .retain(|define| match &define.get_range() {
                        Some(range) => {
                            is_active_region != 0
                                || (is_active_region == 0 && !region_range.contain_bounds(&range))
                        }
                        None => true,
                    });
                preprocessor.includes.retain(|include| {
                    is_active_region != 0
                        || (is_active_region == 0
                            && !region_range.contain_bounds(&include.get_range()))
                });
                Ok(regions)
            }
            let node = region_match.captures[0].node;
            let mut cursor = node.walk();
            regions.append(&mut parse_region(
                symbol_tree,
                symbol_provider,
                preprocessor,
                &mut cursor,
                false,
                context,
                include_callback,
                &mut old_symbols,
                &mut last_processed_position,
            )?)
        }
        // Handle includes that were not dealt by regions.
        let include_lefts = preprocessor
            .includes
            .iter_mut()
            .filter(|include| include.get_range().start > last_processed_position)
            .collect::<Vec<&mut ShaderPreprocessorInclude>>();
        for include_left in include_lefts {
            context.append_defines(get_defined_macros_for_position(
                &last_processed_position,
                &include_left.get_range().start,
                &preprocessor.defines,
            ));
            // Avoid stack overflow
            match symbol_provider.process_include(context, include_left, include_callback) {
                Ok(_) => {}
                Err(err) => match err {
                    ShaderError::SymbolQueryError(message, shader_range) => {
                        // Include not found or limit reached.
                        preprocessor.diagnostics.push(ShaderDiagnostic {
                            severity: ShaderDiagnosticSeverity::Warning,
                            error: message,
                            range: shader_range,
                        });
                    }
                    err => Err(err)?, // Propagate the error.
                },
            }
            last_processed_position = include_left.get_range().end.clone();
        }
        let define_after_last_include = preprocessor
            .defines
            .iter()
            .filter(|define| match &define.get_range() {
                Some(range) => range.start > last_processed_position,
                None => false, // Global, should already be added ?
            })
            .cloned()
            .collect::<Vec<ShaderPreprocessorDefine>>();
        context.append_defines(define_after_last_include);

        Ok(regions)
    }
}
