//! `touring entity` — Entity Identity Registry CLI.
//!
//! D5.3 of Touring v8 Master Plan S5.
//!
//! Provides CRUD operations over the entity registry:
//! - `touring entity define <id> <name> <kind> <crate> [--source <path>] [--line <n>] [--doc <summary>]`
//! - `touring entity resolve <name> [--max-edit-distance <n>]`
//! - `touring entity relate <from> <kind> <to>`
//! - `touring entity list [--crate <name>] [--kind <kind>]`
//! - `touring entity delete <id> [--reason <text>]`

use std::path::PathBuf;
use touring_identity::{Entity, EntityId, EntityKind, IdentityRegistry, MatchKind, RelationKind};

/// CLI entry point for `touring entity` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");

    match subcommand {
        "define" => run_define(args),
        "resolve" => run_resolve(args),
        "relate" => run_relate(args),
        "list" => run_list(args),
        "delete" => run_delete(args),
        "bootstrap" => run_bootstrap(args),
        _ => {
            anyhow::bail!(
                "Unknown entity subcommand: {}. Use: define, resolve, relate, list, delete, bootstrap",
                subcommand
            );
        }
    }
}

// ── define ──────────────────────────────────────────────────────────────────

fn run_define(args: &[String]) -> anyhow::Result<()> {
    let id = args.get(3).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring entity define <id> <name> <kind> <crate>")
    })?;
    let name = args.get(4).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring entity define <id> <name> <kind> <crate>")
    })?;
    let kind_str = args.get(5).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring entity define <id> <name> <kind> <crate>")
    })?;
    let crate_name = args.get(6).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring entity define <id> <name> <kind> <crate>")
    })?;

    let kind = parse_kind(kind_str)?;
    let source_path = extract_flag(args, "--source");
    let definition_line = extract_flag(args, "--line").and_then(|s| s.parse::<u32>().ok());
    let doc_summary = extract_flag(args, "--doc");

    let mut entity = Entity::new(EntityId::from_str(id), name, kind, crate_name);

    if let Some(ref sp) = source_path {
        let line = definition_line.unwrap_or(0);
        entity = entity.with_source(sp, line);
    }
    if let Some(ref doc) = doc_summary {
        entity = entity.with_doc(doc);
    }

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    let id_out = reg.define(&entity).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "defined",
            "id": id_out.as_str(),
            "canonical_name": name,
            "kind": kind_str,
            "crate": crate_name,
        })
    );
    Ok(())
}

// ── resolve ─────────────────────────────────────────────────────────────────

fn run_resolve(args: &[String]) -> anyhow::Result<()> {
    let name = args.get(3).ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: touring entity resolve <name> [--max-edit-distance <n>] [--exact-only]"
        )
    })?;

    let max_edit = extract_flag(args, "--max-edit-distance")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(2);
    // Wire MatchKind into the CLI surface — `--exact-only` filters out fuzzy
    // matches so consumers can request high-confidence resolution only
    // (REGRA #0 potencializar: typed filter instead of unused import).
    let exact_only = args.iter().any(|a| a == "--exact-only");

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    let mut candidates = reg
        .resolve(name, max_edit)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    if exact_only {
        candidates.retain(|c| matches!(c.match_kind, MatchKind::Exact));
    }

    let output: serde_json::Value = if candidates.is_empty() {
        serde_json::json!({
            "status": "not_found",
            "name": name,
            "candidates": [],
        })
    } else {
        serde_json::json!({
            "status": "found",
            "name": name,
            "candidates": candidates.iter().map(|c| {
                serde_json::json!({
                    "id": c.entity.id.as_str(),
                    "canonical_name": c.entity.canonical_name.as_str(),
                    "kind": format!("{:?}", c.entity.kind).to_lowercase(),
                    "crate_name": c.entity.crate_name.as_str(),
                    "source_path": c.entity.source_path.as_ref().map(|s| s.as_str()),
                    "definition_line": c.entity.definition_line,
                    "doc_summary": c.entity.doc_summary.as_ref().map(|s| s.as_str()),
                    "match_kind": format!("{:?}", c.match_kind).to_lowercase(),
                    "confidence": c.confidence,
                })
            }).collect::<Vec<_>>(),
        })
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── relate ───────────────────────────────────────────────────────────────────

fn run_relate(args: &[String]) -> anyhow::Result<()> {
    let from = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("Usage: touring entity relate <from> <kind> <to>"))?;
    let kind_str = args
        .get(4)
        .ok_or_else(|| anyhow::anyhow!("Usage: touring entity relate <from> <kind> <to>"))?;
    let to = args
        .get(5)
        .ok_or_else(|| anyhow::anyhow!("Usage: touring entity relate <from> <kind> <to>"))?;

    let kind = parse_relation_kind(kind_str)?;

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    let rel_id = reg
        .relate(&EntityId::from_str(from), kind, &EntityId::from_str(to))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "related",
            "from": from,
            "relation": kind_str,
            "to": to,
            "relation_id": rel_id,
        })
    );
    Ok(())
}

