# Review Scope — Touring Workspace (Premium-Elite Diagnostic)

> Run: 2026-06-20 · Re-invocation (prior run 2026-06-13 archived → `.full-review/archive-2026-06-13/`)
> Methodology: **Touring-grounded** — every finding anchored in real CLI/tool evidence (VGP / Symbol Verification), not LLM opinion. Agents synthesize + deep-dive on measured hot-spots.

## Target

**ALL** of `/home/gabrielgadea/.claude/rust/crates` — the Touring system itself. Read-only diagnosis against every elite/best-practice axis, to make Touring a Premium-Elite-of-Market repository.

## Inventory (FACT [1.0])

| Metric | Value | Source |
|---|---|---|
| Crate dirs | 50 (45 workspace members; 5 = A2-fusion shim dirs left on disk) | `ls crates` + elite payload |
| LOC src / workspace | **544,590 / 615,119** | `elite_aggregate.py` |
| Source `.rs` files | 1,400 | `find crates/*/src` |
| Test fns | 14,423 | elite payload |
| `#[cfg(test)]` modules in src | 1,093 | grep |

## Baseline metrics (FACT [1.0] — ground truth before agents)

| Axis | Measurement | Verdict |
|---|---|---|
| **Release composite** (`touring-elite`, 13 gates) | **Diamond** (~0.97–1.00; 04_performance now 1.0 PASS) | ✅ elite |
| **50-dim** (`touring-quality --workspace`) | **0.5909 "Unranked"** | ⚠ **but methodologically unfaithful at workspace scope — see "Verifier artifacts" below** |
| **Dependency cycles** (`wiring cycles`) | **0** (Tarjan SCC) | ✅ acyclic |
| **clippy** `--workspace --all-targets` | **0 warnings/errors** (cached) | ✅ (elite `[workspace.lints]`: `clippy::all=deny` p-1 + RBP-11 ratchets) |
| **cargo-deny** | advisories ✅ · licenses ✅ · sources ✅ · **bans ❌** | duplicate `schemars`+`schemars_derive` (+ image/tiff) — D08/D44 |
| **Modularization** | **154 src files >800 LOC; 27 >2000 LOC** | ⚠ structural debt headline |
| **Orphan pub symbols** | raw 4,823 (`status` / `wiring orphans`) | ⚠ needs triage — incl. STALE `touring-telemetry` entries (absorbed W3.6, path gone) + cross-crate pub API + dead code |
| **Runtime health** | doctor 6/6 · `composite_health` 0.577 · ema_reward 0.18 | ✅ daemon healthy; composite is a separate runtime metric |
| Debt signals (raw, src incl. inline tests) | unwrap 4064 · expect 3257 · unsafe 406 · panic! 320 · todo! 26 · unimplemented! 13 · TODO/FIXME 190 | prod-unwrap ≈low after RBP-01 100% (49/49 crates locked); raw count dominated by `#[cfg(test)]` |

## Hot-spots (deep-dive targets for agents)

**Biggest crates (LOC):** touring-server 70.9k · touring-intelligence 67.9k · touring-dispatch 37.5k · touring-cortex 33k · touring-hooks-core 32k · touring-bindings 30k · touring-code 28.5k · touring-foundation 26.9k.

**Biggest single SOURCE files (excl tests):**
1. `touring-generator/src/core/context.rs` — 4,509
2. `touring-hook-handlers/src/hooks/pre_read.rs` — 3,824
3. `touring-hooks-shared/src/gate_metrics.rs` — 3,468  (+ `touring-foundation/src/gate_metrics.rs` — 3,468 ← **possible duplication, identical LOC**)
4. `touring-hook-runtime/src/hook_runtime.rs` — 3,102
5. `touring-server-reasoning/src/reasoning/decomposer.rs` — 2,843
6. `touring-cortex/src/handlers/enrichment.rs` — 2,747
7. `touring-cli/src/cli/handlers/decompose.rs` — 2,701
8. `touring-cli/src/cli_suggester.rs` — 2,693
… 27 files total >2000 LOC.

**Biggest test file:** `touring-dispatch/src/lifecycle/tests.rs` — 19,150 LOC (test-only).

## Repo hygiene

Present: `deny.toml` · `.github/workflows/{ci,release}.yml` · `SECURITY.md` · `CONTRIBUTING.md` · `CHANGELOG.md` · `README.md` · `LICENSE-MIT` · `.config/nextest.toml`.
**Missing:** `rust-toolchain.toml` (MSRV/toolchain pin — RBP-04) · `rustfmt.toml` · `clippy.toml` · `CODEOWNERS` · `LICENSE-APACHE` (only MIT present — dual-license intent unconfirmed).

## Verifier artifacts (avoid false alarms — meta-findings about the quality engine)

The `touring-quality` 50-dim engine produces **unfaithful workspace-scope scores** (root cause of the 0.59 vs Diamond divergence). Treat these as engine defects to fix, NOT crate defects:
- **F1.1 (complexity):** sums CC over the entire directory ("CC≈5218 loc=56800") → always Fail. Should be **per-function**, aggregated.
- **F1.2 (maintainability):** counts 25,006 short-ids across 544k LOC at aggregate → always penalized.
- **F2.1/F2.4 (OWASP/secrets):** **false-positive on `touring-quality`'s own detector source** (its regex/test-fixtures for `ghp_`/`sk_live_`/high-entropy literals trip its own scanner). Needs self-exclusion (`*/touring-quality/src/*`).
- Two health composites diverge (doctor 0.70 · runtime 0.577 · elite Diamond) — reconcile the "one true score" narrative.

## Flags

Security Focus: implied-on · Performance Critical: implied-on · Strict Mode: off · Framework: rust-workspace (auto)

## Review Phases (mapped to the 50-dim taxonomy)

1. **Code Quality & Architecture** (F1.x) — code-reviewer + architect-review
2. **Security & Performance** (F2.x) — security-auditor + performance-engineer
3. **Testing & Documentation** (F3.x) — test-automator + docs-architect
4. **Best Practices & CI/CD** (F4.x) — rust-pro + deployment-engineer
5. **Consolidated Report** — final synthesis, P0–P3 action plan

## Constraints honored

No git (REGRA #11) · no pkill touring (REGRA #19) · read-only/advisory (no code edits — markdown only) · every claim cites `file:line` or CLI output (no hallucinated symbols) · REGRA #0 (orphans surfaced, never silently dropped) · REGRA #21 (no failure dismissed by origin/age).
