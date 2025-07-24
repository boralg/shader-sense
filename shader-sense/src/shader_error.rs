use core::fmt;

use crate::symbols::symbols::ShaderRange;

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl From<&str> for ShaderDiagnosticSeverity {
    fn from(value: &str) -> Self {
        match value {
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
    pub severity: ShaderDiagnosticSeverity,
    pub error: String,
    pub range: ShaderRange,
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

impl ShaderError {
    pub fn into_diagnostic(&self, severity: ShaderDiagnosticSeverity) -> Option<ShaderDiagnostic> {
        match self {
            ShaderError::SymbolQueryError(message, range) => Some(ShaderDiagnostic {
                error: format!(
                    "Symbol Query {}, symbol provider may be impacted: {}",
                    severity.to_string(),
                    message
                ),
                severity: severity,
                range: range.clone(),
            }),
            _ => None,
        }
    }
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
    pub fn join(mut lhs: ShaderDiagnosticList, rhs: ShaderDiagnosticList) -> Self {
        lhs.diagnostics.extend(rhs.diagnostics);
        lhs
    }
}
