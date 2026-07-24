# Phase 3b: Documentation (F3.8–F3.13)

> Half of the Testing & Documentation phase. Read-only. Every finding cites a doc `file:line`
> quote vs the contradicting code/metric, or a literal CLI/gate exit code. The workspace ships
> **its own drift detectors** (`sync_metrics.py`, `gen_reference.py`, `touring evolution drift`)
> — I ran them. The headline is not "docs are thin"; the docs are rich. The headline is that
> **two doc-drift gates are RED right now** and the README/baseline "Diamond" narrative is
> **asserted, not measured** — the live composite is **Gold 0.8856**, dragged down by a
> `06_documentation = 0.00 FAIL`.

## Verdict

**Rich, self-policing, drift-aware — but currently failing its own honesty gates.**
0 Critical · 3 High · 4 Medium · 3 Low. The documentation *infrastructure* is genuinely
elite (a metrics-as-code drift gate wired into CI is a real USP). The defect is that the
gate is **red and has been left red**: ARCHITECTURE.md is stale, `modules.md` is stale, and
the most quoted marketing number ("Diamond") is contradicted by the repo's own aggregator.

## Live evidence (ground truth, run 2026-06-21)

| Probe | Command | Result | Exit |
|-------|---------|--------|------|
| ARCH drift gate | `python3 docs/sync_metrics.py --check` | `DRIFT: ARCHITECTURE.md crate inventory block is stale` | **1 (RED)** |
| Doc reference gate | `python3 docs/gen_reference.py --validate` | `DRIFT: modules.md out of sync` | RED (drives 06_documentation=0.00) |
| Composite | `python3 docs/elite_aggregate.py --check` | **`composite=0.8856 tier=Gold`** · `06_documentation score=0.00 status=FAIL` · `14_craftsmanship 0.50 WARN` | 0 |
| `missing_docs` ratchet | `grep -l deny(missing_docs) crates/*/src/lib.rs` | **47 / 48** lib crates | — |
| Authoritative crate count | `cargo metadata … workspace_members` | **45** | — |
| Measured LOC/tests | `sync_metrics.py --json` | `crates=45 loc_src=537343 loc_workspace=607872 test_fns=14272` · biggest `touring-server (71047)` | — |
| Doctests in CI | `grep 'test --doc' .github/workflows/` | **none** | — |

---

## F3.8 — Inline Documentation — **Verified strong** (1 Low)

**`#![deny(missing_docs)]` is enforced in 47 of 48 workspace lib crates** (grep over
`crates/*/src/lib.rs`). The prior DOC-06 ratchet claim is **verified and even exceeded** —
this is genuinely elite coverage; almost every public item across 537k LOC must be documented
or the build fails. The 1 crate not on `deny` is `touring-quality` (on `warn(missing_docs)`),
and `touring-foundation` carries **both** `warn` and `deny` lines (the `deny` wins).

- **Low — DOC-INLINE-1:** `touring-quality` uses `#![warn(missing_docs)]` not `deny`
  (`crates/touring-quality/src/lib.rs`), so its public surface can silently lose docs. It is
  the workspace's own 50-dim quality engine — it should hold itself to the bar it enforces.
  **Fix:** promote to `deny(missing_docs)` (1-line ratchet), same as the other 47.

