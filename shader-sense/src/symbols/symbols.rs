use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
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
        Self { start, end }
    }
    pub fn whole(_content: &str) -> Self {
        todo!()
    }
    pub fn contain_bounds(&self, position: &ShaderRange) -> bool {
        self.contain(&position.start) && self.contain(&position.end)
    }
    pub fn contain(&self, position: &ShaderPosition) -> bool {
        assert!(
            self.start.file_path == self.end.file_path,
            "Position start & end should have same value."
        );
        // Check same file
        if position.file_path == self.start.file_path {
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
    defines: HashMap<String, String>, // TODO: Should store position aswell... At least target file path.
    include_handler: IncludeHandler,
    dirty_files: HashSet<PathBuf>, // Dirty files that need to be recomputed no matter what.
    depth: usize,
}

impl ShaderPreprocessorContext {
    pub fn main(file_path: &Path, symbol_params: ShaderSymbolParams) -> Self {
        Self {
            defines: symbol_params.defines,
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
    pub fn append_defines(&mut self, defines: Vec<ShaderPreprocessorDefine>) {
        for define in defines {
            self.defines
                .insert(define.name, define.value.unwrap_or("".into()));
        }
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
        context.defines != self.defines || context.dirty_files.contains(file_path)
    }
    pub fn get_define_value(&self, name: &str) -> Option<String> {
        self.defines
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    }
    pub fn get_defines(&self) -> &HashMap<String, String> {
        &self.defines
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessorInclude {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub range: ShaderRange,
    pub cache: Option<ShaderSymbols>,
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessorDefine {
    pub name: String,
    pub range: Option<ShaderRange>,
    pub value: Option<String>,
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
            name,
            range: Some(range),
            value,
        }
    }
}
impl ShaderPreprocessorInclude {
    pub fn new(relative_path: String, absolute_path: PathBuf, range: ShaderRange) -> Self {
        Self {
            relative_path,
            absolute_path,
            range,
            cache: None,
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
    pub fn preprocess_symbols(&self, shader_symbols: &mut ShaderSymbolList) {
        // Filter inactive regions symbols
        shader_symbols.retain(|_symbol_type, symbol| {
            let is_in_inactive_region = match &symbol.range {
                Some(range) => {
                    for region in &self.regions {
                        if !region.is_active && region.range.contain_bounds(&range) {
                            return false; // Symbol is in inactive region. Remove it.
                        }
                    }
                    true
                }
                None => true, // keep
            };
            is_in_inactive_region
        });
        // Add defines
        let mut define_symbols: Vec<ShaderSymbol> = self
            .defines
            .iter()
            .map(|define| {
                ShaderSymbol {
                    label: define.name.clone(),
                    description: match &define.value {
                        Some(value) => {
                            format!("Preprocessor macro. Expanding to \n```\n{}\n```", value)
                        }
                        None => format!("Preprocessor macro."),
                    },
                    version: "".into(),
                    stages: vec![],
                    link: None,
                    data: ShaderSymbolData::Macro {
                        value: match &define.value {
                            Some(value) => value.clone(),
                            None => "".into(),
                        },
                    },
                    range: define.range.clone(),
                    scope: None,
                    scope_stack: None, // No scope for define
                }
            })
            .collect();
        // Add includes as symbol
        let mut include_symbols: Vec<ShaderSymbol> = self
            .includes
            .iter()
            .map(|include| {
                ShaderSymbol {
                    label: include.relative_path.clone(),
                    description: format!("Including file {}", include.absolute_path.display()),
                    version: "".into(),
                    stages: vec![],
                    link: None,
                    data: ShaderSymbolData::Link {
                        target: ShaderPosition::new(include.absolute_path.clone(), 0, 0),
                    },
                    range: Some(include.range.clone()),
                    scope: None,
                    scope_stack: None, // No scope for include
                }
            })
            .collect();
        shader_symbols.macros.append(&mut define_symbols);
        shader_symbols.includes.append(&mut include_symbols);
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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum ShaderSymbolData {
    #[default]
    None,
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
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ShaderSymbolType {
    #[default]
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
    // Could use maps for faster search access (hover provider)
    // Should use a tree to store symbols with scopes.
    // And will need a symbol kind aswell.
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

impl ShaderSymbolList {
    pub fn parse_from_json(file_content: String) -> ShaderSymbolList {
        serde_json::from_str::<ShaderSymbolList>(&file_content)
            .expect("Failed to parse ShaderSymbolList")
    }
    fn is_symbol_defined_at(
        shader_symbol: &ShaderSymbol,
        cursor_position: &ShaderPosition,
    ) -> bool {
        match &shader_symbol.range {
            Some(symbol_range) => {
                if symbol_range.start.file_path == cursor_position.file_path {
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
            .map(|(sl, ty)| {
                if !ty.is_transient() {
                    sl.iter()
                        .filter(|symbol| {
                            if symbol.label == *label {
                                Self::is_symbol_defined_at(symbol, position)
                            } else {
                                false
                            }
                        })
                        .collect::<Vec<&ShaderSymbol>>()
                } else {
                    vec![]
                }
            })
            .collect::<Vec<Vec<&ShaderSymbol>>>()
            .concat()
    }
    pub fn filter_scoped_symbol(&self, cursor_position: &ShaderPosition) -> ShaderSymbolList {
        let mut filter_scoped_symbols = self.clone();
        filter_scoped_symbols.retain(|symbol_type, symbol| {
            !symbol_type.is_transient() && Self::is_symbol_defined_at(symbol, cursor_position)
        });
        filter_scoped_symbols
    }
    pub fn find_symbols(&self, label: &String) -> Vec<&ShaderSymbol> {
        self.iter()
            .map(|(sl, ty)| {
                if !ty.is_transient() {
                    sl.iter()
                        .filter_map(|e| if e.label == *label { Some(e) } else { None })
                        .collect::<Vec<&ShaderSymbol>>()
                } else {
                    vec![]
                }
            })
            .collect::<Vec<Vec<&ShaderSymbol>>>()
            .concat()
    }
    pub fn find_symbol(&self, label: &String) -> Option<&ShaderSymbol> {
        for symbol_list in self.iter() {
            match symbol_list.0.iter().find(|e| e.label == *label) {
                Some(symbol) => return Some(symbol),
                None => {}
            }
        }
        None
    }
    pub fn find_type_symbol(&self, label: &String) -> Option<ShaderSymbol> {
        self.types
            .iter()
            .find(|s| s.label == *label)
            .map(|s| s.clone())
    }
    pub fn append(&mut self, shader_symbol_list: ShaderSymbolList) {
        let mut shader_symbol_list_mut = shader_symbol_list;
        self.functions.append(&mut shader_symbol_list_mut.functions);
        self.variables.append(&mut shader_symbol_list_mut.variables);
        self.call_expression
            .append(&mut shader_symbol_list_mut.variables);
        self.constants.append(&mut shader_symbol_list_mut.constants);
        self.types.append(&mut shader_symbol_list_mut.types);
        self.keywords.append(&mut shader_symbol_list_mut.keywords);
        self.macros.append(&mut shader_symbol_list_mut.macros);
        self.includes.append(&mut shader_symbol_list_mut.includes);
    }
    pub fn iter(&self) -> ShaderSymbolListIterator {
        ShaderSymbolListIterator {
            list: self,
            next: Some(ShaderSymbolType::Types), // First one
        }
    }
    pub fn filter<P: Fn(&ShaderSymbol) -> bool>(&self, predicate: P) -> ShaderSymbolList {
        ShaderSymbolList {
            types: self
                .types
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            constants: self
                .constants
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            variables: self
                .variables
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            call_expression: self
                .call_expression
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            functions: self
                .functions
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            keywords: self
                .keywords
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            macros: self
                .macros
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
                .collect(),
            includes: self
                .includes
                .iter()
                .filter(|e| predicate(*e))
                .cloned()
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
}

pub struct ShaderSymbolListIterator<'a> {
    list: &'a ShaderSymbolList,
    next: Option<ShaderSymbolType>,
}

impl<'a> Iterator for ShaderSymbolListIterator<'a> {
    type Item = (&'a Vec<ShaderSymbol>, ShaderSymbolType);

    fn next(&mut self) -> Option<Self::Item> {
        match &self.next {
            Some(ty) => match ty {
                ShaderSymbolType::Types => {
                    self.next = Some(ShaderSymbolType::Constants);
                    Some((&self.list.types, ShaderSymbolType::Types))
                }
                ShaderSymbolType::Constants => {
                    self.next = Some(ShaderSymbolType::Variables);
                    Some((&self.list.constants, ShaderSymbolType::Constants))
                }
                ShaderSymbolType::Variables => {
                    self.next = Some(ShaderSymbolType::CallExpression);
                    Some((&self.list.variables, ShaderSymbolType::Variables))
                }
                ShaderSymbolType::CallExpression => {
                    self.next = Some(ShaderSymbolType::Functions);
                    Some((&self.list.call_expression, ShaderSymbolType::CallExpression))
                }
                ShaderSymbolType::Functions => {
                    self.next = Some(ShaderSymbolType::Keyword);
                    Some((&self.list.functions, ShaderSymbolType::Functions))
                }
                ShaderSymbolType::Keyword => {
                    self.next = Some(ShaderSymbolType::Macros);
                    Some((&self.list.keywords, ShaderSymbolType::Keyword))
                }
                ShaderSymbolType::Macros => {
                    self.next = Some(ShaderSymbolType::Include);
                    Some((&self.list.macros, ShaderSymbolType::Macros))
                }
                ShaderSymbolType::Include => {
                    self.next = None;
                    Some((&self.list.includes, ShaderSymbolType::Include))
                }
            },
            None => None,
        }
    }
}

pub struct ShaderSymbolListIntoIterator {
    list: ShaderSymbolList,
    next: Option<ShaderSymbolType>,
}
impl Iterator for ShaderSymbolListIntoIterator {
    type Item = (Vec<ShaderSymbol>, ShaderSymbolType);

    fn next(&mut self) -> Option<Self::Item> {
        match self.next.clone() {
            Some(next) => match next {
                ShaderSymbolType::Types => {
                    self.next = Some(ShaderSymbolType::Constants);
                    Some((
                        std::mem::take(&mut self.list.types),
                        ShaderSymbolType::Types,
                    ))
                }
                ShaderSymbolType::Constants => {
                    self.next = Some(ShaderSymbolType::Variables);
                    Some((
                        std::mem::take(&mut self.list.constants),
                        ShaderSymbolType::Constants,
                    ))
                }
                ShaderSymbolType::Variables => {
                    self.next = Some(ShaderSymbolType::CallExpression);
                    Some((
                        std::mem::take(&mut self.list.variables),
                        ShaderSymbolType::Variables,
                    ))
                }
                ShaderSymbolType::CallExpression => {
                    self.next = Some(ShaderSymbolType::Functions);
                    Some((
                        std::mem::take(&mut self.list.call_expression),
                        ShaderSymbolType::CallExpression,
                    ))
                }
                ShaderSymbolType::Functions => {
                    self.next = Some(ShaderSymbolType::Keyword);
                    Some((
                        std::mem::take(&mut self.list.functions),
                        ShaderSymbolType::Functions,
                    ))
                }
                ShaderSymbolType::Keyword => {
                    self.next = Some(ShaderSymbolType::Macros);
                    Some((
                        std::mem::take(&mut self.list.keywords),
                        ShaderSymbolType::Keyword,
                    ))
                }
                ShaderSymbolType::Macros => {
                    self.next = Some(ShaderSymbolType::Include);
                    Some((
                        std::mem::take(&mut self.list.macros),
                        ShaderSymbolType::Macros,
                    ))
                }
                ShaderSymbolType::Include => {
                    self.next = None;
                    Some((
                        std::mem::take(&mut self.list.includes),
                        ShaderSymbolType::Include,
                    ))
                }
            },
            None => None,
        }
    }
}

impl IntoIterator for ShaderSymbolList {
    type Item = (Vec<ShaderSymbol>, ShaderSymbolType);
    type IntoIter = ShaderSymbolListIntoIterator;

    fn into_iter(self) -> Self::IntoIter {
        ShaderSymbolListIntoIterator {
            list: self,
            next: Some(ShaderSymbolType::Types), // First one
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
    pub fn get_type(&self) -> Option<ShaderSymbolType> {
        match &self.data {
            ShaderSymbolData::None => None,
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
            ShaderSymbolData::None => format!("Unknown {}", self.label.clone()),
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
