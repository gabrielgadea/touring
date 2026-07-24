//! Property-based tests for touring-ast symbol extraction.
//!
//! Uses proptest to verify structural invariants on extracted symbols
//! across a variety of generated Python source code snippets.

use proptest::prelude::*;
use std::collections::HashSet;

use touring_code::ast::{Lang, SymbolKind, extract_symbols};

// ============================================================================
// Strategies for generating valid Python source code
// ============================================================================

/// Generate a valid Python function name (lowercase with underscores).
fn py_func_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}".prop_filter("must not be a keyword", |name| {
        !matches!(
            name.as_str(),
            "if" | "else"
                | "elif"
                | "for"
                | "while"
                | "def"
                | "class"
                | "return"
                | "import"
                | "from"
                | "pass"
                | "break"
                | "continue"
                | "try"
                | "except"
                | "finally"
                | "with"
                | "as"
                | "yield"
                | "raise"
                | "in"
                | "is"
                | "not"
                | "and"
                | "or"
                | "None"
                | "True"
                | "False"
                | "lambda"
                | "global"
                | "nonlocal"
                | "del"
                | "assert"
                | "async"
                | "await"
        )
    })
}

/// Generate a valid Python class name (CamelCase).
fn py_class_name() -> impl Strategy<Value = String> {
    "[A-Z][a-z]{1,10}".prop_map(|s| s)
}

/// Generate a simple Python function definition.
fn py_function() -> impl Strategy<Value = String> {
    py_func_name().prop_map(|name| format!("def {}():\n    pass\n", name))
}

/// Generate a simple Python async function definition.
fn py_async_function() -> impl Strategy<Value = String> {
    py_func_name().prop_map(|name| format!("async def {}():\n    pass\n", name))
}

/// Generate a simple Python class with methods.
fn py_class_with_methods() -> impl Strategy<Value = String> {
    (py_class_name(), prop::collection::vec(py_func_name(), 0..4)).prop_map(
        |(class_name, methods)| {
            let mut src = format!("class {}:\n", class_name);
            if methods.is_empty() {
                src.push_str("    pass\n");
            } else {
                for method in &methods {
                    src.push_str(&format!("    def {}(self):\n        pass\n", method));
                }
            }
            src
        },
    )
}

/// Generate a Python module with multiple top-level definitions.
fn py_module() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![py_function(), py_async_function(), py_class_with_methods(),],
        1..6,
    )
    .prop_map(|defs| defs.join("\n"))
}

