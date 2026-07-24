//! D39 L0 — File-level index (raw tokens)
//!
//! Provides raw token-level indexing of source files.

use std::collections::HashMap;

/// File index entry for a token occurrence.
#[derive(Debug, Clone)]
pub struct FileIndexEntry {
    /// Token text.
    pub token: String,
    /// File path.
    pub file_path: String,
    /// Line number.
    pub line: u32,
    /// Column offset.
    pub column: u32,
}

/// File index - L0 knowledge layer.
///
/// Indexes raw tokens from source files for fast substring matching.
pub struct FileIndex {
    /// Token to occurrences mapping.
    token_map: HashMap<String, Vec<FileIndexEntry>>,
}

impl FileIndex {
    /// Create a new empty file index.
    pub fn new() -> Self {
        Self {
            token_map: HashMap::new(),
        }
    }

    /// Index a single file's content.
    pub fn index_file(&mut self, path: &str, content: &str) {
        for (line_idx, line) in content.lines().enumerate() {
            let mut col_idx: u32 = 0;
            for token in line.split_whitespace() {
                let entry = FileIndexEntry {
                    token: token.to_string(),
                    file_path: path.to_string(),
                    line: line_idx as u32 + 1,
                    column: col_idx,
                };
                self.token_map
                    .entry(token.to_lowercase())
                    .or_default()
                    .push(entry);
                col_idx += token.len() as u32 + 1;
            }
        }
    }

    /// Search for a query string across indexed tokens.
    pub fn search(&self, query: &str) -> Vec<FileIndexEntry> {
        let query_lower = query.to_lowercase();
        self.token_map
            .get(&query_lower)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for FileIndex {
    fn default() -> Self {
        Self::new()
    }
}
