use std::{
    cmp::Ordering,
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{include::IncludeHandler, shader::ShaderStage, shader_error::ShaderDiagnostic};

use super::{symbol_provider::ShaderSymbolParams, symbol_tree::ShaderSymbols};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderParameter {
    pub ty: String,
    pub label: String,
    pub count: Option<u32>,
    pub description: String,
}

#[allow(non_snake_case)] // for JSON
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderSignature {
    pub returnType: String, // Should be an option for constructor
    pub description: String,
    pub parameters: Vec<ShaderParameter>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderLabelSignature {
    pub label: String,
    pub description: String,
    pub signature: ShaderSignature,
}

impl ShaderSignature {
    pub fn format(&self, label: &str) -> String {
        let signature = self
            .parameters
            .iter()
            .map(|p| format!("{} {}", p.ty, p.label))
            .collect::<Vec<String>>();
        format!("{} {}({})", self.returnType, label, signature.join(", "))
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderPosition {
    pub file_path: PathBuf,
    pub line: u32,
    pub pos: u32,
}
impl Ord for ShaderPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.file_path, &self.line, &self.pos).cmp(&(&other.file_path, &other.line, &other.pos))
    }
}

impl PartialOrd for ShaderPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ShaderPosition {
    fn eq(&self, other: &Self) -> bool {
        (&self.file_path, &self.line, &self.pos) == (&other.file_path, &other.line, &other.pos)
    }
}

impl Eq for ShaderPosition {}

impl ShaderPosition {
    pub fn new(file_path: PathBuf, line: u32, pos: u32) -> Self {
        Self {
            file_path,
            line,
            pos,
        }
    }

    pub fn from_byte_offset(
        content: &str,
        byte_offset: usize,
        file_path: &Path,
    ) -> std::io::Result<ShaderPosition> {
        // https://en.wikipedia.org/wiki/UTF-8
        if byte_offset == 0 {
            Ok(ShaderPosition {
                line: 0,
                pos: 0,
                file_path: PathBuf::from(file_path),
            })
        } else if content.len() == 0 {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Content is empty.",
            ))
        } else if byte_offset >= content.len() {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "byte_offset is out of bounds.",
            ))
        } else {
            let line = content[..byte_offset].lines().count() - 1;
            let line_start = content[..byte_offset]
                .lines()
                .last()
                .expect("No last line available.");
            let pos = content[byte_offset..].as_ptr() as usize - line_start.as_ptr() as usize;
            if line_start.is_char_boundary(pos) {
                Ok(ShaderPosition {
                    line: line as u32,
                    pos: pos as u32,
                    file_path: PathBuf::from(file_path),
                })
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Pos in line is not at UTF8 char boundary.",
                ))
            }
        }
    }
    pub fn to_byte_offset(&self, content: &str) -> std::io::Result<usize> {
        // https://en.wikipedia.org/wiki/UTF-8
        match content.lines().nth(self.line as usize) {
            Some(line) => {
                // This pointer operation is safe to operate because lines iterator should start at char boundary.
                let line_byte_offset = line.as_ptr() as usize - content.as_ptr() as usize;
                assert!(
                    content.is_char_boundary(line_byte_offset),
                    "Start of line is not char boundary."
                );
                // We have line offset, find pos offset.
                match content[line_byte_offset..]
                    .char_indices()
                    .nth(self.pos as usize)
                {
                    Some((byte_offset, _)) => {
                        let global_offset = line_byte_offset + byte_offset;
                        if content.len() <= global_offset {
                            Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Byte offset is not in content range.",
                            ))
                        } else if !content.is_char_boundary(global_offset) {
                            Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Position is not at UTF8 char boundary.",
                            ))
                        } else {
                            Ok(global_offset)
                        }
                    }
                    None => {
                        if self.pos as usize == line.chars().count() {
                            assert!(content.is_char_boundary(line_byte_offset + line.len()));
                            Ok(line_byte_offset + line.len())
                        } else {
                            Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                format!("Position is not in range of line"),
                            ))
                        }
                    }
                }
            }
            // Last line in line iterator is skipped if its empty.
            None => Ok(content.len()), // Line is out of bounds, assume its at the end.
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShaderRange {
    pub start: ShaderPosition,
    pub end: ShaderPosition,
}

pub type ShaderScope = ShaderRange;

