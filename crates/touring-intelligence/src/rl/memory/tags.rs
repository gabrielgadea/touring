// #tags: kind:module lang:rust domain:memory purpose:faceted-tagging process:auto-tagging status:experimental
//! Faceted tag vocabulary, parser and linter for the memory hashtag library.
//!
//! Design (bundle `docs/plans/2026-08-11-memory-hashtag-library/`):
//! every memory carries namespaced tags `#facet:value` across orthogonal
//! facets — what it IS (`kind`), what it is FOR (`purpose`, aligning with the
//! portfolio `intent`), language/format (`lang`), subsystem (`domain`),
//! process object (`process`), generated-artifact type (`artifact`) and
//! maturity (`status`). This is Ranganathan's faceted classification (PMEST,
//! ISO 25964) applied to agent memory: free-form folksonomy collapses under
//! synonymy/polysemy/homonymy, so each facet has a controlled-but-extensible
//! vocabulary — unseeded values lint as WARN, never as hard errors (the
//! vocabulary grows by governance, not by accident). A-MEM (arXiv
//! 2502.12110) validates the model academically; Graphiti's `Node.labels` +
//! metadata filtering validates it in production.
//!
//! Tags live inline in generated content (PEP 350 codetag convention:
//! `// #tags: kind:snippet lang:rust purpose:…` on the first line of a
//! snippet; `#facet:value` inline or frontmatter `tags:` in docs) and are
//! mirrored into `memory_tags` / `tags_fts` (see `rlm.rs::ensure_tag_tables`)
//! so retrieval can fuse exact facet filters with BM25 + embeddings.

use std::fmt;

/// DDL of the hashtag-library tables — the SINGLE source of truth consumed by
/// both `rlm.rs::ensure_tag_tables` (in-process path) and the RPC handlers in
/// `touring-hook-runtime` (CLI path). Adding a second hand-written copy of
/// this DDL is the known "five sites write the same edge" failure mode; add
/// columns/tables HERE instead.
pub const TAG_TABLES_DDL: &str =
    "CREATE TABLE IF NOT EXISTS memory_tags (
        entry_key TEXT NOT NULL,
        facet TEXT NOT NULL,
        value TEXT NOT NULL,
        full_tag TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'auto',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (entry_key, full_tag)
    );
    CREATE INDEX IF NOT EXISTS idx_memory_tags_facet_value
        ON memory_tags(facet, value);
    CREATE INDEX IF NOT EXISTS idx_memory_tags_full
        ON memory_tags(full_tag);

    CREATE TABLE IF NOT EXISTS memory_links (
        id TEXT PRIMARY KEY,
        src TEXT NOT NULL,
        dst TEXT NOT NULL,
        rel TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_memory_links_src ON memory_links(src);
    CREATE INDEX IF NOT EXISTS idx_memory_links_dst ON memory_links(dst);";

/// FTS5 sidecar over `memory_tags` plus its sync triggers — same
/// external-content pattern as `memories_fts` (triggers recreated every open,
/// so this batch is safe to run on every startup).
pub const TAG_FTS_DDL: &str =
    "CREATE VIRTUAL TABLE IF NOT EXISTS tags_fts USING fts5(
        full_tag, facet, value,
        content='memory_tags',
        content_rowid='rowid'
    );
    CREATE TRIGGER IF NOT EXISTS memory_tags_fts_ai
    AFTER INSERT ON memory_tags BEGIN
        INSERT INTO tags_fts(rowid, full_tag, facet, value)
        VALUES (new.rowid, new.full_tag, new.facet, new.value);
    END;
    CREATE TRIGGER IF NOT EXISTS memory_tags_fts_ad
    AFTER DELETE ON memory_tags BEGIN
        INSERT INTO tags_fts(tags_fts, rowid, full_tag, facet, value)
        VALUES ('delete', old.rowid, old.full_tag, old.facet, old.value);
    END;
    CREATE TRIGGER IF NOT EXISTS memory_tags_fts_au
    AFTER UPDATE ON memory_tags BEGIN
        INSERT INTO tags_fts(tags_fts, rowid, full_tag, facet, value)
        VALUES ('delete', old.rowid, old.full_tag, old.facet, old.value);
        INSERT INTO tags_fts(rowid, full_tag, facet, value)
        VALUES (new.rowid, new.full_tag, new.facet, new.value);
    END;";

