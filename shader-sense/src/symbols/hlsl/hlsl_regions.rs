use std::num::ParseIntError;

use tree_sitter::{Query, QueryCursor};

use crate::{
    shader_error::{ShaderDiagnostic, ShaderDiagnosticSeverity, ShaderError},
    symbols::{
        symbol_parser::{get_name, SymbolRegionFinder},
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderPreprocessor, ShaderRange, ShaderRegion},
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
                        ShaderRange::new(
                            ShaderPosition::new(
                                $file_path.clone(),
                                $cursor.node().range().start_point.row as u32,
                                $cursor.node().range().start_point.column as u32,
                            ),
                            ShaderPosition::new(
                                $file_path.clone(),
                                $cursor.node().range().end_point.row as u32,
                                $cursor.node().range().end_point.column as u32,
                            ),
                        ),
                    ));
                }
            }
            None => {
                return Err(ShaderError::SymbolQueryError(
                    format!("Missing query field name (at {}:{})", file!(), line!(),),
                    ShaderRange::new(
                        ShaderPosition::new(
                            $file_path.clone(),
                            $cursor.node().range().start_point.row as u32,
                            $cursor.node().range().start_point.column as u32,
                        ),
                        ShaderPosition::new(
                            $file_path.clone(),
                            $cursor.node().range().end_point.row as u32,
                            $cursor.node().range().end_point.column as u32,
                        ),
                    ),
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
                ShaderRange::new(
                    ShaderPosition::new(
                        $file_path.clone(),
                        $cursor.node().range().start_point.row as u32,
                        $cursor.node().range().start_point.column as u32,
                    ),
                    ShaderPosition::new(
                        $file_path.clone(),
                        $cursor.node().range().end_point.row as u32,
                        $cursor.node().range().end_point.column as u32,
                    ),
                ),
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
    pub fn new() -> Self {
        let query_if = Query::new(
            tree_sitter_hlsl::language(),
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
    fn get_define_value(
        preprocessor: &ShaderPreprocessor,
        name: &str,
        position: &ShaderPosition,
    ) -> Option<String> {
        let context = preprocessor
            .context
            .defines
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone());
        if let Some(value) = context {
            return Some(value);
        }
        let local = preprocessor
            .defines
            .iter()
            .find(|define| {
                let same_name = define.name == name;
                let has_range = define.range.is_some(); // None, mean global define.
                let same_file = has_range
                    && define.range.as_ref().unwrap().start.file_path == position.file_path;
                // Check position
                let defined_before =
                    (same_file && define.range.as_ref().unwrap().start < *position) || !same_file;
                same_name && defined_before
            })
            .map(|e| e.value.clone());
        if let Some(value) = local {
            return value;
        } else {
            return None;
        }
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
        preprocessor: &ShaderPreprocessor,
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
                format!("Macro expression solving not implemented ({}). Regions behaviour may be incorrect.", name),
                ShaderRange::new(position.clone(), position.clone()),
            ))
        } else {
            // Here we recurse define value cuz a define might just be an alias for another define.
            // If we dont manage to parse it as a number, parse it as another define.
            match Self::get_define_value(preprocessor, name, position) {
                Some(value) => match Self::parse_number(value.as_str()) {
                    Ok(parsed_value) => Ok(parsed_value),
                    Err(_) => Self::get_define_as_i32_depth(
                        &preprocessor,
                        value.as_str(),
                        position,
                        depth - 1,
                    ),
                },
                None => Ok(0), // Return false instead of error
            }
        }
    }
    fn get_define_as_i32(
        preprocessor: &ShaderPreprocessor,
        name: &str,
        position: &ShaderPosition,
    ) -> Result<i32, ShaderError> {
        match Self::get_define_value(preprocessor, name, position) {
            Some(value) => match value.parse::<i32>() {
                Ok(parsed_value) => Ok(parsed_value),
                Err(_) => {
                    // Recurse result up to 10 times for macro that define other macro.
                    Self::get_define_as_i32_depth(&preprocessor, value.as_str(), &position, 10)
                }
            },
            None => Ok(0), // Return false instead of error
        }
    }
    fn is_define_defined(
        preprocessor: &ShaderPreprocessor,
        name: &str,
        position: &ShaderPosition,
    ) -> i32 {
        Self::get_define_value(preprocessor, name, position).is_some() as i32
    }
    fn resolve_condition(
        cursor: tree_sitter::TreeCursor,
        symbol_tree: &SymbolTree,
        preprocessor: &ShaderPreprocessor,
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
                let position = ShaderPosition::from_tree_sitter_point(
                    cursor.node().range().end_point,
                    &symbol_tree.file_path,
                );
                Ok(Self::is_define_defined(
                    preprocessor,
                    condition_macro,
                    &position,
                ))
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
                    preprocessor,
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
                let left_condition =
                    Self::resolve_condition(cursor.clone(), symbol_tree, preprocessor)?;
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                assert_field_name!(symbol_tree.file_path, cursor, "operator");
                let operator = cursor.node().kind();
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_next_sibling());
                assert_field_name!(symbol_tree.file_path, cursor, "right");
                let right_condition =
                    Self::resolve_condition(cursor.clone(), symbol_tree, preprocessor)?;
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
                let value = Self::resolve_condition(cursor.clone(), symbol_tree, preprocessor)?;
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
                Self::resolve_condition(cursor, symbol_tree, preprocessor)
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
                    format!("Call expression solving not implemented ({}). Regions behaviour may be incorrect.", get_name(&symbol_tree.content, cursor.node())),
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
    fn query_regions_in_node(
        &self,
        symbol_tree: &SymbolTree,
        node: tree_sitter::Node,
        preprocessor: &mut ShaderPreprocessor,
    ) -> Result<Vec<ShaderRegion>, ShaderError> {
        let mut query_cursor = QueryCursor::new();
        let mut regions = Vec::new();
        for region_match in
            query_cursor.matches(&self.query_if, node, symbol_tree.content.as_bytes())
        {
            fn parse_region(
                symbol_tree: &SymbolTree,
                preprocessor: &mut ShaderPreprocessor,
                cursor: &mut tree_sitter::TreeCursor,
                found_active_region: bool,
            ) -> Result<Vec<ShaderRegion>, ShaderError> {
                assert_tree_sitter!(symbol_tree.file_path, cursor.goto_first_child());
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
                            HlslSymbolRegionFinder::is_define_defined(
                                preprocessor,
                                condition_macro,
                                &position,
                            ),
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
                            1 - HlslSymbolRegionFinder::is_define_defined(
                                preprocessor,
                                condition_macro,
                                &position,
                            ),
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
                                preprocessor,
                            ) {
                                Ok(value) => value,
                                Err(err) => {
                                    if let ShaderError::SymbolQueryError(message, range) = err {
                                        preprocessor.diagnostics.push(ShaderDiagnostic {
                                            severity: ShaderDiagnosticSeverity::Warning,
                                            error: message,
                                            range,
                                        });
                                        0 // Return zero as default.
                                    } else {
                                        return Err(err);
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
                                    preprocessor,
                                ) {
                                    Ok(value) => value,
                                    Err(err) => {
                                        if let ShaderError::SymbolQueryError(message, range) = err {
                                            preprocessor.diagnostics.push(ShaderDiagnostic {
                                                severity: ShaderDiagnosticSeverity::Warning,
                                                error: message,
                                                range,
                                            });
                                            0 // Return zero as default.
                                        } else {
                                            return Err(err);
                                        }
                                    }
                                }
                            },
                            position,
                        )
                    }
                    "#elifdef" => {
                        todo!();
                    }
                    child => {
                        return Err(ShaderError::SymbolQueryError(
                            format!("preproc operator not implemented: {}", child),
                            ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
                        ));
                    }
                };
                // Now find alternative blocks & loop on it
                let mut regions = Vec::new();
                while cursor.goto_next_sibling() {
                    match cursor.field_name() {
                        Some(field_name) => {
                            if field_name == "alternative" {
                                let region_end = ShaderPosition::from_tree_sitter_point(
                                    cursor.node().range().start_point,
                                    &symbol_tree.file_path,
                                );
                                let region_range = ShaderRange::new(region_start, region_end);
                                regions.push(ShaderRegion::new(
                                    region_range.clone(),
                                    is_active_region != 0,
                                ));
                                // Filter out local define & include in region as it may impact next region in file.
                                preprocessor.defines.retain(|define| match &define.range {
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
                                            && !region_range.contain_bounds(&include.range))
                                });
                                let mut next_region = parse_region(
                                    symbol_tree,
                                    preprocessor,
                                    cursor,
                                    (is_active_region != 0) || found_active_region,
                                )?;
                                regions.append(&mut next_region);
                                return Ok(regions);
                            }
                        }
                        None => {}
                    }
                }
                // We can reach here on else block or block with no alternative.
                let end_point = if cursor.node().kind() == "#endif" {
                    cursor.node().range().start_point
                } else {
                    cursor.node().range().end_point
                };
                let region_range = ShaderRange::new(
                    region_start,
                    ShaderPosition::from_tree_sitter_point(end_point, &symbol_tree.file_path),
                );
                regions.push(ShaderRegion::new(
                    region_range.clone(),
                    is_active_region != 0,
                ));
                // Filter out local define & include in region as it may impact next region in file.
                preprocessor.defines.retain(|define| match &define.range {
                    Some(range) => {
                        is_active_region != 0
                            || (is_active_region == 0 && !region_range.contain_bounds(&range))
                    }
                    None => true,
                });
                preprocessor.includes.retain(|include| {
                    is_active_region != 0
                        || (is_active_region == 0 && !region_range.contain_bounds(&include.range))
                });
                Ok(regions)
            }
            let node = region_match.captures[0].node;
            let mut cursor = node.walk();
            regions.append(&mut parse_region(
                symbol_tree,
                preprocessor,
                &mut cursor,
                false,
            )?)
        }

        Ok(regions)
    }
}
