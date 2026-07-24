# touring-quality

> **50-dimension quality scoring engine for Touring** — makes any LLM produce
> elite-grade code via auto-scoring, BLOCK enforcement, and auto-remediation.

`touring-quality` scores any granularity — a **file**, a Rust **module**, a
**crate**, an arbitrary **path**, a **feature** slice, a **repo**, or the whole
**workspace** — across **50 quality
dimensions** (F1.1–F4.12) grouped into four phases — Code Quality & Architecture,
Security & Performance, Testing & Documentation, Best Practices & CI/CD — and
rolls them into a weighted **composite** mapped to a **6-tier** grade. Six of the
dimensions are **P0 BLOCK** gates (fail-closed): a hardcoded secret, an OWASP
injection, a known-CVE dependency, an insecure config, a deprecated API, or an
EOL package is a hard stop, not a warning.

The crate eats its own dog food: it holds the same
`#![cfg_attr(not(test), deny(clippy::unwrap_used))]` invariant it scores other
crates on.

## Architecture

```text
                      ┌──────────────────┐
   touring-analysis ─▶│ touring-quality  │─▶ touring-server (CLI + MCP tools)
   (feeder: AST,      │  50-dim engine   │─▶ touring-ceg    (X2 static gate)
    duplication,      │                  │─▶ touring-cortex (fascicle scoring)
    README, cycles)   └──────────────────┘─▶ touring-lsp    (editor diagnostics)

   target → QualityReport { dimensions: BTreeMap<DimId, DimScore>,
                            composite: f32, tier: Tier }
          → CLI output (JSON | HTML | badge | compact)
```

`touring-analysis` supplies the heavy analyzers (duplication, README completeness,
import-cycle detection, CWE/OWASP `SecurityAnalyzer`). It is pulled in by the
default `workspace-integration` feature; `--no-default-features` falls back to
labelled substring sinks so the crate stays standalone-buildable.

## Build

```bash
# workspace-integrated (default — real analyzers)
cargo build -p touring-quality --release

# standalone (no touring-analysis dependency, substring fallback)
cargo build -p touring-quality --release --no-default-features
```

The binary is `touring-quality`; the library crate name is `touring_quality`.

## Usage — CLI

```bash
touring-quality score <TARGET> [--workspace] [--scope <kind>] \
                                [--include <glob>] [--exclude <glob>] \
                                [--dims F1.1,F2.5] \
                                [--format json|html|badge|compact] \
                                [--fail-below 0.80] [-o out.json]
touring-quality check --gate F2.1 --target <TARGET> [--scope <kind>] [--format json]
touring-quality list                                                  # 50 dims + glyph
```

- `score` — score any granularity. The scope is **auto-detected** from the target
  (file → `file`; `[package]` dir → `crate`; `[workspace]` dir → `workspace`;
  `.git`/`README` dir → `repo`; else `path`) or set explicitly with `--scope`:

  | `--scope` | Resolves to | Example |
  |-----------|-------------|---------|
  | `file` | one source file | `score src/lib.rs --scope file` |
  | `module` | a Rust module: root file (`foo.rs`/`foo/mod.rs`) **+** its `foo/` submodule dir | `score src/verifications/mod.rs --scope module` |
  | `path` | an arbitrary directory subtree | `score src/builtins --scope path` |
  | `feature` | a slice = root + `--include`/`--exclude` globs (git-free "new code") | `score . --include 'src/cli/**'` |
  | `crate` | one Cargo crate (its file-set + `Cargo.toml`) | `score crates/foo --scope crate` |
  | `repo` / `project` | a repository root (repo-level artifacts active) | `score . --scope repo` |
  | `system` | a glob-bounded multi-crate set | `score . --scope system --include 'crates/touring-*/**'` |
  | `workspace` | every `[workspace]` member (alias `--workspace`) | `score --workspace` |

  All 50 dimensions are present at **every** scope. The **workspace-level**
  artifacts (CHANGELOG / architecture docs / CI) are *inherited* by walking up to
  the enclosing repository root, so a member crate that lacks its own is credited
  for the workspace's — never falsely capped. `README` stays **per-crate** (each
  crate should ship its own). `module` is opt-in only, so bare-file / bare-dir
  auto-detection is unchanged.
