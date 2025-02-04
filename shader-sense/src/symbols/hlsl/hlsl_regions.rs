use std::num::ParseIntError;

use tree_sitter::{Query, QueryCursor};

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::get_name,
        symbol_tree::SymbolTree,
        symbols::{
            ShaderPosition, ShaderPreprocessor, ShaderPreprocessorDefine, ShaderRange, ShaderRegion,
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
    ($condition:expr) => {
        if !$condition {
            return Err(ShaderError::InternalErr(format!(
                "Unexpected query failure at : ({}:{})",
                file!(),
                line!()
            )));
        }
    };
}
macro_rules! assert_field_name {
    ($cursor:expr, $value:expr) => {
        if $cursor.field_name().unwrap() != $value {
            return Err(ShaderError::InternalErr(format!(
                "Unexpected query field name {} at : ({}:{})",
                $value,
                file!(),
                line!()
            )));
        }
    };
}
macro_rules! assert_node_kind {
    ($cursor:expr, $value:expr) => {
        if $cursor.node().kind() != $value {
            return Err(ShaderError::InternalErr(format!(
                "Unexpected query node kind {} at : ({}:{})",
                $value,
                file!(),
                line!()
            )));
        }
    };
}

fn get_define_value<'a>(
    preprocessor: &'a ShaderPreprocessor,
    name: &str,
    position: &ShaderPosition,
) -> Option<&'a ShaderPreprocessorDefine> {
    preprocessor.defines.iter().find(|define| {
        let same_name = define.name == name;
        let has_range = define.range.is_some(); // None, mean global define.
        let same_file =
            has_range && define.range.as_ref().unwrap().start.file_path == position.file_path;
        // Here, there is also case where macro defined in another file.
        // We cannot check this, but it should be correctly set in preprocessor.
        let defined_before =
            (same_file && define.range.as_ref().unwrap().start < *position) || !same_file;
        same_name && defined_before
    })
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
fn get_define_as_i32_depth<'a>(
    preprocessor: &'a ShaderPreprocessor,
    name: &str,
    position: &ShaderPosition,
    depth: u32,
) -> Result<i32, ShaderError> {
    if depth == 0 {
        return Err(ShaderError::SymbolQueryError(
            format!("Failed to parse number_literal {}", name),
            ShaderRange::new(position.clone(), position.clone()),
        ));
    } else {
        // Here we recurse define value cuz a define might just be an alias for another define.
        // If we dont manage to parse it as a number, parse it as another define.
        match get_define_value(preprocessor, name, position) {
            Some(define) => match &define.value {
                Some(value) => match parse_number(value) {
                    Ok(parsed_value) => Ok(parsed_value),
                    Err(_) => get_define_as_i32_depth(
                        &preprocessor,
                        value,
                        &ShaderPosition::default(),
                        depth - 1,
                    ),
                },
                None => Ok(0), // Return false instead of error
            },
            None => Ok(0), // Return false instead of error
        }
    }
}
fn get_define_as_i32<'a>(
    preprocessor: &'a ShaderPreprocessor,
    name: &str,
    position: &ShaderPosition,
) -> Result<i32, ShaderError> {
    match get_define_value(preprocessor, name, position) {
        Some(define) => match &define.value {
            Some(value) => match value.parse::<i32>() {
                Ok(parsed_value) => Ok(parsed_value),
                Err(_) => {
                    // Recurse result up to 10 times.
                    get_define_as_i32_depth(&preprocessor, value, &ShaderPosition::default(), 10)
                }
            },
            None => Ok(0), // Return false instead of error
        },
        None => Ok(0), // Return false instead of error
    }
}
fn is_define_defined(
    preprocessor: &ShaderPreprocessor,
    name: &str,
    position: &ShaderPosition,
) -> i32 {
    get_define_value(preprocessor, name, position).is_some() as i32
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
                "("
                (identifier) @identifier
                ")"
            )"#;
            assert_tree_sitter!(cursor.goto_first_child());
            assert_node_kind!(cursor, "defined");
            assert_tree_sitter!(cursor.goto_next_sibling());
            assert_node_kind!(cursor, "(");
            assert_tree_sitter!(cursor.goto_next_sibling());
            assert_node_kind!(cursor, "identifier");
            let condition_macro = get_name(&symbol_tree.content, cursor.node());
            let position = ShaderPosition::from_tree_sitter_point(
                cursor.node().range().end_point,
                &symbol_tree.file_path,
            );
            Ok(is_define_defined(preprocessor, condition_macro, &position))
        }
        "number_literal" => {
            // As simple as it is, but need to be warry of hexa or octal values.
            let _ = r#"condition: (number_literal)"#;
            let number_str = get_name(&symbol_tree.content, cursor.node());
            let parsed_number = parse_number(number_str);
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
            let value = get_define_as_i32(
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
            assert_tree_sitter!(cursor.goto_first_child());
            assert_field_name!(cursor, "left");
            let left_condition = match cursor.node().kind() {
                "identifier" => get_define_as_i32(
                    preprocessor,
                    &get_name(&symbol_tree.content, cursor.node()),
                    &ShaderPosition::from_tree_sitter_point(
                        cursor.node().start_position(),
                        &symbol_tree.file_path,
                    ),
                )?,
                _ => resolve_condition(cursor.clone(), symbol_tree, preprocessor)?,
            };
            assert_tree_sitter!(cursor.goto_next_sibling());
            assert_field_name!(cursor, "operator");
            let operator = cursor.node().kind();
            assert_tree_sitter!(cursor.goto_next_sibling());
            assert_field_name!(cursor, "right");
            let right_condition = match cursor.node().kind() {
                "identifier" => get_define_as_i32(
                    preprocessor,
                    &get_name(&symbol_tree.content, cursor.node()),
                    &ShaderPosition::from_tree_sitter_point(
                        cursor.node().start_position(),
                        &symbol_tree.file_path,
                    ),
                )?,
                _ => resolve_condition(cursor.clone(), symbol_tree, preprocessor)?,
            };
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
            assert_tree_sitter!(cursor.goto_first_child());
            assert_field_name!(cursor, "operator");
            let operator = cursor.node().kind(); //get_name(&symbol_tree.content, cursor.node());
            assert_tree_sitter!(cursor.goto_next_sibling());
            assert_field_name!(cursor, "argument");
            let argument = get_name(&symbol_tree.content, cursor.node());
            let position = ShaderPosition::from_tree_sitter_point(
                cursor.node().start_position(),
                &symbol_tree.file_path,
            );
            let value = get_define_as_i32(preprocessor, &argument, &position)?;
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
            assert_tree_sitter!(cursor.goto_first_child());
            assert_tree_sitter!(cursor.goto_next_sibling());
            resolve_condition(cursor, symbol_tree, preprocessor)
        }
        value => Err(ShaderError::SymbolQueryError(
            format!("Condition unhandled for {}", value),
            ShaderRange::from_range(cursor.node().range(), &symbol_tree.file_path),
        )),
    }
}

