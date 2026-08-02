//! tower-lsp server bridging Touring's cross-file capabilities to editors.
//!
//! This module is gated behind the `lsp-bridge` feature so the default
//! workspace build never pulls `tower-lsp` / `tokio`. The async
//! [`LanguageServer`] impl is a thin shell over the pure mapping layer in
//! [`crate::mapping`] plus the Touring backends
//! `touring_hooks::cli_handlers_semantics::{cli_find_references, cli_rename}`.
//!
//! ## Sync/async bridge
//! The Touring backends are synchronous (`fn(&mut HookRuntime, &Value) -> String`)
//! and `HookRuntime` is not `Send`-friendly across `.await` points, so each
//! request runs the backend call inside [`tokio::task::spawn_blocking`] on a
//! freshly constructed [`HookRuntime`] owned by the LSP server's project root.
//! This keeps the async runtime un-blocked and side-steps any `!Send` guard.
//!
//! ## Dependency direction
//! `touring-lsp` -> `touring-hooks` (never the reverse). The LSP server *owns*
//! its runtime via `HookRuntime::new(project_root)`.

use std::path::PathBuf;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    InitializeParams, InitializeResult, InitializedParams, Location, MessageType, OneOf, Position,
    Range, ReferenceParams, RenameParams, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

use crate::mapping::{
    LspPosition, backend_location_to_lsp, parse_references_response, parse_rename_edits,
    references_request_payload, rename_request_payload,
};

/// The Touring LSP backend. Holds the client handle (for log messages) and the
/// workspace root used to construct a fresh `HookRuntime` per request.
#[derive(Debug)]
pub struct Backend {
    /// Client handle for server-to-client notifications.
    pub client: Client,
    /// Workspace root passed to `HookRuntime::new` on each blocking call.
    pub project_root: PathBuf,
}

impl Backend {
    /// Construct a backend bound to a workspace root.
    pub fn new(client: Client, project_root: PathBuf) -> Self {
        Self {
            client,
            project_root,
        }
    }

    /// Resolve a file `Url` to a filesystem path string the backend understands.
    /// Falls back to the URL path if it is not a `file:` scheme.
    fn url_to_path(url: &Url) -> String {
        url.to_file_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| url.path().to_string())
    }

    /// Run `cli_find_references(scope="workspace")` off the async executor.
    /// Returns the raw backend JSON string (empty-envelope on any failure).
    async fn call_find_references(&self, file: String, pos: LspPosition) -> String {
        let root = self.project_root.clone();
        tokio::task::spawn_blocking(move || {
            let payload = references_request_payload(&file, pos);
            match touring_hooks::HookRuntime::new(&root) {
                Ok(mut rt) => {
                    touring_hooks::cli_handlers_semantics::cli_find_references(&mut rt, &payload)
                }
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            }
        })
        .await
        .unwrap_or_else(|_| serde_json::json!({ "error": "join error" }).to_string())
    }

    /// Run `cli_rename(scope="workspace")` off the async executor.
    async fn call_rename(&self, file: String, pos: LspPosition, new_name: String) -> String {
        let root = self.project_root.clone();
        tokio::task::spawn_blocking(move || {
            let payload = rename_request_payload(&file, pos, &new_name);
            match touring_hooks::HookRuntime::new(&root) {
                Ok(mut rt) => touring_hooks::cli_handlers_semantics::cli_rename(&mut rt, &payload),
                Err(e) => {
                    serde_json::json!({ "status": "error", "error": e.to_string() }).to_string()
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            serde_json::json!({ "status": "error", "error": "join error" }).to_string()
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // We support full-document sync semantics only nominally; the
                // backends re-read files from disk, so we advertise NONE to keep
                // the editor from streaming incremental edits we ignore.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::NONE,
                )),
                // REAL: backed by cli_find_references(scope="workspace").
                references_provider: Some(OneOf::Left(true)),
                // REAL (preview): backed by cli_rename(scope="workspace").
                rename_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "touring-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "touring-lsp initialized (references + rename)",
            )
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    /// textDocument/references — cross-file via Touring's symbol_store.
    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let file = Self::url_to_path(&uri);
        let lsp_pos = LspPosition {
            line: pos.line,
            character: pos.character,
        };

        let raw = self.call_find_references(file, lsp_pos).await;
        let resp = parse_references_response(&raw);

        let locations: Vec<Location> = resp
            .locations
            .iter()
            .filter_map(|loc| {
                // Resolve each backend file_path back to a Url. Backend paths are
                // workspace-relative or absolute; join relative ones to the root.
                let abs = if std::path::Path::new(&loc.file_path).is_absolute() {
                    PathBuf::from(&loc.file_path)
                } else {
                    self.project_root.join(&loc.file_path)
                };
                let target_url = Url::from_file_path(&abs).ok()?;
                let p = backend_location_to_lsp(loc);
                let start = Position::new(p.line, p.character);
                // Single-point range (start == end); editors widen to the symbol.
                Some(Location::new(target_url, Range::new(start, start)))
            })
            .collect();

        Ok(Some(locations))
    }

    /// textDocument/rename — cross-file preview via Touring's rename collector.
    /// Returns a [`WorkspaceEdit`] of single-character anchor ranges; each site
    /// carries real (line, column) re-resolved from the backend response.
    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        use std::collections::HashMap;
        use tower_lsp::lsp_types::TextEdit;

        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let file = Self::url_to_path(&uri);
        let lsp_pos = LspPosition {
            line: pos.line,
            character: pos.character,
        };

        let raw = self.call_rename(file, lsp_pos, new_name.clone()).await;
        let edits = parse_rename_edits(&raw);
        if edits.is_empty() {
            // conflict / error / not-applicable -> signal "no rename".
            return Ok(None);
        }

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for edit in &edits {
            let abs = if std::path::Path::new(&edit.file_path).is_absolute() {
                PathBuf::from(&edit.file_path)
            } else {
                self.project_root.join(&edit.file_path)
            };
            let Ok(target_url) = Url::from_file_path(&abs) else {
                continue;
            };
            // Re-resolve the placeholder span to an LSP position (1-based -> 0-based).
            let start = Position::new(edit.line.saturating_sub(1), edit.column.saturating_sub(1));
            // Anchor range at the identifier start; editors apply new text there.
            let range = Range::new(start, start);
            changes
                .entry(target_url)
                .or_default()
                .push(TextEdit::new(range, new_name.clone()));
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }
}
