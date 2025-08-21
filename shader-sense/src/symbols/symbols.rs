use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    include::IncludeHandler,
    position::{ShaderFilePosition, ShaderFileRange, ShaderRange},
    shader::{
        HlslShaderModel, HlslVersion, ShaderCompilationParams, ShaderContextParams, ShaderStage,
    },
    shader_error::ShaderDiagnostic,
};

use super::shader_module::ShaderSymbols;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderParameter {
    pub ty: String,
    pub label: String,
    pub count: Option<u32>,
    pub description: String,
    #[serde(skip)] // Runtime only
    pub range: Option<ShaderRange>,
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
    pub fn format_with_context(&self, label: &str, context: &str) -> String {
        let signature = self
            .parameters
            .iter()
            .map(|p| format!("{} {}", p.ty, p.label))
            .collect::<Vec<String>>();
        format!(
            "{} {}::{}({})",
            self.returnType,
            context,
            label,
            signature.join(", ")
        )
    }
}

pub type ShaderScope = ShaderRange;

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
    pub fn main(file_path: &Path, shader_params: ShaderContextParams) -> Self {
        Self {
            defines: shader_params
                .defines
                .iter()
                .map(|(key, value)| ShaderSymbol {
                    label: key.clone(),
                    description: format!("Preprocessor macro. Expanding to \n```\n{}\n```", value),
                    requirement: None,
                    link: None,
                    data: ShaderSymbolData::Macro {
                        value: value.clone(),
                    },
                    runtime: None,
                })
                .collect(),
            include_handler: IncludeHandler::main(
                &file_path,
                shader_params.includes,
                shader_params.path_remapping,
            ),
            dirty_files: HashSet::new(),
            depth: 0,
        }
    }
    pub fn mark_dirty(&mut self, file_path: PathBuf) {
        self.dirty_files.insert(file_path);
    }
    pub fn search_path_in_includes(&mut self, path: &Path) -> Option<PathBuf> {
        self.include_handler.search_path_in_includes(path)
    }
    pub fn push_directory_stack(&mut self, canonical_path: &Path) {
        self.include_handler.push_directory_stack(canonical_path);
    }
    pub fn push_define(&mut self, name: &str, value: &str) {
        self.defines.push(ShaderSymbol {
            label: name.into(),
            description: format!(
                "Config preprocessor macro. Expanding to \n```\n{}\n```",
                value
            ),
            requirement: None,
            link: None,
            data: ShaderSymbolData::Macro {
                value: value.into(),
            },
            runtime: None,
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
        fn are_defines_equal(lhs: &Vec<ShaderSymbol>, rhs: &Vec<ShaderSymbol>) -> bool {
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
        fn are_includes_equal(lhs: &HashSet<PathBuf>, rhs: &HashSet<PathBuf>) -> bool {
            if lhs.len() != rhs.len() {
                return false;
            }
            for lhs_symbol in lhs.iter() {
                if rhs
                    .iter()
                    .find(|rhs_symbol| lhs_symbol.as_os_str() == rhs_symbol.as_os_str())
                    .is_none()
                {
                    return false;
                }
            }
            true
        }
        !are_defines_equal(&context.defines, &self.defines)
            || !are_includes_equal(
                context.include_handler.get_includes(),
                self.include_handler.get_includes(),
            )
            || context.dirty_files.contains(file_path)
    }
    pub fn get_define_value(&self, name: &str) -> Option<String> {
        self.defines
            .iter()
            .find(|symbol| *symbol.label == *name)
            .map(|symbol| match &symbol.data {
                ShaderSymbolData::Macro { value } => value.clone(),
                _ => panic!("Expected ShaderSymbolData::Macro"),
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
    symbol: ShaderSymbol,
}

#[derive(Debug, Clone)]
pub struct ShaderPreprocessorDefine {
    symbol: ShaderSymbol,
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
    pub fn new(name: String, range: ShaderFileRange, value: Option<String>) -> Self {
        Self {
            symbol: ShaderSymbol {
                label: name.clone(),
                description: match &value {
                    Some(value) => {
                        format!("Preprocessor macro. Expanding to \n```\n{}\n```", value)
                    }
                    None => format!("Preprocessor macro."),
                },
                requirement: None,
                link: None,
                data: ShaderSymbolData::Macro {
                    value: match &value {
                        Some(value) => value.clone(),
                        None => "".into(),
                    },
                },
                runtime: Some(ShaderSymbolRuntime::global(range.file_path, range.range)),
            },
        }
    }
    pub fn get_file_path(&self) -> Option<&Path> {
        self.symbol.runtime.as_ref().map(|r| r.file_path.as_path())
    }
    pub fn get_range(&self) -> Option<&ShaderRange> {
        self.symbol.runtime.as_ref().map(|r| &r.range)
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
    pub fn new(relative_path: String, absolute_path: PathBuf, range: ShaderFileRange) -> Self {
        Self {
            cache: None,
            symbol: ShaderSymbol {
                label: relative_path,
                description: format!("Including file {}", absolute_path.display()),
                requirement: None,
                link: None,
                data: ShaderSymbolData::Link {
                    target: ShaderFilePosition::new(absolute_path, 0, 0),
                },
                runtime: Some(ShaderSymbolRuntime::global(range.file_path, range.range)),
            },
        }
    }
    pub fn get_range(&self) -> &ShaderRange {
        self.symbol
            .runtime
            .as_ref()
            .map(|r| &r.range)
            .expect("Include symbol should have range.")
    }
    pub fn get_file_range(&self) -> ShaderFileRange {
        self.symbol
            .runtime
            .as_ref()
            .map(|r| r.range.clone().into_file(r.file_path.clone()))
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
            shader_symbols.filter(move |_symbol_type, symbol| match &symbol.runtime {
                Some(runtime) => inactive_regions
                    .iter()
                    .find(|r| r.range.contain_bounds(&runtime.range))
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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ShaderMember {
    pub context: String,
    pub parameters: ShaderParameter,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShaderMethod {
    pub label: String,
    pub context: String,
    pub signature: ShaderSignature,
    #[serde(skip)] // Runtime only
    pub range: Option<ShaderRange>,
}

impl ShaderMember {
    pub fn as_symbol(&self, file_path: Option<PathBuf>) -> ShaderSymbol {
        ShaderSymbol {
            label: self.parameters.label.clone(),
            description: self.parameters.description.clone(),
            requirement: None,
            link: None,
            data: ShaderSymbolData::Parameter {
                context: self.context.clone(),
                ty: self.parameters.ty.clone(),
                count: self.parameters.count.clone(),
            },
            runtime: match file_path {
                // We assume it as range if it has path.
                // TODO: This should not be a global.
                Some(file_path) => Some(ShaderSymbolRuntime::global(
                    file_path,
                    self.parameters.range.clone().unwrap(),
                )),
                None => None,
            },
        }
    }
}

impl ShaderMethod {
    pub fn as_symbol(&self, file_path: Option<PathBuf>) -> ShaderSymbol {
        ShaderSymbol {
            label: self.label.clone(),
            description: self.signature.description.clone(),
            requirement: None,
            link: None,
            data: ShaderSymbolData::Method {
                context: self.context.clone(),
                signatures: vec![self.signature.clone()],
            },
            runtime: match file_path {
                Some(file_path) => Some(ShaderSymbolRuntime::global(
                    file_path,
                    self.range.clone().unwrap(),
                )),
                None => None,
            },
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
    Parameter {
        context: String,
        ty: String,
        count: Option<u32>,
    },
    Method {
        context: String,
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
        target: ShaderFilePosition,
    },
    Macro {
        value: String,
    },
}

#[allow(non_snake_case)] // for JSON
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct HlslRequirementParameter {
    pub stages: Option<Vec<ShaderStage>>, // Stage required by this symbol.
    pub min_version: Option<HlslVersion>, // Minimum HLSL version for this symbol.
    pub version: Option<HlslVersion>,     // Exact HLSL version for this symbol.
    pub min_shader_model: Option<HlslShaderModel>, // Minimum shader model for this symbol.
    pub shader_model: Option<HlslShaderModel>, // Exact shader model for this symbol.
    pub spirv: Option<bool>,              // Requires SPIRV
    pub enable_16bit_types: Option<bool>, // Requires 16bit types.
}
#[allow(non_snake_case)] // for JSON
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct GlslRequirementParameter {
    pub stages: Option<Vec<ShaderStage>>,
    pub min_version: Option<u32>,  // min glsl version
    pub extension: Option<String>, // Extension required for this symbol.
}
#[allow(non_snake_case)] // for JSON
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct WgslRequirementParameter {}

#[allow(non_snake_case)] // for JSON
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub enum RequirementParameter {
    #[default]
    None, // no filter parameters
    Hlsl(HlslRequirementParameter),
    Glsl(GlslRequirementParameter),
    Wgsl(WgslRequirementParameter),
}

impl RequirementParameter {
    pub fn is_met(&self, shader_compilation_params: &ShaderCompilationParams) -> bool {
        match self {
            RequirementParameter::None => true, // No requirement. All good.
            RequirementParameter::Hlsl(requirement) => {
                // TODO: Should try to detect shader stage from file name at a higher level.
                let is_stage_ok = match &requirement.stages {
                    Some(required_stages) => match shader_compilation_params.shader_stage {
                        Some(param_stage) => required_stages.contains(&param_stage),
                        None => true, // requirement, but no stage set. Pass them all
                    },
                    None => true, // No requirements, dont care about stage set.
                };
                let is_version_ok = match &requirement.version {
                    Some(version) => *version == shader_compilation_params.hlsl.version,
                    None => true,
                };
                let is_min_version_ok = match &requirement.min_version {
                    Some(min_version) => *min_version <= shader_compilation_params.hlsl.version,
                    None => true,
                };
                let is_shader_model_ok = match &requirement.shader_model {
                    Some(shader_model) => {
                        *shader_model == shader_compilation_params.hlsl.shader_model
                    }
                    None => true,
                };
                let is_min_shader_model_ok = match &requirement.min_shader_model {
                    Some(min_shader_model) => {
                        *min_shader_model <= shader_compilation_params.hlsl.shader_model
                    }
                    None => true,
                };
                let is_spirv_ok = match &requirement.spirv {
                    Some(spirv) => *spirv == shader_compilation_params.hlsl.spirv,
                    None => true,
                };
                let is_16bit_ok = match &requirement.enable_16bit_types {
                    Some(enable_16bit_types) => {
                        *enable_16bit_types == shader_compilation_params.hlsl.enable16bit_types
                    }
                    None => true,
                };
                is_stage_ok
                    && is_min_version_ok
                    && is_version_ok
                    && is_min_shader_model_ok
                    && is_shader_model_ok
                    && is_spirv_ok
                    && is_16bit_ok
            }
            RequirementParameter::Glsl(requirement) => {
                let is_stage_ok = match &requirement.stages {
                    Some(required_stages) => match shader_compilation_params.shader_stage {
                        Some(param_stage) => required_stages.contains(&param_stage),
                        None => true, // requirement, but no stage set. Pass them all
                    },
                    None => true, // No requirements, dont care about stage set.
                };
                let is_version_ok = match &requirement.min_version {
                    Some(_min_version) => true, // TODO: make this work.
                    None => true,
                };
                let is_extension_ok = match &requirement.extension {
                    Some(_extension) => true, // TODO: make this work.
                    None => true,
                };
                is_stage_ok && is_version_ok && is_extension_ok
            }
            RequirementParameter::Wgsl(_) => {
                // Nothing yet here to filter.
                true
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShaderSymbolRuntime {
    pub file_path: PathBuf,            // file of the symbol.
    pub range: ShaderRange,            // Range of symbol in shader
    pub scope: Option<ShaderScope>,    // Owning scope
    pub scope_stack: Vec<ShaderScope>, // Stack of declaration
}

impl ShaderSymbolRuntime {
    pub fn new(
        file_path: PathBuf,
        range: ShaderRange,
        scope: Option<ShaderScope>,
        scope_stack: Vec<ShaderScope>,
    ) -> Self {
        Self {
            file_path,
            range,
            scope,
            scope_stack,
        }
    }
    pub fn global(file_path: PathBuf, range: ShaderRange) -> Self {
        Self::new(file_path, range, None, Vec::new())
    }
    pub fn owner(file_path: PathBuf, range: ShaderRange, scope: Option<ShaderScope>) -> Self {
        Self::new(file_path, range, scope, Vec::new())
    }
    pub fn variable(file_path: PathBuf, range: ShaderRange, scope_stack: Vec<ShaderScope>) -> Self {
        Self::new(file_path, range, None, scope_stack)
    }
}

#[allow(non_snake_case)] // for JSON
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShaderSymbol {
    pub label: String, // Label for the item
    // TODO:OPTIM: description & link are only used by intrinsics and should not take all memory.
    pub description: String,                       // Description of the item
    pub link: Option<String>,                      // Link to some external documentation
    pub requirement: Option<RequirementParameter>, // Used for filtering symbols.
    pub data: ShaderSymbolData,                    // Data for the variable

    #[serde(skip)] // Runtime info. No serialization.
    pub runtime: Option<ShaderSymbolRuntime>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
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
    pub fn as_ref<'a>(&'a self) -> ShaderSymbolListRef<'a> {
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
    pub fn to_owned(&self) -> ShaderSymbolList {
        ShaderSymbolList {
            types: self.types.iter().map(|s| (*s).clone()).collect(),
            constants: self.constants.iter().map(|s| (*s).clone()).collect(),
            variables: self.variables.iter().map(|s| (*s).clone()).collect(),
            call_expression: self.call_expression.iter().map(|s| (*s).clone()).collect(),
            functions: self.functions.iter().map(|s| (*s).clone()).collect(),
            keywords: self.keywords.iter().map(|s| (*s).clone()).collect(),
            macros: self.macros.iter().map(|s| (*s).clone()).collect(),
            includes: self.includes.iter().map(|s| (*s).clone()).collect(),
        }
    }
    fn is_symbol_defined_at(
        shader_symbol: &ShaderSymbol,
        cursor_position: &ShaderFilePosition,
    ) -> bool {
        match &shader_symbol.runtime {
            Some(runtime) => {
                if runtime.file_path.as_os_str() == cursor_position.file_path.as_os_str() {
                    // Ensure symbols are already defined at pos
                    let is_already_defined =
                        if runtime.range.start.line == cursor_position.position.line {
                            cursor_position.position.pos > runtime.range.start.pos
                        } else {
                            cursor_position.position.line > runtime.range.start.line
                        };
                    if is_already_defined {
                        // If we are in main file, check if scope in range.
                        for symbol_scope in &runtime.scope_stack {
                            if !symbol_scope.contain(&cursor_position.position) {
                                return false; // scope not in range
                            }
                        }
                        true // scope in range
                    } else {
                        false
                    }
                } else {
                    // If we are not in main file, only show whats in global scope.
                    // TODO: should handle include position in file aswell.
                    runtime.scope_stack.is_empty() // Global scope or inaccessible
                }
            }
            None => true, // intrinsics
        }
    }
    pub fn find_symbols_at(
        &'a self,
        label: &str,
        position: &ShaderFilePosition,
    ) -> Vec<&'a ShaderSymbol> {
        self.iter()
            .filter(|s| {
                !s.is_transient() && s.label == *label && Self::is_symbol_defined_at(s, position)
            })
            .collect()
    }
    pub fn filter_scoped_symbol(
        &'a self,
        cursor_position: &ShaderFilePosition,
    ) -> ShaderSymbolListRef<'a> {
        self.filter(|symbol_type, symbol| {
            !symbol_type.is_transient() && Self::is_symbol_defined_at(symbol, cursor_position)
        })
    }
    pub fn find_symbols(&'a self, label: &str) -> Vec<&'a ShaderSymbol> {
        self.iter()
            .filter(|s| s.label == *label && !s.is_transient())
            .collect::<Vec<&ShaderSymbol>>()
    }
    pub fn find_symbol(&'a self, label: &str) -> Option<&'a ShaderSymbol> {
        match self.iter().find(|e| e.label == *label) {
            Some(symbol) => return Some(symbol),
            None => None,
        }
    }
    pub fn find_function_symbol(&'a self, label: &str) -> Option<&'a ShaderSymbol> {
        self.functions
            .iter()
            .find(|s| s.label == *label)
            .map(|s| *s)
    }
    pub fn find_type_symbol(&'a self, label: &str) -> Option<&'a ShaderSymbol> {
        self.types.iter().find(|s| s.label == *label).map(|s| *s)
    }
    pub fn filter<P: Fn(ShaderSymbolType, &ShaderSymbol) -> bool>(
        &'a self,
        predicate: P,
    ) -> ShaderSymbolListRef<'a> {
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
    pub fn iter(&'a self) -> ShaderSymbolListIterator<'a> {
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
            ShaderSymbolData::Parameter {
                context: _,
                ty: _,
                count: _,
            } => Some(ShaderSymbolType::Variables),
            ShaderSymbolData::Method {
                context: _,
                signatures: _,
            } => Some(ShaderSymbolType::Functions),
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
            ShaderSymbolData::Parameter { context, ty, count } => match count {
                Some(count) => format!("{} {}::{}[{}]", ty, context, self.label, count),
                None => format!("{} {}::{}", ty, context, self.label),
            },
            ShaderSymbolData::Method {
                context,
                signatures,
            } => signatures[0].format_with_context(&self.label, context), // TODO: append +1 symbol
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
                if target.position.line == target.position.pos && target.position.line == 0 {
                    format!("#include \"{}\"", self.label) // No need to display it as we are at start of file.
                } else {
                    format!(
                        "#include \"{}\" at {}:{}",
                        self.label, target.position.line, target.position.pos
                    )
                }
            }
            ShaderSymbolData::Macro { value } => {
                format!("#define {} {}", self.label, value)
            }
        }
    }
}
