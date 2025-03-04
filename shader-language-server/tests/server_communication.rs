use std::{
    env,
    io::{BufReader, Read, Write},
    process::{ChildStdin, Command, Stdio},
};

use lsp_types::{
    notification::{Exit, Initialized},
    request::{Initialize, Shutdown},
    InitializeParams, InitializedParams,
};

fn send_request<T: lsp_types::request::Request, U: Write>(
    stdin: &mut U,
    id: i32,
    params: &T::Params,
) {
    let request = lsp_server::Message::Request(lsp_server::Request::new(
        lsp_server::RequestId::from(id),
        T::METHOD.into(),
        params,
    ));
    println!("Send request: {}", serde_json::to_string(&request).unwrap());
    lsp_server::Message::write(request, stdin).unwrap();
}

fn send_notification<T: lsp_types::notification::Notification, U: Write>(
    stdin: &mut U,
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

fn read_response<U: Read>(reader: &mut BufReader<U>) {
    let message = lsp_server::Message::read(reader).unwrap();
    println!("Received response: {:?}", message);
}

#[test]
// Run on PC only to test WASI through WASMTIME
#[cfg(not(target_os = "wasi"))]
fn test_server_wasi_runtime() {
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
        return;
    }
    assert!(test_folder.is_dir(), "Missing Test folder");
    let mut child = Command::new("wasmtime")
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

    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Send an LSP initialize request
    let params = InitializeParams::default();
    send_request::<Initialize, ChildStdin>(stdin, 1, &params);
    read_response(&mut reader);
    send_notification::<Initialized, ChildStdin>(stdin, &InitializedParams {});
    send_request::<Shutdown, ChildStdin>(stdin, 2, &());
    read_response(&mut reader);
    send_notification::<Exit, ChildStdin>(stdin, &());
    read_response(&mut reader);
}
