//! Snapshot tests for `SynWiringGateAdapter::suggest_consumer_stub`.
//!
//! Exercises the synergy between the three always-on proc-macro deps:
//! - `syn`       — `parse_str::<Ident>` validates the orphan/crate names
//! - `quote`     — `quote!{}` builds the consumer body
//! - `proc-macro2` — `TokenStream` carries the emitted code
//!
//! A silent shift in any of these crates (grammar change in syn, format
//! tweak in quote's `ToTokens`, hash change in proc-macro2 Ident display)
//! flips the snapshot and forces `cargo insta review`.
//!
//! Review: `cargo insta review -p touring-generator`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::pedantic)]

use touring_generator::SynWiringGateAdapter;

fn stub_to_string(adapter: &SynWiringGateAdapter, name: &str, krate: Option<&str>) -> String {
    adapter.suggest_consumer_stub(name, krate).to_string()
}

#[test]
fn snapshot_consumer_stub_without_crate() {
    let adapter = SynWiringGateAdapter::new();
    let rendered = stub_to_string(&adapter, "FooWidget", None);
    insta::assert_snapshot!("consumer_stub_no_crate", rendered);
}

#[test]
fn snapshot_consumer_stub_with_snake_crate() {
    let adapter = SynWiringGateAdapter::new();
    let rendered = stub_to_string(&adapter, "FooWidget", Some("touring_ast"));
    insta::assert_snapshot!("consumer_stub_snake_crate", rendered);
}

#[test]
fn snapshot_consumer_stub_with_dashed_crate() {
    // `touring-ast` is a valid cargo crate name but not a valid Rust ident.
    // suggest_consumer_stub normalizes dashes to underscores.
    let adapter = SynWiringGateAdapter::new();
    let rendered = stub_to_string(&adapter, "FooWidget", Some("touring-ast"));
    insta::assert_snapshot!("consumer_stub_dashed_crate", rendered);
}

#[test]
fn consumer_stub_returns_empty_on_invalid_orphan_name() {
    let adapter = SynWiringGateAdapter::new();
    let rendered = stub_to_string(&adapter, "123-not-an-ident", None);
    assert!(
        rendered.is_empty(),
        "invalid orphan name must produce empty TokenStream, got `{rendered}`"
    );
}

#[test]
fn consumer_stub_is_deterministic() {
    // Same inputs → byte-identical output. Guards against nondeterministic
    // Ident/Span allocation drift in future syn/quote versions.
    let adapter = SynWiringGateAdapter::new();
    let a = stub_to_string(&adapter, "Widget", Some("touring_ast"));
    let b = stub_to_string(&adapter, "Widget", Some("touring_ast"));
    assert_eq!(a, b, "suggest_consumer_stub must be deterministic");
}
