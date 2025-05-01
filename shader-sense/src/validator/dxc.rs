use hassle_rs::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    include::IncludeHandler,
    shader::{HlslShaderModel, HlslVersion, ShaderStage},
    shader_error::{ShaderDiagnostic, ShaderDiagnosticList, ShaderDiagnosticSeverity, ShaderError},
    symbols::symbols::{ShaderPosition, ShaderRange},
};

use super::validator::{ValidationParams, Validator};

pub struct Dxc {
    compiler: hassle_rs::DxcCompiler,
    library: hassle_rs::DxcLibrary,

    validator: Option<hassle_rs::DxcValidator>,
    dxil: Option<hassle_rs::wrapper::Dxil>,

    #[allow(dead_code)] // Need to keep dxc alive while dependencies created
    dxc: hassle_rs::wrapper::Dxc,

    // Cache regex for parsing.
    diagnostic_regex: regex::Regex,
    internal_diagnostic_regex: regex::Regex,
}

struct DxcIncludeHandler<'a> {
    include_handler: IncludeHandler,
    include_callback: &'a mut dyn FnMut(&Path) -> Option<String>,
}

impl<'a> DxcIncludeHandler<'a> {
    pub fn new(
        file: &Path,
        includes: Vec<String>,
        path_remapping: HashMap<PathBuf, PathBuf>,
        include_callback: &'a mut dyn FnMut(&Path) -> Option<String>,
    ) -> Self {
        Self {
            include_handler: IncludeHandler::main(file, includes, path_remapping),
            include_callback: include_callback,
        }
    }
}

impl hassle_rs::wrapper::DxcIncludeHandler for DxcIncludeHandler<'_> {
    fn load_source(&mut self, filename: String) -> Option<String> {
        // DXC include handler kinda bad.
        // First path are already preprocessed by dxc before calling this
        // (adding ./ in front of relative path & convert slash to backslash)
        // Tricky to solve virtual path. Done in include handler.
        // Second, we dont have any knowledge about the parent includer here.
        // And its not something they are going to solve:
        // https://github.com/microsoft/DirectXShaderCompiler/issues/6093
        // So includes can behave weirdly with dxc if too many subfolders.
        let path = Path::new(filename.as_str());
        self.include_handler
            .search_in_includes(&path, self.include_callback)
            .map(|e| e.0)
    }
}

