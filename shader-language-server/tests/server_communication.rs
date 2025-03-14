use std::{
    env,
    io::BufReader,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use lsp_types::{
    notification::{Exit, Initialized},
    request::{Initialize, Shutdown},
    InitializeParams, InitializedParams,
};

fn send_request<T: lsp_types::request::Request>(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    id: i32,
    params: &T::Params,
    callback: fn(T::Result),
) {
    let request = lsp_server::Message::Request(lsp_server::Request::new(
        lsp_server::RequestId::from(id),
        T::METHOD.into(),
        params,
    ));
    println!("Send request: {}", serde_json::to_string(&request).unwrap());
    lsp_server::Message::write(request, stdin).unwrap();
    // Wait for response
    loop {
        let message = lsp_server::Message::read(reader).unwrap();
        println!("Received message: {:?}", message);
        match message.unwrap() {
            lsp_server::Message::Response(response) => {
                match response.result {
                    Some(result) => {
                        let response: T::Result = serde_json::from_value(result).unwrap();
                        callback(response);
                    }
                    None => {}
                }
                break;
            }
            lsp_server::Message::Notification(_) => {} // Ignore
            lsp_server::Message::Request(_) => {}      // Ignore
        }
    }
}

fn send_notification<T: lsp_types::notification::Notification>(
    stdin: &mut ChildStdin,
    params: &T::Params,
) {
    let notification =
        lsp_server::Message::Notification(lsp_server::Notification::new(T::METHOD.into(), params));
    println!(
        "Send notification: {}",
        serde_json::to_string(&notification).unwrap()
    );
    lsp_server::Message::write(notification, stdin).unwrap();
}

fn start_server_wasi() -> Option<Child> {
    use std::path::Path;

    use shader_sense::include::canonicalize;
    let server_path = canonicalize(Path::new(&format!(
        "../target/wasm32-wasip1-threads/debug/{}.{}",
        env!("CARGO_PKG_NAME").replace("_", "-"),
        "wasm"
    )))
    .unwrap();
    let test_folder = canonicalize(Path::new("../shader-sense/test")).unwrap();
    println!("Server path: {}", server_path.display());
    println!("Test folder: {}", test_folder.display());
    // If wasm is not built, simply skip the test.
    // On PC build workflow, no WASI available, too heavy to rebuild it, so skip instead.
    if !server_path.is_file() {
        println!("WASI server not built, skipping test.");
        return None;
    }
    assert!(test_folder.is_dir(), "Missing Test folder");
    let child = Command::new("wasmtime")
        .args([
            "--wasi",
            "threads=y",
            "--dir",
            format!("{}::/test", test_folder.display()).as_str(),
            format!("{}", server_path.display()).as_str(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("RUST_LOG", "shader_language_server=trace")
        .spawn()
        .unwrap();
    Some(child)
}

fn start_server_desktop() -> Option<Child> {
    use std::path::Path;

    use shader_sense::include::canonicalize;
    let server_path = canonicalize(Path::new(&format!(
        "../target/debug/{}{}",
        env!("CARGO_PKG_NAME").replace("_", "-"),
        std::env::consts::EXE_SUFFIX
    )))
    .unwrap();
    let test_folder = canonicalize(Path::new("../shader-sense/test")).unwrap();
    println!("Server path: {}", server_path.display());
    println!("Test folder: {}", test_folder.display());
    // If wasm is not built, simply skip the test.
    // On PC build workflow, no WASI available, too heavy to rebuild it, so skip instead.
    if !server_path.is_file() {
        println!("Desktop server not built, skipping test.");
        return None;
    }
    assert!(test_folder.is_dir(), "Missing Test folder");
    let child = Command::new(server_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("RUST_LOG", "shader_language_server=trace")
        .spawn()
        .unwrap();
    Some(child)
}

#[test]
// Run on PC only to test WASI through WASMTIME
#[cfg(not(target_os = "wasi"))]
fn test_server_wasi_runtime() {
    let mut child = match start_server_wasi() {
        Some(child) => child,
        None => {
            return;
        }
    };

    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Send an LSP initialize request
    let params = InitializeParams::default();
    send_request::<Initialize>(stdin, &mut reader, 1, &params, |_| {});
    send_notification::<Initialized>(stdin, &InitializedParams {});
    send_request::<Shutdown>(stdin, &mut reader, 2, &(), |_| {});
    send_notification::<Exit>(stdin, &());

    child.wait().unwrap();
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

    let mut child = start_server_desktop().unwrap();

    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

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
    fn has_symbol(response: Option<DocumentSymbolResponse>, symbol: &str) {
        let symbols = response.unwrap();
        match symbols {
            DocumentSymbolResponse::Flat(symbol_informations) => assert!(symbol_informations
                .iter()
                .find(|e| e.name == symbol)
                .is_some()),
            _ => panic!("Should not be reached."),
        }
    }

    // Send an LSP initialize request
    let params = InitializeParams::default();
    send_request::<Initialize>(stdin, &mut reader, 1, &params, |_| {});
    send_notification::<Initialized>(stdin, &InitializedParams {});
    send_notification::<DidOpenTextDocument>(
        stdin,
        &DidOpenTextDocumentParams {
            text_document: item.clone(),
        },
    );
    send_request::<DocumentSymbolRequest>(
        stdin,
        &mut reader,
        2,
        &document_symbol_params,
        |response| has_symbol(response, "mainError"),
    );
    send_notification::<DidChangeShaderVariant>(
        stdin,
        &DidChangeShaderVariantParams {
            text_document: identifier.clone(),
            shader_variant: Some(ShaderVariant {
                entry_point: "".into(),
                stage: None,
                defines: HashMap::from([("VARIANT_DEFINE".into(), "1".into())]),
                includes: Vec::new(),
            }),
        },
    );
    send_request::<DocumentSymbolRequest>(
        stdin,
        &mut reader,
        3,
        &document_symbol_params,
        |response| has_symbol(response, "mainOk"),
    );
    send_notification::<DidCloseTextDocument>(
        stdin,
        &DidCloseTextDocumentParams {
            text_document: identifier.clone(),
        },
    );
    send_request::<Shutdown>(stdin, &mut reader, 3, &(), |_| {});
    send_notification::<Exit>(stdin, &());
    // Server does not want to exit somehow...
    child.kill().unwrap();
}
