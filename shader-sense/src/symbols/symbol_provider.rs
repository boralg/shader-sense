use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use tree_sitter::{Query, QueryCursor};

use crate::{
    shader::ShadingLanguageTag,
    shader_error::{ShaderDiagnostic, ShaderDiagnosticSeverity, ShaderError},
    symbols::{symbol_parser::ShaderWordRange, symbols::ShaderPreprocessorDefine},
};

use super::{
    shader_language::ShaderLanguage,
    symbol_parser::{
        ShaderSymbolListBuilder, SymbolRegionFinder, SymbolTreeFilter, SymbolTreeParser,
        SymbolTreePreprocessorParser, SymbolWordProvider,
    },
    symbol_tree::{ShaderModule, ShaderModuleHandle, ShaderSymbols, SymbolTree},
    symbols::{
        ShaderPosition, ShaderPreprocessor, ShaderPreprocessorContext, ShaderPreprocessorInclude,
        ShaderPreprocessorMode, ShaderRange, ShaderScope, ShaderSymbol, ShaderSymbolList,
    },
};

#[derive(Default, Debug, Clone)]
pub struct ShaderSymbolParams {
    pub defines: HashMap<String, String>,
    pub includes: Vec<String>,
    pub path_remapping: HashMap<PathBuf, PathBuf>,
}

pub struct SymbolProvider {
    symbol_parsers: Vec<(Box<dyn SymbolTreeParser>, tree_sitter::Query)>,
    symbol_filters: Vec<Box<dyn SymbolTreeFilter>>,
    scope_query: Query,
    error_query: Query,

    preprocessor_parsers: Vec<(Box<dyn SymbolTreePreprocessorParser>, tree_sitter::Query)>,
    region_finder: Box<dyn SymbolRegionFinder>,
    word_provider: Box<dyn SymbolWordProvider>,
}

pub type SymbolIncludeCallback<'a> =
    dyn FnMut(&ShaderPreprocessorInclude) -> Result<Option<ShaderModuleHandle>, ShaderError> + 'a;

pub fn default_include_callback<T: ShadingLanguageTag>(
    include: &ShaderPreprocessorInclude,
) -> Result<Option<ShaderModuleHandle>, ShaderError> {
    let mut language = ShaderLanguage::new(T::get_language());
    let include_module = language.create_module(
        &include.get_absolute_path(),
        std::fs::read_to_string(&include.get_absolute_path())
            .unwrap()
            .as_str(),
    )?;
    Ok(Some(Rc::new(RefCell::new(include_module))))
}

