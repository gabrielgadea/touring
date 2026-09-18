//! Integration tests for `touring_resilience`.
//!
//! These run via `cargo test -p touring-resilience`. Add scenario tests here that
//! exercise the public API end-to-end.

use touring_resilience::Item;

#[test]
fn integration_item_construction() {
    let it = Item::new("id1", "label1");
    assert_eq!(it.id, "id1");
    assert_eq!(it.label, "label1");
}