// ── list ────────────────────────────────────────────────────────────────────

fn run_list(args: &[String]) -> anyhow::Result<()> {
    let crate_filter = extract_flag(args, "--crate");
    let kind_filter = extract_flag(args, "--kind").and_then(|s| parse_kind(&s).ok());

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    let entities = reg
        .list(crate_filter.as_deref(), kind_filter)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "count": entities.len(),
            "entities": entities.iter().map(|e| {
                serde_json::json!({
                    "id": e.id.as_str(),
                    "canonical_name": e.canonical_name.as_str(),
                    "kind": format!("{:?}", e.kind).to_lowercase(),
                    "crate_name": e.crate_name.as_str(),
                    "source_path": e.source_path.as_ref().map(|s| s.as_str()),
                    "definition_line": e.definition_line,
                    "doc_summary": e.doc_summary.as_ref().map(|s| s.as_str()),
                })
            }).collect::<Vec<_>>(),
        })
    );
    Ok(())
}

// ── delete ─────────────────────────────────────────────────────────────────

fn run_delete(args: &[String]) -> anyhow::Result<()> {
    let id = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("Usage: touring entity delete <id> [--reason <text>]"))?;
    let reason = extract_flag(args, "--reason").unwrap_or_else(|| "no reason given".to_string());

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    reg.delete(&EntityId::from_str(id), &reason)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "deleted",
            "id": id,
            "reason": reason,
        })
    );
    Ok(())
}

// ── bootstrap ────────────────────────────────────────────────────────────────

fn run_bootstrap(args: &[String]) -> anyhow::Result<()> {
    let symbols_db: PathBuf = extract_flag(args, "--from")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var("TOURING_SYMBOLS_DB")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let uid = unsafe { super::libc_getuid() };
                    PathBuf::from(format!("/run/user/{uid}/touring/symbols.db"))
                })
        });

    let limit: Option<usize> = extract_flag(args, "--limit").and_then(|s| s.parse::<usize>().ok());

    let dry_run = args.contains(&"--dry-run".to_string());

    eprintln!("[bootstrap] Opening symbols DB: {:?}", symbols_db);
    let sym_conn = rusqlite::Connection::open(&symbols_db)
        .map_err(|e| anyhow::anyhow!("Cannot open symbols DB: {}", e))?;

    let count: i64 = sym_conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))
        .map_err(|e| anyhow::anyhow!("Cannot count symbols: {}", e))?;
    eprintln!("[bootstrap] Found {} symbols in source index", count);

    let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();
    let mut stmt = sym_conn
        .prepare(&format!(
            "SELECT name, file_path, line FROM symbols \
             WHERE name NOT LIKE '$%' AND name NOT LIKE '.%' \
             ORDER BY access_count DESC, name ASC{}",
            limit_clause
        ))
        .map_err(|e| anyhow::anyhow!("Cannot prepare symbols query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| anyhow::anyhow!("Cannot query symbols: {}", e))?;

    let mut entities: Vec<Entity> = Vec::new();
    for row in rows {
        let (name, file_path, line) = row.map_err(|e| anyhow::anyhow!("Row error: {}", e))?;

        let crate_name = extract_crate_name(&file_path);
        let kind = infer_kind_from_name(&name);
        if matches!(kind, EntityKind::Unknown) {
            continue;
        }
        let id_str = format!("{}::{}", crate_name, name);
        let entity = Entity::new(EntityId::from_str(&id_str), &id_str, kind, &crate_name)
            .with_source(&file_path, line as u32)
            .with_auto_seeded(); // D5.8: bootstrap entities are auto-seeded

        entities.push(entity);
    }

    if dry_run {
        println!(
            "{}",
            serde_json::json!({
                "status": "dry_run",
                "would_define": entities.len(),
                "sample": entities.iter().take(5).map(|e| {
                    serde_json::json!({
                        "id": e.id.as_str(),
                        "kind": format!("{:?}", e.kind).to_lowercase(),
                        "crate_name": e.crate_name.as_str(),
                    })
                }).collect::<Vec<_>>(),
            })
        );
        return Ok(());
    }

    let db_path = default_db_path()?;
    let mut reg = IdentityRegistry::open_or_create(&db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open registry: {}", e))?;

    let count = reg
        .define_batch(&entities)
        .map_err(|e| anyhow::anyhow!("Batch insert failed: {}", e))?;

    println!(
        "{}",
        serde_json::json!({
            "status": "bootstrapped",
            "entities_defined": count,
            "source_db": symbols_db.to_string_lossy(),
            "registry_db": db_path.to_string_lossy(),
        })
    );
    Ok(())
}

/// Infers [`EntityKind`] from the symbol name using naming conventions.
fn infer_kind_from_name(name: &str) -> EntityKind {
    let s = name;

    // Skip names that are clearly not valid identifiers
    if s.is_empty() || s.starts_with('$') || s.starts_with('.') {
        return EntityKind::Unknown;
    }

    // SCREAMING_SNAKE_CASE → Constant
    if s.chars()
        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
    {
        return EntityKind::Constant;
    }
    // CamelCase → Type (struct/enum)
    if s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) && !s.contains("::") {
        return EntityKind::Type;
    }
    // snake_case or camelCase → Function
    if s.contains('_') || s.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
        return EntityKind::Function;
    }

    EntityKind::Unknown
}

