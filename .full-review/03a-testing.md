# Phase 3a: Testing (F3.1–F3.7)

> Authored by the orchestrator (the test-automator subagent thrashed context twice; gathered via capped CLI). Evidence = real counts / `file:line`.

## Verdict: strong base, three precise gaps. 0 Critical · 2 High · 3 Medium · 2 Low.

## Pyramid (F3.3) — ✅ ELITE (base-heavy, not ice-cream-cone)
`11,666` unit `#[test]`/`#[tokio::test]` · `177` integration (`crates/*/tests/`) · `89` `*_e2e.rs` · `30` criterion benches · `5` loom concurrency proofs (`touring-loom-proofs/`). Ratio is healthy.

## Findings

| # | Sev | Dim | Finding | Evidence | Fix |
|---|-----|-----|---------|----------|-----|
| **T1** | **High** | F3.4 | **Untrusted-input parsers have 0 property/fuzz tests** — `touring-ast-polyglot`, `touring-ast`, and **`touring-rkyv` (IPC deserialization of arbitrary bytes)** have 0 `proptest!` files; **0 fuzz dirs workspace-wide**. A code-intelligence tool parses arbitrary source + deserializes rkyv over IPC — the highest-value unfuzzed surface. | `grep proptest! crates/touring-{ast,ast-polyglot,rkyv}/src` = 0; `find -type d -name fuzz` = 0 | `cargo-fuzz` targets for the polyglot parser + rkyv decode; proptest roundtrip invariants (`decode(encode(x))==x`) |
| **T2** | **High** | F3.1/CI | **CI gates only `cargo test --workspace --lib`** (`ci.yml:80`) — the 177 integration + 89 e2e + doctests do NOT run in CI. The full `--no-fail-fast` suite is green *locally* but CI can't catch integration/e2e regressions. `.config/nextest.toml` exists but is never invoked in `ci.yml`. | `ci.yml:79-80` | Add a CI job: `cargo nextest run --workspace --no-fail-fast` + `cargo test --doc` |
| **T3** | Medium | F3.7 | **No binding p99 guard on the actual hot path.** p99 budget tests EXIST (`touring-code/tests/latency_p99_guard.rs` ×4, `touring-hooks/tests/predictive_wave_p99_guards.rs` ×4) — but none guards `hook_dispatch_latency` (the p99=199ms offender, Phase-2 P-1). The pattern is right; it's just not applied to the path that's slow. | guards hit touring-code + predictive-wave, not dispatch | Add `hook_dispatch_p99_guard` asserting end-to-end dispatch p99 < 50ms (catches P-1 regression) |
| **T4** | Medium | F3.6 | **SEC-02 (web bind 0.0.0.0+no-auth) has no negative security test.** SEC-01 path-traversal has 5 (✅ credit). The bindings tests are `e2e_generator_health/e2e_roundtrip/integration_test/smoke/web_css_contract` — none asserts loopback-default or unauth rejection. | `crates/touring-bindings/tests/` listing; bind strings only in `src/web/` | Negative test: default bind == `127.0.0.1`; `POST /api/mcp/call` without bearer → 401 |
| **T5** | Medium | F3.2 | **Mutation testing is advisory, not gated.** `cargo-mutants` is configured (`Cargo.toml:113-123`, W2.7 baseline) + CI runs `--in-diff`, but no mutation-SCORE floor gates merges. Coupled to Phase-2 P-2 (mutants spawned on every edit but score never enforced). | `Cargo.toml:113` "(advisory)" | Set a mutation-score floor on the critical crates (foundation/ceg/dispatch) in CI |
| **T6** | Low | F3.5 | **1 `#[ignore]` masks a real deferred bug** — `cli_decompose_ready` "Wave 8 collateral … delegate returns unexpected shape … needs investigation". The other 36 ignores are legitimate env-gates (18 "requires daemon socket", model-download, bare-metal SIMD). | `grep #[ignore] … "needs investigation"` | Fix the shape bug + re-enable, or convert to a tracked issue (REGRA #21 spirit) |
| **T7** | Low | F3.1 | **Hot-path coverage 0.40–0.52** (TDG coverage dim, Phase-1 Q5) on the biggest/most-changed prod files (`hook_runtime.rs`, `post_edit.rs`, `decompose.rs`) — the enabler for safely splitting them. | Phase-1 TDG | Behavior tests on the extracted units (after the F1 splits) |

## Verified-elite (do not regress)
11.6k unit tests + healthy base-heavy pyramid · **30 criterion benches + versioned baseline regression gate** (elite gate 04_performance PASS) · **5 loom concurrency proofs** (model-checking — rare/elite) · p99-guard pattern present (8 asserts) · SEC-01 = 5 negative tests · nextest config authored · CI clippy `--all-targets -D warnings` + missing_docs gate + sync_metrics drift gate.

## Top 2 by impact
1. **T1** — fuzz/proptest the rkyv-IPC + polyglot-parser untrusted-input surface (security + robustness; highest unfuzzed value).
2. **T2** — make CI run the integration+e2e+doctests (nextest), not just `--lib`; today CI under-gates what the local suite already proves.
