//! Server following the [LSP protocol](https://microsoft.github.io/language-server-protocol/) to validate and inspect shaders using [`shader-sense`].
//!
//! It can be launched using the following options:
//! ```bash
//!     --config        Pass a custom config as a JSON string for server.
//!     --config-file   Pass a custom config as a file for server.
//!     --stdio         Use the stdio transport. Default transport.
//!     --tcp           Use tcp transport. Not implemented yet.
//!     --memory        Use memory transport. Not implemented yet.
//!     --cwd           Set current working directory of server. If not set, will be the server executable path.
//!     --version | -v  Print server version.
//!     --help | -h     Print this helper.
//! ```

// For test.
pub mod server;
