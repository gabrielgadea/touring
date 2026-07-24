//! Integration tests for `touring_lsp`.
//!
//! These run via `cargo test -p touring-lsp`. Add scenario tests here that
//! exercise the public API end-to-end.

use touring_lsp::{Item, Result};

#[test]
fn integration_item_construction() {
    let it = Item::new("id1", "label1");
    assert_eq!(it.id, "id1");
    assert_eq!(it.label, "label1");
}

#[test]
fn integration_result_alias_compiles() -> Result<()> {
    let _it = Item::new("id2", "label2");
    Ok(())
}
