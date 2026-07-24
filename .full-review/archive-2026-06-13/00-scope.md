# Review Scope

## Target

**Touring** — full Rust workspace at `~/.claude/rust`. Exhaustive, deep, multi-dimensional
code review with one north-star objective: **what stands between the current state and a
Premium, Elite-of-Market system/repository.**

Touring is the AI code-intelligence engine (daemon + CLI + MCP + Claude Code hooks) that
powers the TACO orchestration stack. This review is a **dogfooding-aware second pass**: the
2026-06-04 elite diagnostic (scored 5.2/10 across 9 dimensions) drove a 19-wave masterplan;
the 2026-06-13 in-loco verification confirmed the P0 monolith was decomposed and 8/8
credibility gaps closed (composite_health 0.63 → ~0.81). **This review must build ON that** —
not re-tread it — and find the deeper, code-level and market-readiness gaps that remain.

## Ground Truth (measured 2026-06-13, FACT [1.0])

| Signal | Value | Note |
|---|---|---|
| Crates | 46 | largest `touring-server` 67,887 LOC = **13.6%** (< 15% target — monolith resolved) |
| Source LOC (crates/*/src) | 498,697 | |
| Test fns | ~13,942 | 13,624 `#[test]` + 311 `#[tokio::test]` + 7 `#[rstest]` |
| Biggest prod file | `touring-hooks-core/src/knowledge.rs` 4,456 LOC | under 5k file-size gate |
| Biggest test file | `touring-dispatch/src/lifecycle/tests.rs` 19,296 LOC | test, not prod |
| `.unwrap()` non-test | **~3,686** | + 4,537 `.expect(`, 375 `panic!(`, 57 `unimplemented!/todo!` |
| `unsafe` occurrences | 424 | mostly FFI/bindings/landlock |
| Crate-root lints | 4 `deny` + 4 `forbid` + 12 `warn` | of 46 crates — **weak workspace lint policy** |
| Dependencies (Cargo.lock) | 1,558 packages | large supply-chain surface |
| clippy `--workspace -D warnings` | **0** | clean |
| wiring cycles | **0** | Tarjan SCC clean |
| composite_health_score | **~0.81** | North Star 0.85 not yet reached |
| Top docs | README, SECURITY, CONTRIBUTING, LICENSE-{MIT,APACHE}, CHANGELOG, ARCHITECTURE | present |
| CI | `.github/workflows/{ci.yml, release.yml}` | dogfooded Python gates |

## Files / Surface

Whole workspace. Per-dimension agents focus on representative high-signal areas:
- **Largest crates**: touring-server (67.9k), touring-intelligence (64.3k), touring-dispatch (37.5k), touring-cortex (31.8k), touring-hooks-core (31.8k)
- **Security-critical**: touring-ceg / gateway + capability (landlock LSM, sandbox, supervised exec), touring-offensive (cvc5 solver), `unsafe` sites, bindings
- **Public API surface**: touring-cli, touring-generator, the MCP tool surface, would-be touring-sdk
- **Hot paths**: hooks (pre_read/pre_edit/post_edit), tantivy index, knowledge.rs, enrichment

## Flags

- Security Focus: no (default) — but security is a first-class phase
- Performance Critical: no (default)
- Strict Mode: no (default)
- Framework: rust-workspace (auto-detected)

## Constraints (operator = TACO; non-negotiable)

- **NEVER git** (REGRA #11) — Touring is source of truth; review is read-only + report-only.
- **NEVER pkill/kill touring procs** (REGRA #19) — use `touring daemon-ctl` if needed.
- **Data 100% real** — no mocks, no invented symbols; cite `file:line` + CLI evidence.
- **NEVER** run `cargo test --test graph_service_e2e` (deterministic hang).
- Review is **advisory** — produces findings + action plan; no code mutation in this pass.

## Review Phases

1. Code Quality & Architecture
2. Security & Performance
3. Testing & Documentation
4. Best Practices & Standards (Rust idioms, deps, CI/CD, release/ops)
5. Consolidated Report — gap map to Premium Elite-of-Market
