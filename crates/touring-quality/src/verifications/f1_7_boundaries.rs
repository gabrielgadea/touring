//! F1.7 — Component Boundaries verifier (D07).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_boundaries`] — a visibility-aware
//! scanner that classifies top-level item visibility (`pub` /
//! `pub(crate)`·`pub(super)` / private), counts `pub` struct fields
//! (C-STRUCT-PRIVATE, the Rust API Guidelines future-proofing signal), and
//! scores the public-exposure surface. This replaces the prior stub, which
//! counted only lines beginning `pub fn`/`pub struct`/… — it was blind to
//! `pub(crate)` (neither leak nor encapsulation) and to `pub` struct fields,
//! and used an arbitrary `pub_count / 50` threshold that punished any large
//! legitimate public API.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior `pub`-line
//! count heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`) — intra-file surface only;
//! cross-module "pub symbol with zero consumers" is a wiring concern (F1.8 /
//! `touring wiring impact`).

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.7 verifier — Component Boundaries.
#[allow(non_camel_case_types)]
pub struct F1_7_Boundaries;

impl Verification for F1_7_Boundaries {
    fn id(&self) -> DimId {
        DimId::F1_7
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_boundaries_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

/// Default architectural-layer rank for a workspace crate (W4 2026-07-02).
/// Lower = more foundational. Hyphens/underscores are normalised so a `crates/
/// touring-web-server/` path and a `use touring_web_server::` import map to the
/// same key. Unknown crates return `None` (skipped → no false hint). This is a
/// **default heuristic** for the touring workspace; a declared [`LayerPolicy`]
/// (`.touring-layers.toml`) overrides it per crate and extends coverage
/// (see [`resolve_layer_rank`]).
#[cfg(feature = "workspace-integration")]
fn crate_layer_rank(crate_name: &str) -> Option<u8> {
    let n = crate_name
        .trim_start_matches("touring-")
        .trim_start_matches("touring_")
        .replace('-', "_");
    let rank = match n.as_str() {
        "foundation" | "contracts" | "identity" | "rkyv" | "license" => 0,
        "code" | "storage" | "simd" | "analysis" | "ast" => 1,
        "intelligence" | "cognitive" | "learning" | "generator" | "assists" | "cortex"
        | "quality" => 2,
        "server" | "web" | "web_server" | "cli" | "hooks" | "orchestration" | "ceg"
        | "dispatch" | "lsp" => 3,
        _ => return None,
    };
    Some(rank)
}

/// A **declared** architectural-layer policy — the "teeth" for the F1.7
/// layer-inversion check (W4.1 follow-up, 2026-07-04).
///
/// [`crate_layer_rank`] is a heuristic *default*. A declared policy makes the
/// intended architecture EXPLICIT and authoritative: each crate's layer is
/// stated, so a lower-layer crate importing a higher-layer one violates a
/// *written* contract (dependency-cruiser's `forbidden` philosophy) — stricter
/// than cargo's acyclicity, which permits any acyclic import regardless of
/// layering. A declared rank overrides the default for that crate and
/// additionally covers crates the default table does not know.
///
/// Loaded from `<workspace-root>/.touring-layers.toml`, a flat map
/// (lower rank = more foundational; `[layers]` header optional, `#` comments):
/// ```toml
/// [layers]
/// touring-foundation = 0
/// touring-storage    = 1
/// touring-cli        = 3
/// ```
/// An absent/empty file yields an empty policy → behavior is byte-identical to
/// the pre-policy default table.
#[cfg(feature = "workspace-integration")]
#[derive(Debug, Default, Clone)]
struct LayerPolicy {
    ranks: std::collections::HashMap<String, u8>,
}

#[cfg(feature = "workspace-integration")]
impl LayerPolicy {
    /// The canonical policy key for a crate name — strip the `touring-`/
    /// `touring_` prefix and unify separators (same scheme as
    /// [`crate_layer_rank`]).
    fn norm(name: &str) -> String {
        name.trim_start_matches("touring-")
            .trim_start_matches("touring_")
            .replace('-', "_")
    }

    /// The declared rank for a crate, if the policy names it.
    fn rank(&self, crate_name: &str) -> Option<u8> {
        self.ranks.get(&Self::norm(crate_name)).copied()
    }

    /// Whether any layer was declared (an empty policy defers entirely to the
    /// default table).
    fn is_declared(&self) -> bool {
        !self.ranks.is_empty()
    }

    /// Parse the flat `crate = rank` policy text. Tolerant: `#` line comments,
    /// an optional `[layers]` (or any `[...]`) header, quoted or bare keys,
    /// blank lines. A malformed line is skipped, never an error — a broken
    /// policy file degrades to "fewer declared crates", never a harness crash.
    fn parse(content: &str) -> Self {
        let mut ranks = std::collections::HashMap::new();
        for raw in content.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().trim_matches(['"', '\'']);
            if let Ok(rank) = val.trim().parse::<u8>() {
                if !key.is_empty() {
                    ranks.insert(Self::norm(key), rank);
                }
            }
        }
        Self { ranks }
    }

    /// Load the policy from `<root>/.touring-layers.toml`. A missing file yields
    /// an empty policy (the default table then governs).
    fn load_from_root(root: &Path) -> Self {
        std::fs::read_to_string(root.join(".touring-layers.toml"))
            .map(|c| Self::parse(&c))
            .unwrap_or_default()
    }
}

/// The declared layer policy in effect for `target`, discovered by walking up to
/// the workspace root ([`super::find_repo_root`]) and reading
/// `.touring-layers.toml`. Memoized for the process — the harness scores one
/// workspace per run; a cross-workspace mis-cache only ever changes an ADVISORY
/// hint, never a gate.
#[cfg(feature = "workspace-integration")]
fn layer_policy_for(target: &Path) -> &'static LayerPolicy {
    static POLICY: std::sync::OnceLock<LayerPolicy> = std::sync::OnceLock::new();
    POLICY.get_or_init(|| {
        super::find_repo_root(target)
            .map(|root| LayerPolicy::load_from_root(&root))
            .unwrap_or_default()
    })
}

