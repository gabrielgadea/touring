//! `touring-lsp` binary — the editor entry point.
//!
//! Built only with `--features lsp-bridge`:
//!   cargo build -p touring-lsp --features lsp-bridge --bin touring-lsp
//!
//! An editor launches this binary and speaks LSP over stdin/stdout. Example
//! (Neovim / VS Code generic LSP client): point the server command at the built
//! binary; it serves references + rename for the workspace it is launched in.

#[cfg(feature = "lsp-bridge")]
#[tokio::main]
async fn main() {
    use tower_lsp::{LspService, Server};

    // The workspace root the backend resolves relative paths against. We use the
    // process CWD, which an editor sets to the project root on launch.
    let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(move |client| {
        touring_lsp::server::Backend::new(client, project_root.clone())
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(not(feature = "lsp-bridge"))]
fn main() {
    eprintln!(
        "touring-lsp was built without the `lsp-bridge` feature. \
         Rebuild with: cargo build -p touring-lsp --features lsp-bridge --bin touring-lsp"
    );
    std::process::exit(2);
}
