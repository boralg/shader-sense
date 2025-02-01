use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::shader::ShaderStage;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderParameter {
    pub ty: String,
    pub label: String,
    pub description: String,
}

#[allow(non_snake_case)] // for JSON
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderSignature {
    pub returnType: String,
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
    pub fn from_byte_offset(content: &str, byte_offset: usize, file_path: &Path) -> ShaderPosition {
        let line = content[..byte_offset].lines().count() - 1;
        let pos = content[byte_offset..].as_ptr() as usize
            - content[..byte_offset].lines().last().unwrap().as_ptr() as usize;
        ShaderPosition {
            line: line as u32,
            pos: pos as u32,
            file_path: PathBuf::from(file_path),
        }
    }
    pub fn to_byte_offset(&self, content: &str) -> usize {
        match content.lines().nth(self.line as usize) {
            Some(line) => {
                let pos = line.as_ptr() as usize - content.as_ptr() as usize;
                pos + self.pos as usize
            }
            None => 0, // Error
        }
    }
}

#[derive(Debug, Default, Clone)]
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
pub struct ShaderPreprocessorInclude {
    pub path: PathBuf,
    pub range: ShaderRange,
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessorDefine {
    pub name: String,
    pub range: ShaderRange,
    pub value: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ShaderPreprocessor {
    pub includes: Vec<ShaderPreprocessorInclude>,
    pub defines: Vec<ShaderPreprocessorDefine>,
    pub regions: Vec<ShaderRegion>,
}
impl ShaderPreprocessorDefine {
    pub fn new(name: String, range: ShaderRange, value: Option<String>) -> Self {
        Self { name, range, value }
    }
}
impl ShaderPreprocessorInclude {
    pub fn new(path: PathBuf, range: ShaderRange) -> Self {
        Self { path, range }
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
            },
            range: None, // Should have a position ?
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
            scope_stack: None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum ShaderSymbolData {
    #[default]
    None,
    Types {
        ty: String,
    },
    Struct {
        members: Vec<ShaderMember>,
        methods: Vec<ShaderMethod>,
    },
    Constants {
        ty: String,
        qualifier: String,
        value: String,
    },
    Variables {
        ty: String,
    },
    Functions {
        signatures: Vec<ShaderSignature>,
    },
    Keyword {},
    Link {
        target: ShaderPosition,
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
    pub scope_stack: Option<Vec<ShaderScope>>, // Stack of declaration
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum ShaderSymbolType {
    #[default]
    Types,
    Constants,
    Variables,
    Functions,
    Keyword,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShaderSymbolList {
    // Could use maps for faster search access (hover provider)
    pub types: Vec<ShaderSymbol>,
    pub constants: Vec<ShaderSymbol>,
    pub variables: Vec<ShaderSymbol>,
    pub functions: Vec<ShaderSymbol>,
    pub keywords: Vec<ShaderSymbol>,
}

impl ShaderSymbolList {
    pub fn parse_from_json(file_content: String) -> ShaderSymbolList {
        serde_json::from_str::<ShaderSymbolList>(&file_content)
            .expect("Failed to parse ShaderSymbolList")
    }
    pub fn find_symbols(&self, label: String) -> Vec<ShaderSymbol> {
        self.iter()
            .map(|e| {
                e.0.iter()
                    .filter_map(|e| {
                        if e.label == label {
                            Some(e.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<ShaderSymbol>>()
            })
            .collect::<Vec<Vec<ShaderSymbol>>>()
            .concat()
    }
    pub fn find_symbol(&self, label: &String) -> Option<ShaderSymbol> {
        for symbol_list in self.iter() {
            match symbol_list.0.iter().find(|e| e.label == *label) {
                Some(symbol) => return Some(symbol.clone()),
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
        self.constants.append(&mut shader_symbol_list_mut.constants);
        self.types.append(&mut shader_symbol_list_mut.types);
        self.keywords.append(&mut shader_symbol_list_mut.keywords);
    }
    pub fn iter(&self) -> ShaderSymbolListIterator {
        ShaderSymbolListIterator {
            list: self,
            ty: Some(ShaderSymbolType::Types), // First one
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
        }
    }
    pub fn retain<P: Fn(&ShaderSymbol) -> bool>(&mut self, predicate: P) {
        self.types.retain(&predicate);
        self.constants.retain(&predicate);
        self.functions.retain(&predicate);
        self.variables.retain(&predicate);
        self.keywords.retain(&predicate);
    }
    pub fn filter_scoped_symbol(&self, cursor_position: ShaderPosition) -> ShaderSymbolList {
        // Ensure symbols are already defined at pos
        let filter_position = |shader_symbol: &ShaderSymbol| -> bool {
            match &shader_symbol.scope_stack {
                Some(scope) => {
                    if scope.is_empty() {
                        true // Global space
                    } else {
                        match &shader_symbol.range {
                            Some(range) => {
                                if range.start.line == cursor_position.line {
                                    cursor_position.pos > range.start.pos
                                } else {
                                    cursor_position.line > range.start.line
                                }
                            }
                            None => true, // intrinsics
                        }
                    }
                }
                None => true, // Global space
            }
        };
        // Ensure symbols are in scope
        let filter_scope = |shader_symbol: &ShaderSymbol| -> bool {
            match &shader_symbol.range {
                Some(symbol_range) => {
                    if symbol_range.start.file_path == cursor_position.file_path {
                        // If we are in main file, check if scope in range.
                        match &shader_symbol.scope_stack {
                            Some(symbol_scope_stack) => {
                                for symbol_scope in symbol_scope_stack {
                                    if !symbol_scope.contain(&cursor_position) {
                                        return false;
                                    }
                                }
                                true
                            }
                            None => true,
                        }
                    } else {
                        // If we are not in main file, only show whats in global scope.
                        match &shader_symbol.scope_stack {
                            Some(symbol_scope_stack) => symbol_scope_stack.is_empty(), // Global scope or inaccessible
                            None => true,
                        }
                    }
                }
                None => true,
            }
        };
        // TODO: should add a filter for when multiple same definition: pick latest (shadowing)
        let filter_all = |shader_symbols: &ShaderSymbol| -> Option<ShaderSymbol> {
            if filter_position(shader_symbols) && filter_scope(shader_symbols) {
                Some(shader_symbols.clone())
            } else {
                None
            }
        };
        ShaderSymbolList {
            functions: self.functions.iter().filter_map(filter_all).collect(),
            types: self.types.iter().filter_map(filter_all).collect(),
            constants: self.constants.iter().filter_map(filter_all).collect(),
            variables: self.variables.iter().filter_map(filter_all).collect(),
            keywords: self.keywords.iter().filter_map(filter_all).collect(),
        }
    }
}

pub struct ShaderSymbolListIterator<'a> {
    list: &'a ShaderSymbolList,
    ty: Option<ShaderSymbolType>,
}

impl<'a> Iterator for ShaderSymbolListIterator<'a> {
    type Item = (&'a Vec<ShaderSymbol>, ShaderSymbolType);

    fn next(&mut self) -> Option<Self::Item> {
        match &self.ty {
            Some(ty) => match ty {
                ShaderSymbolType::Types => {
                    self.ty = Some(ShaderSymbolType::Constants);
                    Some((&self.list.types, ShaderSymbolType::Types))
                }
                ShaderSymbolType::Constants => {
                    self.ty = Some(ShaderSymbolType::Variables);
                    Some((&self.list.constants, ShaderSymbolType::Constants))
                }
                ShaderSymbolType::Variables => {
                    self.ty = Some(ShaderSymbolType::Functions);
                    Some((&self.list.variables, ShaderSymbolType::Variables))
                }
                ShaderSymbolType::Functions => {
                    self.ty = Some(ShaderSymbolType::Keyword);
                    Some((&self.list.functions, ShaderSymbolType::Functions))
                }
                ShaderSymbolType::Keyword => {
                    self.ty = None;
                    Some((&self.list.keywords, ShaderSymbolType::Keyword))
                }
            },
            None => None,
        }
    }
}

pub struct ShaderSymbolListIntoIterator {
    list: ShaderSymbolList,
    ty: Option<ShaderSymbolType>,
}
impl Iterator for ShaderSymbolListIntoIterator {
    type Item = (Vec<ShaderSymbol>, ShaderSymbolType);

    fn next(&mut self) -> Option<Self::Item> {
        match self.ty.clone() {
            Some(ty) => match ty {
                ShaderSymbolType::Types => {
                    self.ty = Some(ShaderSymbolType::Constants);
                    Some((
                        std::mem::take(&mut self.list.types),
                        ShaderSymbolType::Types,
                    ))
                }
                ShaderSymbolType::Constants => {
                    self.ty = Some(ShaderSymbolType::Variables);
                    Some((
                        std::mem::take(&mut self.list.constants),
                        ShaderSymbolType::Constants,
                    ))
                }
                ShaderSymbolType::Variables => {
                    self.ty = Some(ShaderSymbolType::Functions);
                    Some((
                        std::mem::take(&mut self.list.variables),
                        ShaderSymbolType::Variables,
                    ))
                }
                ShaderSymbolType::Functions => {
                    self.ty = Some(ShaderSymbolType::Keyword);
                    Some((
                        std::mem::take(&mut self.list.functions),
                        ShaderSymbolType::Functions,
                    ))
                }
                ShaderSymbolType::Keyword => {
                    self.ty = None;
                    Some((
                        std::mem::take(&mut self.list.keywords),
                        ShaderSymbolType::Keyword,
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
            ty: Some(ShaderSymbolType::Types), // First one
        }
    }
}

impl ShaderSymbol {
    pub fn format(&self) -> String {
        match &self.data {
            ShaderSymbolData::None => format!("Unknown {}", self.label.clone()),
            ShaderSymbolData::Types { ty } => format!("{}", ty), // ty == label
            ShaderSymbolData::Struct {
                members: _,
                methods: _,
            } => format!("struct {}", self.label.clone()),
            ShaderSymbolData::Constants {
                ty,
                qualifier,
                value,
            } => format!("{} {} {} = {};", qualifier, ty, self.label.clone(), value),
            ShaderSymbolData::Variables { ty } => format!("{} {}", ty, self.label),
            ShaderSymbolData::Functions { signatures } => signatures[0].format(&self.label), // TODO: append +1 symbol
            ShaderSymbolData::Keyword {} => format!("{}", self.label.clone()),
            ShaderSymbolData::Link { target } => {
                format!("\"{}\":{}:{}", self.label, target.line, target.pos)
            }
        }
    }
}
