use log::{error, info, warn};
use shader_language_server::server::{self, server_config::ServerConfig, Transport};

fn get_version() -> &'static str {
    static VERSION: &str = env!("CARGO_PKG_VERSION");
    return VERSION;
}

fn print_version() {
    println!("shader-language-server v{}", get_version());
}

fn run_server(config: ServerConfig, transport: Transport) {
    info!(
        "shader-language-server v{} ({})",
        get_version(),
        std::env::consts::OS
    );
    if let Ok(current_exe) = std::env::current_exe() {
        info!("Server running from {}", current_exe.display());
    }
    server::run(config, transport);
}

fn usage() {
    print_version();
    println!("Overview: This is a shader language server following lsp protocol.");
    println!("Usage: shader-language-server [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --config                  Pass a custom config as a JSON string for server.");
    println!("  --config-file             Pass a custom config as a file for server.");
    println!("  --stdio                   Use the stdio transport. Default transport.");
    println!("  --tcp                     Use tcp transport. Not implemented yet.");
    println!("  --memory                  Use memory transport. Not implemented yet.");
    println!("  --version | -v            Print server version.");
    println!("  --help | -h               Print this helper.");
}

pub fn main() {
    env_logger::init();
    let mut args = std::env::args().into_iter();
    let _exe = args.next().unwrap();
    let mut transport = Transport::default();
    let mut config = ServerConfig::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return print_version(),
            "-v" => return print_version(),
            "--help" => return usage(),
            "-h" => return usage(),
            "--config" => {
                if let Some(config_str) = args.next() {
                    match serde_json::from_str::<ServerConfig>(&config_str) {
                        Ok(config_parsed) => config = config_parsed,
                        Err(err) => {
                            error!("Failed to parse config {}: {}", config_str, err);
                            return usage();
                        }
                    }
                } else {
                    return usage();
                }
            }
            "--config-file" => {
                if let Some(config_file) = args.next() {
                    match std::fs::read_to_string(&config_file) {
                        Ok(config_str) => match serde_json::from_str::<ServerConfig>(&config_str) {
                            Ok(config_parsed) => config = config_parsed,
                            Err(err) => {
                                error!("Failed to parse config file {}: {}", config_str, err);
                                return usage();
                            }
                        },
                        Err(err) => {
                            error!("Failed to open config file {}: {}", config_file, err);
                            return usage();
                        }
                    }
                } else {
                    return usage();
                }
            }
            "--stdio" => transport = Transport::Stdio,
            "--tcp" => transport = Transport::Tcp,
            "--memory" => transport = Transport::Memory,
            arg => {
                warn!("Argument {} unknown", arg);
            }
        }
    }
    run_server(config, transport);
}
