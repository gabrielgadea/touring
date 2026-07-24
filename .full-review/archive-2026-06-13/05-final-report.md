# Comprehensive Code Review Report — Touring → Premium Elite-of-Market

> Workspace: `~/.claude/rust` · 46 crates · 498,697 src LOC · ~13,942 test fns · 2026-06-13
> Operator: TACO · 8 specialist agents · 4 phases · read-only/advisory pass
> Detail: `.full-review/0[1-4]*.md` (+ `0Xa/0Xb` per-dimension)

## Executive Summary

Touring is **substantially better than its own 2026-06-04 diagnostic (5.2/10) implied** — the P0 monolith was decomposed (cycles=0, clippy `-D warnings`=0, largest crate 13.6%), the prototype-era robustness debt was largely paid (real prod `unwrap` ≈124, not 3,686), and it contains genuinely **elite-grade** components: the Code Execution Gateway (deny-by-default + landlock fail-closed, proven by E2E), a panic-guarded fail-open actor, ~89% single-source `[workspace.dependencies]`, and zero lock-across-`.await` UB.

But the review surfaced **one true P0 security hole, a measured hook-latency tail, and a release/CI posture that has literally never run.** The dominant pattern across all 8 dimensions is not missing capability — it is **"invented but not binding":** the elite mechanism for nearly every gap already exists in-repo (workspace.lints, deny.toml, nextest slow-timeout, cargo-fuzz, llvm-cov, the CEG capability model, the doc-as-code gates) — it is just **shallow, unwired, unrun, or fail-open.** Reaching elite is therefore overwhelmingly an **activation + binding + ratcheting** exercise, not green-field building. That is a cheap, high-confidence path — and it is exactly what closes the masterplan's remaining public-release waves (B-W1/B-W3/B-W4).

## Findings by Priority

### P0 — Critical (act immediately)