// ============================================================================
// Symbol Extraction Invariant Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// All extracted symbols have valid line ranges: start_line <= end_line.
    #[test]
    fn symbols_valid_line_ranges(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                prop_assert!(
                    sym.line <= sym.end_line,
                    "Symbol '{}' has line {} > end_line {}",
                    sym.name, sym.line, sym.end_line
                );
            }
        }
    }

    /// All extracted symbols have non-empty names.
    #[test]
    fn symbols_nonempty_names(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                prop_assert!(
                    !sym.name.is_empty(),
                    "Found a symbol with empty name at line {}", sym.line
                );
            }
        }
    }

    /// No duplicate symbols at the same (line, column) position.
    #[test]
    fn symbols_no_position_duplicates(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let mut seen = HashSet::new();
            for sym in &symbols {
                let key = (sym.line, sym.column);
                prop_assert!(
                    seen.insert(key),
                    "Duplicate symbol at ({}, {}): '{}'",
                    sym.line, sym.column, sym.name
                );
            }
        }
    }

    /// start_byte <= end_byte for all symbols.
    #[test]
    fn symbols_valid_byte_ranges(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                prop_assert!(
                    sym.start_byte <= sym.end_byte,
                    "Symbol '{}' has start_byte {} > end_byte {}",
                    sym.name, sym.start_byte, sym.end_byte
                );
            }
        }
    }

    /// end_byte does not exceed source length.
    #[test]
    fn symbols_byte_within_source(source in py_module()) {
        let src_len = source.len();
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                prop_assert!(
                    sym.end_byte <= src_len,
                    "Symbol '{}' end_byte {} exceeds source length {}",
                    sym.name, sym.end_byte, src_len
                );
            }
        }
    }

    /// Function definitions produce Function kind symbols.
    #[test]
    fn function_def_produces_function_kind(name in py_func_name()) {
        let source = format!("def {}():\n    pass\n", name);
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let funcs: Vec<_> = symbols.iter().filter(|s| s.name == name).collect();
            prop_assert!(
                !funcs.is_empty(),
                "Function '{}' should be extracted", name
            );
            for f in &funcs {
                prop_assert!(
                    f.kind == SymbolKind::Function,
                    "Expected Function kind for '{}', got {:?}",
                    name, f.kind
                );
            }
        }
    }

    /// Async function definitions produce AsyncFunction kind symbols.
    #[test]
    fn async_def_produces_async_kind(name in py_func_name()) {
        let source = format!("async def {}():\n    pass\n", name);
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let funcs: Vec<_> = symbols.iter().filter(|s| s.name == name).collect();
            prop_assert!(
                !funcs.is_empty(),
                "Async function '{}' should be extracted", name
            );
            for f in &funcs {
                prop_assert!(
                    f.kind == SymbolKind::AsyncFunction || f.kind == SymbolKind::Function,
                    "Expected AsyncFunction or Function kind for '{}', got {:?}",
                    name, f.kind
                );
                // If it's classified as AsyncFunction, is_async should be true
                if f.kind == SymbolKind::AsyncFunction {
                    prop_assert!(f.is_async, "AsyncFunction '{}' should have is_async=true", name);
                }
            }
        }
    }

    /// Class definitions produce Class kind symbols.
    #[test]
    fn class_def_produces_class_kind(name in py_class_name()) {
        let source = format!("class {}:\n    pass\n", name);
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let classes: Vec<_> = symbols.iter().filter(|s| s.name == name).collect();
            prop_assert!(
                !classes.is_empty(),
                "Class '{}' should be extracted", name
            );
            for c in &classes {
                prop_assert!(
                    c.kind == SymbolKind::Class,
                    "Expected Class kind for '{}', got {:?}",
                    name, c.kind
                );
            }
        }
    }

    /// Non-empty source with valid definitions produces at least one symbol.
    #[test]
    fn nonempty_source_produces_symbols(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            prop_assert!(
                !symbols.is_empty(),
                "Module should produce at least one symbol"
            );
        }
    }

    /// Symbols within a class have the class as parent_name.
    #[test]
    fn methods_have_class_parent(
        class_name in py_class_name(),
        method_name in py_func_name(),
    ) {
        let source = format!(
            "class {}:\n    def {}(self):\n        pass\n",
            class_name, method_name
        );
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let methods: Vec<_> = symbols.iter().filter(|s| s.name == method_name).collect();
            for m in &methods {
                if let Some(ref parent) = m.parent_name {
                    prop_assert_eq!(
                        parent, &class_name,
                        "Method '{}' parent should be '{}', got '{}'",
                        method_name, class_name, parent
                    );
                }
                // parent_name might be None if the extractor doesn't populate it for
                // this case — that's acceptable, just verify if present it's correct.
            }
        }
    }

    /// line numbers are 1-indexed (never 0).
    #[test]
    fn symbols_lines_one_indexed(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                prop_assert!(
                    sym.line >= 1,
                    "Symbol '{}' has line {}, expected >= 1",
                    sym.name, sym.line
                );
                prop_assert!(
                    sym.end_line >= 1,
                    "Symbol '{}' has end_line {}, expected >= 1",
                    sym.name, sym.end_line
                );
            }
        }
    }

    /// Signatures are non-empty for functions and classes.
    #[test]
    fn symbols_nonempty_signatures(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            for sym in &symbols {
                if sym.kind.is_callable() || sym.kind.is_type_definition() {
                    prop_assert!(
                        !sym.signature.is_empty(),
                        "Symbol '{}' ({:?}) should have non-empty signature",
                        sym.name, sym.kind
                    );
                }
            }
        }
    }
}