/// Extension → seeded `lang` value (lookup table keeps `lang_from_path`
/// cyclomatic-flat and makes new mappings a data edit, not a code edit).
pub(crate) const EXT_TO_LANG: &[(&str, &str)] = &[
    ("rs", "rust"),
    ("py", "python"),
    ("sh", "bash"),
    ("bash", "bash"),
    ("md", "md"),
    ("markdown", "md"),
    ("toml", "toml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("json", "json"),
    ("sql", "sql"),
    ("svg", "svg"),
    ("ts", "ts"),
    ("js", "js"),
];

/// Maps a file extension to the seeded `lang` value it proves.
fn lang_from_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    let lower = ext.to_ascii_lowercase();
    EXT_TO_LANG
        .iter()
        .find(|(e, _)| *e == lower)
        .map(|(_, lang)| *lang)
}

/// Resolves an `entry_type` to a seeded `kind` value: exact match first,
/// then boundary-aware affix match (`transcript_lesson` → `lesson` via
/// `_lesson`, never `lesson` inside an unrelated word).
fn kind_from_entry_type(et: &str) -> Option<&'static str> {
    let seeds = Facet::Kind.seed_values();
    seeds
        .iter()
        .find(|s| et == **s)
        .or_else(|| {
            seeds.iter().find(|s| {
                et.ends_with(format!("_{s}").as_str()) || et.starts_with(format!("{s}_").as_str())
            })
        })
        .copied()
}

/// Collects seeded domain tokens present in `hay` (already lowercased).
fn domains_in(hay: &str, push: &mut impl FnMut(Facet, String)) {
    let normalized = hay.replace(['/', '-', '_', '.', ':'], " ");
    for seg in normalized.split_whitespace() {
        if Facet::Domain.seed_values().contains(&seg) {
            push(Facet::Domain, seg.to_string());
        }
    }
}

/// Derives the deterministic facets of an entry at store time — the F1
/// auto-tagging pass. Only tags that are *provable* from the inputs are
/// emitted (no LLM guessing here; the authoring agent attaches
/// `purpose`/`domain`-level nuance via explicit `--tag`):
///
/// * `lang` ← `file_path` extension (rs→rust, py→python, …)
/// * `kind` ← `entry_type` when it is already a seeded kind value
/// * `domain` ← path segments matching the domain seed vocabulary
/// * `status:stable` — the default maturity for anything that reached the store
///
/// The key itself is scanned for domain hints after the path, so memories
/// without a file still classify (e.g. key `lesson:wiring:…` → `domain:wiring`).
pub fn derive_tags(key: &str, entry_type: &str, file_path: Option<&str>) -> Vec<ParsedTag> {
    let mut out: Vec<ParsedTag> = Vec::new();
    let push = |facet: Facet, value: String, out: &mut Vec<ParsedTag>| {
        let full = format!("{}:{}", facet.as_str(), value);
        if !out.iter().any(|t: &ParsedTag| t.full_tag == full) {
            out.push(ParsedTag {
                facet,
                value,
                full_tag: full,
            });
        }
    };

    if let Some(lang) = file_path.and_then(lang_from_path) {
        push(Facet::Lang, lang.to_string(), &mut out);
    }

    let et = entry_type.trim().to_ascii_lowercase();
    if let Some(kind) = kind_from_entry_type(&et) {
        push(Facet::Kind, kind.to_string(), &mut out);
    } else {
        // Fallback: the key's first `:`-segment often IS the kind
        // (`lesson:wiring:…`, `outcome:bash:…`) even when entry_type is
        // free-form.
        let prefix = key.split(':').next().unwrap_or("").to_ascii_lowercase();
        if Facet::Kind.seed_values().contains(&prefix.as_str()) {
            push(Facet::Kind, prefix, &mut out);
        }
    }

    let hay = format!("{} {}", file_path.unwrap_or(""), key).to_ascii_lowercase();
    domains_in(&hay, &mut |f, v| push(f, v, &mut out));

    push(Facet::Status, "stable".to_string(), &mut out);
    out
}