| ID | Finding | Source | Note |
|---|---|---|---|
| **SEC-01** | `touring_file_ops` MCP tool = unrestricted arbitrary FS read/write/delete on un-canonicalized paths, **bypassing the CEG**. Prompt-injection → `~/.ssh/id_rsa`, overwrite `authorized_keys`, `rm -rf ~`. `tools_core.rs:1050-1370`,`:1066` | 2A · corroborated by T-01 | **Exploitable today.** The single act-now item. |
| **T-01** | The containment invariant for SEC-01 has **zero** tests (the one existing test covers a different, unused guard = false assurance) | 3A | The *binding* for SEC-01. |
| **SEC-03** | `cargo deny check advisories` is **RED** — postgres-protocol RUSTSEC-2026-0179 **CVSS 8.7** + pyo3 0.24 + tokio-postgres; **ungated** in CI | 2A/4A/4B | Live advisory; point-release fixes exist. |
| **F1** | Inline full-workspace recursive `read_dir` E2E scan on every `post_edit/post_write`, on the serial actor → the measured hook tail (p99=488ms, p999=1.30s) | 2B | Reliability/UX P0. |
| **F2** | Heavy handlers run inline on the serial per-project actor → head-of-line blocking (timeout doesn't stop the work) | 2B | Structural latency. |
| **T-03** | No perf gate on the real hook dispatch tail — F1/F2 can silently regress | 3A | The *binding* for F1/F2. |
| **DOC-01 / CICD-04** | Binary prints `0.1.0` while README says `30.0.0`, ARCHITECTURE `v30.3.6`, install.sh `v31.0.0`, brew `0.30.0`; `publish=false` blocks `cargo install` | 3B/4B | Trust + release blocker. |
| **DOC-02** | ARCHITECTURE.md **body** still describes the pre-decomposition architecture (phantom crates, "touring-hooks 127,575 LOC", "38 crates" total) — contradicts its own G-1-synced header; `sync_metrics.py` only checks the first match so body drift is invisible | 3B | Self-contradicting flagship doc. |
| **CICD-01** | **No `.git` directory** → the entire CI + release pipeline has never run and cannot fire (Potemkin CI). Root of B-W1 | 4B | **Gabriel-only** (REGRA #11). |
| **T-02** | `graph_service_e2e` hangs (spawns real binary, no timeout) → server-integration + daemon-RPC tier untested in CI | 3A | Coverage hole on the server layer. |

### P1 — High (before public release)

- **A1** `touring-server` is the next monolith (67.9k, two products: CLI + MCP) → split `touring-cli-app` + `touring-mcp`. (2B build-time + 1B)
- **MCP-behind-CEG** (SEC #1 lever) — route all path/exec MCP tools through the CEG capability model + canonicalize/root-guard; neutralizes SEC-01/02/08 at once; revive the dead `mcp-curated` gate (A6) as the vehicle. (2A/1B)
- **RBP-01/02 + lint ceiling** — install elite `[workspace.lints]` (unwrap_used/expect_used/missing_docs/indexing_slicing/curated pedantic), give the 8 lint-escapee crates `[lints] workspace=true`, ratchet `deny` outward from the clean CEG. (1A/4A)
- **CI binding** — one `fmt-deny-doc` job (`cargo fmt --check` + `cargo deny check` + `cargo doc -D warnings`), un-defang the `|| true` / advisory gates, gate `cargo nextest` (config already exists), add fuzz smoke + llvm-cov floor. (4B/3A)
- **SEC-04/SEC-05** — sandbox forwards cloud credentials by default (contradicting SECURITY.md); transcript miner persists unredacted secrets (`redact_secrets` exists, unused). (2A/3B)
- **F3/F4** — no hook execution budget; rkyv ipc *response* path does full deserialize (request path is fine). (2B/4A)
- **RBP-03** — typed public errors (thiserror `#[from]`) for the SDK; 373 `map_err(format!)` + 141 `-> Result<_,String>`. (4A)
- **A2** — finish the shim fusion (touring-{ast,learning,cognitive,antt,wasm} double-naming trap). (1B)
- **DOC-06 / missing_docs ratchet** onto touring-ceg → touring-server → touring-intelligence (1,756 undocumented pub items); add `cargo doc -D warnings`. (3B)
- **SEC-06 / RBP-05** — `unsafe impl Send for HookRuntime` with no SAFETY comment (latent UB). (2A/4A)
- **RBP-04 / MSRV** — pin + test MSRV (1.80 declared, 18 crates claim 1.75); add `rust-toolchain.toml` + MSRV job. (4A)
- **CICD-05/06/07/08** — repo identity + target-triple unification; SBOM/cosign/sigstore/SLSA; tamper-proof installer. (4B)

### P2 — Medium (next sprint)

`knowledge.rs` 3.1k god-file split (1A) · 195 `cli_*` handler dedup via `CliHandler` trait (1A) · JSON envelope helper across 61 files (1A) · `touring-foundation` god-kernel split (A4) · data-layer ownership to `touring-storage` (A5) · IoC seam consistency (A7) · 73 `allow(dead_code)` REGRA #0 sweep (1A) · 244 `eprintln!`→tracing (1A) · duplicate-version sprawl `cargo tree -d` (RBP-06) · `#[non_exhaustive]` on public enums (RBP-08) · inverted test pyramid + 37 `#[ignore]` audit (3A) · README accuracy + broken links + external-contribution model (3B/DOC-04/07/08) · `redact_secrets` token-pattern hardening (3A T-09) · LLM provider trait → contracts + real impl (A8, masterplan B-W2).

### P3 — Backlog

edition 2024 migration (RBP-10) · 45 glob re-exports (RBP-09) · `lints.rust` enrichment (RBP-11) · 19k-LOC single test file split (3A T-08) · mockall vs no-mocks policy reconciliation (3A T-11) · CHANGELOG signal (3B) · cold-start lazy index warm (2B F6).

## Findings by Category

| Dimension | Crit | High | Med | Low | Verdict |
|---|---:|---:|---:|---:|---|
| Code Quality (1A) | 0 | 3 | 7 | 6 | Floor strong; ceiling unenforced |
| Architecture (1B) | 0 | 3 | 6 | 1 | Decomposition succeeded; next monolith + shim drift |
| Security (2A) | **1** | 5 | 5 | 3 | CEG elite; MCP surface bypasses it |
| Performance (2B) | **2** | 4 | 4 | 2 | Hooks fast-path elite; tail unbounded |
| Testing (3A) | **3** | 4 | 4 | 2 | Count-rich, gate-poor; invariants unasserted |
| Documentation (3B) | **2** | 6 | 7 | 4 | 6/8 docs contradict the code |
| Rust Best Practices (4A) | 0 | 4 | 7 | 4 | Rare elite hygiene; lint ceiling + MSRV gaps |
| CI/CD & DevOps (4B) | **2** | 7 | 8 | 4 | Potemkin CI — authored, never run |
| **Total** | **~10** | **36** | **48** | **26** | **~120 findings** |

## What is already Elite (do NOT regress)

CEG capability model + landlock fail-closed (E2E-proven) · panic-guarded fail-open actor · graceful shutdown (flushes WAL/LinUCB/CRDT) · ~89% single-source workspace deps · 0 lock-across-`.await` UB · `LazyLock`/`let-else`/`#[must_use]` idiom density · async DB write-offload (the exact pattern F1/F2 need) · moka-bounded DryRunCache · a 23KB high-quality `deny.toml` and a complete CI template — both authored, awaiting activation.

## Recommended Action Plan (ordered; effort S/M/L)

1. **[S, now] Patch SEC-01** — canonicalize + root-containment guard on `touring_file_ops`; add the failing `security_invariants_e2e` test (T-01) so it's binding. *(Only act-now item; exploitable today.)*
2. **[S, now] Un-RED cargo-deny (SEC-03)** — bump postgres-protocol/tokio-postgres point releases, plan pyo3 0.24→0.29; add `cargo deny check` as a binding CI step.
3. **[M] Kill the hook tail (F1+F2+F3+T-03)** — move the post_edit/post_write E2E scan off the response path (debounced fire-and-forget, reusing `AsyncFileKnowledgeDB`); add the `hook_dispatch_p99_guard` hdrhistogram budget test.
4. **[S] Reconcile truth (DOC-01/DOC-02/CICD-04)** — single version source; fix ARCHITECTURE.md body; extend `sync_metrics.py` to check the body topology, not just the first match. Set the real crate version + remove `publish=false` when ready.
5. **[M] Bind the lint ceiling (RBP-01/02)** — elite `[workspace.lints]` + 8 escapee crates + `deny(unwrap_used)` ratchet from the CEG.
6. **[M] Activate CI (CICD-01 → then the rest)** — *Gabriel publishes repo + `v*` tag* (B-W1), then the `fmt-deny-doc` job, un-defang fail-open gates, gate nextest + fuzz smoke + llvm-cov floor; add the `wait_timeout` harness to re-enable the server tier (T-02).
7. **[L] Split `touring-server` → `touring-cli-app` + `touring-mcp`; route MCP through CEG; revive `mcp-curated`** (A1+A6+SEC #1 lever) — removes the next monolith and gives Touring its first semver-governable, secure public API (prerequisite for B-W3 SDK).
8. **[M] SDK-readiness** — typed errors (RBP-03), `missing_docs` + `cargo doc -D warnings` ratchet (DOC-06), SECURITY.md/credential reconciliation (SEC-04/05), MSRV pin (RBP-04).
9. **[ongoing] P2/P3** — structural splits (knowledge.rs, foundation, data layer), dedup, the LLM provider (B-W2), supply-chain provenance (SBOM/cosign).

**Mapping to North Star:** items 1-6 are the cheap "binding" wins that move composite_health 0.81 → ≥0.85 and convert the masterplan's claims from asserted to enforced. Items 7-8 are the structural prerequisites for the public-release/SDK waves (B-W1/B-W3/B-W4). The engineering is mostly *activation*, which is why confidence is high and cost is low.

## Review Metadata

- Date: 2026-06-13 · Phases: 1-5 complete (checkpoint-1 approved: continue)
- Agents: code-reviewer, architect-review, security-auditor, performance-engineer, test-automator, docs-architect, rust-pro, deployment-engineer (all opus)
- Flags: none active (framework=rust-workspace auto-detected)
- Constraints honored: no git (REGRA #11), no pkill (REGRA #19), read-only/advisory, real evidence with `file:line`, no `graph_service_e2e` run.
- Ground-truth corrections made by the review (honesty): prod unwraps ≈124 (not 3,686); lint policy via `workspace.lints` exists (not "8/46 weak"); rkyv request path IS zero-copy (F4 response-path only); ann_search p99 tail was cold-start, not hot-path.
