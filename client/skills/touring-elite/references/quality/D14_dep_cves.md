# D14 — Dependency CVEs (F2.5) — **⛔ BLOCK**

**Phase**: 2 (Security & Performance) | **Priority**: P0 | **Tier target**: 1.0 (no CVEs)
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_5_dep_cves::F2_5_DepCves`
**Enforcement**: **⛔ BLOCK** on PreToolUse:Write (fail-closed for Cargo.toml/package.json edits)
**Elite reference**: Snyk Open Source, Dependabot, OSV-Scanner

## Definition

Dependency CVEs = known vulnerabilities in third-party packages.
Detection: parse manifest (Cargo.toml, package.json, pyproject.toml) + check against CVE database.

## Thresholds

| Vulnerable deps | Score | Status | Action |
|-----------------|-------|--------|--------|
| 0               | 1.0   | ✅ Pass | No action |
| 1+              | 0.0   | ❌ Fail | **⛔ BLOCK** build |

## BLOCK Hook

```json
{
  "PreToolUse:Write": {
    "matcher": "Cargo.toml|package.json|pyproject.toml",
    "action": "BLOCK if F2.5 < 0.5",
    "canonical_fix": "Edit tool --path Cargo.toml --operation ssr (ver ~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md Pattern 5)"
  }
}
```

## MUST Commands

```bash
touring-quality check --gate F2.5 --target Cargo.toml
# 0 errors expected; FAIL = BLOCK in pre-write hook
```

## SHOULD Commands

```bash
# `Edit tool --path Cargo.toml --operation ssr --pattern '^<dep> = "1\.0\.[0-9]+"' --replacement '<dep> = "1.0.200"'` (OSV.dev / RustSec)  # Dependabot + Snyk config
touring-quality check --gate F2.5 --target Cargo.toml
```

## MAY Commands

```bash
# Production: integrate OSV.dev API for real-time CVE lookups
# curl -X POST https://api.osv.dev/v1/query -d '{"package":{"name":"serde","version":"1.0.130"}}'
```

## Context7 Best Practice

- `/rustsec/rustsec` — Rust Security Advisory database
- `/osv-dev/osv.dev` — Open Source Vulnerability database
- `/snyk/snyk` — Snyk CLI

## Common Pitfalls

- **Stale deps**: not updating Cargo.lock for years
- **Unpinned versions**: `serde = "*"` allows any version
- **Transitive deps**: vulnerable sub-dep not visible in Cargo.toml
- **Typosquatting**: `serder` instead of `serde` (supply chain attack)

## Auto-Remediation

```bash
# `Edit tool --path Cargo.toml --operation ssr --pattern '^<dep> = "1\.0\.[0-9]+"' --replacement '<dep> = "1.0.200"'` (OSV.dev / RustSec)
# Creates:
# - .github/dependabot.yml
# - .snyk
# - .osv-scanner.toml
# - scripts/audit-deps.sh (cron-friendly)
```

## Examples

- ✅ `serde = "1.0.200"` → no CVE, score=1.0
- ❌ `serde = "1.0.130"` (historical CVE) → FAIL, score=0.0, **BLOCK**

## Implementation

- Verifier: `~/projects/touring/crates/touring-quality/src/verifications/f2_5_dep_cves.rs`
- W1 MVP: static list of known-bad versions (placeholder)
- Production: replace with OSV.dev API client (W2+)
- Returns 0.0 if ANY known-bad version detected

## Rollout Plan (FP mitigation)

- W5 baseline: WARN for 30d
- After 30d FP < 5%: promote to BLOCK
- W6: integrate OSV.dev
- W7: full BLOCK enforcement

## Elite best practices (context7 — `/websites/embarkstudios_github_io_cargo-deny`)

1. **`cargo deny check advisories` contra RustSec advisory-db** — fonte canônica de CVEs Rust. Config: `[advisories] db-urls = ["https://github.com/RustSec/advisory-db"]`. Fonte: cargo-deny advisories.
2. **`yanked`, `unmaintained`, `unsound` explícitos** — `yanked = "warn"`, `unmaintained = "all"`, `unsound = "workspace"`. Captura risco além de CVE formal. Fonte: cargo-deny advisories cfg.
3. **`maximum-db-staleness = "P90D"`** — falha se a advisory-db estiver velha demais (evita falso-verde por DB desatualizada). Fonte: cargo-deny advisories.
4. **Ignore com justificativa + expiração, nunca silencioso** — `ignore = [{ crate = "x", reason = "..." }]` + `unused-ignored-advisory = "warn"`. Fonte: cargo-deny advisories cfg. Toda exceção é auditável.
5. **Cadeia transitiva, não só Cargo.toml direto** — auditar o grafo completo (`cargo audit` resolve Cargo.lock). [training-data: RustSec / OSV.dev] — sub-dep vulnerável não aparece no manifest.

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo-deny + RustSec) — maintained by touring-quality_