/// The canonical facets (controlled vocabulary). Order is the citation order
/// used when rendering a tag set for humans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Facet {
    /// What the memory IS: snippet, doc, script, map, plan, strategy, lesson,
    /// gotcha, decision, process.
    Kind,
    /// What it is FOR — aligns with the portfolio `intent` facet.
    Purpose,
    /// Language or format: rust, python, bash, md, toml, svg, …
    Lang,
    /// Subsystem or project domain: wiring, memory, index, hooks, adw, ceg, …
    Domain,
    /// Process the memory is an object of: diagnose, explore, converge, …
    Process,
    /// Generated-artifact type: map, dashboard, report, diorama, …
    Artifact,
    /// Maturity: stable, experimental, deprecated.
    Status,
}

impl Facet {
    /// Every canonical facet, in citation order.
    pub const ALL: [Facet; 7] = [
        Facet::Kind,
        Facet::Purpose,
        Facet::Lang,
        Facet::Domain,
        Facet::Process,
        Facet::Artifact,
        Facet::Status,
    ];

    /// Canonical lowercase name as it appears in `#facet:value`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Facet::Kind => "kind",
            Facet::Purpose => "purpose",
            Facet::Lang => "lang",
            Facet::Domain => "domain",
            Facet::Process => "process",
            Facet::Artifact => "artifact",
            Facet::Status => "status",
        }
    }

    /// Case-insensitive lookup with a small alias table. Aliases are the
    /// governed escape hatch: they map observed variants onto the canonical
    /// facet instead of letting a parallel vocabulary emerge (the folksonomy
    /// failure mode this design exists to prevent).
    pub fn from_str_ci(raw: &str) -> Option<Facet> {
        let norm = raw.trim().to_ascii_lowercase();
        Some(match norm.as_str() {
            "kind" | "type" | "tipo" => Facet::Kind,
            "purpose" | "intent" | "goal" | "propósito" | "proposito" => Facet::Purpose,
            "lang" | "language" | "format" | "fmt" => Facet::Lang,
            "domain" | "subsystem" | "area" | "dominio" => Facet::Domain,
            "process" | "processo" | "workflow" | "phase" => Facet::Process,
            "artifact" | "artefato" | "output" | "deliverable" => Facet::Artifact,
            "status" | "maturity" | "state" => Facet::Status,
            _ => return None,
        })
    }

    /// Seed values per facet. NOT exhaustive: a value outside this list lints
    /// as [`TagViolation::UnseededValue`] (WARN) so the vocabulary can grow
    /// deliberately, while unknown *facets* are hard errors.
    pub fn seed_values(self) -> &'static [&'static str] {
        match self {
            Facet::Kind => &[
                "snippet", "doc", "script", "map", "plan", "strategy", "lesson", "gotcha",
                "decision", "process", "module", "runbook",
                // F4: the kinds the live store actually carries (measured
                // 2026-08-11: transcript_lesson, insight, lesson, text, …).
                "insight", "outcome", "case", "observation", "pattern", "reference",
                "note", "project", "followup", "text",
            ],
            Facet::Purpose => &[
                "blast-radius", "pre-edit-gate", "diagnose", "explore", "converge",
                "map-rendering", "auto-tagging", "retrieval", "indexing", "wiring",
            ],
            Facet::Lang => &[
                "rust", "python", "bash", "md", "toml", "yaml", "json", "sql", "svg", "ts", "js",
            ],
            Facet::Domain => &[
                "memory", "wiring", "index", "hooks", "adw", "ceg", "portfolio", "quality",
                "daemon", "cli", "generator", "intelligence",
            ],
            Facet::Process => &[
                "diagnose", "explore", "converge", "cross-audit", "decompose", "pre-edit",
                "post-edit", "recall", "store", "reward",
            ],
            Facet::Artifact => &[
                "map", "dashboard", "report", "diorama", "infographic", "deck", "table",
            ],
            Facet::Status => &["stable", "experimental", "deprecated"],
        }
    }
}

impl fmt::Display for Facet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed, validated `#facet:value` tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParsedTag {
    /// The canonical facet the tag belongs to.
    pub facet: Facet,
    /// The value as written, lowercased. Intra-facet hierarchy uses `.`
    /// (`wiring.orphans` < `wiring` — ISO 25964-1 §10 polyhierarchy).
    pub value: String,
    /// Canonical render: `facet:value` (no leading `#`).
    pub full_tag: String,
}

impl fmt::Display for ParsedTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full_tag)
    }
}

