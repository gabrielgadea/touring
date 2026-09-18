//! B6 — the PATH, not the component.
//!
//! `python_qualified_uses` (touring-code) and `resolve_import_path_with_source`
//! (touring-hooks-core) each have unit tests, and the index rebuild is the only
//! place they meet: the extractor yields `(module_path, symbol)` and the resolver
//! turns the module path into the file whose producer row carries the symbol. A
//! green test on each half proves nothing about the join — the lesson this repo
//! paid for twice (a permission granted with three green tests while execution
//! stayed denied; a TTY barrier proven to refuse and never proven to admit).
//!
//! These tests write REAL files and walk the real join, so a change in either
//! half that breaks the handshake fails here.

use std::fs;
use std::path::{Path, PathBuf};

/// A throwaway package tree: `pacote/modulo.py` defining `PROCESSADO`, plus the
/// consumer file the caller writes.
fn fixture(name: &str, consumer_src: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("b6-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("pacote")).expect("create package dir");
    fs::write(root.join("pacote/__init__.py"), "").expect("write __init__");
    fs::write(
        root.join("pacote/modulo.py"),
        "PROCESSADO = 1\n\n\ndef ler():\n    return PROCESSADO\n",
    )
    .expect("write module");
    let consumer = root.join("consumidor.py");
    fs::write(&consumer, consumer_src).expect("write consumer");
    (root, consumer)
}

/// The join under test: every pair the extractor yields, resolved to a file.
fn resolved_edges(consumer: &Path) -> Vec<(String, String)> {
    let src = fs::read_to_string(consumer).expect("read consumer");
    touring_code::ast::graph::python_qualified_uses(&src)
        .into_iter()
        .filter_map(|(module_path, symbol)| {
            touring_hooks_core::symbol_extractors::resolve_import_path_with_source(
                &module_path,
                "python",
                consumer.to_str(),
            )
            .map(|file| (file, symbol))
        })
        .collect()
}

#[test]
fn a_from_import_without_alias_reaches_the_file_that_defines_the_symbol() {
    let (root, consumer) = fixture(
        "plain",
        "from pacote import modulo\n\n\ndef usa():\n    return modulo.PROCESSADO\n",
    );
    let edges = resolved_edges(&consumer);
    assert_eq!(
        edges,
        vec![(
            root.join("pacote/modulo.py").to_string_lossy().into_owned(),
            "PROCESSADO".to_string()
        )],
        "the producer row lives in modulo.py; without this edge PROCESSADO reads as an orphan"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_aliased_form_reaches_the_same_file() {
    let (root, consumer) = fixture(
        "alias",
        "from pacote import modulo as m\n\n\ndef usa():\n    return m.PROCESSADO\n",
    );
    let edges = resolved_edges(&consumer);
    assert_eq!(edges.len(), 1, "{edges:?}");
    assert!(edges[0].0.ends_with("pacote/modulo.py"), "{edges:?}");
    assert_eq!(edges[0].1, "PROCESSADO");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_name_that_is_a_symbol_and_not_a_module_writes_no_edge() {
    // NEGATIVE CONTROL for the join, and the reason the extractor needs no guard
    // of its own: `from pacote import Classe` binds a CLASS, so `pacote.Classe`
    // names no file. The resolver returning None is what keeps a wrong producer
    // key — worse than a missing edge — out of the graph.
    let (root, consumer) = fixture(
        "symbol",
        "from pacote import Classe\n\n\ndef usa():\n    return Classe.PROCESSADO\n",
    );
    assert!(
        resolved_edges(&consumer).is_empty(),
        "a class attribute must not be recorded as a module-qualified use"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_old_extractor_shape_would_have_found_nothing_here() {
    // CONTROL that pins the CAUSE: before B6 the extractor matched only
    // `import_statement`, so this file produced zero pairs. If some other change
    // ever makes the positive tests above pass for a different reason, this one
    // still says what B6 was for — the `from` form carries the whole family
    // (measured 16/09/2026: 53 of 53 symbols in the judge's scope, 126 project-wide).
    let (root, consumer) = fixture(
        "shape",
        "from pacote import modulo\n\n\ndef usa():\n    return modulo.PROCESSADO\n",
    );
    let src = fs::read_to_string(&consumer).unwrap();
    assert!(
        !src.lines().any(|l| l.starts_with("import ")),
        "the fixture must contain NO plain `import` — otherwise the edge could come from the old branch"
    );
    assert_eq!(
        touring_code::ast::graph::python_qualified_uses(&src).len(),
        1
    );
    let _ = fs::remove_dir_all(&root);
}