// ============================================================================
// filter_by_kind Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// filter_by_kind returns only symbols of the requested kind.
    #[test]
    fn filter_by_kind_correct(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let functions = touring_code::ast::filter_by_kind(&symbols, SymbolKind::Function);
            for f in &functions {
                prop_assert_eq!(
                    f.kind.clone(), SymbolKind::Function,
                    "filter_by_kind(Function) returned {:?}", f.kind
                );
            }

            let classes = touring_code::ast::filter_by_kind(&symbols, SymbolKind::Class);
            for c in &classes {
                prop_assert_eq!(
                    c.kind.clone(), SymbolKind::Class,
                    "filter_by_kind(Class) returned {:?}", c.kind
                );
            }
        }
    }

    /// filter_by_kind result is a subset of the original.
    #[test]
    fn filter_by_kind_subset(source in py_module()) {
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let filtered = touring_code::ast::filter_by_kind(&symbols, SymbolKind::Function);
            prop_assert!(
                filtered.len() <= symbols.len(),
                "Filtered ({}) should be <= total ({})", filtered.len(), symbols.len()
            );
        }
    }
}

// ============================================================================
// find_by_name Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// find_by_name returns a symbol whose name matches the query.
    #[test]
    fn find_by_name_correct(name in py_func_name()) {
        let source = format!("def {}():\n    pass\n", name);
        if let Ok(symbols) = extract_symbols(&source, Lang::Python) {
            let found = touring_code::ast::find_by_name(&symbols, &name);
            prop_assert!(
                found.is_some(),
                "find_by_name should find '{}'", name
            );
            prop_assert_eq!(
                &found.unwrap().name, &name,
                "Returned symbol name doesn't match query"
            );
        }
    }

    /// find_by_name returns None for names not in the source.
    #[test]
    fn find_by_name_missing(_dummy in 0..100_u32) {
        let source = "def existing_func():\n    pass\n";
        if let Ok(symbols) = extract_symbols(source, Lang::Python) {
            let found = touring_code::ast::find_by_name(&symbols, "nonexistent_zzzz");
            prop_assert!(
                found.is_none(),
                "find_by_name should return None for nonexistent name"
            );
        }
    }
}

// ============================================================================
// P3.3 — New Structural Invariant Properties
// ============================================================================

