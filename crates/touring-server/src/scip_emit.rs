//! SCIP (Source Code Intelligence Protocol) emitter.
//!
//! Exports the touring symbol index as a SCIP binary file
//! consumable by Sourcegraph, VS Code, and JetBrains IDEs.
//!
//! Feature-gated: `scip-emit`
//!
//! # Usage
//!
//! ```toml
//! [features]
//! scip-emit = ["dep:prost", "dep:prost-types"]
//! ```
//!
//! ```rust,ignore
//! use touring_server::scip_emit::{ScipEmitter, SymbolEntry};
//! let emitter = ScipEmitter::new("/path/to/project");
//! let count = emitter.emit(&symbols, Path::new("index.scip"))?;
//! ```

#![cfg(feature = "scip-emit")]

use prost::Message;
use std::path::Path;

// ── SCIP protobuf schema (simplified, wire-compatible with Sourcegraph tooling) ──

/// SCIP Index — top-level container.
#[derive(Clone, prost::Message)]
pub struct ScipIndex {
    /// Metadata about the index.
    #[prost(message, optional, tag = "1")]
    pub metadata: Option<ScipMetadata>,
    /// One document per source file.
    #[prost(message, repeated, tag = "2")]
    pub documents: Vec<ScipDocument>,
}

/// Index metadata.
#[derive(Clone, prost::Message)]
pub struct ScipMetadata {
    /// SCIP version (always 1).
    #[prost(int32, tag = "1")]
    pub version: i32,
    /// Tool that produced this index.
    #[prost(string, tag = "2")]
    pub tool_info: String,
    /// Project root URI.
    #[prost(string, tag = "3")]
    pub project_root: String,
}

/// A single source file with its symbols.
#[derive(Clone, prost::Message)]
pub struct ScipDocument {
    /// Relative path from project root.
    #[prost(string, tag = "1")]
    pub relative_path: String,
    /// Programming language.
    #[prost(string, tag = "2")]
    pub language: String,
    /// Symbol occurrences in this file.
    #[prost(message, repeated, tag = "3")]
    pub occurrences: Vec<ScipOccurrence>,
    /// Symbol information (definitions).
    #[prost(message, repeated, tag = "4")]
    pub symbols: Vec<ScipSymbolInfo>,
}

/// A symbol occurrence (definition or reference).
#[derive(Clone, prost::Message)]
pub struct ScipOccurrence {
    /// [line, col_start, col_end] — 0-indexed.
    #[prost(int32, repeated, tag = "1")]
    pub range: Vec<i32>,
    /// Symbol identifier (e.g., "touring rust crate::lib structFoo#").
    #[prost(string, tag = "2")]
    pub symbol: String,
    /// Role: 0 = reference, 1 = definition.
    #[prost(int32, tag = "3")]
    pub symbol_roles: i32,
}

/// Symbol information (documentation, relationships).
#[derive(Clone, prost::Message)]
pub struct ScipSymbolInfo {
    /// Symbol identifier (same as in occurrences).
    #[prost(string, tag = "1")]
    pub symbol: String,
    /// Documentation lines.
    #[prost(string, repeated, tag = "2")]
    pub documentation: Vec<String>,
    /// Relationships (implements, extends, etc.).
    #[prost(message, repeated, tag = "3")]
    pub relationships: Vec<ScipRelationship>,
}

/// Relationship between two symbols.
#[derive(Clone, prost::Message)]
pub struct ScipRelationship {
    /// Target symbol identifier.
    #[prost(string, tag = "1")]
    pub symbol: String,
    /// True if this symbol implements the target.
    #[prost(bool, tag = "2")]
    pub is_implementation: bool,
    /// True if this symbol is a type definition of the target.
    #[prost(bool, tag = "3")]
    pub is_type_definition: bool,
}

// ── Entry type (decoupled from internal SymbolLocation) ──────────────────────

/// A symbol entry from the touring index, ready for SCIP emission.
pub struct SymbolEntry {
    /// Symbol name (e.g., "MyStruct", "my_fn").
    pub name: String,
    /// Absolute file path.
    pub file_path: String,
    /// Symbol kind (struct, fn, trait, etc.).
    pub kind: String,
    /// 0-indexed line number.
    pub line: u32,
    /// Programming language identifier.
    pub language: String,
    /// Fully-qualified module path (e.g., "crate::module::sub").
    pub module_path: Option<String>,
    /// Documentation string, if available.
    pub docstring: Option<String>,
}

// ── Emitter ───────────────────────────────────────────────────────────────────

