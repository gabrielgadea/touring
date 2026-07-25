---
name: touring-elite
description: Auto-load rule for the Touring Elite 17-Dimensional Harness. Activates on any code-intent (edit/refactor/audit/release) to surface the composite EliteScore and prevent regression below Diamond tier. Companion to the touring-elite skill (load on demand for full docs).
---

# Touring Elite — Auto-Load (constitutional, 2026-06-13)

> **Composite Score 0.9703 (Diamond tier)** — see [skill](file:///home/gabrielgadea/.claude/skills/touring-elite/SKILL.md) for full 17-dim catalog.

## Mandatory Reflex Triggers

| Trigger | Action | Exit Criterion |
|---------|--------|----------------|
| User says "release", "ship", "merge", "PR", "publish" | `python3 docs/elite_aggregate.py --check` | tier ≥ Gold (0.80) |
| User says "audit", "review", "quality check" | `python3 docs/elite_aggregate.py --json` | composite ≥ 0.95 (Diamond) |
| User says "refactor" + file > 500 LOC | `touring ast tdg <file>` first | grade ≥ B |
| User says "add new public API" | `touring index find <SymbolName>` (VGP) + `touring pre-edit` | score ≥ 0.8 |
| 13-gate score drops below 0.80 (Gold) | BLOCK; suggest remediation | composite ≥ 0.80 |

## Quick Reference (1 command)

```bash
# Full composite with breakdown
python3 docs/elite_aggregate.py --check

# 6-tier mapping: Diamond 0.95+ | Platinum 0.90+ | Gold 0.80+ | Silver 0.70+ | Bronze 0.60+ | Unranked <0.60
```

## The 13 Aggregated Gates (canonical)

| # | Gate | Weight | Default Status |
|---|------|--------|----------------|
| 02 | architecture | 1.0 | PASS (wiring_integrity_gate) |
| 03 | security_advisories | 1.5 | PASS (cargo-deny) |
| 04 | performance | 0.7 | ADVISORY (perf_p99_gate) |
| 05 | testing | 1.0 | PASS (file_size_gate) |
| 06 | documentation | 1.0 | PASS (gen_reference) |
| 08 | ci_cd_devops | 0.8 | PASS (root_hygiene_gate) |
| 09 | modularization | 0.8 | PASS (file_size_gate) |
| 10 | scalability | 0.7 | PASS (scalability_scan) |
| 11 | extensibility | 0.6 | PASS (extensibility_scan) |
| 14 | craftsmanship | 0.7 | PASS (craftsmanship_tdg_gate) |
| 15 | dependencies | 1.5 | PASS (cargo-deny) |
| 16 | ux | 0.6 | PASS (ux_audit) |
| 17 | product_docs | 0.9 | PASS (sync_metrics) |

## CI Integration (already wired in `.github/workflows/ci.yml`)

```yaml
- name: elite_aggregate — composite EliteScore
  run: python3 docs/elite_aggregate.py --check
```

**BLOCK-by-default**: any BLOCK-tier FAIL fails the build.

## Hard Rule (constitutional)

If `touring-elite` composite drops below **0.80 (Gold tier)**, the assistant MUST:
1. Surface the regression to the user
2. Identify the failing gate(s)
3. Propose remediation before proceeding with any release-tagged action

If between **0.80-0.94 (Silver-Platinum)**, advisory only (WARN) — proceed with human review.

If **≥ 0.95 (Diamond)**, no special action — release-ready.

## Files (canonical)

- **Composite scorer**: `docs/elite_aggregate.py` (180 LOC)
- **Per-gate scripts**: `docs/{perf_p99_gate, scalability_scan, extensibility_scan, ux_audit, craftsmanship_tdg_gate, root_hygiene_gate}.py`
- **Skill** (on-demand): `~/.claude/skills/touring-elite/SKILL.md` (169 LOC)
- **Audit suite**: `docs/touring-elite-audit.sh` (one-shot)

## Cross-references

- TACO protocol: `~/.claude/skills/Touring/references/TACO-subagent-rule.md`
- Touring CLI ranks: `~/.claude/rules/touring-cli-index.md`
- Decision matrix: `~/.claude/rules/touring-decision-matrix.md`
- Tool combination patterns: `~/.claude/rules/tool-combination-patterns.md`
- Skill (full docs): `~/.claude/skills/touring-elite/SKILL.md`
