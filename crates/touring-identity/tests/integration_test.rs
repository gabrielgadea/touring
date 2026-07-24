//! Integration tests for `touring_identity`.
//!
//! These run via `cargo test -p touring-identity`. Add scenario tests here that
//! exercise the public API end-to-end.

use touring_identity::{Entity, EntityId, EntityKind, IdentityRegistry};

fn make_registry() -> IdentityRegistry {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    drop(tmp);
    IdentityRegistry::open_or_create(&path).unwrap()
}

#[test]
fn integration_define_and_resolve_exact() {
    let mut reg = make_registry();

    let e = Entity::new(
        EntityId::from_str("touring-ast::CosineComputer"),
        "touring-ast::CosineComputer",
        EntityKind::Type,
        "touring-ast",
    );

    let id = reg.define(&e).unwrap();
    assert_eq!(id.as_str(), "touring-ast::CosineComputer");

    let candidates = reg.resolve("touring-ast::CosineComputer", 2).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].match_kind, touring_identity::MatchKind::Exact);
    assert_eq!(candidates[0].confidence, 1.0);
}

#[test]
fn integration_resolve_not_found() {
    let mut reg = make_registry();
    let candidates = reg.resolve("NonExistent", 2).unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn integration_resolve_fuzzy() {
    let mut reg = make_registry();

    let e = Entity::new(
        EntityId::from_str("touring::FooBar"),
        "touring::FooBar",
        EntityKind::Function,
        "touring",
    );
    reg.define(&e).unwrap();

    let candidates = reg.resolve("touring::FooBaz", 2).unwrap();
    assert!(!candidates.is_empty());
    assert_eq!(candidates[0].match_kind, touring_identity::MatchKind::Fuzzy);
}

#[test]
fn integration_relate_and_list() {
    let mut reg = make_registry();

    let e1 = Entity::new(EntityId::from_str("A"), "A", EntityKind::Type, "x");
    let e2 = Entity::new(EntityId::from_str("B"), "B", EntityKind::Type, "x");
    reg.define(&e1).unwrap();
    reg.define(&e2).unwrap();

    let rel_id = reg
        .relate(&e1.id, touring_identity::RelationKind::Refines, &e2.id)
        .unwrap();
    assert!(rel_id > 0);

    let all = reg.list(None, None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn integration_delete_removes_entity() {
    let mut reg = make_registry();

    let e = Entity::new(
        EntityId::from_str("touring::ToDelete"),
        "touring::ToDelete",
        EntityKind::Constant,
        "touring",
    );
    reg.define(&e).unwrap();

    reg.delete(&e.id, "test deletion").unwrap();

    let candidates = reg.resolve("touring::ToDelete", 2).unwrap();
    assert!(candidates.is_empty());
}