/// Extracts the crate name from a file path like
/// `crates/touring-ast/src/semantic_search.rs` → `touring-ast`.
fn extract_crate_name(file_path: &str) -> String {
    let path = PathBuf::from(file_path);
    let components: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();

    // Try crates/<name>/ pattern
    if components.first().map(|c| c.as_str()) == Some("crates") && components.len() >= 2 {
        return components[1].clone();
    }

    // Fallback: parent directory of 'src'
    if let Some(pos) = components.iter().position(|c| c == "src") {
        if pos > 0 {
            let parent = &components[pos - 1];
            if parent != "src" {
                return parent.clone();
            }
        }
    }

    // Absolute path fallback: last directory component
    if !components.is_empty() {
        let last = &components[components.len() - 1];
        if last != "src" && last != "lib" && last != "main.rs" {
            return last.clone();
        }
        if components.len() >= 2 {
            return components[components.len() - 2].clone();
        }
    }

    "unknown".to_string()
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn default_db_path() -> anyhow::Result<PathBuf> {
    let uid = unsafe { super::libc_getuid() };
    let base = std::env::var("TOURING_IDENTITY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("TOURING_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(format!("/tmp/touring-identity-{uid}")))
        });
    std::fs::create_dir_all(&base)?;
    Ok(base.join("registry.db"))
}

fn extract_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|s| s == flag)
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| {
            let prefixed = format!("{}=", flag);
            args.iter().find(|s| s.starts_with(&prefixed)).map(|s| {
                s.strip_prefix(&prefixed)
                    .expect("guarded by starts_with(&prefixed)")
                    .to_string()
            })
        })
}

fn parse_kind(s: &str) -> anyhow::Result<EntityKind> {
    match s.to_lowercase().as_str() {
        "function" => Ok(EntityKind::Function),
        "type" => Ok(EntityKind::Type),
        "module" => Ok(EntityKind::Module),
        "constant" => Ok(EntityKind::Constant),
        "trait" => Ok(EntityKind::Trait),
        "macro" => Ok(EntityKind::Macro),
        "file" => Ok(EntityKind::File),
        "config" => Ok(EntityKind::Config),
        _ => anyhow::bail!(
            "Unknown entity kind: {}. Valid: function, type, module, constant, trait, macro, file, config",
            s
        ),
    }
}

fn parse_relation_kind(s: &str) -> anyhow::Result<RelationKind> {
    match s.to_lowercase().as_str() {
        "derived_from" => Ok(RelationKind::DerivedFrom),
        "refines" => Ok(RelationKind::Refines),
        "supersedes" => Ok(RelationKind::Supersedes),
        "equivalent" => Ok(RelationKind::Equivalent),
        "see_also" => Ok(RelationKind::SeeAlso),
        "wraps" => Ok(RelationKind::Wraps),
        _ => anyhow::bail!(
            "Unknown relation kind: {}. Valid: derived_from, refines, supersedes, equivalent, see_also, wraps",
            s
        ),
    }
}
