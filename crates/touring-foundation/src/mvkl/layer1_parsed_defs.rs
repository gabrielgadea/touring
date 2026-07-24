//! D39 L1 — Parsed definitions (functions, structs, traits)
//!
//! Provides parsed definition-level indexing.

use std::collections::HashMap;

/// Kind of parsed definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParsedDefKind {
    /// Function or method.
    Function,
    /// Struct definition.
    Struct,
    /// Enum definition.
    Enum,
    /// Trait definition.
    Trait,
    /// Module.
    Module,
    /// Constant.
    Constant,
    /// Type alias.
    TypeAlias,
    /// Other definition.
    Other,
}

/// A parsed definition entry.
#[derive(Debug, Clone)]
pub struct ParsedDef {
    /// Definition name.
    pub name: String,
    /// Kind of definition.
    pub kind: ParsedDefKind,
    /// File path.
    pub file_path: String,
    /// Line number where defined.
    pub line: u32,
    /// Column offset.
    pub column: u32,
    /// Optionally scoped name (e.g., "module::Struct::method").
    pub qualified_name: Option<String>,
}

/// Parsed definitions index - L1 knowledge layer.
///
/// Indexes parsed definitions from source files.
pub struct ParsedDefsIndex {
    /// Name to definitions mapping.
    defs_by_name: HashMap<String, Vec<ParsedDef>>,
    /// All definitions grouped by file.
    defs_by_file: HashMap<String, Vec<ParsedDef>>,
}

impl ParsedDefsIndex {
    /// Create a new empty parsed definitions index.
    pub fn new() -> Self {
        Self {
            defs_by_name: HashMap::new(),
            defs_by_file: HashMap::new(),
        }
    }

    /// Index a parsed definition.
    pub fn index_def(&mut self, def: ParsedDef) {
        // Add to name index
        self.defs_by_name
            .entry(def.name.clone())
            .or_default()
            .push(def.clone());

        // Add to file index
        self.defs_by_file
            .entry(def.file_path.clone())
            .or_default()
            .push(def);
    }

    /// Search definitions by name substring.
    pub fn search(&self, query: &str) -> Vec<ParsedDef> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<ParsedDef> = Vec::new();

        for (name, defs) in &self.defs_by_name {
            if name.to_lowercase().contains(&query_lower) {
                results.extend(defs.clone());
            }
        }

        // Sort by name match quality (exact > prefix > contains)
        results.sort_by(|a, b| {
            let a_lower = a.name.to_lowercase();
            let b_lower = b.name.to_lowercase();
            let query_lower = query.to_lowercase();

            let a_exact = a_lower == query_lower;
            let b_exact = b_lower == query_lower;
            let a_prefix = a_lower.starts_with(&query_lower);
            let b_prefix = b_lower.starts_with(&query_lower);

            match (a_exact, b_exact) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match (a_prefix, b_prefix) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                },
            }
        });

        results
    }

    /// Get all definitions in a file.
    pub fn get_in_file(&self, file_path: &str) -> Vec<ParsedDef> {
        self.defs_by_file
            .get(file_path)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for ParsedDefsIndex {
    fn default() -> Self {
        Self::new()
    }
}