/// Resolve a crate's layer rank: a DECLARED policy rank takes precedence (the
/// authoritative, teeth-bearing source), falling back to the heuristic
/// [`crate_layer_rank`] default table.
#[cfg(feature = "workspace-integration")]
fn resolve_layer_rank(crate_name: &str, policy: &LayerPolicy) -> Option<u8> {
    policy
        .rank(crate_name)
        .or_else(|| crate_layer_rank(crate_name))
}

/// Advisory layer-inversion signal: this file's crate importing a strictly
/// higher-layer crate (a base layer reaching up into an application layer).
/// Returns `(penalty, note)`. Only confirmed crate pairs in [`crate_layer_rank`]
/// count; unknown crates never fabricate a hint. Cargo already forbids the cyclic
/// case, so this fires only for the acyclic-but-inverted imports a pure
/// dependency-graph check would miss.
#[cfg(feature = "workspace-integration")]
fn layer_inversion(target: &Path, raw: &str) -> (f32, String) {
    let policy = layer_policy_for(target);
    let p = target.to_string_lossy();
    let Some(own) = p.split("crates/").nth(1).and_then(|s| s.split('/').next()) else {
        return (0.0, String::new());
    };
    let Some(own_rank) = resolve_layer_rank(own, policy) else {
        return (0.0, String::new());
    };
    let mut ups: Vec<String> = Vec::new();
    for line in raw.lines() {
        let l = line.trim_start();
        if !(l.starts_with("use touring_") || l.starts_with("pub use touring_")) {
            continue;
        }
        if let Some(rest) = l.split("touring_").nth(1) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if let Some(r) = resolve_layer_rank(&name, policy)
                && r > own_rank
                && !ups.contains(&name)
            {
                ups.push(name);
            }
        }
    }
    if ups.is_empty() {
        (0.0, String::new())
    } else {
        // The note names the policy source so the harness output shows whether
        // the check has "teeth" (an authoritative declared policy) or is running
        // on the heuristic default.
        let source = if policy.is_declared() {
            "declared"
        } else {
            "default"
        };
        (
            0.20,
            format!(
                " | layer-inversion ({source}, advisory): L{own_rank} crate imports higher-layer {ups:?}"
            ),
        )
    }
}

// ── Real engine: visibility-aware boundary analysis ───────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_boundaries_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_boundaries, score_boundaries};

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_boundaries(&raw, lang);

    // W4 (2026-07-02): default architectural-layer policy (advisory). A file in
    // a lower-layer crate that imports a strictly-higher-layer crate is a layer
    // inversion — cargo permits it when there's no back-edge, but it erodes the
    // intended architecture (dependency-cruiser's `forbidden` philosophy). The
    // rank table below is a SENSIBLE DEFAULT for the touring workspace,
    // overridable by an explicit policy; F1.7 is advisory so a mis-ranked hint
    // never blocks.
    let (layer_penalty, layer_note) = layer_inversion(target, &raw);
    let value = (score_boundaries(&r) - layer_penalty).clamp(0.0, 1.0);
    let evidence = format!(
        "F1.7: {} pub / {} restricted / {} private top-level items, {}/{} pub struct field(s) \
         (exposure {:.0}%) ({lang}) — score={value:.3} (touring-analysis analyze_boundaries, \
         C-STRUCT-PRIVATE + exposure ratio){layer_note}",
        r.public_items,
        r.restricted_items,
        r.private_items,
        r.pub_fields,
        r.struct_fields,
        r.exposure_ratio * 100.0
    );
    Ok((value, evidence))
}

