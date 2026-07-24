//! Snapshot tests for `extract_imports_resolved` output stability.
//!
//! `ImportResolver` feeds wiring analysis and module_tree construction.
//! Silent shape changes in `ResolvedImport` (new field, reordering, alias
//! normalization) would silently corrupt downstream fan-in/fan-out metrics.
//! Snapshots lock the contract across tree-sitter grammar bumps.
//!
//! Review new snapshots: `cargo insta review -p touring-ast`.

use touring_code::ast::import_resolver::{ResolvedImport, extract_imports_resolved};
use touring_code::ast::languages::Lang;

/// Project to the stable tuple — line numbers are kept because they anchor
/// the import back to source and are part of the wiring contract.
fn project(imports: &[ResolvedImport]) -> Vec<(String, Option<String>, bool, usize)> {
    let mut out: Vec<_> = imports
        .iter()
        .map(|i| (i.path.clone(), i.alias.clone(), i.is_glob, i.line))
        .collect();
    out.sort();
    out
}

#[test]
fn snapshot_rust_imports_mixed() {
    let source = r#"
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tokio::net::UnixStream as Stream;
use crate::error::*;
"#;
    let resolver = extract_imports_resolved(source, Lang::Rust);
    insta::assert_yaml_snapshot!("rust_imports_mixed", project(&resolver.imports));
}

#[test]
fn snapshot_python_imports_mixed() {
    let source = r#"
import os
import sys as system
from typing import List, Optional
from collections import OrderedDict
"#;
    let resolver = extract_imports_resolved(source, Lang::Python);
    insta::assert_yaml_snapshot!("python_imports_mixed", project(&resolver.imports));
}
