# D44 — Package Management (F4.5)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P0 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_5_pkg_mgmt::F4_5_PkgMgmt`
**Enforcement**: **⛔ BLOCK** on PreToolUse:Write (if dep is EOL/abandoned — W5+)
**Elite reference**: Dependabot, Snyk, npm-check-updates

## Definition

Package management = total dependency count + freshness + security.
Excessive deps = bloat, supply chain risk, slow builds.

## Thresholds (per ecosystem)

| Ecosystem | Soft cap | Score=0.0 at |
|-----------|----------|--------------|
| Cargo.toml | 50       | 100 deps     |
| package.json | 100   | 200 deps     |
| pyproject.toml | 30  | 60 deps      |
| requirements.txt | 30 | 60 deps    |

| Total deps | Score  | Status | Action |
|------------|--------|--------|--------|
| 0          | 1.0    | ✅ Pass | No action |
| ≤ cap      | 0.5-1.0| ✅ Pass | Acceptable |
| cap..2×cap | 0.0-0.5| ⚠ Warn | Review |
| > 2×cap    | 0.0    | ❌ Fail | Refactor |

## BLOCK Hook (W5+)

```json
{
  "PreToolUse:Write": {
    "matcher": "Cargo.toml",
    "action": "BLOCK if F4.5 < 0.5 AND new dep is EOL",
    "canonical_fix": "Edit tool --path Cargo.toml --operation ssr (ver ~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md Pattern 5)"
  }
}
```

## MUST Commands

```bash
touring-quality check --gate F4.5 --target Cargo.toml
# > cap deps = WARN/FAIL
```

## SHOULD Commands

```bash
# `Edit tool --path Cargo.toml --operation ssr --pattern '<old_ver>' --replacement '<new_ver>'` (cargo outdated + cargo machete)  # review deps
cargo outdated  # check for newer versions
cargo update    # update within semver
```

## MAY Commands

```bash
# Production: integrate npm audit / pip-audit / cargo-audit
cargo install cargo-audit --locked
cargo audit
```

## Context7 Best Practice

- `/rust-lang/cargo` — Cargo dependencies best practices
- `/npm/npm` — npm package management
- `/pypa/pip` — pip dependencies

## Common Pitfalls

- **Unused deps**: declared but not imported (remove)
- **Duplicate functionality**: 3 date libraries when 1 suffices
- **Heavy deps for trivial use**: lodash for one function
- **Dev deps in production**: serde_derive shouldn't be `[dependencies]`

## Auto-Remediation

```bash
# `Edit tool --path Cargo.toml --operation ssr --pattern '<old_ver>' --replacement '<new_ver>'` (cargo outdated + cargo machete)
# Identifies unused deps (cargo machete, depcheck)
# Suggests lighter alternatives
# Generates audit-deps.sh
```

## Examples

- ✅ 2 cargo deps → score=1.0
- ⚠ 80 cargo deps → score=0.0 (review)

## Implementation

- Verifier: `~/projects/touring/crates/touring-quality/src/verifications/f4_5_pkg_mgmt.rs`
- Counts: `serde = "X.Y"`, `"pkg": "^X.Y"`, `pkg==X.Y`
- Caps per ecosystem (table above)
- Linear penalty: 0=1.0, 2×cap=0.0

## Elite best practices (context7 — `/websites/embarkstudios_github_io_cargo-deny` + `/rust-lang/cargo`)

1. **`[bans] multiple-versions = "deny"` + `wildcards = "deny"`** — elimina duplicação de versões e specs `*` (bloat + supply-chain). Fonte: cargo-deny bans cfg.
2. **`unmaintained = "all"` para detectar pacotes abandonados/EOL** — risco principal de pkg-mgmt não é quantidade, é abandono. Fonte: cargo-deny advisories.
3. **`cargo machete` / `bans.workspace-dependencies.unused`** — remover deps declaradas e não usadas (bloat + superfície de ataque). Fonte: cargo-deny + [training-data: cargo machete].
4. **`workspace-default-features = "warn"`** — controlar features default que arrastam deps pesadas. Fonte: cargo-deny bans.
5. **Pin + `cargo update` dentro do semver, `cargo outdated` para majors** — manter Cargo.lock fresco sem upgrades cegos. Fonte: `/rust-lang/cargo`.

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo-deny + cargo) — maintained by touring-quality_
