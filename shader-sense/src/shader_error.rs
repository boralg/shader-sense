use core::fmt;
use std::path::PathBuf;

use crate::symbols::symbols::ShaderRange;

#[derive(Debug, Clone)]
pub enum ShaderDiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}
impl fmt::Display for ShaderDiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ShaderDiagnosticSeverity::Error => write!(f, "error"),
            ShaderDiagnosticSeverity::Warning => write!(f, "warning"),
            ShaderDiagnosticSeverity::Information => write!(f, "info"),
            ShaderDiagnosticSeverity::Hint => write!(f, "hint"),
        }
    }
}

impl From<String> for ShaderDiagnosticSeverity {
    fn from(value: String) -> Self {
        match value.as_str() {
            "error" => ShaderDiagnosticSeverity::Error,
            "warning" => ShaderDiagnosticSeverity::Warning,
            "info" => ShaderDiagnosticSeverity::Information,
            "hint" => ShaderDiagnosticSeverity::Hint,
            _ => ShaderDiagnosticSeverity::Error,
        }
    }
}

impl ShaderDiagnosticSeverity {
    pub fn is_required(&self, required_severity: ShaderDiagnosticSeverity) -> bool {
        self.get_enum_index() <= required_severity.get_enum_index()
    }
    fn get_enum_index(&self) -> u32 {
        match self {
            ShaderDiagnosticSeverity::Error => 0,
            ShaderDiagnosticSeverity::Warning => 1,
            ShaderDiagnosticSeverity::Information => 2,
            ShaderDiagnosticSeverity::Hint => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShaderDiagnostic {
    pub file_path: Option<PathBuf>,
    pub severity: ShaderDiagnosticSeverity,
    pub error: String,
    pub line: u32,
    pub pos: u32,
}
#[derive(Debug, Default, Clone)]
pub struct ShaderDiagnosticList {
    pub diagnostics: Vec<ShaderDiagnostic>,
}

#[derive(Debug)]
pub enum ShaderError {
    ValidationError(String),
    NoSymbol,
    ParseSymbolError(String),
    SymbolQueryError(String, ShaderRange),
    IoErr(std::io::Error),
    InternalErr(String),
}

impl From<regex::Error> for ShaderError {
    fn from(error: regex::Error) -> Self {
        match error {
            regex::Error::CompiledTooBig(err) => {
                ShaderError::InternalErr(format!("Regex compile too big: {}", err))
            }
            regex::Error::Syntax(err) => {
                ShaderError::InternalErr(format!("Regex syntax invalid: {}", err))
            }
            err => ShaderError::InternalErr(format!("Regex error: {:#?}", err)),
        }
    }
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ShaderError::IoErr(err) => write!(f, "IoError: {}", err),
            ShaderError::InternalErr(err) => write!(f, "Error: {}", err),
            ShaderError::NoSymbol => write!(f, "NoSymbol"),
            ShaderError::ParseSymbolError(err) => write!(f, "ParseSymbolError: {}", err),
            ShaderError::ValidationError(err) => write!(f, "ValidationError: {}", err),
            ShaderError::SymbolQueryError(err, range) => {
                write!(f, "SymbolQueryError: {} at {:?}", err, range)
            }
        }
    }
}

impl From<std::io::Error> for ShaderError {
    fn from(err: std::io::Error) -> Self {
        ShaderError::IoErr(err)
    }
}
impl From<ShaderDiagnostic> for ShaderDiagnosticList {
    fn from(err: ShaderDiagnostic) -> Self {
        Self {
            diagnostics: vec![err],
        }
    }
}
impl ShaderDiagnosticList {
    pub fn empty() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }
    pub fn push(&mut self, error: ShaderDiagnostic) {
        self.diagnostics.push(error);
    }
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}