impl ShaderRange {
    pub fn new(start: ShaderPosition, end: ShaderPosition) -> Self {
        debug_assert!(
            start.file_path == end.file_path,
            "Position start & end should have same value."
        );
        Self { start, end }
    }
    pub fn whole(file_path: &Path, content: &str) -> Self {
        let (last_line, last_pos) = if content.len() == 0 {
            (0, 0)
        } else {
            let line_count = content.lines().count();
            let char_count = content.lines().last().unwrap().char_indices().count();
            (line_count - 1, char_count - 1)
        };
        Self {
            start: ShaderPosition::new(file_path.into(), 0, 0),
            end: ShaderPosition::new(file_path.into(), last_line as u32, last_pos as u32),
        }
    }
    pub fn contain_bounds(&self, range: &ShaderRange) -> bool {
        if self.start.file_path.as_os_str() == range.start.file_path.as_os_str() {
            debug_assert!(
                range.start.file_path == self.start.file_path,
                "Raw string identical but not components"
            );
            if range.start.line > self.start.line && range.end.line < self.end.line {
                true
            } else if range.start.line == self.start.line && range.end.line == self.end.line {
                range.start.pos >= self.start.pos && range.end.pos <= self.end.pos
            } else if range.start.line == self.start.line && range.end.line < self.end.line {
                range.start.pos >= self.start.pos
            } else if range.end.line == self.end.line && range.start.line > self.start.line {
                range.end.pos <= self.end.pos
            } else {
                false
            }
        } else {
            debug_assert!(
                range.start.file_path != self.start.file_path,
                "Raw string different but not components"
            );
            false
        }
    }
    pub fn contain(&self, position: &ShaderPosition) -> bool {
        debug_assert!(
            self.start.file_path == self.end.file_path,
            "Position start & end should have same value."
        );
        // Check same file. Comparing components is hitting perf, so just compare raw path, which should already be canonical.
        if position.file_path.as_os_str() == self.start.file_path.as_os_str() {
            debug_assert!(
                position.file_path == self.start.file_path,
                "Raw string identical but not components"
            );
            // Check line & position bounds.
            if position.line > self.start.line && position.line < self.end.line {
                true
            } else if position.line == self.start.line && position.line == self.end.line {
                position.pos >= self.start.pos && position.pos <= self.end.pos
            } else if position.line == self.start.line && position.line < self.end.line {
                position.pos >= self.start.pos
            } else if position.line == self.end.line && position.line > self.start.line {
                position.pos <= self.end.pos
            } else {
                false
            }
        } else {
            debug_assert!(
                position.file_path != self.start.file_path,
                "Raw string different but not components"
            );
            false
        }
    }
    pub fn join(mut lhs: ShaderRange, rhs: ShaderRange) -> ShaderScope {
        lhs.start.line = std::cmp::min(lhs.start.line, rhs.start.line);
        lhs.start.pos = std::cmp::min(lhs.start.pos, rhs.start.pos);
        lhs.end.line = std::cmp::max(lhs.end.line, rhs.end.line);
        lhs.end.pos = std::cmp::max(lhs.end.pos, rhs.end.pos);
        lhs
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShaderRegion {
    pub range: ShaderRange,
    // Could add some ShaderRegionType::Condition / ShaderRegionType::User...
    pub is_active: bool, // Is this region passing preprocess
}

impl ShaderRegion {
    pub fn new(range: ShaderRange, is_active: bool) -> Self {
        Self { range, is_active }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessorContext {
    defines: Vec<ShaderSymbol>,
    include_handler: IncludeHandler,
    dirty_files: HashSet<PathBuf>, // Dirty files that need to be recomputed no matter what.
    depth: usize,
}

impl ShaderPreprocessorContext {
    pub fn main(file_path: &Path, symbol_params: ShaderSymbolParams) -> Self {
        Self {
            defines: symbol_params
                .defines
                .iter()
                .map(|(key, value)| ShaderSymbol {
                    label: key.clone(),
                    description: format!("Preprocessor macro. Expanding to \n```\n{}\n```", value),
                    version: "".into(),
                    stages: vec![],
                    link: None,
                    data: ShaderSymbolData::Macro {
                        value: value.clone(),
                    },
                    range: None,
                    scope: None,
                    scope_stack: None,
                })
                .collect(),
            include_handler: IncludeHandler::main(
                &file_path,
                symbol_params.includes,
                symbol_params.path_remapping,
            ),
            dirty_files: HashSet::new(),
            depth: 0,
        }
    }
    pub fn mark_dirty(&mut self, file_path: &Path) {
        self.dirty_files.insert(file_path.into());
    }
    pub fn search_path_in_includes(&mut self, path: &Path) -> Option<PathBuf> {
        self.include_handler.search_path_in_includes(path)
    }
    pub fn push_define(&mut self, name: &str, value: &str) {
        self.defines.push(ShaderSymbol {
            label: name.into(),
            description: format!(
                "Config preprocessor macro. Expanding to \n```\n{}\n```",
                value
            ),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Macro {
                value: value.into(),
            },
            range: None,
            scope: None,
            scope_stack: None,
        });
    }
    pub fn append_defines(&mut self, defines: Vec<ShaderPreprocessorDefine>) {
        self.defines
            .extend(defines.iter().map(|define| define.symbol.clone()));
    }
    pub fn increase_depth(&mut self) -> bool {
        if self.depth < IncludeHandler::DEPTH_LIMIT {
            self.depth += 1;
            true
        } else {
            false
        }
    }
    pub fn decrease_depth(&mut self) {
        assert!(self.depth > 0, "Decreasing depth but zero.");
        self.depth -= 1;
    }
    pub fn get_visited_count(&mut self, path: &Path) -> usize {
        self.include_handler.get_visited_count(path)
    }
    pub fn is_dirty(&self, file_path: &Path, context: &ShaderPreprocessorContext) -> bool {
        // Compare defines to determine if context is different.
        // Check if we need to force an update aswell.
        fn compare_defines(lhs: &Vec<ShaderSymbol>, rhs: &Vec<ShaderSymbol>) -> bool {
            if lhs.len() != rhs.len() {
                return false;
            }
            for lhs_symbol in lhs.iter() {
                if rhs
                    .iter()
                    .find(|rhs_symbol| {
                        lhs_symbol.label == rhs_symbol.label
                            && match (&lhs_symbol.data, &rhs_symbol.data) {
                                (
                                    ShaderSymbolData::Macro { value: l_value },
                                    ShaderSymbolData::Macro { value: r_value },
                                ) => l_value == r_value,
                                _ => false,
                            }
                    })
                    .is_none()
                {
                    return false;
                }
            }
            true
        }
        compare_defines(&context.defines, &self.defines) || context.dirty_files.contains(file_path)
    }
    pub fn get_define_value(&self, name: &str) -> Option<String> {
        self.defines
            .iter()
            .find(|symbol| *symbol.label == *name)
            .map(|symbol| {
                match &symbol.data {
                    ShaderSymbolData::Macro { value } => value.clone(),
                    _ => panic!("Expected ShaderSymbolData::Macro"),
                }
                .clone()
            })
    }
    pub fn get_defines(&self) -> &Vec<ShaderSymbol> {
        &self.defines
    }
}

#[derive(Debug, Clone)]
pub struct ShaderPreprocessorInclude {
    // TODO: move cache to symbol data
    pub cache: Option<ShaderSymbols>,
    pub symbol: ShaderSymbol,
}

#[derive(Debug, Clone)]
pub struct ShaderPreprocessorDefine {
    pub symbol: ShaderSymbol,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ShaderPreprocessorMode {
    #[default]
    Default,
    Once,
    OnceVisited,
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessor {
    pub context: ShaderPreprocessorContext, // Defines from includer files when included, or config.

    pub includes: Vec<ShaderPreprocessorInclude>,
    pub defines: Vec<ShaderPreprocessorDefine>,
    pub regions: Vec<ShaderRegion>,
    pub diagnostics: Vec<ShaderDiagnostic>, // preprocessor errors
    pub mode: ShaderPreprocessorMode,
}
impl ShaderPreprocessorDefine {
    pub fn new(name: String, range: ShaderRange, value: Option<String>) -> Self {
        Self {
            symbol: ShaderSymbol {
                label: name.clone(),
                description: match &value {
                    Some(value) => {
                        format!("Preprocessor macro. Expanding to \n```\n{}\n```", value)
                    }
                    None => format!("Preprocessor macro."),
                },
                version: "".into(),
                stages: vec![],
                link: None,
                data: ShaderSymbolData::Macro {
                    value: match &value {
                        Some(value) => value.clone(),
                        None => "".into(),
                    },
                },
                range: Some(range.clone()),
                scope: None,
                scope_stack: None, // No scope for define
            },
        }
    }
    pub fn get_range(&self) -> Option<&ShaderRange> {
        self.symbol.range.as_ref()
    }
    pub fn get_name(&self) -> &String {
        &self.symbol.label
    }
    pub fn get_value(&self) -> Option<&String> {
        match &self.symbol.data {
            ShaderSymbolData::Macro { value } => Some(value),
            _ => None,
        }
    }
}
impl ShaderPreprocessorInclude {
    pub fn new(relative_path: String, absolute_path: PathBuf, range: ShaderRange) -> Self {
        Self {
            cache: None,
            symbol: ShaderSymbol {
                label: relative_path,
                description: format!("Including file {}", absolute_path.display()),
                version: "".into(),
                stages: vec![],
                link: None,
                data: ShaderSymbolData::Link {
                    target: ShaderPosition::new(absolute_path, 0, 0),
                },
                range: Some(range),
                scope: None,
                scope_stack: None, // No scope for include
            },
        }
    }
    pub fn get_range(&self) -> &ShaderRange {
        self.symbol
            .range
            .as_ref()
            .expect("Include symbol should have range.")
    }
    pub fn get_relative_path(&self) -> &String {
        &self.symbol.label
    }
    pub fn get_absolute_path(&self) -> &Path {
        match &self.symbol.data {
            ShaderSymbolData::Link { target } => &target.file_path,
            _ => panic!("Expected ShaderSymbolData::Link"),
        }
    }
    pub fn get_cache(&self) -> &ShaderSymbols {
        self.cache.as_ref().unwrap()
    }
    pub fn get_cache_mut(&mut self) -> &mut ShaderSymbols {
        self.cache.as_mut().unwrap()
    }
}

impl ShaderPreprocessor {
    pub fn new(context: ShaderPreprocessorContext) -> Self {
        Self {
            context: context,
            includes: Vec::new(),
            defines: Vec::new(),
            regions: Vec::new(),
            diagnostics: Vec::new(),
            mode: ShaderPreprocessorMode::default(),
        }
    }
    pub fn preprocess_symbols<'a>(
        &'a self,
        shader_symbols: &'a ShaderSymbolList,
    ) -> ShaderSymbolListRef<'a> {
        // Filter inactive regions symbols
        let inactive_regions: Vec<&ShaderRegion> =
            self.regions.iter().filter(|r| !r.is_active).collect();
        let mut preprocessed_symbols =
            shader_symbols.filter(move |_symbol_type, symbol| match &symbol.range {
                Some(range) => inactive_regions
                    .iter()
                    .find(|r| r.range.contain_bounds(&range))
                    .is_none(),
                None => true, // Global range
            });
        // Add defines
        let mut define_symbols: Vec<&ShaderSymbol> =
            self.defines.iter().map(|define| &define.symbol).collect();
        // Add includes as symbol
        let mut include_symbols: Vec<&ShaderSymbol> = self
            .includes
            .iter()
            .map(|include| &include.symbol)
            .collect();
        preprocessed_symbols.macros.append(&mut define_symbols);
        preprocessed_symbols.includes.append(&mut include_symbols);
        preprocessed_symbols
    }
}

pub type ShaderMember = ShaderParameter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShaderMethod {
    pub label: String,
    pub signature: ShaderSignature,
}

impl ShaderMember {
    pub fn as_symbol(&self) -> ShaderSymbol {
        ShaderSymbol {
            label: self.label.clone(),
            description: self.description.clone(),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Variables {
                ty: self.ty.clone(),
                count: self.count,
            },
            range: None, // Should have a position ?
            scope: None, // TODO: Should be scope of parent
            scope_stack: None,
        }
    }
}

impl ShaderMethod {
    pub fn as_symbol(&self) -> ShaderSymbol {
        ShaderSymbol {
            label: self.label.clone(),
            description: self.signature.description.clone(),
            version: "".into(),
            stages: vec![],
            link: None,
            data: ShaderSymbolData::Functions {
                signatures: vec![self.signature.clone()],
            },
            range: None, // Should have a position ?
            scope: None, // TODO: Should be scope of parent
            scope_stack: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ShaderSymbolData {
    // A bit of duplicate from variables ? Should be struct (Which should be renamed something else)
    Types {
        constructors: Vec<ShaderSignature>,
    },
    Struct {
        constructors: Vec<ShaderSignature>, // Need a range aswell for hover.
        members: Vec<ShaderMember>,         // Need a range aswell for hover.
        methods: Vec<ShaderMethod>,         // Need a range aswell for hover.
    },
    Constants {
        ty: String,
        qualifier: String,
        value: String,
    },
    Functions {
        signatures: Vec<ShaderSignature>,
    },
    Keyword {},
    // Mostly runtime, but GLSL has global variable in builtin that need serial.
    Variables {
        ty: String,
        count: Option<u32>,
    },
    #[serde(skip)] // This is runtime only. No serialization.
    CallExpression {
        label: String,
        range: ShaderRange, // label range.
        parameters: Vec<(String, ShaderRange)>,
    },
    #[serde(skip)] // This is runtime only. No serialization.
    Link {
        target: ShaderPosition,
    },
    #[serde(skip)] // This is runtime only. No serialization.
    Macro {
        value: String,
    },
}

#[allow(non_snake_case)] // for JSON
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShaderSymbol {
    pub label: String,            // Label for the item
    pub description: String,      // Description of the item
    pub version: String,          // Minimum version required for the item.
    pub stages: Vec<ShaderStage>, // Shader stages of the item
    pub link: Option<String>,     // Link to some external documentation
    pub data: ShaderSymbolData,   // Data for the variable
    // Runtime info. No serialization.
    #[serde(skip)]
    pub range: Option<ShaderRange>, // Range of symbol in shader
    #[serde(skip)]
    pub scope: Option<ShaderScope>, // Owning scope
    #[serde(skip)]
    pub scope_stack: Option<Vec<ShaderScope>>, // Stack of declaration
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ShaderSymbolType {
    Types,
    Constants,
    Variables,
    CallExpression,
    Functions,
    Keyword,
    Macros,
    Include,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShaderSymbolList {
    pub types: Vec<ShaderSymbol>,
    pub constants: Vec<ShaderSymbol>,
    pub variables: Vec<ShaderSymbol>,
    #[serde(skip)] // Only used at runtime.
    pub call_expression: Vec<ShaderSymbol>,
    pub functions: Vec<ShaderSymbol>,
    pub keywords: Vec<ShaderSymbol>,
    pub macros: Vec<ShaderSymbol>,
    pub includes: Vec<ShaderSymbol>,
}
#[derive(Debug, Default, Clone)]
pub struct ShaderSymbolListRef<'a> {
    pub types: Vec<&'a ShaderSymbol>,
    pub constants: Vec<&'a ShaderSymbol>,
    pub variables: Vec<&'a ShaderSymbol>,
    pub call_expression: Vec<&'a ShaderSymbol>,
    pub functions: Vec<&'a ShaderSymbol>,
    pub keywords: Vec<&'a ShaderSymbol>,
    pub macros: Vec<&'a ShaderSymbol>,
    pub includes: Vec<&'a ShaderSymbol>,
}

impl ShaderSymbolList {
    // Parse intrinsic database
    pub fn parse_from_json(file_content: String) -> ShaderSymbolList {
        serde_json::from_str::<ShaderSymbolList>(&file_content)
            .expect("Failed to parse ShaderSymbolList")
    }
    // Append another symbol list to this one.
    pub fn append(&mut self, shader_symbol_list: ShaderSymbolList) {
        let mut shader_symbol_list_mut = shader_symbol_list;
        self.functions.append(&mut shader_symbol_list_mut.functions);
        self.variables.append(&mut shader_symbol_list_mut.variables);
        self.call_expression
            .append(&mut shader_symbol_list_mut.call_expression);
        self.constants.append(&mut shader_symbol_list_mut.constants);
        self.types.append(&mut shader_symbol_list_mut.types);
        self.keywords.append(&mut shader_symbol_list_mut.keywords);
        self.macros.append(&mut shader_symbol_list_mut.macros);
        self.includes.append(&mut shader_symbol_list_mut.includes);
    }
    pub fn as_ref(&self) -> ShaderSymbolListRef {
        ShaderSymbolListRef {
            types: self.types.iter().collect(),
            constants: self.constants.iter().collect(),
            variables: self.variables.iter().collect(),
            call_expression: self.call_expression.iter().collect(),
            functions: self.functions.iter().collect(),
            keywords: self.keywords.iter().collect(),
            macros: self.macros.iter().collect(),
            includes: self.includes.iter().collect(),
        }
    }
    pub fn filter<'a, P: Fn(ShaderSymbolType, &ShaderSymbol) -> bool>(
        &'a self,
        predicate: P,
    ) -> ShaderSymbolListRef<'a> {
        ShaderSymbolListRef {
            types: self
                .types
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Types, *e))
                .collect(),
            constants: self
                .constants
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Constants, *e))
                .collect(),
            variables: self
                .variables
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Variables, *e))
                .collect(),
            call_expression: self
                .call_expression
                .iter()
                .filter(|e| predicate(ShaderSymbolType::CallExpression, *e))
                .collect(),
            functions: self
                .functions
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Functions, *e))
                .collect(),
            keywords: self
                .keywords
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Keyword, *e))
                .collect(),
            macros: self
                .macros
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Macros, *e))
                .collect(),
            includes: self
                .includes
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Include, *e))
                .collect(),
        }
    }
}
impl<'a> ShaderSymbolListRef<'a> {
    fn is_symbol_defined_at(
        shader_symbol: &ShaderSymbol,
        cursor_position: &ShaderPosition,
    ) -> bool {
        match &shader_symbol.range {
            Some(symbol_range) => {
                if symbol_range.start.file_path.as_os_str() == cursor_position.file_path.as_os_str()
                {
                    // Ensure symbols are already defined at pos
                    let is_already_defined = if symbol_range.start.line == cursor_position.line {
                        cursor_position.pos > symbol_range.start.pos
                    } else {
                        cursor_position.line > symbol_range.start.line
                    };
                    if is_already_defined {
                        // If we are in main file, check if scope in range.
                        match &shader_symbol.scope_stack {
                            Some(symbol_scope_stack) => {
                                for symbol_scope in symbol_scope_stack {
                                    if !symbol_scope.contain(cursor_position) {
                                        return false; // scope not in range
                                    }
                                }
                                true // scope in range
                            }
                            None => true, // Global space
                        }
                    } else {
                        false
                    }
                } else {
                    // If we are not in main file, only show whats in global scope.
                    // TODO: should handle include position in file aswell.
                    match &shader_symbol.scope_stack {
                        Some(symbol_scope_stack) => symbol_scope_stack.is_empty(), // Global scope or inaccessible
                        None => true,                                              // Global space
                    }
                }
            }
            None => true, // intrinsics
        }
    }
    pub fn find_symbols_at(&self, label: &String, position: &ShaderPosition) -> Vec<&ShaderSymbol> {
        self.iter()
            .filter(|s| {
                !s.is_transient() && s.label == *label && Self::is_symbol_defined_at(s, position)
            })
            .collect()
    }
    pub fn filter_scoped_symbol(
        &'a self,
        cursor_position: &ShaderPosition,
    ) -> ShaderSymbolListRef<'a> {
        self.filter(|symbol_type, symbol| {
            !symbol_type.is_transient() && Self::is_symbol_defined_at(symbol, cursor_position)
        })
    }
    pub fn find_symbols(&self, label: &String) -> Vec<&ShaderSymbol> {
        self.iter()
            .filter(|s| s.label == *label && !s.is_transient())
            .collect::<Vec<&ShaderSymbol>>()
    }
    pub fn find_symbol(&self, label: &String) -> Option<&ShaderSymbol> {
        match self.iter().find(|e| e.label == *label) {
            Some(symbol) => return Some(symbol),
            None => None,
        }
    }
    pub fn find_type_symbol(&self, label: &String) -> Option<&ShaderSymbol> {
        self.types.iter().find(|s| s.label == *label).map(|s| *s)
    }
    pub fn filter<P: Fn(ShaderSymbolType, &ShaderSymbol) -> bool>(
        &self,
        predicate: P,
    ) -> ShaderSymbolListRef {
        ShaderSymbolListRef {
            types: self
                .types
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Types, *e))
                .map(|s| *s)
                .collect(),
            constants: self
                .constants
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Constants, *e))
                .map(|s| *s)
                .collect(),
            variables: self
                .variables
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Variables, *e))
                .map(|s| *s)
                .collect(),
            call_expression: self
                .call_expression
                .iter()
                .filter(|e| predicate(ShaderSymbolType::CallExpression, *e))
                .map(|s| *s)
                .collect(),
            functions: self
                .functions
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Functions, *e))
                .map(|s| *s)
                .collect(),
            keywords: self
                .keywords
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Keyword, *e))
                .map(|s| *s)
                .collect(),
            macros: self
                .macros
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Macros, *e))
                .map(|s| *s)
                .collect(),
            includes: self
                .includes
                .iter()
                .filter(|e| predicate(ShaderSymbolType::Include, *e))
                .map(|s| *s)
                .collect(),
        }
    }
    pub fn retain<P: Fn(ShaderSymbolType, &ShaderSymbol) -> bool>(&mut self, predicate: P) {
        self.types.retain(|s| predicate(ShaderSymbolType::Types, s));
        self.constants
            .retain(|s| predicate(ShaderSymbolType::Constants, s));
        self.functions
            .retain(|s| predicate(ShaderSymbolType::Functions, s));
        self.variables
            .retain(|s| predicate(ShaderSymbolType::Variables, s));
        self.call_expression
            .retain(|s| predicate(ShaderSymbolType::CallExpression, s));
        self.keywords
            .retain(|s| predicate(ShaderSymbolType::Keyword, s));
        self.macros
            .retain(|s| predicate(ShaderSymbolType::Macros, s));
        self.includes
            .retain(|s| predicate(ShaderSymbolType::Include, s));
    }
    pub fn iter(&self) -> ShaderSymbolListIterator {
        ShaderSymbolListIterator::new(&self)
    }
    pub fn append_as_reference(&mut self, shader_symbol_list: &'a ShaderSymbolList) {
        self.functions
            .append(&mut shader_symbol_list.functions.iter().collect());
        self.variables
            .append(&mut shader_symbol_list.variables.iter().collect());
        self.call_expression
            .append(&mut shader_symbol_list.call_expression.iter().collect());
        self.constants
            .append(&mut shader_symbol_list.constants.iter().collect());
        self.types
            .append(&mut shader_symbol_list.types.iter().collect());
        self.keywords
            .append(&mut shader_symbol_list.keywords.iter().collect());
        self.macros
            .append(&mut shader_symbol_list.macros.iter().collect());
        self.includes
            .append(&mut shader_symbol_list.includes.iter().collect());
    }
    pub fn append(&mut self, shader_symbol_list: ShaderSymbolListRef<'a>) {
        let mut shader_symbol_list_mut = shader_symbol_list;
        self.functions.append(&mut shader_symbol_list_mut.functions);
        self.variables.append(&mut shader_symbol_list_mut.variables);
        self.call_expression
            .append(&mut shader_symbol_list_mut.call_expression);
        self.constants.append(&mut shader_symbol_list_mut.constants);
        self.types.append(&mut shader_symbol_list_mut.types);
        self.keywords.append(&mut shader_symbol_list_mut.keywords);
        self.macros.append(&mut shader_symbol_list_mut.macros);
        self.includes.append(&mut shader_symbol_list_mut.includes);
    }
}