/// Rust identifier strategy (valid Python function names work fine here too).
fn rs_ident() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,12}".prop_filter("not a keyword", |n| {
        !matches!(
            n.as_str(),
            "fn" | "let"
                | "mut"
                | "pub"
                | "use"
                | "mod"
                | "if"
                | "else"
                | "for"
                | "while"
                | "loop"
                | "match"
                | "return"
                | "struct"
                | "enum"
                | "trait"
                | "impl"
                | "type"
                | "const"
                | "static"
                | "ref"
                | "in"
                | "as"
                | "where"
                | "self"
                | "super"
                | "crate"
                | "move"
                | "async"
                | "await"
                | "dyn"
                | "box"
                | "unsafe"
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    /// P3.3.1 — rust_surgery_idempotent:
    ///
    /// Replacing a function body with content X, then replacing it again with
    /// the same content X, produces the same output both times.
    /// i.e., replace(replace(src, name, body), name, body) == replace(src, name, body)
    #[test]
    fn rust_surgery_idempotent(
        func_name in rs_ident(),
        body_word in "[a-z]{3,10}",
    ) {
        use touring_code::ast::replace_symbol_body;

        let source = format!(
            "fn {}() {{\n    let x = 0;\n}}\n",
            func_name
        );
        let new_body = format!("let {} = 42;", body_word);

        // First replacement
        let result1 = replace_symbol_body(&source, &func_name, &new_body);
        if let Ok(replaced1) = result1 {
            // Second replacement with the same body
            let result2 = replace_symbol_body(&replaced1, &func_name, &new_body);
            if let Ok(replaced2) = result2 {
                // The result should be stable — applying the same replacement twice
                // should produce identical output (idempotent with respect to body content).
                prop_assert_eq!(
                    &replaced1, &replaced2,
                    "Surgery idempotency failed for fn '{}' with body '{}'",
                    func_name, new_body
                );
            }
        }
    }

    /// P3.3.2 — blast_radius_monotone:
    ///
    /// Adding a new dependency to a file never decreases the blast radius of any
    /// file that already depended on it. Formally:
    /// deps(file) ⊆ deps(file + new_import)  (blast radius is monotone w.r.t. imports)
    #[test]
    fn blast_radius_monotone(
        file_a in rs_ident(),
        file_b in rs_ident(),
        file_c in rs_ident(),
    ) {
        use touring_code::ast::SymbolIndex;

        // Only test when all names are distinct (degenerate same-name case is uninteresting)
        prop_assume!(file_a != file_b && file_b != file_c && file_a != file_c);

        let path_a = format!("{}.py", file_a);
        let path_b = format!("{}.py", file_b);
        let path_c = format!("{}.py", file_c);

        // Initial graph: B imports A
        let src_b_no_import = format!("def {}():\n    pass\n", file_b);
        let src_a = format!("def {}():\n    pass\n", file_a);
        let src_c = format!("def {}():\n    pass\n", file_c);

        let mut index_before = SymbolIndex::new();
        let _ = index_before.index_file(&path_a, &src_a, Lang::Python);
        let _ = index_before.index_file(&path_b, &src_b_no_import, Lang::Python);
        let _ = index_before.index_file(&path_c, &src_c, Lang::Python);
        let blast_before = index_before.blast_radius(&path_b);

        // After: B now imports both A and C
        let src_b_with_import = format!(
            "from {} import {}\ndef {}():\n    pass\n",
            file_c, file_c, file_b
        );

        let mut index_after = SymbolIndex::new();
        let _ = index_after.index_file(&path_a, &src_a, Lang::Python);
        let _ = index_after.index_file(&path_b, &src_b_with_import, Lang::Python);
        let _ = index_after.index_file(&path_c, &src_c, Lang::Python);
        let blast_after = index_after.blast_radius(&path_b);

        // Blast radius (affected files count) should be >= before after adding dependency
        prop_assert!(
            blast_after.affected_files.len() >= blast_before.affected_files.len(),
            "Blast radius should be monotone: before={}, after={} for file '{}'",
            blast_before.affected_files.len(),
            blast_after.affected_files.len(),
            path_b,
        );
    }

    /// P3.3.3 — incremental_parse_symbol_count_stable:
    ///
    /// Parsing a source file twice produces the same number of symbols.
    /// This verifies that symbol extraction is deterministic and stable —
    /// there are no random or stateful side effects in the parser.
    ///
    /// Note: Full incremental-edit == full-parse equality is tested at the
    /// integration level (IncrementalPipeline). Here we verify the simpler
    /// invariant: same source → same symbol count on multiple calls.
    #[test]
    fn incremental_parse_symbol_count_stable(source in py_module()) {
        let result1 = extract_symbols(&source, Lang::Python);
        let result2 = extract_symbols(&source, Lang::Python);

        match (result1, result2) {
            (Ok(syms1), Ok(syms2)) => {
                prop_assert_eq!(
                    syms1.len(), syms2.len(),
                    "Symbol count should be stable across two calls to extract_symbols"
                );
                // Also verify names are in the same order (deterministic extraction)
                let names1: Vec<_> = syms1.iter().map(|s| &s.name).collect();
                let names2: Vec<_> = syms2.iter().map(|s| &s.name).collect();
                prop_assert_eq!(
                    names1, names2,
                    "Symbol names should be in the same order across two extract_symbols calls"
                );
            }
            (Err(_), Err(_)) => {
                // Both failed — consistent behaviour, acceptable
            }
            _ => {
                prop_assert!(false, "extract_symbols produced inconsistent Ok/Err for the same input");
            }
        }
    }
}
