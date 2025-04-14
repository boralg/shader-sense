use core::panic;

mod test_server;

#[test]
// Run on PC only to test WASI through WASMTIME
#[cfg(not(target_os = "wasi"))]
fn test_server_wasi_runtime() {
    use test_server::TestServer;

    match TestServer::wasi() {
        Some(mut server) => {
            // Send an LSP initialize request
            server.initialize();
            server.exit();
        }
        None => {
            // Should ignore test instead to be clear.
            // https://github.com/rust-lang/rust/issues/68007
            println!("WASI executable not built. Skipping.");
        }
    };
}

#[test]
#[cfg(not(target_os = "wasi"))]
fn test_variant() {
    use std::{collections::HashMap, path::Path};

    use lsp_types::{
        notification::{DidCloseTextDocument, DidOpenTextDocument},
        request::DocumentSymbolRequest,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
        DocumentSymbolResponse, PartialResultParams, TextDocumentIdentifier, TextDocumentItem, Url,
        WorkDoneProgressParams,
    };
    use shader_language_server::server::shader_variant::{
        DidChangeShaderVariant, DidChangeShaderVariantParams, ShaderVariant,
    };
    use shader_sense::include::canonicalize;
    use test_server::TestServer;

    let mut server = TestServer::desktop().unwrap();

    // Test document
    let file_path = canonicalize(Path::new("../shader-sense/test/hlsl/variants.hlsl")).unwrap();
    println!("Opening file {}", file_path.display());
    let content = std::fs::read_to_string(&file_path).unwrap();
    let uri = Url::from_file_path(&file_path).unwrap();
    let item = TextDocumentItem {
        uri: uri.clone(),
        language_id: "hlsl".into(),
        version: 0,
        text: content,
    };
    let identifier = TextDocumentIdentifier { uri: uri.clone() };
    let document_symbol_params = DocumentSymbolParams {
        text_document: identifier.clone(),
        work_done_progress_params: WorkDoneProgressParams {
            work_done_token: None,
        },
        partial_result_params: PartialResultParams {
            partial_result_token: None,
        },
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

    // Send an LSP initialize request
    server.initialize();
    server.send_notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
        text_document: item.clone(),
    });
    server.send_request::<DocumentSymbolRequest>(&document_symbol_params, |response| {
        assert!(
            has_symbol(response, "mainError"),
            "Missing symbol mainError for variant"
        );
    });
    server.send_notification::<DidChangeShaderVariant>(&DidChangeShaderVariantParams {
        text_document: identifier.clone(),
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
        text_document: identifier.clone(),
    });
    server.exit();
}