- `check --gate <dim>` — evaluate one dimension at any `--scope` (a P0 dim below
  `0.5` exits non-zero — the BLOCK contract).
- `--fail-below <N>` — exit `1` when the composite is under `N` (the delivery
  gate; Gold `0.80` is the TACO floor, Diamond `0.95` the release bar).
- `list` — print all 50 dimensions with their enforcement glyph (⛔ BLOCK / ⚠ WARN).

## Usage — library

```rust
use touring_quality::{score_target, OutputFormat};
use std::path::Path;

let report = score_target(Path::new("src/main.rs"), &[], OutputFormat::Json)?;
println!("Composite: {:.2} ({})", report.composite, report.tier);
for (id, dim) in &report.dimensions {
    println!("  {}: {:.2} ({})", id, dim.value, dim.status);
}
```

Scope-aware scoring (per-file → rolled up by `AggKind`) is available via
`scope_report::score_scope` with a `Scope` / `ScopeKind`; the enforcement harness
(`runner::run_harness` + `HarnessConfig` + `should_block`) drives the PreToolUse /
PostToolUse BLOCK hooks.

## The 50 dimensions

| Phase | Dims | Theme | P0 BLOCK |
|-------|------|-------|----------|
| **F1** Code Quality & Architecture | F1.1–F1.12 | complexity, maintainability, duplication, SOLID, tech-debt, error-handling, boundaries, deps, API, data-model, patterns, consistency | — |
| **F2** Security & Performance | F2.1–F2.13 | OWASP, input-validation, authN/Z, crypto/secrets, dep-CVEs, config, DB/mem/cache/IO/concurrency/frontend/scalability | **F2.1 F2.4 F2.5 F2.6** |
| **F3** Testing & Documentation | F3.1–F3.13 | coverage, mutation, pyramid, edge-cases, maintainability, sec/perf gaps, inline/API/arch docs, README, accuracy, changelog | — |
| **F4** Best Practices & CI/CD | F4.1–F4.12 | idioms, framework, deprecated, modernization, pkg-mgmt, build, CI/CD, deploy, IaC, observability, incident, env | **F4.3 F4.5** |

Conditional artifact dimensions (deployment, IaC, incident-response, …) report
`NotApplicable` and are excluded from the composite when the target project type
does not carry that artifact.

## Tiers

| Composite | Tier | Meaning |
|-----------|------|---------|
| ≥ 0.95 | 💎 Diamond | release-ready |
| ≥ 0.90 | 🥇 Platinum | best-in-class |
| ≥ 0.80 | 🥈 Gold | production floor (TACO delivery minimum) |
| ≥ 0.70 | 🥉 Silver | human review required |
| ≥ 0.60 | ⚪ Bronze | refactor before merge |
| < 0.60 | ⚫ Unranked | rewrite |

A single `Fail` dimension (a *blocker*) can cap the tier below what the composite
alone would grant — the composite measures average quality, the tier also honors
fail-closed floors.

## Tests

```bash
cargo test -p touring-quality              # unit + verifier suite (344 tests)
cargo clippy -p touring-quality -- -D warnings
```

Every one of the 50 verifiers (`src/verifications/f1_1..f4_12.rs`) carries its own
true-positive / false-positive fixtures; a new dimension is not wired until its
verifier's tp/fp pair is green.

## Contributing

The crate co-evolves with its scorers: a change to a verifier
(`src/verifications/`) moves in lockstep with its D-rule
(`skills/touring-elite/references/quality/D{nn}.md`) and this README. Run the
harness on your own diff before opening it — `touring-quality score <changed> --fail-below 0.80`
must stay ≥ Gold, and the six P0 BLOCK dims must stay `Pass`.

## Cross-references

- Constitutional keystone: `~/.claude/rules/elite-50-quality.md` (dim → owner → D-rule)
- Per-dimension rules: `~/.claude/skills/touring-elite/references/quality/D01..D52.md`
- Release composite (13-gate aggregator): `touring-elite` / `docs/elite_aggregate.py`

## License

Part of the Touring workspace; see the workspace root for licensing.
