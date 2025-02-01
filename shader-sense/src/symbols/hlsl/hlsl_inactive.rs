use tree_sitter::{Query, QueryCursor};

use crate::{
    shader_error::ShaderError,
    symbols::{
        symbol_parser::get_name,
        symbol_provider::SymbolProvider,
        symbol_tree::SymbolTree,
        symbols::{ShaderPosition, ShaderRange, ShaderSymbolData, ShaderSymbolList},
    },
};

use super::HlslSymbolProvider;

impl HlslSymbolProvider {
    fn resolve_condition(
        cursor: &mut tree_sitter::TreeCursor,
        symbol_tree: &SymbolTree,
        symbol_cache: &ShaderSymbolList,
    ) -> bool {
        match cursor.node().kind() {
            "preproc_defined" => {
                assert!(cursor.goto_first_child());
                assert!(cursor.node().kind() == "defined");
                assert!(cursor.goto_next_sibling());
                assert!(cursor.goto_next_sibling());
                assert!(cursor.node().kind() == "identifier");
                let condition_macro = get_name(&symbol_tree.content, cursor.node());
                symbol_cache
                    .constants
                    .iter()
                    .find(|symbol| symbol.label == condition_macro)
                    .is_some()
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
                match symbol_cache
                    .constants
                    .iter()
                    .find(|symbol| symbol.label == condition_macro)
                {
                    Some(symbol) => {
                        if let ShaderSymbolData::Constants {
                            ty: _,
                            qualifier: _,
                            value,
                        } = &symbol.data
                        {
                            match value.parse::<i32>() {
                                Ok(value) => value != 0,
                                Err(_) => false,
                            }
                        } else {
                            false
                        }
                    }
                    None => false,
                }
            }
            value => {
                assert!(false, "case unhandled for {}", value);
                false
            }
        }
    }
    // Could rename this function "get_region" or something which return struct with
    // info about regions for folding (is_active...)
    pub fn query_inactive_regions_in_node(
        &self,
        symbol_tree: &SymbolTree,
        _node: tree_sitter::Node,
        symbol_cache: Option<&ShaderSymbolList>,
    ) -> Result<Vec<ShaderRange>, ShaderError> {
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
        let inactive_ranges = query_cursor
            .matches(
                &query_if,
                symbol_tree.tree.root_node(),
                symbol_tree.content.as_bytes(),
            )
            .into_iter()
            .map(|matches| {
                fn find_inactive_regions(
                    symbol_tree: &SymbolTree,
                    symbol_cache: &ShaderSymbolList,
                    cursor: &mut tree_sitter::TreeCursor,
                    found_active_region: bool,
                ) -> Vec<ShaderRange> {
                    assert!(cursor.goto_first_child());
                    let (is_active_region, region_start) = match cursor.node().kind() {
                        "#ifdef" => {
                            assert!(cursor.goto_next_sibling());
                            let field_name = cursor.field_name().unwrap();
                            assert!(field_name == "name");
                            let condition_macro = get_name(&symbol_tree.content, cursor.node());
                            (
                                symbol_cache
                                    .constants
                                    .iter()
                                    .find(|symbol| symbol.label == condition_macro)
                                    .is_some(),
                                ShaderPosition::from_tree_sitter_point(
                                    cursor.node().range().end_point,
                                    &symbol_tree.file_path,
                                ),
                            )
                        }
                        "#ifndef" => {
                            assert!(cursor.goto_next_sibling());
                            let field_name = cursor.field_name().unwrap();
                            assert!(field_name == "name");
                            let condition_macro = get_name(&symbol_tree.content, cursor.node());
                            (
                                symbol_cache
                                    .constants
                                    .iter()
                                    .find(|symbol| symbol.label == condition_macro)
                                    .is_none(),
                                ShaderPosition::from_tree_sitter_point(
                                    cursor.node().range().end_point,
                                    &symbol_tree.file_path,
                                ),
                            )
                        }
                        "#if" => {
                            assert!(cursor.goto_next_sibling());
                            let field_name = cursor.field_name().unwrap();
                            assert!(field_name == "condition");
                            (
                                HlslSymbolProvider::resolve_condition(
                                    cursor,
                                    symbol_tree,
                                    symbol_cache,
                                ),
                                ShaderPosition::from_tree_sitter_point(
                                    cursor.node().range().end_point,
                                    &symbol_tree.file_path,
                                ),
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
                            (
                                HlslSymbolProvider::resolve_condition(
                                    cursor,
                                    symbol_tree,
                                    symbol_cache,
                                ) && !found_active_region,
                                ShaderPosition::from_tree_sitter_point(
                                    cursor.node().range().end_point,
                                    &symbol_tree.file_path,
                                ),
                            )
                        }
                        child => {
                            assert!(false, "{} is not implemented", child);
                            (false, ShaderPosition::default())
                        }
                    };
                    // Now find alternative & loop on it
                    let mut inactive_ranges = Vec::new();
                    while cursor.goto_next_sibling() {
                        match cursor.field_name() {
                            Some(field_name) => {
                                if field_name == "alternative" {
                                    if !is_active_region {
                                        let region_end = ShaderPosition::from_tree_sitter_point(
                                            cursor.node().range().start_point,
                                            &symbol_tree.file_path,
                                        );
                                        inactive_ranges
                                            .push(ShaderRange::new(region_start, region_end));
                                    }
                                    let mut next_inactive_ranges = find_inactive_regions(
                                        symbol_tree,
                                        symbol_cache,
                                        cursor,
                                        is_active_region || found_active_region,
                                    );
                                    inactive_ranges.append(&mut next_inactive_ranges);
                                    return inactive_ranges;
                                }
                            }
                            None => {}
                        }
                    }
                    // We can reach here on else block or block with no alternative.
                    if !is_active_region {
                        let end_point = if cursor.node().kind() == "#endif" {
                            cursor.node().range().start_point
                        } else {
                            cursor.node().range().end_point
                        };
                        let region_end = ShaderPosition::from_tree_sitter_point(
                            end_point,
                            &symbol_tree.file_path,
                        );
                        inactive_ranges.push(ShaderRange::new(region_start, region_end));
                    }
                    inactive_ranges
                }
                let symbol_cache = match symbol_cache {
                    Some(symbol_cache) => symbol_cache.clone(),
                    None => {
                        // This does not query symbols from included files...
                        self.query_file_symbols(&symbol_tree)
                    }
                };
                let node = matches.captures[0].node;
                let mut cursor = node.walk();
                find_inactive_regions(symbol_tree, &symbol_cache, &mut cursor, false)
            })
            .collect::<Vec<Vec<ShaderRange>>>()
            .concat();

        Ok(inactive_ranges)
    }
}
