//! Server following the [LSP protocol](https://microsoft.github.io/language-server-protocol/) to validate and inspect shaders using [`shader-sense`].
//!
//! It can be launched using the following options:
//! ```bash
//!     --config        Pass a custom config as a JSON string for server.
//!     --config-file   Pass a custom config as a file for server.
//!     --stdio         Use the stdio transport. Default transport.
//!     --tcp           Use tcp transport. Not implemented yet.
//!     --memory        Use memory transport. Not implemented yet.
//!     --version | -v  Print server version.
//!     --help | -h     Print this helper.
//! ```

// For test.
pub mod server;

#[cfg(test)]
mod tests {
    use crate::server::server_config::ServerConfig;

    #[test]
    fn test_empty_config() {
        let cfg: ServerConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.get_validate() == ServerConfig::DEFAULT_VALIDATE);
        let cfg_inverse: ServerConfig = serde_json::from_str(
            format!(
                "{{\"validate\": {}}}",
                if ServerConfig::DEFAULT_VALIDATE {
                    "false"
                } else {
                    "true"
                }
            )
            .as_str(),
        )
        .unwrap();
        assert!(cfg_inverse.get_validate() == !ServerConfig::DEFAULT_VALIDATE);
    }

    #[test]
    fn test_default_config() {
        let cfg = ServerConfig::default();
        assert!(cfg.get_symbols() == ServerConfig::DEFAULT_SYMBOLS);
        assert!(cfg.get_validate() == ServerConfig::DEFAULT_VALIDATE);
        assert!(cfg.get_symbol_diagnostics() == ServerConfig::DEFAULT_SYMBOL_DIAGNOSTIC);
        assert!(cfg.is_verbose() == ServerConfig::DEFAULT_TRACE.is_verbose());
        assert!(cfg.get_severity() == ServerConfig::DEFAULT_SEVERITY);
    }
}