/// One lint finding from [`validate_tag`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagViolation {
    /// No `facet:value` separator found.
    MissingColon,
    /// Facet is not in the controlled vocabulary (hard error). Carries the
    /// closest canonical facet when one is within edit distance ≤ 2.
    UnknownFacet {
        /// The facet string as written.
        raw: String,
        /// Closest canonical facet, if any is close enough.
        suggestion: Option<Facet>,
    },
    /// Value has invalid shape (empty, bad chars, bad case).
    InvalidValue {
        /// The resolved facet.
        facet: Facet,
        /// The offending value (lowercased).
        value: String,
        /// Static explanation of the violated shape rule.
        reason: &'static str,
    },
    /// Value is well-formed but outside the facet's seed list. WARN — the
    /// vocabulary grows by governance; the linter records it so aliases can
    /// be promoted deliberately.
    UnseededValue {
        /// The resolved facet.
        facet: Facet,
        /// The unseeded value (lowercased).
        value: String,
    },
}

impl TagViolation {
    /// Hard errors block acceptance; warnings are recorded but accepted.
    pub const fn is_hard(&self) -> bool {
        !matches!(self, TagViolation::UnseededValue { .. })
    }
}

fn is_valid_value(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("empty value");
    }
    if value.len() > 64 {
        return Err("value longer than 64 chars");
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err("malformed hierarchy separator '.'");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Err("allowed chars: a-z 0-9 . _ -");
    }
    Ok(())
}

/// Levenshtein distance over bytes (both inputs are short ASCII facet names;
/// the O(n·m) DP is a handful of operations at this size).
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Closest canonical facet within edit distance ≤ 2, for linter suggestions.
pub fn suggest_facet(raw: &str) -> Option<Facet> {
    let norm = raw.trim().to_ascii_lowercase();
    Facet::ALL
        .into_iter()
        .map(|f| (edit_distance(&norm, f.as_str()), f))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, f)| f)
}

/// Parses `#facet:value` (leading `#` optional) into a [`ParsedTag`].
/// Returns the lint violations on failure; success implies zero violations.
///
/// ```
/// use touring_intelligence::rl::memory::tags::parse_tag;
/// let t = parse_tag("#kind:snippet").unwrap();
/// assert_eq!(t.full_tag, "kind:snippet");
/// ```
pub fn parse_tag(raw: &str) -> Result<ParsedTag, Vec<TagViolation>> {
    let body = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    let Some((facet_raw, value_raw)) = body.split_once(':') else {
        return Err(vec![TagViolation::MissingColon]);
    };
    let facet = match Facet::from_str_ci(facet_raw) {
        Some(f) => f,
        None => {
            return Err(vec![TagViolation::UnknownFacet {
                raw: facet_raw.to_string(),
                suggestion: suggest_facet(facet_raw),
            }])
        }
    };
    let value = value_raw.trim().to_ascii_lowercase();
    if let Err(reason) = is_valid_value(&value) {
        return Err(vec![TagViolation::InvalidValue {
            facet,
            value,
            reason,
        }]);
    }
    Ok(ParsedTag {
        facet,
        full_tag: format!("{}:{}", facet.as_str(), value),
        value,
    })
}

/// Full lint pass: parse + seed-vocabulary check. Well-formed tags with
/// unseeded values return `Ok` plus a WARN violation in the second slot, so
/// callers can accept-and-record in one pass.
pub fn validate_tag(raw: &str) -> (Result<ParsedTag, Vec<TagViolation>>, Vec<TagViolation>) {
    match parse_tag(raw) {
        Ok(tag) => {
            let warns = if tag.facet.seed_values().contains(&tag.value.as_str()) {
                Vec::new()
            } else {
                vec![TagViolation::UnseededValue {
                    facet: tag.facet,
                    value: tag.value.clone(),
                }]
            };
            (Ok(tag), warns)
        }
        Err(errs) => (Err(errs), Vec::new()),
    }
}

/// Where a stored tag came from — drives trust ordering when sources conflict
/// (explicit > code-sync > auto > backfill).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TagSource {
    /// Derived offline over pre-existing entries (lowest trust).
    Backfill,
    /// Derived deterministically at store time (lang←extension, kind←type).
    Auto,
    /// Re-harvested from an in-code `#tags:` anchor by the snippet indexer.
    CodeSync,
    /// Stated by the author/LLM at creation (highest trust).
    Explicit,
}

impl TagSource {
    /// Canonical lowercase render persisted in `memory_tags.source`.
    pub const fn as_str(self) -> &'static str {
        match self {
            TagSource::Backfill => "backfill",
            TagSource::Auto => "auto",
            TagSource::CodeSync => "code_sync",
            TagSource::Explicit => "explicit",
        }
    }
}