impl Dxc {
    pub fn new() -> Result<Self, hassle_rs::HassleError> {
        let dxc = hassle_rs::Dxc::new(None)?;
        let library = dxc.create_library()?;
        let compiler = dxc.create_compiler()?;
        let (dxil, validator) = match Dxil::new(None) {
            Ok(dxil) => {
                let validator_option = match dxil.create_validator() {
                    Ok(validator) => Some(validator),
                    Err(_) => None,
                };
                (Some(dxil), validator_option)
            }
            Err(_) => (None, None),
        };
        Ok(Self {
            dxc,
            compiler,
            library,
            dxil,
            validator,
            diagnostic_regex: regex::Regex::new(r"(?m)^(.*?:\d+:\d+: .*:.*?)$").unwrap(),
            internal_diagnostic_regex: regex::Regex::new(r"(?s)^(.*?):(\d+):(\d+): (.*?):(.*)")
                .unwrap(),
        })
    }
    fn parse_dxc_errors(
        &self,
        errors: &String,
        file_path: &Path,
        params: &ValidationParams,
    ) -> Result<ShaderDiagnosticList, ShaderError> {
        let mut shader_error_list = ShaderDiagnosticList::empty();
        let mut starts = Vec::new();
        for capture in self.diagnostic_regex.captures_iter(errors.as_str()) {
            if let Some(pos) = capture.get(0) {
                starts.push(pos.start());
            }
        }
        starts.push(errors.len()); // Push the end.
        let mut include_handler = IncludeHandler::main(
            file_path,
            params.includes.clone(),
            params.path_remapping.clone(),
        );
        // Cache includes as its a heavy operation.
        let mut include_cache: HashMap<String, PathBuf> = HashMap::new();
        for start in 0..starts.len() - 1 {
            let begin = starts[start];
            let end = starts[start + 1];
            let block = &errors[begin..end];
            if let Some(capture) = self.internal_diagnostic_regex.captures(block) {
                let relative_path = capture.get(1).map_or("", |m| m.as_str());
                let line = capture.get(2).map_or("", |m| m.as_str());
                let pos = capture.get(3).map_or("", |m| m.as_str());
                let level = capture.get(4).map_or("", |m| m.as_str());
                let msg = capture.get(5).map_or("", |m| m.as_str());
                let file_path = include_cache
                    .entry(relative_path.into())
                    .or_insert_with(|| {
                        include_handler
                            .search_path_in_includes(Path::new(&relative_path))
                            .unwrap_or(file_path.into())
                    });
                shader_error_list.push(ShaderDiagnostic {
                    severity: match level {
                        "error" => ShaderDiagnosticSeverity::Error,
                        "warning" => ShaderDiagnosticSeverity::Warning,
                        "note" => ShaderDiagnosticSeverity::Information,
                        "hint" => ShaderDiagnosticSeverity::Hint,
                        _ => ShaderDiagnosticSeverity::Error,
                    },
                    error: String::from(msg),
                    range: ShaderRange::new(
                        ShaderPosition::new(
                            file_path.clone(),
                            line.parse::<u32>().unwrap_or(1) - 1,
                            pos.parse::<u32>().unwrap_or(0),
                        ),
                        ShaderPosition::new(
                            file_path.clone(),
                            line.parse::<u32>().unwrap_or(1) - 1,
                            pos.parse::<u32>().unwrap_or(0),
                        ),
                    ),
                });
            }
        }

        if shader_error_list.is_empty() {
            Err(ShaderError::InternalErr(format!(
                "Failed to parse errors: {}",
                errors
            )))
        } else {
            Ok(shader_error_list)
        }
    }
    fn from_hassle_error(
        &self,
        error: HassleError,
        file_path: &Path,
        params: &ValidationParams,
    ) -> Result<ShaderDiagnosticList, ShaderError> {
        match error {
            HassleError::CompileError(err) => self.parse_dxc_errors(&err, file_path, &params),
            HassleError::ValidationError(err) => Ok(ShaderDiagnosticList::from(ShaderDiagnostic {
                severity: ShaderDiagnosticSeverity::Error,
                error: err.to_string(),
                range: ShaderRange::new(
                    ShaderPosition::new(file_path.into(), 0, 0),
                    ShaderPosition::new(file_path.into(), 0, 0),
                ),
            })),
            HassleError::LibLoadingError(err) => Err(ShaderError::InternalErr(err.to_string())),
            HassleError::LoadLibraryError { filename, inner } => {
                Err(ShaderError::InternalErr(format!(
                    "Failed to load library {}: {}",
                    filename.display(),
                    inner.to_string()
                )))
            }
            HassleError::Win32Error(err) => Err(ShaderError::InternalErr(format!(
                "Win32 error: HRESULT={}",
                err
            ))),
            HassleError::WindowsOnly(err) => Err(ShaderError::InternalErr(format!(
                "Windows only error: {}",
                err
            ))),
        }
    }
}

fn get_profile(shader_stage: Option<ShaderStage>) -> &'static str {
    // https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-models
    match shader_stage {
        Some(shader_stage) => match shader_stage {
            ShaderStage::Vertex => "vs",
            ShaderStage::Fragment => "ps",
            ShaderStage::Compute => "cs",
            ShaderStage::TesselationControl => "ds",
            ShaderStage::TesselationEvaluation => "hs",
            ShaderStage::Geometry => "gs",
            // Mesh shader not in spec. but seems to be it
            ShaderStage::Mesh => "ms", // Check these
            ShaderStage::Task => "as", // Check these
            // All RT seems to use lib profile.
            ShaderStage::RayGeneration
            | ShaderStage::ClosestHit
            | ShaderStage::AnyHit
            | ShaderStage::Callable
            | ShaderStage::Miss
            | ShaderStage::Intersect => "lib",
        },
        // Use lib profile if no stage.
        None => "lib",
    }
}

