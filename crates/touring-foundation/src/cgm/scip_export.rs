//! D41 — SCIP Export for Code Graph Model
//!
//! Provides SCIP (Source Code Intelligence Protocol) export for interoperability
//! with other code analysis tools.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error from [`CgmScipExport`] JSON (de)serialization (F-8 / RBP-03: typed in
/// place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum ScipExportError {
    /// SCIP export → JSON serialization failed.
    #[error("SCIP JSON serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    /// JSON → SCIP export deserialization failed.
    #[error("SCIP JSON deserialization failed: {0}")]
    Deserialize(#[source] serde_json::Error),
}

/// A SCIP symbol (fully qualified name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgmScipSymbol {
    /// The symbol's document (usually filename).
    pub document: String,
    /// The symbol's package/namespace.
    pub package: String,
    /// The symbol's descriptor (local name).
    pub descriptor: String,
}

impl CgmScipSymbol {
    /// Create a new SCIP symbol.
    pub fn new(document: &str, package: &str, descriptor: &str) -> Self {
        Self {
            document: document.to_string(),
            package: package.to_string(),
            descriptor: descriptor.to_string(),
        }
    }

    /// Convert to SCIP string representation.
    pub fn to_scip_string(&self) -> String {
        format!("{} {} {}", self.document, self.package, self.descriptor)
    }
}

/// A SCIP occurrence (reference to a symbol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgmScipOccurrence {
    /// Occurrence range (start, end) in characters.
    pub range: (usize, usize),
    /// The symbol this occurrence refers to.
    pub symbol: CgmScipSymbol,
    /// Optional syntax kind.
    pub syntax_kind: Option<String>,
}

/// A SCIP document (parsed source file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgmScipDocument {
    /// Document path.
    pub path: String,
    /// Language identifier (e.g., "rust", "python").
    pub language: String,
    /// Occurrences in this document.
    pub occurrences: Vec<CgmScipOccurrence>,
    /// Symbols defined in this document.
    pub symbols: Vec<CgmScipSymbol>,
}

impl CgmScipDocument {
    /// Create a new empty SCIP document.
    pub fn new(path: &str, language: &str) -> Self {
        Self {
            path: path.to_string(),
            language: language.to_string(),
            occurrences: Vec::new(),
            symbols: Vec::new(),
        }
    }

    /// Add an occurrence.
    pub fn add_occurrence(&mut self, occ: CgmScipOccurrence) {
        self.occurrences.push(occ);
    }

    /// Add a symbol definition.
    pub fn add_symbol(&mut self, sym: CgmScipSymbol) {
        self.symbols.push(sym);
    }
}

/// Export result containing the complete SCIP payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgmScipExport {
    /// Export version.
    pub version: String,
    /// Documents in this export.
    pub documents: Vec<CgmScipDocument>,
    /// Optional metadata.
    pub metadata: HashMap<String, String>,
}

impl CgmScipExport {
    /// Create a new empty SCIP export.
    pub fn new() -> Self {
        Self {
            version: "2.0.0".to_string(),
            documents: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a document to the export.
    pub fn add_document(&mut self, doc: CgmScipDocument) {
        self.documents.push(doc);
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, ScipExportError> {
        serde_json::to_string_pretty(self).map_err(ScipExportError::Serialize)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, ScipExportError> {
        serde_json::from_str(json).map_err(ScipExportError::Deserialize)
    }
}

impl Default for CgmScipExport {
    fn default() -> Self {
        Self::new()
    }
}

/// Export a code graph to SCIP format.
///
/// Takes a document path, language, and symbol definitions and generates
/// a SCIP-compatible export.
pub fn export_to_scip(
    path: &str,
    language: &str,
    symbols: &[(String, String, String)],
    occurrences: &[(usize, usize, String, String, String)],
) -> CgmScipExport {
    let mut export = CgmScipExport::new();

    let mut doc = CgmScipDocument::new(path, language);

    // Add symbol definitions
    for (doc_part, package, descriptor) in symbols {
        let sym = CgmScipSymbol::new(doc_part, package, descriptor);
        doc.add_symbol(sym);
    }

    // Add occurrences
    for (start, end, doc_part, package, descriptor) in occurrences {
        let sym = CgmScipSymbol::new(doc_part, package, descriptor);
        doc.add_occurrence(CgmScipOccurrence {
            range: (*start, *end),
            symbol: sym,
            syntax_kind: None,
        });
    }

    export.add_document(doc);
    export
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scip_symbol() {
        let sym = CgmScipSymbol::new("main.rs", "my_package", "main");
        assert_eq!(sym.document, "main.rs");
        assert_eq!(sym.package, "my_package");
        assert_eq!(sym.descriptor, "main");
    }

    #[test]
    fn test_scip_export_roundtrip() {
        let mut export = CgmScipExport::new();
        let mut doc = CgmScipDocument::new("test.rs", "rust");
        doc.add_symbol(CgmScipSymbol::new("test.rs", "test_pkg", "TestFn"));
        export.add_document(doc);

        let json = export.to_json().unwrap();
        let parsed = CgmScipExport::from_json(&json).unwrap();

        assert_eq!(parsed.documents.len(), 1);
        assert_eq!(parsed.documents[0].symbols.len(), 1);
    }
}
