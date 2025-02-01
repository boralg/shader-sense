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

fn get_define<'a>(
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
fn is_define_defined(
    preprocessor: &ShaderPreprocessor,
    name: &str,
    position: &ShaderPosition,
) -> bool {
    get_define(preprocessor, name, position).is_some()
}
fn resolve_condition(
    cursor: tree_sitter::TreeCursor,
    symbol_tree: &SymbolTree,
    preprocessor: &ShaderPreprocessor,
) -> bool {
    let mut cursor = cursor;
    match cursor.node().kind() {
        "preproc_defined" => {
            assert!(cursor.goto_first_child());
            assert!(cursor.node().kind() == "defined");
            assert!(cursor.goto_next_sibling());
            assert!(cursor.goto_next_sibling());
            assert!(cursor.node().kind() == "identifier");
            let condition_macro = get_name(&symbol_tree.content, cursor.node());
            let position = ShaderPosition::from_tree_sitter_point(
                cursor.node().range().end_point,
                &symbol_tree.file_path,
            );
            is_define_defined(preprocessor, condition_macro, &position)
        }
        "number_literal" => {
            let number = get_name(&symbol_tree.content, cursor.node());
            match number.parse::<i32>() {
                Ok(value) => value != 0,
                Err(_) => false,
            }
        }
        "identifier" => {
            let condition_macro = get_name(&symbol_tree.content, cursor.node());
            match get_define(
                preprocessor,
                condition_macro,
                &ShaderPosition::from_tree_sitter_point(
                    cursor.node().start_position(),
                    &symbol_tree.file_path,
                ),
            ) {
                Some(define) => match &define.value {
                    Some(value) => match value.parse::<i32>() {
                        Ok(value) => value != 0,
                        Err(_) => false,
                    },
                    None => false,
                },
                None => false,
            }
        }
        value => {
            assert!(false, "condition unhandled for {}", value);
            false
        }
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
    let regions = query_cursor
        .matches(&query_if, node, symbol_tree.content.as_bytes())
        .into_iter()
        .map(|matches| {
            fn find_regions(
                symbol_tree: &SymbolTree,
                preprocessor: &ShaderPreprocessor,
                cursor: &mut tree_sitter::TreeCursor,
                found_active_region: bool,
            ) -> Vec<ShaderRegion> {
                assert!(cursor.goto_first_child());
                let (is_active_region, region_start) = match cursor.node().kind() {
                    "#ifdef" => {
                        assert!(cursor.goto_next_sibling());
                        let field_name = cursor.field_name().unwrap();
                        assert!(field_name == "name");
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
                        assert!(cursor.goto_next_sibling());
                        let field_name = cursor.field_name().unwrap();
                        assert!(field_name == "name");
                        let condition_macro = get_name(&symbol_tree.content, cursor.node());
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            !is_define_defined(preprocessor, condition_macro, &position),
                            position,
                        )
                    }
                    "#if" => {
                        assert!(cursor.goto_next_sibling());
                        let field_name = cursor.field_name().unwrap();
                        assert!(field_name == "condition");
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            resolve_condition(cursor.clone(), symbol_tree, preprocessor),
                            position,
                        )
                    }
                    "#else" => (
                        !found_active_region,
                        ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        ),
                    ),
                    "#elif" => {
                        assert!(cursor.goto_next_sibling());
                        let field_name = cursor.field_name().unwrap();
                        assert!(field_name == "condition");
                        let position = ShaderPosition::from_tree_sitter_point(
                            cursor.node().range().end_point,
                            &symbol_tree.file_path,
                        );
                        (
                            !found_active_region
                                && resolve_condition(cursor.clone(), symbol_tree, preprocessor),
                            position,
                        )
                    }
                    child => {
                        assert!(false, "{} is not implemented", child);
                        (false, ShaderPosition::default())
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
                                    is_active_region,
                                ));
                                let mut next_region = find_regions(
                                    symbol_tree,
                                    preprocessor,
                                    cursor,
                                    is_active_region || found_active_region,
                                );
                                regions.append(&mut next_region);
                                return regions;
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
                    is_active_region,
                ));
                regions
            }
            let node = matches.captures[0].node;
            let mut cursor = node.walk();
            find_regions(symbol_tree, &preprocessor, &mut cursor, false)
        })
        .collect::<Vec<Vec<ShaderRegion>>>()
        .concat();

    Ok(regions)
}
