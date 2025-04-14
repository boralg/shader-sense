# Shader language server

[![shader_language_server](https://img.shields.io/crates/v/shader_language_server)](https://crates.io/crates/shader_language_server)

This application is a language server for shaders (HLSL, GLSL, WGSL) that is mainly meant to be used as a server for vscode extension [shader-validator](https://github.com/antaalt/shader-validator). It is following the [LSP protocol](https://microsoft.github.io/language-server-protocol/) to communicate with the extension so it could be used with any editor supporting it. It can be built to desktop or [WASI](https://wasi.dev/). WASI will let the server run even in the browser, but it suffer from limitations. See below for more informations.

## Features

This language server support a few options :

- **Diagnostics**: lint the code as you type.
- **Completion**: suggest completion values as you type.
- **Signature**: view the signatures of the current function.
- **Hover**: view the declaration of an element by hovering it.
- **Goto**: allow to go to declaration of an element.
- **Document symbol**: Request symbols for document.
- **Workspace symbol**: Request symbols for workspace.
- **Inactive regions**: Detect inactive preprocessor regions and disable them.

The server support HLSL, GLSL, WGSL diagnostics, but symbol requests are not implemented for WGSL yet.

### Diagnostics

Diagnostics are generated following language specifics API:

- **GLSL** uses [glslang-rs](https://github.com/SnowflakePowered/glslang-rs) as backend. It provide complete linting for GLSL trough glslang API bindings from C.
- **HLSL** uses [hassle-rs](https://github.com/Traverse-Research/hassle-rs) as backend. It provides bindings to directx shader compiler in rust.
- **WGSL** uses [naga](https://github.com/gfx-rs/naga) as backend for linting.

### Symbols

Symbols are retrieved using queries based on [tree-sitter](https://tree-sitter.github.io/tree-sitter/) API.

## Web support

This server can be run in the browser when compiled to WASI. Because of this restriction, we can't use dxc here as it does not compile to WASI and instead rely on glslang, which is more limited in linting (Only support some basic features of SM 6.0, while DXC support all newly added SM (current 6.8)).