// ── Standalone fallback: pub-line count heuristic (no visibility awareness) ────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_boundaries_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let pub_count = raw
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("pub fn ")
                || t.starts_with("pub struct ")
                || t.starts_with("pub enum ")
                || t.starts_with("pub trait ")
                || t.starts_with("pub mod ")
                || t.starts_with("pub const ")
                || t.starts_with("pub static ")
        })
        .count();
    let value = (1.0 - (pub_count as f32) / 50.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{pub_count} pub items in module (substring heuristic; build --features \
         workspace-integration for visibility-aware boundary analysis)"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_rs(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_boundaries_returns_valid_score() {
        let f = write_temp_rs("pub fn api() {}\nfn helper() {}\n");
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn typescript_target_uses_polyglot_visibility() {
        // P-F end-to-end: the verifier threads lang_from_ext(.ts) → analyze_boundaries,
        // so an all-exported TS file reads as real high-exposure (a meaningful
        // score < 1.0), not the pre-P-F 0-items silent 1.0.
        let mut f = tempfile::Builder::new()
            .suffix(".ts")
            .tempfile()
            .expect("temp");
        f.write_all(
            b"export class A {}\nexport class B {}\nexport function c() {}\nexport type D = number;\n",
        )
        .expect("write");
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!((0.0..1.0).contains(&s.value), "high TS exposure < 1.0, got {}", s.value);
        assert!(
            s.evidence.contains("(typescript)"),
            "evidence must report the TS language dispatch: {}",
            s.evidence
        );
    }

    #[test]
    fn test_boundaries_empty_file() {
        let f = write_temp("");
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// A well-encapsulated module (private internals, restricted visibility) is high.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_well_encapsulated_scores_high() {
        let code = "pub fn api() {}\npub(crate) fn internal() {}\nfn helper() {}\n\
                    struct Inner {\n    secret: u32,\n}\n";
        let f = write_temp_rs(code);
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "well-encapsulated module should be high, got {}",
            s.value
        );
    }

    /// **End-to-end FP fix vs stub**: a struct of all-`pub` fields (C-STRUCT-PRIVATE
    /// leak) must score below a clean module — the stub did not see `pub` fields.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_pub_field_bag_lowers_score() {
        let bag = write_temp_rs(
            "pub struct Config {\n    pub host: String,\n    pub port: u16,\n    pub retries: u8,\n}\n",
        );
        let clean = write_temp_rs("pub fn f() {}\nfn g() {}\n");
        let sb = F1_7_Boundaries.check(bag.path()).expect("check");
        let sc = F1_7_Boundaries.check(clean.path()).expect("check");
        assert!(
            sb.value < sc.value,
            "all-pub-field bag ({}) must score below clean module ({})",
            sb.value,
            sc.value
        );
        assert!(
            sb.value < 0.5,
            "all-pub-field bag should warn/fail, got {}",
            sb.value
        );
    }

    /// `pub(crate)` everywhere is disciplined encapsulation, not exposure (the
    /// stub counted neither — it was invisible to restricted visibility).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_pub_crate_not_penalised() {
        let code =
            "pub(crate) fn a() {}\npub(crate) fn b() {}\npub(crate) struct C {\n    x: u32,\n}\n";
        let f = write_temp_rs(code);
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!(
            (s.value - 1.0).abs() < 1e-6,
            "pub(crate) is not a leak, got {}",
            s.value
        );
    }

    /// A pure public-fn API (`lib.rs`-style) must not be over-punished the way the
    /// `pub_count / 50` stub would have been.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_public_fn_api_not_over_punished() {
        let code = (0..12)
            .map(|i| format!("pub fn item_{i}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        let f = write_temp_rs(&code);
        let s = F1_7_Boundaries.check(f.path()).expect("check");
        assert!(s.value >= 0.8, "public fn API should pass, got {}", s.value);
    }

    /// W4 (2026-07-02): default layer policy. A base-layer crate (`foundation`,
    /// L0) importing an application-layer crate (`server`, L3) is a layer
    /// inversion → advisory penalty + note. A same/lower-layer import is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_layer_inversion_default_policy() {
        use std::path::Path;
        // Base crate importing a top crate → inversion.
        let (pen, note) = layer_inversion(
            Path::new("/x/crates/touring-foundation/src/lib.rs"),
            "use touring_server::Thing;\npub fn f() {}\n",
        );
        assert!(pen > 0.0, "base→top import must be flagged");
        assert!(note.contains("layer-inversion"), "note: {note}");
        // Top crate importing a base crate → NOT an inversion (correct direction).
        let (pen2, _) = layer_inversion(
            Path::new("/x/crates/touring-server/src/lib.rs"),
            "use touring_foundation::Config;\npub fn f() {}\n",
        );
        assert_eq!(pen2, 0.0, "top→base is the correct direction, no penalty");
        // Unknown crate → no fabricated hint.
        let (pen3, _) = layer_inversion(
            Path::new("/x/crates/touring-mystery/src/lib.rs"),
            "use touring_server::Thing;\n",
        );
        assert_eq!(pen3, 0.0, "unknown crate must not fabricate a hint");
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_crate_layer_rank_normalises_separators() {
        // Path hyphen form and import underscore form map to the same rank.
        assert_eq!(crate_layer_rank("touring-web-server"), Some(3));
        assert_eq!(crate_layer_rank("web_server"), Some(3));
        assert_eq!(crate_layer_rank("foundation"), Some(0));
        assert_eq!(crate_layer_rank("nonexistent"), None);
    }

    // ── Declared LayerPolicy (W4.1 teeth, 2026-07-04) ───────────────────────

    /// The parser tolerates comments, an optional `[layers]` header, quoted and
    /// bare keys, blank lines, and normalises crate names to the policy key.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn layer_policy_parses_flat_map() {
        let policy = LayerPolicy::parse(
            "# architectural layers\n\
             [layers]\n\
             touring-foundation = 0   # base\n\
             \"touring-storage\"  = 1\n\
             touring-cli        = 3\n\
             \n\
             malformed line without equals\n",
        );
        assert!(policy.is_declared());
        assert_eq!(policy.rank("touring-foundation"), Some(0));
        assert_eq!(policy.rank("touring_foundation"), Some(0)); // separator-normalised
        assert_eq!(policy.rank("touring-storage"), Some(1)); // quoted key
        assert_eq!(policy.rank("touring-cli"), Some(3));
        assert_eq!(policy.rank("touring-unlisted"), None);
    }

    /// An empty (absent-file) policy declares nothing → the default table governs.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn empty_policy_defers_to_default() {
        let empty = LayerPolicy::default();
        assert!(!empty.is_declared());
        // resolve_layer_rank falls back to the heuristic default table.
        assert_eq!(resolve_layer_rank("touring-foundation", &empty), Some(0));
        assert_eq!(resolve_layer_rank("touring-cli", &empty), Some(3));
        assert_eq!(resolve_layer_rank("touring-unknown", &empty), None);
    }

    /// The teeth: a DECLARED rank overrides the default, AND covers crates the
    /// default table does not know (so a new inversion becomes detectable).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn declared_policy_overrides_and_extends_default() {
        let policy = LayerPolicy::parse(
            "touring-quality = 0\n\
             my-app-crate    = 3\n",
        );
        // Override: default puts quality at L2; the declaration wins (L0).
        assert_eq!(crate_layer_rank("touring-quality"), Some(2));
        assert_eq!(resolve_layer_rank("touring-quality", &policy), Some(0));
        // Extend: a crate unknown to the default is now ranked → coverable.
        assert_eq!(crate_layer_rank("my-app-crate"), None);
        assert_eq!(resolve_layer_rank("my-app-crate", &policy), Some(3));
        // Undeclared-but-default crate still resolves via the default fallback.
        assert_eq!(resolve_layer_rank("touring-foundation", &policy), Some(0));
    }

    /// End-to-end: `load_from_root` discovers `.touring-layers.toml` on disk and
    /// a missing file yields an empty policy.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn load_from_root_reads_declared_file() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // No file yet → empty policy.
        assert!(!LayerPolicy::load_from_root(tmp.path()).is_declared());
        // Write the policy file → loaded + parsed.
        std::fs::write(
            tmp.path().join(".touring-layers.toml"),
            "[layers]\ntouring-foundation = 0\ntouring-cli = 3\n",
        )
        .expect("write policy");
        let policy = LayerPolicy::load_from_root(tmp.path());
        assert!(policy.is_declared());
        assert_eq!(policy.rank("touring-foundation"), Some(0));
        assert_eq!(policy.rank("touring-cli"), Some(3));
    }
}