impl Validator for Dxc {
    fn validate_shader(
        &mut self,
        shader_source: &String,
        file_path: &Path,
        params: &ValidationParams,
        include_callback: &mut dyn FnMut(&Path) -> Option<String>,
    ) -> Result<ShaderDiagnosticList, ShaderError> {
        let file_name = self.get_file_name(file_path);

        let blob = match self
            .library
            .create_blob_with_encoding_from_str(&shader_source)
        {
            Ok(blob) => blob,
            Err(err) => match self.from_hassle_error(err, file_path, &params) {
                Ok(diagnostics) => return Ok(diagnostics),
                Err(error) => return Err(error),
            },
        };

        let defines_copy = params.defines.clone();
        let defines: Vec<(&str, Option<&str>)> = defines_copy
            .iter()
            .map(|v| (&v.0 as &str, Some(&v.1 as &str)))
            .collect();
        let mut include_handler = DxcIncludeHandler::new(
            file_path,
            params.includes.clone(),
            params.path_remapping.clone(),
            include_callback,
        );
        let dxc_options = {
            let hlsl_version = format!(
                "-HV {}",
                match params.hlsl_version {
                    HlslVersion::V2016 => "2016",
                    HlslVersion::V2017 => "2017",
                    HlslVersion::V2018 => "2018",
                    HlslVersion::V2021 => "2021",
                }
            );

            if params.hlsl_enable16bit_types {
                vec![hlsl_version, "-enable-16bit-types".into()]
            } else {
                vec![hlsl_version]
            }
        };
        let dxc_options_str: Vec<&str> = dxc_options.iter().map(|s| s.as_str()).collect();
        let result = self.compiler.compile(
            &blob,
            file_name.as_str(),
            match &params.entry_point {
                Some(entry_point) => entry_point.as_str(),
                None => "",
            },
            format!(
                "{}_{}",
                get_profile(params.shader_stage),
                match params.hlsl_shader_model {
                    HlslShaderModel::ShaderModel6 => "6_0",
                    HlslShaderModel::ShaderModel6_1 => "6_1",
                    HlslShaderModel::ShaderModel6_2 => "6_2",
                    HlslShaderModel::ShaderModel6_3 => "6_3",
                    HlslShaderModel::ShaderModel6_4 => "6_4",
                    HlslShaderModel::ShaderModel6_5 => "6_5",
                    HlslShaderModel::ShaderModel6_6 => "6_6",
                    HlslShaderModel::ShaderModel6_7 => "6_7",
                    HlslShaderModel::ShaderModel6_8 => "6_8",
                }
            )
            .as_str(),
            &dxc_options_str,
            Some(&mut include_handler),
            &defines,
        );

        match result {
            Ok(dxc_result) => {
                let result_blob = match dxc_result.get_result() {
                    Ok(blob) => blob,
                    Err(err) => match self.from_hassle_error(err, file_path, &params) {
                        Ok(diagnostics) => return Ok(diagnostics),
                        Err(error) => return Err(error),
                    },
                };
                // Skip validation if dxil.dll does not exist.
                if let (Some(_dxil), Some(validator)) = (&self.dxil, &self.validator) {
                    let data = result_blob.to_vec();
                    let blob_encoding = match self.library.create_blob_with_encoding(data.as_ref())
                    {
                        Ok(blob) => blob,
                        Err(err) => match self.from_hassle_error(err, file_path, &params) {
                            Ok(diagnostics) => return Ok(diagnostics),
                            Err(error) => return Err(error),
                        },
                    };

                    match validator.validate(blob_encoding.into()) {
                        Ok(_) => Ok(ShaderDiagnosticList::empty()),
                        Err(dxc_err) => {
                            //let error_blob = dxc_err.0.get_error_buffer().map_err(|e| self.from_hassle_error(e))?;
                            //let error_emitted = self.library.get_blob_as_string(&error_blob.into()).map_err(|e| self.from_hassle_error(e))?;
                            match self.from_hassle_error(dxc_err.1, file_path, &params) {
                                Ok(diag) => Ok(diag),
                                Err(err) => Err(err),
                            }
                        }
                    }
                } else {
                    Ok(ShaderDiagnosticList::empty())
                }
            }
            Err((dxc_result, _hresult)) => {
                let error_blob = match dxc_result.get_error_buffer() {
                    Ok(blob) => blob,
                    Err(err) => match self.from_hassle_error(err, file_path, &params) {
                        Ok(diagnostics) => return Ok(diagnostics),
                        Err(error) => return Err(error),
                    },
                };
                let error_emitted = match self.library.get_blob_as_string(&error_blob.into()) {
                    Ok(blob) => blob,
                    Err(err) => match self.from_hassle_error(err, file_path, &params) {
                        Ok(diagnostics) => return Ok(diagnostics),
                        Err(error) => return Err(error),
                    },
                };
                match self.from_hassle_error(
                    HassleError::CompileError(error_emitted),
                    file_path,
                    &params,
                ) {
                    Err(error) => Err(error),
                    Ok(diag) => Ok(diag),
                }
            }
        }
    }
}
