//! D5.10 — Pilot integration: touring-semantics public API entities.
//!
//! This module adds 3 entities for the core touring-semantics public API:
//! - `touring_semantics::Definition` — unified symbol definition
//! - `touring_semantics::DefinitionId` — stable identifier for definitions
//! - `touring_semantics::Semantics` — facade for definition resolution
//!
//! After adding these entities, `touring entity resolve <name>` can find them
//! and VGP can reference them by EntityId in contracts.

use touring_identity::{Entity, EntityId, EntityKind, IdentityRegistry};

fn make_registry() -> IdentityRegistry {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    drop(tmp);
    IdentityRegistry::open_or_create(&path).unwrap()
}

#[test]
fn d510_define_semantics_entities() {
    let mut reg = make_registry();

    // Entity 1: Definition — the central unified enum
    let definition = Entity::new(
        EntityId::from_str("touring_semantics::Definition"),
        "touring_semantics::Definition",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_doc("Unified enum for all symbol kinds across languages")
    .with_canonical();

    // Entity 2: DefinitionId — stable identifier
    let definition_id = Entity::new(
        EntityId::from_str("touring_semantics::DefinitionId"),
        "touring_semantics::DefinitionId",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_doc("Stable identifier for definitions")
    .with_canonical();

    // Entity 3: Semantics — the facade
    let semantics = Entity::new(
        EntityId::from_str("touring_semantics::Semantics"),
        "touring_semantics::Semantics",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_doc("Facade for resolving definitions from syntax nodes")
    .with_canonical();

    reg.define(&definition).unwrap();
    reg.define(&definition_id).unwrap();
    reg.define(&semantics).unwrap();

    // All 3 resolvable — exact-only to avoid substring matches (DefinitionId contains "Definition")
    let def_candidates = reg.resolve("touring_semantics::Definition", 2).unwrap();
    let exact_def = def_candidates
        .iter()
        .find(|c| c.match_kind == touring_identity::MatchKind::Exact);
    assert!(exact_def.is_some(), "Definition should resolve exact");

    let id_candidates = reg.resolve("touring_semantics::DefinitionId", 2).unwrap();
    let exact_id = id_candidates
        .iter()
        .find(|c| c.match_kind == touring_identity::MatchKind::Exact);
    assert!(exact_id.is_some(), "DefinitionId should resolve exact");

    let sem_candidates = reg.resolve("touring_semantics::Semantics", 2).unwrap();
    let exact_sem = sem_candidates
        .iter()
        .find(|c| c.match_kind == touring_identity::MatchKind::Exact);
    assert!(exact_sem.is_some(), "Semantics should resolve exact");
}

#[test]
fn d510_list_filters_by_crate() {
    let mut reg = make_registry();

    let def = Entity::new(
        EntityId::from_str("touring_semantics::Definition"),
        "touring_semantics::Definition",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_canonical();

    let other = Entity::new(
        EntityId::from_str("touring_ast::Node"),
        "touring_ast::Node",
        EntityKind::Type,
        "touring_ast",
    )
    .with_canonical();

    reg.define(&def).unwrap();
    reg.define(&other).unwrap();

    let semantics_only = reg.list(Some("touring_semantics"), None).unwrap();
    assert_eq!(semantics_only.len(), 1);
    assert_eq!(
        semantics_only[0].id.as_str(),
        "touring_semantics::Definition"
    );
}

#[test]
fn d510_entity_relation_defines_depends_on() {
    let mut reg = make_registry();

    let def = Entity::new(
        EntityId::from_str("touring_semantics::Definition"),
        "touring_semantics::Definition",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_canonical();

    let id = Entity::new(
        EntityId::from_str("touring_semantics::DefinitionId"),
        "touring_semantics::DefinitionId",
        EntityKind::Type,
        "touring_semantics",
    )
    .with_canonical();

    reg.define(&def).unwrap();
    reg.define(&id).unwrap();

    // Definition depends on DefinitionId (has_a relationship)
    let rel_id = reg
        .relate(&def.id, touring_identity::RelationKind::Refines, &id.id)
        .unwrap();

    assert!(rel_id > 0);
}
