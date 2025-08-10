use std::{collections::HashMap, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ShadingLanguage {
    Wgsl,
    Hlsl,
    Glsl,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment, // aka pixel shader
    Compute,
    TesselationControl,    // aka hull shader
    TesselationEvaluation, // aka domain shader
    Mesh,
    Task, // aka amplification shader
    Geometry,
    RayGeneration,
    ClosestHit,
    AnyHit,
    Callable,
    Miss,
    Intersect,
}

impl ShaderStage {
    pub fn from_file_name(file_name: &String) -> Option<ShaderStage> {
        // TODO: add control for these
        let paths = HashMap::from([
            ("vert", ShaderStage::Vertex),
            ("frag", ShaderStage::Fragment),
            ("comp", ShaderStage::Compute),
            ("task", ShaderStage::Task),
            ("mesh", ShaderStage::Mesh),
            ("tesc", ShaderStage::TesselationControl),
            ("tese", ShaderStage::TesselationEvaluation),
            ("geom", ShaderStage::Geometry),
            ("rgen", ShaderStage::RayGeneration),
            ("rchit", ShaderStage::ClosestHit),
            ("rahit", ShaderStage::AnyHit),
            ("rcall", ShaderStage::Callable),
            ("rmiss", ShaderStage::Miss),
            ("rint", ShaderStage::Intersect),
        ]);
        let extension_list = file_name.rsplit(".");
        for extension in extension_list {
            if let Some(stage) = paths.get(extension) {
                return Some(stage.clone());
            } else {
                continue;
            }
        }
        // For header files & undefined, will output issue with missing version...
        None
    }
}

impl ToString for ShaderStage {
    fn to_string(&self) -> String {
        match self {
            ShaderStage::Vertex => "vertex".to_string(),
            ShaderStage::Fragment => "fragment".to_string(),
            ShaderStage::Compute => "compute".to_string(),
            ShaderStage::TesselationControl => "tesselationcontrol".to_string(),
            ShaderStage::TesselationEvaluation => "tesselationevaluation".to_string(),
            ShaderStage::Mesh => "mesh".to_string(),
            ShaderStage::Task => "task".to_string(),
            ShaderStage::Geometry => "geometry".to_string(),
            ShaderStage::RayGeneration => "raygeneration".to_string(),
            ShaderStage::ClosestHit => "closesthit".to_string(),
            ShaderStage::AnyHit => "anyhit".to_string(),
            ShaderStage::Callable => "callable".to_string(),
            ShaderStage::Miss => "miss".to_string(),
            ShaderStage::Intersect => "intersect".to_string(),
        }
    }
}

impl FromStr for ShadingLanguage {
    type Err = ();

    fn from_str(input: &str) -> Result<ShadingLanguage, Self::Err> {
        match input {
            "wgsl" => Ok(ShadingLanguage::Wgsl),
            "hlsl" => Ok(ShadingLanguage::Hlsl),
            "glsl" => Ok(ShadingLanguage::Glsl),
            _ => Err(()),
        }
    }
}
impl ToString for ShadingLanguage {
    fn to_string(&self) -> String {
        String::from(match &self {
            ShadingLanguage::Wgsl => "wgsl",
            ShadingLanguage::Hlsl => "hlsl",
            ShadingLanguage::Glsl => "glsl",
        })
    }
}

pub trait ShadingLanguageTag {
    fn get_language() -> ShadingLanguage;
}
pub struct HlslShadingLanguageTag {}
impl ShadingLanguageTag for HlslShadingLanguageTag {
    fn get_language() -> ShadingLanguage {
        ShadingLanguage::Hlsl
    }
}
pub struct GlslShadingLanguageTag {}
impl ShadingLanguageTag for GlslShadingLanguageTag {
    fn get_language() -> ShadingLanguage {
        ShadingLanguage::Glsl
    }
}
pub struct WgslShadingLanguageTag {}
impl ShadingLanguageTag for WgslShadingLanguageTag {
    fn get_language() -> ShadingLanguage {
        ShadingLanguage::Wgsl
    }
}

// DXC only support shader model up to 6.0
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum HlslShaderModel {
    ShaderModel1,
    ShaderModel1_1,
    ShaderModel1_2,
    ShaderModel1_3,
    ShaderModel1_4,
    ShaderModel2,
    ShaderModel3,
    ShaderModel4,
    ShaderModel4_1,
    ShaderModel5,
    ShaderModel5_1,
    ShaderModel6,
    ShaderModel6_1,
    ShaderModel6_2,
    ShaderModel6_3,
    ShaderModel6_4,
    ShaderModel6_5,
    ShaderModel6_6,
    ShaderModel6_7,
    #[default]
    ShaderModel6_8,
}

impl HlslShaderModel {
    pub fn earliest() -> HlslShaderModel {
        HlslShaderModel::ShaderModel1
    }
    pub fn latest() -> HlslShaderModel {
        HlslShaderModel::ShaderModel6_8
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum HlslVersion {
    V2016,
    V2017,
    V2018,
    #[default]
    V2021,
}

#[derive(Default, Debug, Clone)]
pub struct HlslCompilationParams {
    pub shader_model: HlslShaderModel,
    pub version: HlslVersion,
    pub enable16bit_types: bool,
    pub spirv: bool,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GlslTargetClient {
    Vulkan1_0,
    Vulkan1_1,
    Vulkan1_2,
    #[default]
    Vulkan1_3,
    OpenGL450,
}

impl GlslTargetClient {
    pub fn is_opengl(&self) -> bool {
        match *self {
            GlslTargetClient::OpenGL450 => true,
            _ => false,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GlslSpirvVersion {
    SPIRV1_0,
    SPIRV1_1,
    SPIRV1_2,
    SPIRV1_3,
    SPIRV1_4,
    SPIRV1_5,
    #[default]
    SPIRV1_6,
}
#[derive(Default, Debug, Clone)]
pub struct GlslCompilationParams {
    pub client: GlslTargetClient,
    pub spirv: GlslSpirvVersion,
}

#[derive(Default, Debug, Clone)]
pub struct WgslCompilationParams {}

#[derive(Default, Debug, Clone)]
pub struct ShaderContextParams {
    pub defines: HashMap<String, String>,
    pub includes: Vec<String>,
    pub path_remapping: HashMap<PathBuf, PathBuf>,
}

#[derive(Default, Debug, Clone)]
pub struct ShaderCompilationParams {
    pub entry_point: Option<String>,
    pub shader_stage: Option<ShaderStage>,
    pub hlsl: HlslCompilationParams,
    pub glsl: GlslCompilationParams,
    pub wgsl: WgslCompilationParams,
}

#[derive(Default, Debug, Clone)]
pub struct ShaderParams {
    pub context: ShaderContextParams,
    pub compilation: ShaderCompilationParams,
}