/// Typed memory↔memory relation (A-MEM link generation; ids follow REGRA #17:
/// `make_id` is a pure function of `{src}|{rel}|{dst}` — same inputs, same id,
/// across sessions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkRel {
    /// Loose semantic association (A-MEM's dynamic link).
    RelatesTo,
    /// `src` replaces `dst` (mirrors `memory_entries.superseded_by`).
    Supersedes,
    /// `src` builds on / refines `dst`.
    Extends,
    /// `src` is a concrete instance of the pattern in `dst`.
    Exemplifies,
    /// `src` was produced by the process/artifact in `dst`.
    GeneratedBy,
}

impl LinkRel {
    /// Every relation, in declaration order.
    pub const ALL: [LinkRel; 5] = [
        LinkRel::RelatesTo,
        LinkRel::Supersedes,
        LinkRel::Extends,
        LinkRel::Exemplifies,
        LinkRel::GeneratedBy,
    ];

    /// Canonical kebab-case render persisted in `memory_links.rel`.
    pub const fn as_str(self) -> &'static str {
        match self {
            LinkRel::RelatesTo => "relates-to",
            LinkRel::Supersedes => "supersedes",
            LinkRel::Extends => "extends",
            LinkRel::Exemplifies => "exemplifies",
            LinkRel::GeneratedBy => "generated-by",
        }
    }

    /// Case-insensitive lookup of a relation by its canonical name.
    pub fn from_str_ci(raw: &str) -> Option<LinkRel> {
        LinkRel::ALL
            .into_iter()
            .find(|r| r.as_str().eq_ignore_ascii_case(raw.trim()))
    }
}

impl fmt::Display for LinkRel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Deterministic link id (REGRA #17 / RFC-004 shape: derived from canonical
/// parts, never from uuid/rand/creation order).
pub fn make_link_id(src: &str, rel: LinkRel, dst: &str) -> String {
    format!("{}|{}|{}", src.trim(), rel.as_str(), dst.trim())
}

// ── SQLite helpers — the single implementation of the tag write/read paths ──
//
// Both storage paths consume these: `RlmMemory` (in-process) and the RPC
// handlers in `touring-hook-runtime` (CLI). The SQL lives here so the two
// paths can never drift into divergent upsert semantics.

/// Applies the hashtag-library schema (tables + FTS + triggers) idempotently.
pub fn ensure_tag_schema(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(TAG_TABLES_DDL)?;
    conn.execute_batch(TAG_FTS_DDL)?;
    Ok(())
}

/// Upserts one tag on an entry with trust-ordered provenance: an explicit or
/// code-synced tag is never downgraded by an auto/backfill re-derivation,
/// while higher-trust sources always replace lower ones.
pub fn upsert_tag(
    conn: &rusqlite::Connection,
    key: &str,
    tag: &ParsedTag,
    source: TagSource,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memory_tags(entry_key, facet, value, full_tag, source)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(entry_key, full_tag) DO UPDATE SET source = excluded.source
         WHERE excluded.source IN ('explicit', 'code_sync')
            OR memory_tags.source IN ('backfill', 'auto')",
        rusqlite::params![
            key,
            tag.facet.as_str(),
            tag.value,
            tag.full_tag,
            source.as_str()
        ],
    )?;
    Ok(())
}

/// Fetches every tag attached to an entry, ordered by facet then value.
pub fn fetch_tags(conn: &rusqlite::Connection, key: &str) -> rusqlite::Result<Vec<ParsedTag>> {
    let mut stmt = conn.prepare(
        "SELECT facet, value, full_tag FROM memory_tags WHERE entry_key = ?1
         ORDER BY facet, value",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(facet, value, full_tag)| {
            Facet::from_str_ci(&facet).map(|f| ParsedTag {
                facet: f,
                value,
                full_tag,
            })
        })
        .collect())
}

/// Removes one tag from an entry (code-sync tombstone path). Returns the
/// number of rows removed.
pub fn delete_tag(conn: &rusqlite::Connection, key: &str, full_tag: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM memory_tags WHERE entry_key = ?1 AND full_tag = ?2",
        rusqlite::params![key, full_tag],
    )
}

