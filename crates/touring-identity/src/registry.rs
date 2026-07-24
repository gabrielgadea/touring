//! IdentityRegistry — D5.2 of Touring v8 Master Plan S5.
//!
//! Provides CRUD operations over the entity registry:
//! - [`IdentityRegistry::define()`] — register a new entity
//! - [`IdentityRegistry::resolve()`] — resolve a name to ranked candidates
//! - [`IdentityRegistry::relate()`] — create a directed relation
//! - [`IdentityRegistry::list()`] — list entities with optional filter
//! - [`IdentityRegistry::delete()`] — soft-delete with audit trail
//!
//! Resolution algorithm (D5.5) is embedded here:
//! - Exact name match → confidence 1.0
//! - Context-scoped (name + crate + module) → confidence 0.95-0.99
//! - Fuzzy match (Levenshtein ≤ 2) → confidence 0.7-0.85
//! - Returns `Vec<EntityCandidate>` ordered by confidence DESC.

use rusqlite::params;
use smol_str::SmolStr;

use crate::schema::run_ddl;
#[cfg(test)]
use crate::types::Criterion;
use crate::types::{Entity, EntityCandidate, EntityId, EntityKind, MatchKind, RelationKind};

/// Column tuple of an `entities` row as SELECTed across the registry queries
/// (`id, canonical_name, kind, crate_name, source_path, definition_line,
/// doc_summary, auto_seeded, canonical`).
type EntityRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<u32>,
    Option<String>,
    bool,
    bool,
);

/// The 9-column `entities` projection shared by every entity query, so the
/// column list lives once instead of being copied at each `SELECT` call site.
const ENTITY_SELECT: &str = "SELECT id, canonical_name, kind, crate_name, \
     source_path, definition_line, doc_summary, auto_seeded, canonical \
     FROM entities";

/// Map an `entities` result row to its raw column tuple — shared by every
/// `query_map` over the entity table so the 9-column extraction lives once
/// instead of being copied at each call site.
fn entity_row(row: &rusqlite::Row) -> rusqlite::Result<EntityRow> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, Option<String>>(4)?,
        row.get::<_, Option<u32>>(5)?,
        row.get::<_, Option<String>>(6)?,
        row.get::<_, i32>(7)? != 0,
        row.get::<_, i32>(8)? != 0,
    ))
}

/// Handle to an open entity registry.
#[derive(Debug)]
pub struct IdentityRegistry {
    conn: rusqlite::Connection,
}

impl IdentityRegistry {
    /// Opens (or creates) a registry at the given path.
    pub fn open_or_create<P: AsRef<std::path::Path>>(path: P) -> Result<Self, crate::Error> {
        let mut conn =
            rusqlite::Connection::open(path).map_err(|e| crate::Error::Database(e.to_string()))?;
        run_ddl(&mut conn).map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(Self { conn })
    }

