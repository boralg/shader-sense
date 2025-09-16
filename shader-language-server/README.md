# Shader language server

[![shader_language_server](https://img.shields.io/crates/v/shader_language_server)](https://crates.io/crates/shader_language_server)

This application is a language server for shaders (HLSL, GLSL, WGSL) that is mainly meant to be used as a server for vscode extension [shader-validator](https://github.com/antaalt/shader-validator). It is following the [LSP protocol](https://microsoft.github.io/language-server-protocol/) to communicate with the extension so it could be used with any editor supporting it. It can be built to desktop or [WASI](https://wasi.dev/). WASI will let the server run even in the browser, but it suffer from limitations. See below for more informations.

## How to use

It can be launched using the following options:
```sh
    --config        Pass a custom config as a JSON string for server.
    --config-file   Pass a custom config as a file for server.
    --stdio         Use the stdio transport. Default transport.
    --tcp           Use tcp transport. Not implemented yet.
    --memory        Use memory transport. Not implemented yet.
    --cwd           Set current working directory of server. If not set, will be the server executable path.
    --version | -v  Print server version.
    --help | -h     Print this helper.
```

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

## Specific features

The server follows the lsp protocol, but it also offer some custom commands specific to this server. Handling them is not mandatory but can improve the experience using the extension.

### Shader variant commands

This server offer a variant concept to handle shader database which can have a lot of entry points, even in a single shader file. 
In order to offer a better experience with all providers and active regions, you can specify the current variant, aka current entry point, along with some macro and includes for the permutation. 
Your client can have an interface letting user create variants and select the active one, which will be sent to server through the notification "textDocument/didChangeShaderVariant".

- Change shader variant notification: "textDocument/didChangeShaderVariant"
Set it to null to remove current variant.
```typescript
interface DidChangeShaderVariantParams {
    shaderVariant: ShaderVariant | null
}

interface ShaderVariant {
    url: string, // file of variant
    shadingLanguage: string, // language id of variant
    entryPoint: string, // The name of the entry point function.
    stage: string | null, // Correspond to the value of the enum ShaderStage in shader-sense, case sensitive. 
    defines: Object, // defines and its values
    includes: string[], // include folders for this variant
}
```

### Debug commands:

The server offer some specific debug request to help inspect the current state of the server.

- Dump AST request: "debug/dumpAst"
```typescript
interface DumpAstParams {
    uri: string,
}
```
Result will be either a string or null

- Dump dependencies request: "debug/dumpDependency"
```typescript
interface DumpDependencyParams {
    uri: string,
}
```
Result will be either a string or null

## Behind the hood

### Diagnostics

Diagnostics are generated following language specifics API:

- **GLSL** uses [glslang-rs](https://github.com/SnowflakePowered/glslang-rs) as backend. It provide complete linting for GLSL trough glslang API bindings from C.
- **HLSL** uses [hassle-rs](https://github.com/Traverse-Research/hassle-rs) as backend. It provides bindings to directx shader compiler in rust.
- **WGSL** uses [naga](https://github.com/gfx-rs/naga) as backend for linting.

### Symbols

Symbols are retrieved using queries based on [tree-sitter](https://tree-sitter.github.io/tree-sitter/) API.

## Web support

This server can be run in the browser when compiled to WASI. Because of this restriction, we can't use dxc here as it does not compile to WASI and instead rely on glslang, which is more limited in linting (Only support some basic features of SM 6.0, while DXC support all newly added SM (current 6.8)).
