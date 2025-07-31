// For test.
pub mod server;

#[cfg(test)]
mod tests {
    use crate::server::server_config::ServerConfig;

    #[test]
    fn test_empty_config() {
        let cfg: ServerConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.get_validate() == true);
        let cfg_true: ServerConfig = serde_json::from_str("{\"validate\": false}").unwrap();
        assert!(cfg_true.get_validate() == false);
    }
}
