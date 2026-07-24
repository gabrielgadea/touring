# Comprehensive Code Review — Touring Workspace (Premium-Elite Diagnostic)

> Date: 2026-06-20/21 · Target: ALL `/home/gabrielgadea/.claude/rust/crates` (45 members, 537,343 LOC src, 14,272 test fns) · Methodology: Touring-grounded (every finding cites real `file:line`/CLI output; agents verified, didn't assert). Prior run archived → `.full-review/archive-2026-06-13/`.

## Executive Summary

**Touring is already a Premium-Elite codebase — `touring-elite` composite = Diamond 0.9703 (13/13 gates), 0 dependency cycles, a true zero-dep kernel, clippy `--all-targets -D warnings` clean, error-handling at Diamond (`deny(clippy::unwrap_used)` in 48/48 lib crates), a kernel-enforced landlock sandbox, and 0 CVEs/secrets/injection.** It is not "failing" anywhere. The gap between *Diamond-on-the-release-gate* and *per-file-elite-everywhere* is **activation and mechanics, not invention**: one 1-line supply-chain fix, making CI *enforce* what the local suite already proves, one hot-path latency offender, fuzzing the untrusted-input surface, finishing typed errors, and reducing the size of ~27 oversized files. Confidence is high and cost is low because most of the work is enforcement, not design. **0 Critical findings.**

Two issues were **remediated in-session** (REGRA #21): 7,235 LOC of dead-on-disk code removed (build proven green), and a Diamond→Gold doc-drift regression (partly triggered by that removal) re-synced back to Diamond 0.9703.

## Findings by Priority

### P0 — Critical (fix immediately): **NONE**
Genuinely zero. No data-loss, auth-bypass, exploitable-today, or stability threat. (The most "dangerous" item, SEC-02, is in a default-off opt-in binary → P1.)

### P1 — High (fix before next public release)
| ID | Finding | Effort | Why P1 |
|----|---------|--------|--------|
| **F-1** | **Unify `schemars` dep** — `touring-harness-mcp/Cargo.toml:21` `="0.8"` → `{ workspace = true }` (or drop rmcp's schemars feature) | **S (1-line)** | Clears `cargo deny check bans` RED. Converges **A1 = SEC-06 = BP1 = CD2** — one edit, four findings. Highest leverage in the review. |
| **F-2** | **CI must run integration + e2e + doctests** — today `ci.yml:80` gates only `cargo test --workspace --lib`; 177 integration + 89 e2e + 34 doctests never run in CI (`nextest.toml` exists, unused) | S–M | Converges **T2 = CD1**. The biggest "green locally, unproven in CI" gap. |
| **F-3** | **SEC-02: web dashboard binds `0.0.0.0:3000`, no auth, CORS `Any`** (`touring-bindings/src/web/server/mod.rs:2259,2195`) | S (3-line) | Unauth LAN peer → `POST /api/mcp/call`. Default-off (`required-features=bind-web`) keeps it off-P0; fix = loopback default + CORS allowlist + bearer + the missing negative test (T4). |
| **F-4** | **P-1: hook hot-path runs a synchronous full-project scan** — `budget_ms:Some(40)` declared but never consulted; calls un-capped `analyze_wiring` not `analyze_wiring_incremental` (`pipeline.rs:414`, `post_edit.rs:317`, `pre_read.rs:503`) → live **p99=199ms** on the editor critical path | M | Enforce the deadline + offload via existing `handle.spawn`; add the `hook_dispatch_p99_guard` (T3) so it can't regress. |
| **F-5** | **Fuzz the untrusted-input surface** — `touring-ast-polyglot`, `touring-ast`, **`touring-rkyv` (IPC deserialization)** have 0 proptest, 0 fuzz workspace-wide (T1) | M | Arbitrary-bytes/arbitrary-source decode is the highest-value unfuzzed surface for a security-sensitive tool. cargo-fuzz + proptest roundtrip. |
| **F-6** | **P-3: `JOB_REGISTRY` unbounded + `gc` never called** (`job_registry.rs:128`) → slow leak (~1.3 GB RSS) | S | Wire `gc` into a daemon tick + bound the registry. |
| **F-7** | **P-2: `cargo mutants` spawned on every non-test edit, no concurrency cap** (`post_edit.rs:269`) | S | Cap via the CEG `ExecPool` Semaphore pattern, or gate behind opt-in. |
| **F-8** | **Finish RBP-03 typed errors** — 231 `Result<_,String>` remain (bindings 51, hook-runtime 42); SDK-readiness blocker (A3) | M–L | thiserror on consumer-observed APIs. |
| **F-9** | **Split the 27 files >2000 LOC** — start `cli/handlers/decompose.rs` (`cli_decompose_create` CC=388) + `GeneratorContext` god-struct (35 fields/229 fns/4509 LOC) (Q2/Q3/Q4) | L | Drives the worst per-file F1.1/F1.4; use the crate's existing `#[path]` pattern. |

### P2 — Medium (next sprint)
- **SEC-03** daemon socket no `set_permissions` (`daemon.rs:633` → 0o600); **SEC-04** find/tree/glob follow in-jail symlinks out (`file_tools.rs:598-651`); **SEC-05** error/abs-path disclosure + no security headers.
- **P-4..7**: redundant pre_read file reads · missing batch txn in decompose · regex-compile-in-loop in enrichment · histogram mutex contention.
- **Q5/T7** coverage 0.40–0.52 on hot paths · **T5** mutation-score floor · **T6** the one `#[ignore]` masking a real bug (`cli_decompose_ready`).
- **A6** wiring/orphan DB has 57 phantom-path stale entries + ~320 genuinely-dead orphans (of 4,823 raw; ~93% are cross-crate API or intra-crate-used) — prune + REGRA #0 triage.
- **BP2** `rust-toolchain.toml` (MSRV *value* already declared 1.85) · **BP3** `rustfmt.toml`/`clippy.toml`.
- **D4** ADRs + C4/mermaid (CONTRIBUTING.md:43 points at non-existent `docs/rfcs/`) · **D6** README hook-count self-contradiction (198/218/140) · **D7** CHANGELOG → Keep-a-Changelog + A2/A5/schemars migration entry.
- **CD3** actionlint + incident runbooks · **A5** `touring-server` 70.9k split (justified, not urgent).
- **Quality-engine meta-fix**: `touring-quality --workspace` is unfaithful (sums CC, self-FPs on its own detector) → per-function CC + self-exclusion + per-file-then-aggregate. (Per-file scoring is already faithful.)

### P3 — Low (backlog)
`CODEOWNERS`; `resolver="2"` under edition-2024; `LICENSE` symlink polish; the misc Low items in `01a`/`02a`/`02b`/`03a`/`04a`.

## Findings by Category (post-dedup, normalized severity)
| Category | C | H | M | L |
|----------|---|---|---|---|
| Code Quality (F1.1–6) | 0 | 3 (+Q1 done) | 1 | 4 |
| Architecture (F1.7–12) | 0 | 1 (schemars→F-1) | 4 | 4 |
| Security (F2.1–6) | 0 | 1 | 4 | 3 |
| Performance (F2.7–13) | 0 | 3 | 5 | 4 |
| Testing (F3.1–7) | 0 | 2 | 3 | 2 |
| Documentation (F3.8–13) | 0 | 3 (**R2 done**) | 4 | 3 |
| Rust BP (F4.1–6) | 0 | 1 (=F-1) | 2 | 3 |
| CI/CD DevOps (F4.7–12) | 0 | 2 (=F-1,F-2) | 5 | 4 |
| **Unique (deduped)** | **0** | **9** | **~22** | **~15** |

## Recommended Action Plan (ordered; the cheap binding wins first)
1. **[S, now] F-1 schemars 1-line** → un-RED cargo-deny bans (closes 4 findings).
2. **[S, now] F-3 SEC-02** loopback + CORS allowlist + bearer + negative test.
3. **[S, now] F-6 + F-7** wire JOB_REGISTRY `gc`; cap mutants concurrency.
4. **[S–M] F-2** CI nextest full run + doctests (makes CI enforce what's already green).
5. **[M] F-4** enforce the hook budget + offload + p99 guard (p99 199ms→<50ms).
6. **[M] F-5** cargo-fuzz + proptest the rkyv/polyglot untrusted-input surface.
7. **[M–L] F-8** finish typed errors (SDK-readiness).
8. **[L] F-9** split the top oversized files (decompose.rs, GeneratorContext, the 27 >2000 LOC).
9. **[ongoing] P2/P3** + the quality-engine meta-fix + ADRs/CHANGELOG + MSRV/fmt config.

Items 1–4 are 1-line-to-small "binding" wins that move the *enforced* posture toward the *measured* Diamond and clear every CI red. Items 5–8 are the structural prerequisites for the public-release/SDK goal (B-W1).

## Remediations Applied This Session
- **Q1 ✅** — removed 12 dead-on-disk `.rs` (**7,235 LOC**); `cargo check --workspace` rc=0 (26.11s); backup `/tmp/dead-files-2026-06-20.tgz`. Gabriel: `git rm` to commit (REGRA #11).
- **R2 ✅** — doc-drift re-sync (`sync_metrics --sync` + `gen_reference.py`): composite **Gold 0.8856 → Diamond 0.9703**, `06_documentation` FAIL→PASS, both drift gates green.

## What Is Already Elite (do NOT regress)
0 dependency cycles (Tarjan) · zero-dep kernel (`touring-foundation` deps `=[]`) · typestate generator pipeline · move-utils-down playbook · **`[workspace.lints]` clippy::all=deny + 8 RBP-11 ratchets** · clippy `--all-targets` clean · **error-handling Diamond** (`deny(unwrap_used)` 48/48, 0 prod unwraps) · **CEG landlock fail-closed** (kernel-enforced, deny-by-default net, credential-strip) · SEC-01 path-traversal remediated+5-tests · **0 CVEs/secrets/injection**, unsafe elite (`#![forbid(unsafe_code)]` ×4) · CEG `ExecPool`+`DryRunCache`+`moka_policies` bounded · `query_cache` single-flight anti-stampede · real hdrhistogram p99 · **0 lock-across-await** · healthy base-heavy test pyramid (11.6k unit) + 5 loom proofs + 30 criterion baselines + 8 p99 guards · **`gate-metrics` + `sync_metrics` observability/drift USPs** · 47/48 `deny(missing_docs)` · edition 2024 uniform · defensive profiles (REGRA #12) · cosign OIDC + SLSA release · cargo-deny advisories live · MSRV 1.85 declared · LICENSE-MIT + LICENSE-APACHE.

## Review Metadata
- Phases 1–5 complete · Checkpoint-1 approved (continue) · 0 Critical · 9 High (2 already remediated: Q1, R2-trio) · ~22 Medium · ~15 Low.
- Agents: code-reviewer, architect-review, security-auditor, performance-engineer, docs (general-purpose), rust-pro, deployment-engineer (+ testing authored by orchestrator after the test-automator agent thrashed context ×2).
- Constraints honored: no git (REGRA #11) · no pkill touring (REGRA #19) · real-exit-codes (rc read explicitly, not wrapper) · evidence-grounded (`file:line`/CLI) · REGRA #0 (orphans surfaced) · REGRA #21 (Q1+R2 failures fixed in-session, none dismissed by origin).
- Honesty corrections the review made to its own baseline: LICENSE-APACHE present (not MIT-only); MSRV declared (only toolchain-file missing); orphans ~93% API/used not dead; the "Diamond" claim was momentarily Gold mid-session and was re-earned, not asserted.
