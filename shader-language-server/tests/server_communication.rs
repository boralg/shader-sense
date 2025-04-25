use core::panic;
use std::collections::HashMap;
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};

use lsp_types::{
    notification::{DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument},
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Position,
    Range, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use shader_sense::shader::ShadingLanguage;
use test_server::{TestFile, TestServer};

mod test_server;

#[test]
// Run on PC only to test WASI through WASMTIME
#[cfg(not(target_os = "wasi"))]
fn test_server_wasi_runtime() {
    use test_server::TestServer;

    match TestServer::wasi() {
        Some(_) => {}
        None => {
            // Should ignore test instead to be clear.
            // https://github.com/rust-lang/rust/issues/68007
            println!("WASI executable not built. Skipping.");
        }
    };
}

// Share a single server for all test.
fn desktop_server() -> &'static Mutex<TestServer> {
    static SERVER: OnceLock<Mutex<TestServer>> = OnceLock::new();
    SERVER.get_or_init(|| Mutex::new(TestServer::desktop().unwrap()))
}

#[test]
#[cfg(not(target_os = "wasi"))]
fn test_variant() {
    use lsp_types::{
        request::DocumentSymbolRequest, DocumentSymbolParams, DocumentSymbolResponse,
        PartialResultParams, WorkDoneProgressParams,
    };
    use shader_language_server::server::shader_variant::{
        DidChangeShaderVariant, DidChangeShaderVariantParams, ShaderVariant,
    };

    let server_locked = desktop_server();

    // Test document
    let file = TestFile::new(
        Path::new("../shader-sense/test/hlsl/variants.hlsl"),
        ShadingLanguage::Hlsl,
    );
    println!("Opening file {}", file.url);
    let document_symbol_params = DocumentSymbolParams {
        text_document: file.identifier(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    fn has_symbol(response: Option<DocumentSymbolResponse>, symbol: &str) -> bool {
        let symbols = response.unwrap();
        match symbols {
            DocumentSymbolResponse::Flat(symbol_informations) => symbol_informations
                .iter()
                .find(|e| e.name == symbol)
                .is_some(),
            _ => panic!("Should not be reached."),
        }
    }

    let mut server = server_locked.lock().unwrap();
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: file.item(),
    });
    server.send_request::<DocumentSymbolRequest>(&document_symbol_params, |response| {
        assert!(
            has_symbol(response, "mainError"),
            "Missing symbol mainError for variant"
        );
    });
    server.send_notification::<DidChangeShaderVariant>(&DidChangeShaderVariantParams {
        text_document: file.identifier(),
        shader_variant: Some(ShaderVariant {
            entry_point: "".into(),
            stage: None,
            defines: HashMap::from([("VARIANT_DEFINE".into(), "1".into())]),
            includes: Vec::new(),
        }),
    });
    server.send_request::<DocumentSymbolRequest>(&document_symbol_params, |response| {
        assert!(
            has_symbol(response, "mainOk"),
            "Missing symbol mainOk for variant"
        );
    });
    server.send_notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
        text_document: file.identifier(),
    });
}

#[test]
fn test_utf8_edit() {
    let server_locked = desktop_server();

    let file = TestFile::new(
        Path::new("../shader-sense/test/hlsl/utf8.hlsl"),
        ShadingLanguage::Hlsl,
    );

    let mut server = server_locked.lock().unwrap();
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: file.item(),
    });
    let utf8_content_inserted = "こんにちは世界!";
    server.send_notification::<DidChangeTextDocument>(&DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: file.url.clone(),
            version: 0,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 3,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            }),
            range_length: Some(0),
            text: utf8_content_inserted.into(),
        }],
    });
    server.send_notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
        text_document: file.identifier(),
    });
}

#[test]
fn test_dependencies() {
    let server_locked = desktop_server();

    let file = TestFile::new(
        Path::new("../shader-sense/test/glsl/include-level.comp.glsl"),
        ShadingLanguage::Glsl,
    );
    let deps0 = TestFile::new(
        Path::new("../shader-sense/test/glsl/inc0/level0.glsl"),
        ShadingLanguage::Glsl,
    );
    let deps1 = TestFile::new(
        Path::new("../shader-sense/test/glsl/inc0/inc1/level1.glsl"),
        ShadingLanguage::Glsl,
    );

    let mut server = server_locked.lock().unwrap();
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: file.item(),
    });
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: deps0.item(),
    });
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: deps1.item(),
    });
    server.send_notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
        text_document: deps1.identifier(),
    });
    server.send_notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
        text_document: file.identifier(),
    });
    server.send_notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
        text_document: deps0.identifier(),
    });
}