pub fn query_regions_in_node(
    symbol_tree: &SymbolTree,
    node: tree_sitter::Node,
    preprocessor: &ShaderPreprocessor,
) -> Result<Vec<ShaderRegion>, ShaderError> {
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
    let mut query_cursor = QueryCursor::new();
    let mut regions = Vec::new();
    for region_match in query_cursor.matches(&query_if, node, symbol_tree.content.as_bytes()) {
        fn find_regions(
            symbol_tree: &SymbolTree,
            preprocessor: &ShaderPreprocessor,
            cursor: &mut tree_sitter::TreeCursor,
            found_active_region: bool,
        ) -> Result<Vec<ShaderRegion>, ShaderError> {
            assert_tree_sitter!(cursor.goto_first_child());
            let (is_active_region, region_start) = match cursor.node().kind() {
                "#ifdef" => {
                    assert_tree_sitter!(cursor.goto_next_sibling());
                    let field_name = cursor.field_name().unwrap();
                    assert_tree_sitter!(field_name == "name");
                    let condition_macro = get_name(&symbol_tree.content, cursor.node());
                    let position = ShaderPosition::from_tree_sitter_point(
                        cursor.node().range().end_point,
                        &symbol_tree.file_path,
                    );
                    (
                        is_define_defined(preprocessor, condition_macro, &position),
                        position,
                    )
                }
                "#ifndef" => {
                    assert_tree_sitter!(cursor.goto_next_sibling());
                    let field_name = cursor.field_name().unwrap();
                    assert_tree_sitter!(field_name == "name");
                    let condition_macro = get_name(&symbol_tree.content, cursor.node());
                    let position = ShaderPosition::from_tree_sitter_point(
                        cursor.node().range().end_point,
                        &symbol_tree.file_path,
                    );
                    (
                        1 - is_define_defined(preprocessor, condition_macro, &position),
                        position,
                    )
                }
                "#if" => {
                    assert_tree_sitter!(cursor.goto_next_sibling());
                    let field_name = cursor.field_name().unwrap();
                    assert_tree_sitter!(field_name == "condition");
                    let position = ShaderPosition::from_tree_sitter_point(
                        cursor.node().range().end_point,
                        &symbol_tree.file_path,
                    );
                    (
                        resolve_condition(cursor.clone(), symbol_tree, preprocessor)?,
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
                    assert_tree_sitter!(cursor.goto_next_sibling());
                    let field_name = cursor.field_name().unwrap();
                    assert_tree_sitter!(field_name == "condition");
                    let position = ShaderPosition::from_tree_sitter_point(
                        cursor.node().range().end_point,
                        &symbol_tree.file_path,
                    );
                    (
                        if found_active_region {
                            0
                        } else {
                            resolve_condition(cursor.clone(), symbol_tree, preprocessor)?
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
                    ))
                }
            };
            // Now find alternative & loop on it
            let mut regions = Vec::new();
            while cursor.goto_next_sibling() {
                match cursor.field_name() {
                    Some(field_name) => {
                        if field_name == "alternative" {
                            let region_end = ShaderPosition::from_tree_sitter_point(
                                cursor.node().range().start_point,
                                &symbol_tree.file_path,
                            );
                            regions.push(ShaderRegion::new(
                                ShaderRange::new(region_start, region_end),
                                is_active_region != 0,
                            ));
                            let mut next_region = find_regions(
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
            let region_end =
                ShaderPosition::from_tree_sitter_point(end_point, &symbol_tree.file_path);
            regions.push(ShaderRegion::new(
                ShaderRange::new(region_start, region_end),
                is_active_region != 0,
            ));
            Ok(regions)
        }
        let node = region_match.captures[0].node;
        let mut cursor = node.walk();
        regions.append(&mut find_regions(
            symbol_tree,
            &preprocessor,
            &mut cursor,
            false,
        )?)
    }

    Ok(regions)
}