impl SymbolProvider {
    pub fn new(
        language: tree_sitter::Language,
        parsers: Vec<Box<dyn SymbolTreeParser>>,
        filters: Vec<Box<dyn SymbolTreeFilter>>,
        preprocessor_parsers: Vec<Box<dyn SymbolTreePreprocessorParser>>,
        region_finder: Box<dyn SymbolRegionFinder>,
        word_provider: Box<dyn SymbolWordProvider>,
    ) -> Self {
        let scope_query = r#"(compound_statement
            "{"? @scope.start
            "}"? @scope.end
        ) @scope"#;
        let error_query = r#"(ERROR) @error"#;
        Self {
            symbol_parsers: parsers
                .into_iter()
                .map(|e| {
                    // Cache query
                    let query = Query::new(language, e.get_query().as_str()).unwrap();
                    (e, query)
                })
                .collect(),
            symbol_filters: filters,
            scope_query: tree_sitter::Query::new(language.clone(), scope_query).unwrap(),
            error_query: tree_sitter::Query::new(language.clone(), error_query).unwrap(),
            preprocessor_parsers: preprocessor_parsers
                .into_iter()
                .map(|e| {
                    // Cache query
                    let query = Query::new(language, e.get_query().as_str()).unwrap();
                    (e, query)
                })
                .collect(),
            region_finder: region_finder,
            word_provider,
        }
    }
    pub fn query_file_scopes(&self, symbol_tree: &SymbolTree) -> Vec<ShaderScope> {
        // TODO: look for namespace aswell.
        // Should be per lang instead.
        let mut query_cursor = QueryCursor::new();
        let mut scopes = Vec::new();
        for matche in query_cursor.matches(
            &self.scope_query,
            symbol_tree.tree.root_node(),
            symbol_tree.content.as_bytes(),
        ) {
            scopes.push(match matche.captures.len() {
                // one body
                1 => {
                    ShaderScope::from_range(matche.captures[0].node.range(), &symbol_tree.file_path)
                }
                // a bit weird, a body and single curly brace ? mergin them to be safe.
                2 => ShaderScope::join(
                    ShaderScope::from_range(
                        matche.captures[0].node.range(),
                        &symbol_tree.file_path,
                    ),
                    ShaderScope::from_range(
                        matche.captures[1].node.range(),
                        &symbol_tree.file_path,
                    ),
                ),
                // Remove curly braces from scope.
                3 => {
                    let curly_start = matche.captures[1].node.range();
                    let curly_end = matche.captures[2].node.range();
                    ShaderScope::from_range(
                        tree_sitter::Range {
                            start_byte: curly_start.end_byte,
                            end_byte: curly_end.start_byte,
                            start_point: curly_start.end_point,
                            end_point: curly_end.start_point,
                        },
                        &symbol_tree.file_path,
                    )
                }
                _ => unreachable!("Query should not return more than 3 match."),
            });
        }
        scopes
    }
    pub fn query_symbols_with_context<'a>(
        &self,
        shader_module: &ShaderModule,
        context: &mut ShaderPreprocessorContext,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        old_symbols: Option<ShaderSymbols>,
    ) -> Result<ShaderSymbols, ShaderError> {
        // Either we create it from context, or we store it in context (no need to store 2 ref to it).
        let preprocessor =
            self.query_preprocessor(shader_module, context, include_callback, old_symbols)?;
        let symbol_list = if let ShaderPreprocessorMode::OnceVisited = preprocessor.mode {
            ShaderSymbolList::default() // if once, no symbols.
        } else {
            // TODO: should not always need to recompute this.
            self.query_file_symbols(shader_module)?
        };
        Ok(ShaderSymbols {
            preprocessor,
            symbol_list,
        })
    }
    pub fn query_symbols<'a>(
        &self,
        shader_module: &ShaderModule,
        symbol_params: ShaderSymbolParams,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        old_symbols: Option<ShaderSymbols>,
    ) -> Result<ShaderSymbols, ShaderError> {
        let mut context = ShaderPreprocessorContext::main(&shader_module.file_path, symbol_params);
        self.query_symbols_with_context(shader_module, &mut context, include_callback, old_symbols)
    }
    pub(super) fn process_include<'a>(
        &self,
        context: &mut ShaderPreprocessorContext,
        include: &mut ShaderPreprocessorInclude,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
    ) -> Result<(), ShaderError> {
        if context.increase_depth() {
            // Get module handle using callback.
            let result = match include_callback(&include)? {
                Some(include_module_handle) => {
                    // Include found, deal with it.
                    let module = RefCell::borrow(&include_module_handle);
                    match self.query_symbols_with_context(
                        &module,
                        context,
                        include_callback,
                        include.cache.take(),
                    ) {
                        Ok(cache) => {
                            include.cache = Some(cache);
                            Ok(())
                        }
                        Err(err) => Err(err),
                    }
                }
                None => {
                    // Include not found.
                    Err(ShaderError::SymbolQueryError(
                        format!("Failed to find include {}", include.get_relative_path()),
                        include.get_range().clone(),
                    ))
                }
            };
            context.decrease_depth();
            assert!(
                include.cache.is_some(),
                "Failed to compute cache for file {}",
                include.get_absolute_path().display()
            );
            result
        } else {
            // Set empty symbols to avoid crash when getting symbols.
            include.cache = Some(ShaderSymbols::default());
            // Notify
            return Err(ShaderError::SymbolQueryError(
                format!(
                    "Include {} reached maximum include depth",
                    include.get_relative_path()
                ),
                include.get_range().clone(),
            ));
        }
    }
    fn query_preprocessor<'a>(
        &self,
        symbol_tree: &SymbolTree,
        context: &'a mut ShaderPreprocessorContext,
        include_callback: &'a mut SymbolIncludeCallback<'a>,
        old_symbols: Option<ShaderSymbols>,
    ) -> Result<ShaderPreprocessor, ShaderError> {
        let mut preprocessor = ShaderPreprocessor::new(context.clone());

        // Check if context dirty and we need a recompute
        // or if we can reuse old_symbols instead.
        let is_dirty = match &old_symbols {
            Some(old_symbol) => old_symbol
                .get_preprocessor()
                .context
                .is_dirty(&symbol_tree.file_path, &context),
            None => true, // No old_symbol.
        };
        if is_dirty {
            // Recompute everything as its dirty.
            for parser in &self.preprocessor_parsers {
                let mut query_cursor = QueryCursor::new();
                for matches in query_cursor.matches(
                    &parser.1,
                    symbol_tree.tree.root_node(),
                    symbol_tree.content.as_bytes(),
                ) {
                    parser.0.process_match(
                        matches,
                        &symbol_tree.file_path,
                        &symbol_tree.content,
                        &mut preprocessor,
                        context,
                    );
                }
            }
            // Check pragma once macro.
            if preprocessor.mode == ShaderPreprocessorMode::OnceVisited {
                // Return a clean preprocessor.
                let mut empty_preprocessor = ShaderPreprocessor::new(context.clone());
                empty_preprocessor.mode = preprocessor.mode;
                return Ok(empty_preprocessor);
            }
            // Query regions.
            // Will filter includes & defines in inactive regions
            preprocessor.regions = self.region_finder.query_regions_in_node(
                symbol_tree,
                self,
                symbol_tree.tree.root_node(),
                &mut preprocessor,
                context,
                include_callback,
                old_symbols,
            )?;
            // Add errors
            let mut query_error_cursor = QueryCursor::new();
            for matches in query_error_cursor.matches(
                &self.error_query,
                symbol_tree.tree.root_node(),
                symbol_tree.content.as_bytes(),
            ) {
                preprocessor.diagnostics.push(ShaderDiagnostic {
                    severity: ShaderDiagnosticSeverity::Warning,
                    error:
                        "Failed to parse this code. Some symbols might be missing from providers."
                            .into(),
                    range: ShaderRange::from_range(
                        matches.captures[0].node.range(),
                        &symbol_tree.file_path,
                    ),
                });
            }
            Ok(preprocessor)
        } else {
            // Retrieve old symbol, maintain context up to date
            let mut old_symbols = old_symbols.unwrap();
            let included_preprocessor = old_symbols.get_preprocessor_mut();
            let included_includes: Vec<&mut ShaderPreprocessorInclude> =
                included_preprocessor.includes.iter_mut().collect();
            let mut last_position = ShaderPosition::zero(symbol_tree.file_path.clone());
            for included_include in included_includes {
                // Append directory stack and defines.
                context.push_directory_stack(included_include.get_absolute_path());
                context.append_defines(
                    included_preprocessor
                        .defines
                        .iter()
                        .filter(|define| match define.get_range() {
                            Some(range) => {
                                range.start >= last_position
                                    && range.end <= included_include.get_range().start
                            }
                            None => false, // Global define, already filled ?
                        })
                        .cloned()
                        .collect::<Vec<ShaderPreprocessorDefine>>(),
                );
                self.process_include(context, included_include, include_callback)?;
                last_position = included_include.get_range().end.clone();
            }
            // Add all defines after last include to context
            let define_left = included_preprocessor
                .defines
                .iter_mut()
                .filter(|define| match define.get_range() {
                    Some(range) => range.start > last_position,
                    None => false, // Global define
                })
                .map(|d| d.clone())
                .collect::<Vec<ShaderPreprocessorDefine>>();
            context.append_defines(define_left);
            Ok(old_symbols.preprocessor)
        }
    }
    fn query_file_symbols(
        &self,
        symbol_tree: &SymbolTree,
    ) -> Result<ShaderSymbolList, ShaderError> {
        // TODO: Should use something else than name...
        // Required for shader stage filtering...
        let file_name = symbol_tree
            .file_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let filter_symbol = |symbol: &ShaderSymbol| -> bool {
            // Dont filter inactive regions here on parsing, to avoid recomputing all symbols on regions update.
            let mut is_retained = true;
            for filter in &self.symbol_filters {
                is_retained = is_retained & filter.filter_symbol(symbol, &file_name);
            }
            is_retained
        };
        let mut symbol_list_builder = ShaderSymbolListBuilder::new(&filter_symbol);
        let scopes = self.query_file_scopes(symbol_tree);
        for parser in &self.symbol_parsers {
            let mut query_cursor = QueryCursor::new();
            for matches in query_cursor.matches(
                &parser.1,
                symbol_tree.tree.root_node(),
                symbol_tree.content.as_bytes(),
            ) {
                parser.0.process_match(
                    matches,
                    &symbol_tree.file_path,
                    &symbol_tree.content,
                    &scopes,
                    &mut symbol_list_builder,
                );
            }
        }
        Ok(symbol_list_builder.get_shader_symbol_list())
    }
    pub fn get_word_range_at_position(
        &self,
        symbol_tree: &SymbolTree,
        position: &ShaderPosition,
    ) -> Result<ShaderWordRange, ShaderError> {
        self.word_provider.find_word_at_position_in_node(
            symbol_tree,
            symbol_tree.tree.root_node(),
            position,
        )
    }
}
