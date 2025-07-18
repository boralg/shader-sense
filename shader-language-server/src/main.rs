use log::info;
use shader_language_server::server;

fn get_version() -> &'static str {
    static VERSION: &str = env!("CARGO_PKG_VERSION");
    return VERSION;
}

fn print_version() {
    println!("shader-language-server v{}", get_version());
}

fn run_server() {
    env_logger::init();
    info!(
        "shader-language-server v{} ({})",
        get_version(),
        std::env::consts::OS
    );
    if let Ok(current_exe) = std::env::current_exe() {
        info!("Server running from {}", current_exe.display());
    }
    server::run();
}

pub fn main() {
    let last = std::env::args().last();
    match last {
        Some(last) => match last.as_str() {
            "--version" => print_version(),
            "-v" => print_version(),
            _ => run_server(),
        },
        None => run_server(),
    }
}