impl<'a> From<&'a ShaderSymbolList> for ShaderSymbolListRef<'a> {
    fn from(symbol_list: &'a ShaderSymbolList) -> Self {
        Self {
            types: symbol_list.types.iter().collect(),
            constants: symbol_list.constants.iter().collect(),
            variables: symbol_list.variables.iter().collect(),
            call_expression: symbol_list.call_expression.iter().collect(),
            functions: symbol_list.functions.iter().collect(),
            keywords: symbol_list.keywords.iter().collect(),
            macros: symbol_list.macros.iter().collect(),
            includes: symbol_list.includes.iter().collect(),
        }
    }
}

impl<'a> Into<ShaderSymbolList> for ShaderSymbolListRef<'a> {
    fn into(self) -> ShaderSymbolList {
        ShaderSymbolList {
            types: self.types.into_iter().cloned().collect(),
            constants: self.constants.into_iter().cloned().collect(),
            variables: self.variables.into_iter().cloned().collect(),
            call_expression: self.call_expression.into_iter().cloned().collect(),
            functions: self.functions.into_iter().cloned().collect(),
            keywords: self.keywords.into_iter().cloned().collect(),
            macros: self.macros.into_iter().cloned().collect(),
            includes: self.includes.into_iter().cloned().collect(),
        }
    }
}