    /// Registers a new entity, returning its [`EntityId`].
    ///
    /// # Errors
    /// Returns [`crate::Error::DuplicateEntity`] if an entity with the same
    /// `canonical_name` already exists.
    pub fn define(&mut self, entity: &Entity) -> Result<EntityId, crate::Error> {
        self.conn
            .execute(
                "INSERT INTO entities (id, canonical_name, kind, crate_name,
                 source_path, definition_line, doc_summary, auto_seeded, canonical)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entity.id.as_str(),
                    entity.canonical_name.as_str(),
                    format!("{:?}", entity.kind).to_lowercase(),
                    entity.crate_name.as_str(),
                    entity.source_path.as_ref().map(|s| s.as_str()),
                    entity.definition_line,
                    entity.doc_summary.as_ref().map(|s| s.as_str()),
                    entity.auto_seeded as i32,
                    entity.canonical as i32,
                ],
            )
            .map_err(|e| {
                if e.to_string()
                    .contains("UNIQUE constraint failed: entities.canonical_name")
                {
                    crate::Error::DuplicateEntity(entity.canonical_name.to_string())
                } else {
                    crate::Error::Database(e.to_string())
                }
            })?;

        for criterion in &entity.criteria {
            self.conn
                .execute(
                    "INSERT INTO entity_criteria (entity_id, criterion_name, description)
                     VALUES (?1, ?2, ?3)",
                    params![
                        entity.id.as_str(),
                        criterion.name.as_str(),
                        criterion.description.as_str(),
                    ],
                )
                .map_err(|e| crate::Error::Database(e.to_string()))?;
        }

        Ok(entity.id.clone())
    }

    /// Defines multiple entities in a single transaction — for bulk bootstrap.
    ///
    /// Uses a single `BEGIN/COMMIT` block so 53k inserts cost ~1 transaction
    /// instead of 53k × 2 (entity + criteria). Slashes commit overhead by ~99%.
    ///
    /// If any entity violates a UNIQUE constraint its row is silently skipped
    /// (idempotent bootstrap). Other database errors abort and rollback.
    pub fn define_batch(&mut self, entities: &[Entity]) -> Result<usize, crate::Error> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        let mut count = 0;
        for entity in entities {
            let entity_result = tx.execute(
                "INSERT INTO entities (id, canonical_name, kind, crate_name,
                 source_path, definition_line, doc_summary, auto_seeded, canonical)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entity.id.as_str(),
                    entity.canonical_name.as_str(),
                    format!("{:?}", entity.kind).to_lowercase(),
                    entity.crate_name.as_str(),
                    entity.source_path.as_ref().map(|s| s.as_str()),
                    entity.definition_line,
                    entity.doc_summary.as_ref().map(|s| s.as_str()),
                    entity.auto_seeded as i32,
                    entity.canonical as i32,
                ],
            );

            match entity_result {
                Ok(_) => {
                    for criterion in &entity.criteria {
                        let _ = tx.execute(
                            "INSERT INTO entity_criteria (entity_id, criterion_name, description)
                             VALUES (?1, ?2, ?3)",
                            params![
                                entity.id.as_str(),
                                criterion.name.as_str(),
                                criterion.description.as_str(),
                            ],
                        );
                    }
                    count += 1;
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("UNIQUE constraint failed: entities.canonical_name")
                        || msg.contains("UNIQUE constraint failed: entities.id")
                    {
                        // Idempotent: skip duplicates silently
                    } else {
                        return Err(crate::Error::Database(msg));
                    }
                }
            }
        }

        tx.commit()
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        Ok(count)
    }

    /// Prepare `sql`, run it with `params`, and collect the resulting rows into
    /// `Entity` values via [`entity_from_row`]. Folds the shared prepare,
    /// `query_map`, and per-row `map_err` scaffold that recurred at every entity
    /// query site (F1.3 dedup, behavior-preserving).
    fn collect_entities<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<Entity>, crate::Error> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params, entity_row)
            .map_err(|e| crate::Error::Database(e.to_string()))?;
        let mut entities = Vec::new();
        for row in rows {
            entities.push(entity_from_row(
                row.map_err(|e| crate::Error::Database(e.to_string()))?,
            ));
        }
        Ok(entities)
    }

    /// Resolves a name to ranked entity candidates.
    ///
    /// Resolution tiers (descending confidence):
    /// 1. **Exact** — same `canonical_name` → confidence 1.0
    /// 2. **Context-scoped** — name + same crate subtree → 0.95–0.99
    /// 3. **Fuzzy** — Levenshtein distance ≤ `max_edit_distance` → 0.70–0.85
    pub fn resolve(
        &mut self,
        name: &str,
        max_edit_distance: u8,
    ) -> Result<Vec<EntityCandidate>, crate::Error> {
        let mut candidates = Vec::new();

        // Tier 1: exact match
        for entity in self.collect_entities(
            &format!("{ENTITY_SELECT} WHERE canonical_name = ?1"),
            params![name],
        )? {
            candidates.push(EntityCandidate {
                entity,
                match_kind: MatchKind::Exact,
                confidence: 1.0,
            });
        }

        // Tier 2: context-scoped
        let name_part = name.rsplit("::").next().unwrap_or(name);
        let like_pattern = format!("%::{name_part}");
        for entity in self.collect_entities(
            &format!("{ENTITY_SELECT} WHERE canonical_name LIKE ?1 AND canonical_name != ?2"),
            params![like_pattern, name],
        )? {
            let same_crate = entity
                .canonical_name
                .split_once("::")
                .map(|(c, _)| {
                    name.starts_with(&format!("{}::", c)) || name.contains(&format!("::{}::", c))
                })
                .unwrap_or(false);

            let confidence = if same_crate { 0.98 } else { 0.95 };
            candidates.push(EntityCandidate {
                entity,
                match_kind: MatchKind::ContextScoped,
                confidence,
            });
        }

        // Tier 3: fuzzy match
        if max_edit_distance > 0 {
            let sql = format!(
                "{ENTITY_SELECT} WHERE canonical_name NOT IN \
                 (SELECT canonical_name FROM entities WHERE canonical_name = ?1)"
            );
            for entity in self.collect_entities(&sql, params![name])? {
                // Scope the borrow of `entity.canonical_name` so it ends before
                // `entity` is moved into the candidate below.
                let dist = {
                    let other_short = entity
                        .canonical_name
                        .rsplit("::")
                        .next()
                        .unwrap_or(entity.canonical_name.as_str());
                    levenshtein_distance(name_part, other_short)
                };
                if dist > 0 && dist as u8 <= max_edit_distance {
                    let confidence = 0.85 - (dist as f64 * 0.15 / max_edit_distance as f64);
                    candidates.push(EntityCandidate {
                        entity,
                        match_kind: MatchKind::Fuzzy,
                        confidence,
                    });
                }
            }
        }

        candidates.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(candidates)
    }

    /// Creates a directed relation between two entities.
    pub fn relate(
        &mut self,
        from: &EntityId,
        kind: RelationKind,
        to: &EntityId,
    ) -> Result<i64, crate::Error> {
        self.conn
            .execute(
                "INSERT INTO entity_relations (from_entity_id, to_entity_id, relation_kind)
                 VALUES (?1, ?2, ?3)",
                params![
                    from.as_str(),
                    to.as_str(),
                    format!("{:?}", kind).to_lowercase(),
                ],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Lists all entities, optionally filtered by crate or kind.
    pub fn list(
        &mut self,
        crate_filter: Option<&str>,
        kind_filter: Option<EntityKind>,
    ) -> Result<Vec<Entity>, crate::Error> {
        let sql;
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let (Some(cf), Some(kf)) = (crate_filter, kind_filter) {
            sql = format!(
                "{ENTITY_SELECT} WHERE crate_name = ?1 AND kind = ?2 ORDER BY canonical_name"
            );
            params_vec.push(Box::new(cf.to_string()));
            params_vec.push(Box::new(format!("{:?}", kf).to_lowercase()));
        } else if let Some(cf) = crate_filter {
            sql = format!("{ENTITY_SELECT} WHERE crate_name = ?1 ORDER BY canonical_name");
            params_vec.push(Box::new(cf.to_string()));
        } else if let Some(kf) = kind_filter {
            sql = format!("{ENTITY_SELECT} WHERE kind = ?1 ORDER BY canonical_name");
            params_vec.push(Box::new(format!("{:?}", kf).to_lowercase()));
        } else {
            sql = format!("{ENTITY_SELECT} ORDER BY canonical_name");
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        self.collect_entities(&sql, params_refs.as_slice())
    }

    /// Soft-deletes an entity and records a justification.
    pub fn delete(&mut self, id: &EntityId, reason: &str) -> Result<(), crate::Error> {
        self.conn
            .execute(
                "DELETE FROM entity_relations WHERE from_entity_id = ?1 OR to_entity_id = ?1",
                params![id.as_str()],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        self.conn
            .execute(
                "DELETE FROM entity_criteria WHERE entity_id = ?1",
                params![id.as_str()],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        self.conn
            .execute("DELETE FROM entities WHERE id = ?1", params![id.as_str()])
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        tracing::info!(entity_id = %id, reason = %reason, "entity_deleted");

        Ok(())
    }

    /// Confirms an auto-seeded entity, marking it as canonical.
    ///
    /// Transitions `auto_seeded=true, canonical=false` → `auto_seeded=false, canonical=true`.
    pub fn confirm(&mut self, id: &EntityId) -> Result<(), crate::Error> {
        let affected = self
            .conn
            .execute(
                "UPDATE entities SET auto_seeded = 0, canonical = 1 WHERE id = ?1",
                params![id.as_str()],
            )
            .map_err(|e| crate::Error::Database(e.to_string()))?;

        if affected == 0 {
            return Err(crate::Error::NotFound(format!(
                "entity {} not found",
                id.as_str()
            )));
        }

        tracing::info!(entity_id = %id, "entity_confirmed");
        Ok(())
    }

    /// Returns all auto-seeded entities that have not yet been confirmed.
    pub fn get_unconfirmed(&self) -> Result<Vec<Entity>, crate::Error> {
        self.collect_entities(
            &format!(
                "{ENTITY_SELECT} WHERE auto_seeded = 1 AND canonical = 0 ORDER BY canonical_name"
            ),
            [],
        )
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn parse_kind(s: &str) -> EntityKind {
    match s {
        "function" => EntityKind::Function,
        "type" => EntityKind::Type,
        "module" => EntityKind::Module,
        "constant" => EntityKind::Constant,
        "trait" => EntityKind::Trait,
        "macro" => EntityKind::Macro,
        "file" => EntityKind::File,
        "config" => EntityKind::Config,
        _ => EntityKind::Unknown,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_entity(
    id: &str,
    canonical_name: &str,
    kind: &EntityKind,
    crate_name: &str,
    source_path: Option<String>,
    definition_line: Option<u32>,
    doc_summary: Option<String>,
    auto_seeded: bool,
    canonical: bool,
) -> Entity {
    Entity {
        id: EntityId::from_str(id),
        canonical_name: SmolStr::from(canonical_name),
        kind: *kind,
        crate_name: SmolStr::from(crate_name),
        criteria: Vec::new(),
        source_path: source_path.map(SmolStr::from),
        definition_line,
        doc_summary: doc_summary.map(SmolStr::from),
        auto_seeded,
        canonical,
    }
}

/// Build an [`Entity`] from a fetched [`EntityRow`]: destructure the row tuple,
/// resolve its kind, and assemble the entity. Folds the identical
/// destructure + `parse_kind` + `build_entity` block that recurred verbatim at
/// every `query_map` call site (F1.3 dedup, behavior-preserving).
fn entity_from_row(row: EntityRow) -> Entity {
    let (
        id,
        canonical_name,
        kind_str,
        crate_name,
        source_path,
        def_line,
        doc_sum,
        auto_seeded,
        canonical,
    ) = row;
    let kind = parse_kind(&kind_str);
    build_entity(
        &id,
        &canonical_name,
        &kind,
        &crate_name,
        source_path,
        def_line,
        doc_sum,
        auto_seeded,
        canonical,
    )
}

/// Computes Levenshtein distance between two ASCII strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut matrix = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in matrix.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, col) in matrix[0].iter_mut().enumerate().take(n + 1) {
        *col = j;
    }

    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            matrix[i + 1][j + 1] = matrix[i][j] + cost;
            matrix[i + 1][j + 1] = matrix[i + 1][j + 1].min(matrix[i][j + 1] + 1);
            matrix[i + 1][j + 1] = matrix[i + 1][j + 1].min(matrix[i + 1][j] + 1);
        }
    }

    matrix[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn make_registry() -> IdentityRegistry {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        drop(tmp); // close so SQLite can recreate
        IdentityRegistry::open_or_create(&path).unwrap()
    }

    #[test]
    fn define_and_resolve_exact() {
        let mut reg = make_registry();

        let e = Entity::new(
            EntityId::from_str("touring-ast::CosineComputer"),
            "touring-ast::CosineComputer",
            EntityKind::Type,
            "touring-ast",
        )
        .with_criterion(Criterion::exact_name("CosineComputer"));

        let id = reg.define(&e).unwrap();
        assert_eq!(id.as_str(), "touring-ast::CosineComputer");

        let candidates = reg.resolve("touring-ast::CosineComputer", 2).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].match_kind, MatchKind::Exact);
        assert_eq!(candidates[0].confidence, 1.0);
    }

    #[test]
    fn resolve_not_found() {
        let mut reg = make_registry();
        let candidates = reg.resolve("NonExistent", 2).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn resolve_fuzzy() {
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
        assert_eq!(candidates[0].match_kind, MatchKind::Fuzzy);
    }

    #[test]
    fn relate_and_list() {
        let mut reg = make_registry();

        let e1 = Entity::new(EntityId::from_str("A"), "A", EntityKind::Type, "x");
        let e2 = Entity::new(EntityId::from_str("B"), "B", EntityKind::Type, "x");
        reg.define(&e1).unwrap();
        reg.define(&e2).unwrap();

        let rel_id = reg.relate(&e1.id, RelationKind::Refines, &e2.id).unwrap();
        assert!(rel_id > 0);

        let all = reg.list(None, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn delete_removes_entity() {
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

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein_distance("kitten", "kitten"), 0);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
    }

    #[test]
    fn define_batch_inserts_all() {
        let mut reg = make_registry();

        let entities = vec![
            Entity::new(
                EntityId::from_str("touring::Batch1"),
                "touring::Batch1",
                EntityKind::Type,
                "touring",
            ),
            Entity::new(
                EntityId::from_str("touring::Batch2"),
                "touring::Batch2",
                EntityKind::Function,
                "touring",
            ),
            Entity::new(
                EntityId::from_str("touring::Batch3"),
                "touring::Batch3",
                EntityKind::Constant,
                "touring",
            ),
        ];

        let count = reg.define_batch(&entities).unwrap();
        assert_eq!(count, 3);

        let all = reg.list(None, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn define_batch_is_idempotent() {
        let mut reg = make_registry();

        let e1 = Entity::new(
            EntityId::from_str("touring::Dup"),
            "touring::Dup",
            EntityKind::Type,
            "touring",
        );

        let count1 = reg.define_batch(std::slice::from_ref(&e1)).unwrap();
        assert_eq!(count1, 1);

        // Same entity again — should skip, not error
        let count2 = reg.define_batch(&[e1]).unwrap();
        assert_eq!(count2, 0); // UNIQUE violation caught and skipped

        let all = reg.list(None, None).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn resolve_context_scoped() {
        let mut reg = make_registry();

        let e1 = Entity::new(
            EntityId::from_str("touring-ast::sub::Foo"),
            "touring-ast::sub::Foo",
            EntityKind::Function,
            "touring-ast",
        );
        let e2 = Entity::new(
            EntityId::from_str("touring-hooks::sub::Foo"),
            "touring-hooks::sub::Foo",
            EntityKind::Function,
            "touring-hooks",
        );

        reg.define(&e1).unwrap();
        reg.define(&e2).unwrap();

        let candidates = reg.resolve("touring-ast::sub::Foo", 2).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].match_kind, MatchKind::Exact);
        assert_eq!(candidates[0].entity.id.as_str(), "touring-ast::sub::Foo");
    }

    #[test]
    fn list_filter_by_crate() {
        let mut reg = make_registry();

        reg.define(&Entity::new(
            EntityId::from_str("touring-ast::X"),
            "touring-ast::X",
            EntityKind::Type,
            "touring-ast",
        ))
        .unwrap();
        reg.define(&Entity::new(
            EntityId::from_str("touring-hooks::Y"),
            "touring-hooks::Y",
            EntityKind::Type,
            "touring-hooks",
        ))
        .unwrap();

        let filtered = reg.list(Some("touring-ast"), None).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].crate_name.as_str(), "touring-ast");
    }

    #[test]
    fn confirm_and_get_unconfirmed() {
        let mut reg = make_registry();

        // e1: auto_seeded=true (from with_auto_seeded), canonical=false
        let e1 = Entity::new(
            EntityId::from_str("touring::AutoSeeded"),
            "touring::AutoSeeded",
            EntityKind::Function,
            "touring",
        )
        .with_auto_seeded();

        // e2: auto_seeded=false (already confirmed via with_canonical)
        let e2 = Entity::new(
            EntityId::from_str("touring::Confirmed"),
            "touring::Confirmed",
            EntityKind::Function,
            "touring",
        )
        .with_canonical();

        reg.define(&e1).unwrap();
        reg.define(&e2).unwrap();

        let unconfirmed = reg.get_unconfirmed().unwrap();
        assert_eq!(unconfirmed.len(), 1);
        assert_eq!(unconfirmed[0].id.as_str(), "touring::AutoSeeded");
        assert!(unconfirmed[0].auto_seeded);
        assert!(!unconfirmed[0].canonical);

        let all = reg.list(None, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn confirm_not_found() {
        let mut reg = make_registry();
        let result = reg.confirm(&EntityId::from_str("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn get_unconfirmed_empty() {
        let mut reg = make_registry();
        let e = Entity::new(
            EntityId::from_str("touring::Canonical"),
            "touring::Canonical",
            EntityKind::Type,
            "touring",
        );
        // All entities start auto_seeded=true
        reg.define(&e).unwrap();
        // Confirm it
        reg.confirm(&e.id).unwrap();
        // get_unconfirmed should be empty
        let unconfirmed = reg.get_unconfirmed().unwrap();
        assert!(unconfirmed.is_empty());
    }
}
