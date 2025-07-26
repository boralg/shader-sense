use std::io::Write;
use std::{
    cell::RefCell,
    process::{Command, Stdio},
};

use log::info;
use lsp_types::{Position, Range, TextEdit, Url};
use shader_sense::{shader::ShadingLanguage, shader_error::ShaderError};

use crate::server::ServerLanguage;

impl ServerLanguage {
    fn clang_format_path() -> &'static str {
        "clang-format"
    }
    pub fn is_clang_format_available() -> bool {
        if cfg!(target_os = "wasi") {
            false // Cannot spawn an exe in wasi.
        } else {
            match Command::new(Self::clang_format_path())
                .arg("--version")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
            {
                Ok(child) => match child.wait_with_output() {
                    Ok(output) => {
                        info!(
                            "Using {} for formatting",
                            String::from_utf8(output.stdout)
                                .unwrap_or("clang-format version unknown".into())
                                .trim()
                        );
                        true
                    }
                    Err(_) => {
                        info!("clang-format failed to get version.");
                        false
                    }
                },
                Err(e) => {
                    if let std::io::ErrorKind::NotFound = e.kind() {
                        info!("clang-format not found, disabled formatting.");
                        false
                    } else {
                        info!("clang-format failed, disabled formatting.");
                        false
                    }
                }
            }
        }
    }
    pub fn recolt_formatting(&self, uri: &Url) -> Result<Vec<TextEdit>, ShaderError> {
        let cached_file = self.watched_files.get_file(uri).unwrap();
        match &cached_file.shading_language {
            ShadingLanguage::Wgsl => {
                // TODO: Find a formatter for wgsl.
                Ok(vec![])
            }
            // HLSL & GLSL can rely on clang-format.
            ShadingLanguage::Hlsl | ShadingLanguage::Glsl => {
                let mut child = Command::new("clang-format")
                    //.arg(format!("--style={}", style.as_str()))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?;
                // Note we place inside a scope to ensure that stdin is closed
                {
                    let mut stdin = child.stdin.take().expect("no stdin handle");
                    write!(
                        stdin,
                        "{}",
                        RefCell::borrow(&cached_file.shader_module).content
                    )?;
                }
                // Wait for the output and mark it as big edit chunk.
                let output = child.wait_with_output()?;
                if output.status.success() {
                    let shader_module = RefCell::borrow(&cached_file.shader_module);
                    let original_code = &shader_module.content;
                    let formatted_code = String::from_utf8(output.stdout)
                        .map_err(|e| ShaderError::InternalErr(e.utf8_error().to_string()))?;
                    Ok(vec![TextEdit {
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: (original_code.lines().count()) as u32, // Last line
                                character: match original_code.lines().last() {
                                    Some(last_line) => (last_line.char_indices().count()) as u32,
                                    None => (original_code.char_indices().count()) as u32, // No last line, means no line, pick string length
                                },
                            },
                        },
                        new_text: formatted_code,
                    }])
                } else {
                    Err(ShaderError::InternalErr(
                        "Failed to parse file with clang-format".into(),
                    ))
                }
            }
        }
    }
}