pub struct ShaderSymbolListIterator<'a> {
    list: &'a ShaderSymbolListRef<'a>,
    current: Option<ShaderSymbolType>,
    iterator: std::slice::Iter<'a, &'a ShaderSymbol>,
}

impl<'a> ShaderSymbolListIterator<'a> {
    pub fn new(symbol_list: &'a ShaderSymbolListRef<'a>) -> Self {
        Self {
            list: symbol_list,
            current: Some(ShaderSymbolType::Types), // First one
            iterator: symbol_list.types.iter(),
        }
    }
}

impl<'a> Iterator for ShaderSymbolListIterator<'a> {
    type Item = &'a ShaderSymbol;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iterator.next() {
            Some(symbol) => Some(symbol),
            None => match &self.current {
                Some(ty) => match ty {
                    ShaderSymbolType::Types => {
                        self.current = Some(ShaderSymbolType::Constants);
                        self.iterator = self.list.constants.iter();
                        self.next()
                    }
                    ShaderSymbolType::Constants => {
                        self.current = Some(ShaderSymbolType::Variables);
                        self.iterator = self.list.variables.iter();
                        self.next()
                    }
                    ShaderSymbolType::Variables => {
                        self.current = Some(ShaderSymbolType::CallExpression);
                        self.iterator = self.list.call_expression.iter();
                        self.next()
                    }
                    ShaderSymbolType::CallExpression => {
                        self.current = Some(ShaderSymbolType::Functions);
                        self.iterator = self.list.functions.iter();
                        self.next()
                    }
                    ShaderSymbolType::Functions => {
                        self.current = Some(ShaderSymbolType::Keyword);
                        self.iterator = self.list.keywords.iter();
                        self.next()
                    }
                    ShaderSymbolType::Keyword => {
                        self.current = Some(ShaderSymbolType::Macros);
                        self.iterator = self.list.macros.iter();
                        self.next()
                    }
                    ShaderSymbolType::Macros => {
                        self.current = Some(ShaderSymbolType::Include);
                        self.iterator = self.list.includes.iter();
                        self.next()
                    }
                    ShaderSymbolType::Include => {
                        self.current = None;
                        self.next()
                    }
                },
                None => None,
            },
        }
    }
}

