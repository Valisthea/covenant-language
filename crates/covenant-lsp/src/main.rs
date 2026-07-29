//! `covenant-lsp`: Language Server Protocol server for the Covenant language.
//!
//! Communicates over stdin/stdout using the LSP JSON-RPC framing expected by
//! VS Code and other editors. Start via:
//!
//! ```text
//! covenant-lsp
//! ```
//!
//! The server is configured as the language server in the VS Code extension's
//! `package.json` via `serverOptions.command`.

use covenant_lsp::backend::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
