//! D5.8 integration tests — bootstrap workflow + confirm/get_unconfirmed.
//!
//! Tests the D5.8 auto-seeding and confirmation flow:
//! - `define_batch` marks entities as auto_seeded=true
//! - `get_unconfirmed` returns only auto_seeded=true AND canonical=false
//! - `confirm` flips canonical to true
//! - After confirm, entity is no longer in get_unconfirmed list

use tempfile::NamedTempFile;
use touring_identity::{Entity, EntityId, EntityKind, IdentityRegistry};

fn make_registry() -> IdentityRegistry {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    drop(tmp);
    IdentityRegistry::open_or_create(&path).unwrap()
}

#[test]
fn d58_bootstrap_marks_auto_seeded() {
    let mut reg = make_registry();

    // Bootstrap-style batch insert (auto_seeded=true)
    let entities = vec![
        Entity::new(
            EntityId::from_str("touring-ast::CosineComputer"),
            "touring-ast::CosineComputer",
            EntityKind::Type,
            "touring-ast",
        )
        .with_auto_seeded(),
        Entity::new(
            EntityId::from_str("touring-ast::SemanticSearch"),
            "touring-ast::SemanticSearch",
            EntityKind::Function,
            "touring-ast",
        )
        .with_auto_seeded(),
    ];

    let count = reg.define_batch(&entities).unwrap();
    assert_eq!(count, 2);

    // get_unconfirmed should return both
    let unconfirmed = reg.get_unconfirmed().unwrap();
    assert_eq!(unconfirmed.len(), 2);
    assert!(unconfirmed.iter().all(|e| e.auto_seeded && !e.canonical));
}

#[test]
fn d58_confirm_makes_canonical() {
    let mut reg = make_registry();

    let entity = Entity::new(
        EntityId::from_str("touring::MyEntity"),
        "touring::MyEntity",
        EntityKind::Type,
        "touring",
    )
    .with_auto_seeded();

    reg.define(&entity).unwrap();

    // Initially unconfirmed
    let unconfirmed = reg.get_unconfirmed().unwrap();
    assert_eq!(unconfirmed.len(), 1);

    // Confirm it
    reg.confirm(&entity.id).unwrap();

    // Now no longer unconfirmed
    let unconfirmed = reg.get_unconfirmed().unwrap();
    assert!(unconfirmed.is_empty());

    // And it's still resolvable
    let candidates = reg.resolve("touring::MyEntity", 2).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].entity.canonical);
    assert!(!candidates[0].entity.auto_seeded);
}

#[test]
fn d58_confirm_nonexistent_returns_error() {
    let mut reg = make_registry();
    let result = reg.confirm(&EntityId::from_str("nonexistent::Entity"));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        touring_identity::Error::NotFound(_)
    ));
}

#[test]
fn d58_list_shows_both_flags() {
    let mut reg = make_registry();

    let auto_seeded = Entity::new(
        EntityId::from_str("touring::AutoSeeded"),
        "touring::AutoSeeded",
        EntityKind::Function,
        "touring",
    )
    .with_auto_seeded();

    let canonical = Entity::new(
        EntityId::from_str("touring::Canonical"),
        "touring::Canonical",
        EntityKind::Type,
        "touring",
    )
    .with_canonical();

    reg.define(&auto_seeded).unwrap();
    reg.define(&canonical).unwrap();

    let all = reg.list(None, None).unwrap();
    assert_eq!(all.len(), 2);

    let auto_seeded_found = all
        .iter()
        .find(|e| e.id.as_str() == "touring::AutoSeeded")
        .unwrap();
    let canonical_found = all
        .iter()
        .find(|e| e.id.as_str() == "touring::Canonical")
        .unwrap();

    assert!(auto_seeded_found.auto_seeded && !auto_seeded_found.canonical);
    assert!(!canonical_found.auto_seeded && canonical_found.canonical);
}