/// Returns the keys of entries carrying ALL the required tags (conjunctive
/// facet filter over the L1 bipartite graph).
pub fn entry_keys_with_all_tags(
    conn: &rusqlite::Connection,
    required: &[ParsedTag],
    limit: usize,
) -> rusqlite::Result<Vec<String>> {
    if required.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = required.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT entry_key FROM memory_tags
         WHERE full_tag IN ({placeholders})
         GROUP BY entry_key
         HAVING COUNT(DISTINCT full_tag) = ?{}
         LIMIT ?{}",
        required.len() + 1,
        required.len() + 2,
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = required
        .iter()
        .map(|t| Box::new(t.full_tag.clone()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params_vec.push(Box::new(required.len() as i64));
    params_vec.push(Box::new(limit as i64));
    let refs: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(std::convert::AsRef::as_ref).collect();
    let rows = stmt
        .query_map(refs.as_slice(), |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Splits a free-form query into `#facet:value` filters and the remaining
/// text — the CLI grammar of `touring memory query "texto #kind:snippet"`.
pub fn split_query_tags(query: &str) -> (Vec<ParsedTag>, String) {
    let mut tags = Vec::new();
    let mut text = Vec::new();
    for word in query.split_whitespace() {
        if word.starts_with('#')
            && let Ok(tag) = parse_tag(word)
        {
            tags.push(tag);
            continue;
        }
        text.push(word);
    }
    (tags, text.join(" "))
}

// ── Link graph helpers (L3 of the hashtag library) ─────────────────────────
// The memory↔memory layer: typed edges with deterministic ids, so the same
// relation re-asserted across sessions is one row, never a duplicate.

/// One typed edge between two memory entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEdge {
    /// Deterministic id `{src}|{rel}|{dst}` (REGRA #17).
    pub id: String,
    /// Source entry key.
    pub src: String,
    /// Target entry key.
    pub dst: String,
    /// The relation.
    pub rel: LinkRel,
}

/// Inserts (or re-affirms, idempotently) a typed edge. The deterministic id
/// makes the write naturally deduplicated — no existence pre-check needed.
pub fn upsert_link(
    conn: &rusqlite::Connection,
    src: &str,
    rel: LinkRel,
    dst: &str,
) -> rusqlite::Result<LinkEdge> {
    let id = make_link_id(src, rel, dst);
    conn.execute(
        "INSERT INTO memory_links(id, src, dst, rel) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO NOTHING",
        rusqlite::params![id, src.trim(), dst.trim(), rel.as_str()],
    )?;
    Ok(LinkEdge {
        id,
        src: src.trim().to_string(),
        dst: dst.trim().to_string(),
        rel,
    })
}

/// Every edge touching `key`, outgoing first — the 1-hop neighbourhood the
/// recall payload carries.
pub fn fetch_links(conn: &rusqlite::Connection, key: &str) -> rusqlite::Result<Vec<LinkEdge>> {
    let mut stmt = conn.prepare(
        "SELECT id, src, dst, rel FROM memory_links
         WHERE src = ?1 OR dst = ?1
         ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, src, dst, rel)| {
            LinkRel::from_str_ci(&rel).map(|r| LinkEdge {
                id,
                src,
                dst,
                rel: r,
            })
        })
        .collect())
}

/// Removes one edge by its deterministic id. Returns rows removed (0 or 1).
pub fn delete_link(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM memory_links WHERE id = ?1", rusqlite::params![id])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("#kind:snippet", Facet::Kind, "snippet")]
    #[case("kind:snippet", Facet::Kind, "snippet")] // leading # optional
    #[case("#LANG:Rust", Facet::Lang, "rust")] // case-insensitive, value lowercased
    #[case("#domain:wiring.orphans", Facet::Domain, "wiring.orphans")] // intra-facet hierarchy
    #[case("#tipo:lesson", Facet::Kind, "lesson")] // alias resolves
    #[case("#process:cross-audit", Facet::Process, "cross-audit")]
    #[case("#artifact:map", Facet::Artifact, "map")]
    fn parse_valid(#[case] raw: &str, #[case] facet: Facet, #[case] value: &str) {
        let tag = parse_tag(raw).expect("valid tag parses");
        assert_eq!(tag.facet, facet);
        assert_eq!(tag.value, value);
        assert_eq!(tag.full_tag, format!("{}:{}", facet.as_str(), value));
    }

    #[rstest]
    #[case("nosnippet")]
    #[case("#kind")]
    #[case("#kind:")]
    fn parse_missing_or_empty(#[case] raw: &str) {
        let errs = parse_tag(raw).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, TagViolation::MissingColon | TagViolation::InvalidValue { .. })),
            "expected MissingColon/InvalidValue, got {errs:?}"
        );
    }

    #[rstest]
    #[case("#kind:Snippet")] // uppercase rejected post-normalize? no: lowercased first
    #[case("#kind:snip pet")] // space
    #[case("#kind:.orphans")] // leading dot
    #[case("#kind:wiring..orphans")] // doubled dot
    fn parse_invalid_value(#[case] raw: &str) {
        let res = parse_tag(raw);
        if let Ok(tag) = res {
            // "#kind:Snippet" lowercases to a valid tag — assert that path.
            assert_eq!(raw, "#kind:Snippet");
            assert_eq!(tag.value, "snippet");
        }
    }

    #[rstest]
    #[case("#knd:snippet", Some(Facet::Kind))]
    #[case("#porpose:mapa", Some(Facet::Purpose))] // typo within edit distance 1
    #[case("#zzz:x", None)]
    fn unknown_facet_suggests(#[case] raw: &str, #[case] want: Option<Facet>) {
        let errs = parse_tag(raw).unwrap_err();
        match &errs[0] {
            TagViolation::UnknownFacet { suggestion, .. } => assert_eq!(*suggestion, want),
            other => panic!("expected UnknownFacet, got {other:?}"),
        }
    }

    #[rstest]
    fn seeded_value_is_clean() {
        let (res, warns) = validate_tag("#kind:snippet");
        assert!(res.is_ok());
        assert!(warns.is_empty());
    }

    #[rstest]
    fn unseeded_value_warns_but_accepts() {
        let (res, warns) = validate_tag("#domain:transferegov");
        assert!(res.is_ok());
        assert_eq!(warns.len(), 1);
        assert!(!warns[0].is_hard(), "unseeded value is WARN, never hard");
    }

    #[rstest]
    fn edit_distance_sanity() {
        assert_eq!(edit_distance("kind", "knd"), 1);
        assert_eq!(edit_distance("status", "status"), 0);
        assert!(edit_distance("domain", "zzz") > 2);
    }

    #[rstest]
    fn link_id_is_deterministic() {
        // REGRA #17: same inputs → same id, always.
        let a = make_link_id("mem:a", LinkRel::Extends, "mem:b");
        let b = make_link_id("mem:a", LinkRel::Extends, "mem:b");
        assert_eq!(a, b);
        assert_eq!(a, "mem:a|extends|mem:b");
        assert_ne!(
            make_link_id("mem:a", LinkRel::Extends, "mem:b"),
            make_link_id("mem:b", LinkRel::Extends, "mem:a")
        );
    }

    #[rstest]
    fn link_graph_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_tag_schema(&conn).unwrap();

        let e = upsert_link(&conn, "mem:a", LinkRel::Extends, "mem:b").unwrap();
        assert_eq!(e.id, "mem:a|extends|mem:b");
        // Idempotent: re-asserting the same edge is a no-op, not a duplicate.
        upsert_link(&conn, "mem:a", LinkRel::Extends, "mem:b").unwrap();
        upsert_link(&conn, "mem:c", LinkRel::Exemplifies, "mem:b").unwrap();

        let around_b = fetch_links(&conn, "mem:b").unwrap();
        assert_eq!(around_b.len(), 2, "1-hop in both directions");
        assert!(
            around_b
                .iter()
                .any(|l| l.rel == LinkRel::Exemplifies && l.src == "mem:c")
        );

        let around_a = fetch_links(&conn, "mem:a").unwrap();
        assert_eq!(around_a.len(), 1);
        assert_eq!(around_a[0].dst, "mem:b");

        assert_eq!(delete_link(&conn, "mem:a|extends|mem:b").unwrap(), 1);
        assert_eq!(fetch_links(&conn, "mem:b").unwrap().len(), 1);
        assert_eq!(
            delete_link(&conn, "mem:a|extends|mem:b").unwrap(),
            0,
            "second delete is a no-op"
        );
    }

    #[rstest]
    fn rel_roundtrip() {
        for rel in LinkRel::ALL {
            assert_eq!(LinkRel::from_str_ci(rel.as_str()), Some(rel));
        }
    }
}
