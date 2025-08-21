use std::path::Path;

use crate::{
    position::{ShaderFileRange, ShaderRange},
    symbols::{
        symbol_parser::{get_name, SymbolTreePreprocessorParser},
        symbols::{
            ShaderPreprocessor, ShaderPreprocessorContext, ShaderPreprocessorDefine,
            ShaderPreprocessorInclude,
        },
    },
};

pub fn get_glsl_preprocessor_parser() -> Vec<Box<dyn SymbolTreePreprocessorParser>> {
    vec![
        Box::new(GlslIncludeTreePreprocessorParser {}),
        Box::new(GlslDefineTreePreprocessorParser {}),
    ]
}
struct GlslIncludeTreePreprocessorParser {}

impl SymbolTreePreprocessorParser for GlslIncludeTreePreprocessorParser {
    fn get_query(&self) -> String {
        // TODO: string_content unsupported
        // string_content unsupported on tree_sitter 0.20.9
        /*r#"(preproc_include
            (#include)
            path: (string_literal
                (string_content) @include
            )
        )"#*/
        r#"(preproc_include
            (#include)
            path: [(string_literal)(system_lib_string)] @include
        )"#
        .into()
    }
    fn process_match(
        &self,
        matches: tree_sitter::QueryMatch,
        file_path: &Path,
        shader_content: &str,
        preprocessor: &mut ShaderPreprocessor,
        context: &mut ShaderPreprocessorContext,
    ) {
        let include_node = matches.captures[0].node;
        let range =
            ShaderFileRange::from(file_path.into(), ShaderRange::from(include_node.range()));
        let relative_path = get_name(shader_content, include_node);
        let relative_path = &relative_path[1..relative_path.len() - 1]; // TODO: use string_content instead

        // Only add symbol if path can be resolved.
        match context.search_path_in_includes(Path::new(relative_path)) {
            Some(absolute_path) => {
                preprocessor.includes.push(ShaderPreprocessorInclude::new(
                    relative_path.into(),
                    absolute_path,
                    range,
                ));
            }
            None => {}
        }
    }
}
struct GlslDefineTreePreprocessorParser {}

impl SymbolTreePreprocessorParser for GlslDefineTreePreprocessorParser {
    fn get_query(&self) -> String {
        r#"(preproc_def
            (#define)
            name: (identifier) @define.label
            value: (preproc_arg)? @define.value
        )"#
        .into()
    }
    fn process_match(
        &self,
        matches: tree_sitter::QueryMatch,
        file_path: &Path,
        shader_content: &str,
        symbols: &mut ShaderPreprocessor,
        _context: &mut ShaderPreprocessorContext,
    ) {
        let identifier_node = matches.captures[0].node;
        let range =
            ShaderFileRange::from(file_path.into(), ShaderRange::from(identifier_node.range()));
        let name = get_name(shader_content, identifier_node).into();
        let value = if matches.captures.len() > 1 {
            Some(get_name(shader_content, matches.captures[1].node).trim())
        } else {
            None
        };
        // TODO: check exist & first one / last one. Need regions aswell... Duplicate with position as key ?
        symbols.defines.push(ShaderPreprocessorDefine::new(
            name,
            range,
            value.map(|s| s.into()),
        ));
    }
}