**Not a defect (do not chase):** raw `/// ```` doctest blocks = 272 across src, of which
34 are `rust`, 56 `ignore`, 42 `text`, 38 `json` — the inline-doc *coverage* is enforced
mechanically, so a manual sample of undocumented `pub` items is moot: `cargo doc` would have
failed the 47 `deny` crates.

---

## F3.9 — API Documentation — **Medium** (publishable surface vs no doctests in CI)

- **Medium — DOC-API-1 (doctests never run in CI):** there are **51 fenced doc blocks** in
  src and **34 `/// ```rust`** runnable examples, but **`cargo test --doc` appears in zero CI
  workflows** (`grep 'test --doc' .github/workflows/` → empty; CONTRIBUTING.md:12 lists
  `cargo test --workspace` but not `--doc`). Doctests are documentation that *can lie* if
  never compiled. The README's value proposition is "typed, auditable" — runnable examples
  that are never executed undercut that. **Fix:** add `cargo test --doc --workspace` to
  `ci.yml` (1 step), wiring the 34 rust doctests into the gate (D34/D35 best practice:
  examples that can't drift).

- **Note (not a defect):** **only 2 of 45 crates carry `publish = false`** — i.e. ~43 crates
  present a *publishable* API surface. Combined with the 47-crate `deny(missing_docs)` ratchet,
  the rustdoc surface is real and near-complete. If public crates.io release is intended (the
  README markets install + license tiers), the doctest-in-CI gap (above) is the one real hole.

---

## F3.10 — Architecture Documentation — **High** (the A2 drift, quantified)

ARCHITECTURE.md (789 lines) is the canonical detailed reference and is **internally
contradictory and stale**:

- **High — DOC-ARCH-1 (A2 confirmed + quantified):** `sync_metrics.py --check` exits **1**
  (`DRIFT: ARCHITECTURE.md crate inventory block is stale`). The exact divergences:
  - **Crate count is self-contradictory in the same file:** line 3 says `45 crates`, line 142
    says `**45** workspace crates`, **but line 158 (the auto-generated inventory block) says
    `**49 crates · 532,185 LOC (src)**`**. Authoritative = **45** (`cargo metadata` +
    `sync_metrics`). The "49" is a stale snapshot from the 47→13 residual-plan era.
  - **METRICS comment drift** (`ARCHITECTURE.md:4`): declares `loc_src=532180, loc_workspace=602584,
    test_fns=14292` (dated "measured in loco 2026-06-15"); measured today =
    `loc_src=537343 (+5,163 / +1.0%), loc_workspace=607872 (+5,288), test_fns=14272 (−20)`.
  - **Per-crate inventory stale** (`ARCHITECTURE.md:160`): `touring-server | 70,605` vs measured
    `71,047`; `touring-foundation | 22,760` (line 166) while the prior Phase-1 hotspot table and
    elite payload list it ~26.9k. The whole `<!-- CRATES:BEGIN -->` block needs regeneration.
  - **Fix:** `python3 docs/sync_metrics.py --sync` (regenerates the block + METRICS comment
    deterministically). **The gate is already in CI** (`ci.yml:88-89`) — so this drift means
    **CI is currently failing the anti-drift step**. (Correction to Phase-1 A2's "wire `--check`
    into CI": it *is* wired; the action is to **run `--sync` and commit**, not to add the gate.)

- **High — DOC-ARCH-2 (no ADRs, no C4/mermaid):** there are **5 RFCs** (`docs/RFC-001..005`)
  + `CONSTITUTION-v8.md` — good for cross-cutting contracts — but **no `docs/adr/` directory
    and zero ADR files** (`ls docs/ | grep -i adr` → empty), and **ARCHITECTURE.md contains 0
    mermaid/C4 diagrams** (`grep -c 'mermaid\|C4\|flowchart' ARCHITECTURE.md` → 0). For a repo
    aspiring to public release with this much architectural decision history (the 47→13 crate
    fusion, A2/A5 relocations, monolith decomposition), the *why* of those decisions lives
    only in `rust/docs/YYYY-MM-DD-*.md` session reports and memory — not in durable,
    discoverable ADRs. CONTRIBUTING.md:43 even references "an RFC under `docs/rfcs/`" — that
    directory **does not exist** (RFCs are flat in `docs/`). **Fix:** create `docs/adr/` with a
    MADR template; backfill the ~6 keystone decisions (crate-count target, layer model,
    monolith split, CEG sandbox model) as ADRs; add at least one C4-Context mermaid diagram to
    ARCHITECTURE.md (the README's ASCII layer box is a start but not navigable/versioned-as-code).

**Verified-strong:** the ASCII 4-layer diagram in `README.md:57-81` is accurate (matches the
verified acyclic layering from Phase-1 A-section); ARCHITECTURE.md correctly adopts a
**Diátaxis split** (line 6: detailed-reference here, narrative in `docs/explanation/`,
how-to in `docs/how-to/`) — a mature documentation architecture.

---

## F3.11 — README — **Medium** (elite structure, stale numbers)

`README.md` (131 lines) is **structurally elite for a public repo**: tagline, badges,
5-minute quick-start with copy-paste commands, "What it is / is NOT", layer diagram,
stability+license matrix, "where to next" links. This is best-in-class scaffolding.

- **Medium — DOC-README-1 (claim drift):** the README asserts numbers that are stale or
  unverifiable:
  - `README.md:12` — "**45 crates** totaling **~532k LOC**" → LOC is now 537k (+1%); the "45"
    is correct but ARCHITECTURE.md's own inventory says 49 (inconsistency between the two
    top-level docs).
  - `README.md:14` — "**198 lifecycle hooks**" while the footer (`README.md:127`) says
    "hooks: 218" and `TACO-task` skill says 140 — **three different hook counts in the repo**.
    The hook count is not gated by `sync_metrics` (which covers crates/LOC/tests only).
  - `README.md:127` — footer claims `e2e: 0.83 → target 0.90` and `index: 2,147 files /
    52,824 symbols (rebuilt 2026-06-06)` — a 2-week-old hardcoded snapshot presented as live.
  - **Fix:** extend `sync_metrics.py` to also emit hook/CLI/MCP-tool counts (it already walks
    the tree) and template them into README + ARCHITECTURE, closing the 198-vs-218-vs-140
    contradiction; or mark the footer counts explicitly "snapshot @ date" (the HTML comment at
    `README.md:128` half-does this for index counts but not for hooks/e2e).

- **Low — DOC-README-2:** install command `curl -fsSL https://touring.dev/install.sh | sh`
  (`README.md:25`) and `docs/landing/index.md` link point to an unpublished domain/path — fine
  for a pre-release repo, but a public-release blocker to verify before shipping.

---

## F3.12 — Documentation Accuracy — **High** (THE BIG ONE: asserted ≠ measured)

This dimension is where the workspace's own USP turns on itself: it *has* drift gates, and
they are *failing*, and the headline marketing claim is *contradicted by the aggregator*.

- **High — DOC-ACC-1 (Diamond is asserted, Gold is measured):** the review baseline
  (`00-scope.md:24`) and the constitution narrative claim **"Diamond (~0.97–1.00)"**. The
  repo's **own aggregator disagrees**: `python3 docs/elite_aggregate.py --check` →
  **`composite=0.8856 tier=Gold`**, with **`06_documentation score=0.00 status=FAIL`** and
  `14_craftsmanship 0.50 WARN`. The "Diamond 0.9703" figure (cited in `touring-elite.md` and
  memory) is a **historical high-water mark, not the current state** — the doc gate regressed
  it to Gold. Per REGRA #21, this is a live failing gate, not a stylistic nit. **Fix:**
  `sync_metrics.py --sync` + `gen_reference.py` (regenerate `modules.md`) → both gates green →
  `06_documentation` returns to PASS → composite climbs back toward Diamond. Then **stop
  citing "Diamond" until `elite_aggregate --check` literally prints it.**

- **High — DOC-ACC-2 (`modules.md` drift / the 06_documentation FAIL):**
  `python3 docs/gen_reference.py --validate` → `DRIFT: modules.md out of sync`. This is the
  *direct cause* of `06_documentation = 0.00` (elite_aggregate.py:53 runs
  `gen_reference.py --validate` as a **block-tier** gate, weight 1.0). The generated module
  catalog under `docs/reference/modules.md` has diverged from the code. **Fix:**
  `python3 docs/gen_reference.py` (regenerate), commit.

- **Cross-checked claims (3, per the brief):**
  1. "45 crates" — **TRUE** (`cargo metadata` = 45). ✅
  2. "Diamond tier" — **FALSE today** (`elite_aggregate` = Gold 0.8856). ❌
  3. "ARCHITECTURE.md auto-synced / not drifting" (CONTRIBUTING.md:23-25) — **FALSE today**
     (`sync_metrics --check` exit 1). ❌

- **Note:** `touring evolution drift` is referenced as a USP but the *Python* gates
  (`sync_metrics`, `gen_reference`) are the ones actually wired into CI and are the
  authoritative drift signal here — both red.

---

## F3.13 — Changelog / Migration — **Medium**

`CHANGELOG.md` (1,416 lines) has a **two-zone structure**: an auto-synthesized TOON-checkpoint
zone at the top (`changelog_synth.py`, from 102 checkpoints) and a hand-curated
Keep-a-Changelog zone below.

- **Medium — DOC-CL-1 (not consumer-facing Keep-a-Changelog at the top):** the auto-synth zone
  (lines 1-~170) is a **checkpoint dump**, not a changelog: entries like
  `**cross-audit-v3-1-0** (unknown): (checkpoint)` and `(unknown)` authorship are **internal
  wave-history noise**, exactly what Keep-a-Changelog says to avoid ("written for humans, not a
  git/checkpoint dump" — D39). The hand-curated tail **is** proper Keep-a-Changelog (e.g.
  `## [29.7.0] - 2026-03-20` / `### Added` / `### Fixed` with crate-scoped bullets). But the
  top zone is what a consumer reads first.
- **Medium — DOC-CL-2 (SemVer + breaking-change discipline gap):** version headers are
  inconsistent — proper SemVer tags (`## [30.3.0]`, `## [29.7.0]`) coexist with **non-version
  headers used as releases**: `## [Unreleased] - 2026-04-14`, `## [Unreleased] - 2026-04-12`,
  `## [Unreleased] - 2026-04-11` (three dated "Unreleased" blocks — an oxymoron) and
  `## [Predictive Wave]`, `## [GPU Optimization Wave]`, `## [Multi-Core Scaling…]` (wave names,
  not versions). **The A2/A5 relocations and the `schemars` 0.8→1.2 pin (Phase-1 A1 / SEC-06)
  are NOT documented as consumer migration entries** — a consumer of `touring-harness-mcp`
  would get no `### Changed`/breaking note. **Fix:** collapse the synth zone into a single
  `### Internal` fold or move it to `docs/`; enforce SemVer headers (no dated "Unreleased");
  add `### Changed` entries for A2/A5 crate moves + the schemars pin, with a one-line migration
  note (D39 + D42 deprecate-before-remove).

---

## Already elite (verified, do NOT regress)

- **Metrics-as-code drift gate is a genuine USP.** `sync_metrics.py` walks the tree
  deterministically (zero-LLM) and `--check` byte-verifies the ARCHITECTURE.md inventory block
  + METRICS comment, **wired into `ci.yml:88-89`**. Most repos *aspire* to "docs generated from
  code"; this one mechanizes it. The problem is operational (left red), not architectural.
- **47/48 lib crates enforce `#![deny(missing_docs)]`** — inline-doc coverage is mechanically
  guaranteed across 537k LOC. Elite.
- **Diátaxis-structured docs** (ARCHITECTURE.md:6): reference / explanation / how-to split,
  with `docs/reference/` auto-generated catalogs (generators · mcp-tools · hooks · modules).
- **SECURITY.md + CONTRIBUTING.md present and substantive** — SECURITY.md (50 lines) has a real
  scope/out-of-scope section, concrete hardening notes citing `file:line`
  (`sandbox_executor.rs`, `enforce_path_within_roots`, `enforce_linux.rs`), and a private
  disclosure process. CONTRIBUTING.md (49 lines) lists the exact 5-command quality-gate
  contract and the no-drift principle.
- **Rich `rust/docs/` corpus**: 124 markdown files + 5 RFCs + Constitution v8.0 — deep
  institutional knowledge (the *why* exists, it's just in session-reports rather than ADRs).
- **README is structurally best-in-class** for an open-source release (quick-start, is/is-NOT,
  license tiers, where-to-next). Only the *numbers* drift, not the structure.
- **CHANGELOG hand-curated zone** is correct Keep-a-Changelog with crate-scoped SemVer entries.

---

## Severity roll-up

| Sev | Count | Findings |
|-----|-------|----------|
| Critical | 0 | — |
| High | 3 | DOC-ARCH-1 (ARCHITECTURE drift, CI red), DOC-ARCH-2 (no ADRs/C4), DOC-ACC-1 (Diamond asserted vs Gold measured) + DOC-ACC-2 (modules.md drift = 06_documentation FAIL) |
| Medium | 4 | DOC-API-1 (doctests not in CI), DOC-README-1 (claim/hook-count drift), DOC-CL-1 (synth zone not Keep-a-Changelog), DOC-CL-2 (SemVer + missing A2/A5/schemars migration) |
| Low | 3 | DOC-INLINE-1 (touring-quality warn not deny), DOC-README-2 (unpublished install URL), (DOC-ACC cross-check noise) |

## One-shot remediation (the 3 reds → green, ~5 min, no code)

```bash
python3 docs/sync_metrics.py --sync       # fixes DOC-ARCH-1 (crate 49→45, LOC, per-crate block)
python3 docs/gen_reference.py             # fixes DOC-ACC-2 → 06_documentation PASS
python3 docs/elite_aggregate.py --check   # re-verify: composite should climb Gold→~Diamond
# then: stop citing "Diamond" anywhere until this last command literally prints tier=Diamond
```

These three commands flip two CI-gating drift checks from RED to green and move the composite
off the `06_documentation=0.00` cliff — the single highest-leverage doc action in the repo.