impl ShaderSymbolType {
    // Transient symbol are not serialized nor used for hover & completion.
    pub fn is_transient(&self) -> bool {
        match &self {
            Self::CallExpression => true,
            _ => false,
        }
    }
}

impl ShaderSymbol {
    pub fn is_type(&self, ty: ShaderSymbolType) -> bool {
        match self.get_type() {
            Some(tty) => tty == ty,
            None => false,
        }
    }
    pub fn is_transient(&self) -> bool {
        match self.get_type() {
            Some(ty) => ty.is_transient(),
            None => false,
        }
    }
    pub fn get_type(&self) -> Option<ShaderSymbolType> {
        match &self.data {
            ShaderSymbolData::Types { constructors: _ } => Some(ShaderSymbolType::Types),
            ShaderSymbolData::Struct {
                constructors: _,
                members: _,
                methods: _,
            } => Some(ShaderSymbolType::Types),
            ShaderSymbolData::Constants {
                ty: _,
                qualifier: _,
                value: _,
            } => Some(ShaderSymbolType::Constants),
            ShaderSymbolData::Variables { ty: _, count: _ } => Some(ShaderSymbolType::Variables),
            ShaderSymbolData::CallExpression {
                label: _,
                range: _,
                parameters: _,
            } => Some(ShaderSymbolType::CallExpression),
            ShaderSymbolData::Functions { signatures: _ } => Some(ShaderSymbolType::Functions),
            ShaderSymbolData::Keyword {} => Some(ShaderSymbolType::Keyword),
            ShaderSymbolData::Link { target: _ } => Some(ShaderSymbolType::Include),
            ShaderSymbolData::Macro { value: _ } => Some(ShaderSymbolType::Macros),
        }
    }
    pub fn format(&self) -> String {
        match &self.data {
            ShaderSymbolData::Types { constructors: _ } => format!("{}", self.label.clone()),
            ShaderSymbolData::Struct {
                constructors: _,
                members: _,
                methods: _,
            } => format!("struct {}", self.label.clone()),
            ShaderSymbolData::Constants {
                ty,
                qualifier,
                value,
            } => format!("{} {} {} = {};", qualifier, ty, self.label.clone(), value),
            ShaderSymbolData::Variables { ty, count } => match count {
                Some(count) => format!("{} {}[{}]", ty, self.label, count),
                None => format!("{} {}", ty, self.label),
            },
            ShaderSymbolData::CallExpression {
                label,
                range: _,
                parameters,
            } => format!(
                "{}({})",
                label,
                parameters
                    .iter()
                    .map(|(label, _)| label.clone())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            ShaderSymbolData::Functions { signatures } => signatures[0].format(&self.label), // TODO: append +1 symbol
            ShaderSymbolData::Keyword {} => format!("{}", self.label.clone()),
            ShaderSymbolData::Link { target } => {
                format!("\"{}\":{}:{}", self.label, target.line, target.pos)
            }
            ShaderSymbolData::Macro { value } => {
                format!("#define {} {}", self.label, value)
            }
        }
    }
}