/// SCIP emitter — converts touring symbol entries to a SCIP binary index file.
pub struct ScipEmitter {
    project_root: String,
}

/// Error from [`ScipEmitter::emit`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum ScipEmitError {
    /// Protobuf encoding of the SCIP index failed.
    #[error("protobuf encode: {0}")]
    Encode(#[source] prost::EncodeError),
    /// Writing the encoded index to the output path failed.
    #[error("write {path}: {source}")]
    Write {
        /// Output path that could not be written.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl ScipEmitter {
    /// Create a new emitter anchored at `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Build a canonical SCIP symbol identifier for an entry.
    ///
    /// Format: `touring <language> <module_path> <kind><name>#`
    fn make_symbol_id(entry: &SymbolEntry) -> String {
        let module = entry.module_path.as_deref().unwrap_or("");
        format!(
            "touring {} {} {}{}#",
            entry.language, module, entry.kind, entry.name,
        )
    }

    /// Emit a SCIP binary index from `symbols` to `output`.
    ///
    /// Symbols are grouped by file; each file becomes one `ScipDocument`.
    /// File paths are sorted for reproducible output ordering.
    /// Returns the number of documents written.
    ///
    /// # Errors
    /// Returns [`ScipEmitError`] if protobuf encoding or file I/O fails.
    pub fn emit(&self, symbols: &[SymbolEntry], output: &Path) -> Result<usize, ScipEmitError> {
        use std::collections::HashMap;

        // Group symbol indices by file path.
        let mut by_file: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, sym) in symbols.iter().enumerate() {
            by_file.entry(sym.file_path.clone()).or_default().push(idx);
        }

        // Sort file paths for deterministic output.
        let mut sorted_files: Vec<&String> = by_file.keys().collect();
        sorted_files.sort();

        let mut documents = Vec::with_capacity(sorted_files.len());

        for file_path in sorted_files {
            let indices = by_file
                .get(file_path)
                .expect("file_path from by_file.keys()");

            let rel_path = file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(file_path.as_str())
                .trim_start_matches('/');

            let language = indices
                .first()
                .and_then(|&i| symbols.get(i))
                .map(|sym| sym.language.clone())
                .unwrap_or_default();

            let mut occurrences = Vec::with_capacity(indices.len());
            let mut symbol_infos = Vec::with_capacity(indices.len());

            for &i in indices {
                let sym = symbols.get(i).expect("index from enumerate over symbols");
                let sym_id = Self::make_symbol_id(sym);
                let line = i32::try_from(sym.line).unwrap_or(0);
                let name_len = i32::try_from(sym.name.len()).unwrap_or(0);

                occurrences.push(ScipOccurrence {
                    range: vec![line, 0, line, name_len],
                    symbol: sym_id.clone(),
                    symbol_roles: 1, // definition
                });

                let mut docs = Vec::new();
                if let Some(ref ds) = sym.docstring {
                    docs.push(ds.clone());
                }

                symbol_infos.push(ScipSymbolInfo {
                    symbol: sym_id,
                    documentation: docs,
                    relationships: Vec::new(),
                });
            }

            // Sort occurrences by line for stable output.
            occurrences.sort_by_key(|o| o.range.first().copied().unwrap_or(0));

            documents.push(ScipDocument {
                relative_path: rel_path.to_string(),
                language,
                occurrences,
                symbols: symbol_infos,
            });
        }

        let doc_count = documents.len();

        let index = ScipIndex {
            metadata: Some(ScipMetadata {
                version: 1,
                tool_info: "touring-server SCIP emitter v0.1.0".to_string(),
                project_root: self.project_root.clone(),
            }),
            documents,
        };

        // Encode to protobuf binary.
        let mut buf = Vec::with_capacity(index.encoded_len());
        index.encode(&mut buf).map_err(ScipEmitError::Encode)?;

        // Write to output path (creates or truncates).
        std::fs::write(output, &buf).map_err(|e| ScipEmitError::Write {
            path: output.display().to_string(),
            source: e,
        })?;

        Ok(doc_count)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, file: &str, kind: &str, line: u32) -> SymbolEntry {
        SymbolEntry {
            name: name.to_string(),
            file_path: file.to_string(),
            kind: kind.to_string(),
            line,
            language: "rust".to_string(),
            module_path: Some("crate::lib".to_string()),
            docstring: Some(format!("Doc for {name}")),
        }
    }

    fn emit_and_decode(emitter: &ScipEmitter, symbols: &[SymbolEntry], name: &str) -> ScipIndex {
        let output = std::env::temp_dir().join(name);
        emitter.emit(symbols, &output).expect("emit must succeed");
        let bytes = std::fs::read(&output).expect("scip file must be readable");
        let index = ScipIndex::decode(bytes.as_slice()).expect("must decode as protobuf");
        let _ = std::fs::remove_file(&output);
        index
    }

    #[test]
    fn test_emit_creates_valid_binary() {
        let emitter = ScipEmitter::new("/tmp/test_project");
        let symbols = vec![
            make_entry("MyStruct", "/tmp/test_project/src/lib.rs", "struct", 10),
            make_entry("my_function", "/tmp/test_project/src/lib.rs", "fn", 20),
        ];

        let output = std::env::temp_dir().join("test_touring_emit.scip");
        let count = emitter.emit(&symbols, &output).expect("emit must succeed");
        assert_eq!(count, 1, "1 document (both symbols in same file)");

        let bytes = std::fs::read(&output).expect("scip file must be readable");
        assert!(!bytes.is_empty(), "SCIP file must be non-empty");

        let decoded = ScipIndex::decode(bytes.as_slice()).expect("must decode back");
        assert_eq!(decoded.documents.len(), 1);
        assert_eq!(decoded.documents[0].occurrences.len(), 2);

        let meta = decoded.metadata.expect("metadata must be present");
        assert_eq!(meta.version, 1);
        assert_eq!(decoded.documents[0].relative_path, "src/lib.rs");

        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn test_emit_multi_file() {
        let emitter = ScipEmitter::new("/tmp/proj");
        let symbols = vec![
            make_entry("StructA", "/tmp/proj/src/a.rs", "struct", 1),
            make_entry("fn_b", "/tmp/proj/src/b.rs", "fn", 5),
            make_entry("StructB", "/tmp/proj/src/b.rs", "struct", 10),
        ];

        let decoded = emit_and_decode(&emitter, &symbols, "test_touring_multi.scip");
        assert_eq!(decoded.documents.len(), 2, "2 documents (2 files)");

        let b_doc = decoded
            .documents
            .iter()
            .find(|d| d.relative_path == "src/b.rs")
            .expect("b.rs document must exist");
        assert_eq!(b_doc.occurrences.len(), 2, "b.rs has 2 symbols");
    }

    #[test]
    fn test_emit_empty_symbols() {
        let emitter = ScipEmitter::new("/tmp");
        let output = std::env::temp_dir().join("test_touring_empty.scip");
        let count = emitter
            .emit(&[], &output)
            .expect("emit of empty slice must succeed");
        assert_eq!(count, 0, "0 documents for empty input");

        let bytes = std::fs::read(&output).expect("scip file must be readable");
        let decoded = ScipIndex::decode(bytes.as_slice()).expect("empty index must decode");
        assert_eq!(decoded.documents.len(), 0);

        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn test_make_symbol_id_format() {
        let entry = SymbolEntry {
            name: "Foo".to_string(),
            file_path: "src/lib.rs".to_string(),
            kind: "struct".to_string(),
            line: 1,
            language: "rust".to_string(),
            module_path: Some("crate::lib".to_string()),
            docstring: None,
        };
        let id = ScipEmitter::make_symbol_id(&entry);
        assert!(id.contains("Foo"), "id must contain symbol name");
        assert!(id.contains("rust"), "id must contain language");
        assert!(id.contains("struct"), "id must contain kind");
        assert!(id.ends_with('#'), "SCIP symbol ids end with '#'");
    }

    #[test]
    fn test_relative_path_stripping() {
        let emitter = ScipEmitter::new("/workspace/myproject");
        let symbols = vec![make_entry(
            "Bar",
            "/workspace/myproject/src/bar.rs",
            "struct",
            3,
        )];
        let decoded = emit_and_decode(&emitter, &symbols, "test_touring_relpath.scip");
        assert_eq!(decoded.documents[0].relative_path, "src/bar.rs");
    }

    #[test]
    fn test_no_docstring_entry() {
        let emitter = ScipEmitter::new("/tmp");
        let symbols = vec![SymbolEntry {
            name: "NoDocs".to_string(),
            file_path: "/tmp/src/x.rs".to_string(),
            kind: "fn".to_string(),
            line: 0,
            language: "rust".to_string(),
            module_path: None,
            docstring: None,
        }];
        let decoded = emit_and_decode(&emitter, &symbols, "test_touring_nodoc.scip");
        assert_eq!(
            decoded.documents[0].symbols[0].documentation.len(),
            0,
            "no docs when docstring is None"
        );
    }
}
