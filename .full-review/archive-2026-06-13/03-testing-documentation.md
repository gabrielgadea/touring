# Phase 3: Testing & Documentation Review

> Touring workspace · 2026-06-13 · agents: test-automator (3A) + general-purpose docs architect (3B)
> Full detail: `03a-testing.md` · `03b-documentation.md`

## Test Coverage Findings (3A) — 3 Critical / 4 High / 4 Medium / 2 Low
**Verdict: broad-and-shallow, count-rich/gate-poor.** ~13,942 test fns are real and mostly real-data (1,150 tempdir uses), but no coverage % is measured/gated in CI, the server-integration + daemon-RPC tier is skipped in CI, and the elite-critical invariants are unasserted.

- **🔴 T-01 — Security invariants have ZERO regression tests.** Confirmed: `touring_file_ops` (`tools_core.rs:1066`) does `tokio::fs::read_to_string`/`write`/`remove_file` on the raw, un-canonicalized path — no root guard. The only containment test (`file_tools.rs:752`) covers a *different, unused* CLI guard → **false assurance**. Transcript miner: 20+ tests, none assert redaction (SEC-05). No sandbox env-boundary test (SEC-04). **Directly corroborates SEC-01.**
- **🔴 T-02 — `graph_service_e2e` unrunnable; server integration untested in CI.** Root cause: spawns the real `touring` binary (`viz workspace`, `:495`) against the real workspace with no timeout → uncontrolled external process = the hang. CI runs `--lib` only, so the whole cross-project graph / blast-radius / viz layer is skipped. Elite fix: split unit/subprocess + `wait_timeout` harness + nextest `slow-timeout terminate-after` (already configured, unused).
- **🔴 T-03 — No perf gate on the real hook tail.** Phase 2's p99=488ms/p999=1.30s dispatch tail has zero guard. `latency_p99_guard.rs` covers touring-ast parse only; `ceg_baseline.rs` panics on P99 but is a *bench* (never in CI) and covers the CEG fast-path, not actor dispatch.
- **[High] T-04** no llvm-cov/floor in CI (prior 83%/78% stale post-decomposition). **T-05** 8 cargo-fuzz targets never run in CI (W11.6 found 5 real bugs — ROI lost). **T-06** inverted pyramid (11,375 unit vs 2,566 integration; "E2E" = fragile subprocess spawns). **T-07** 37 `#[ignore]` (21 daemon-gated never run in CI + 11 bare + 1 openly-broken Wave-8 contract).
- **[Med]** T-08 19,296-LOC single test file; T-09 `redact_secrets` substring-only (misses bare `ghp_`/`AKIA`); T-10 CI compiles `--tests` but never *runs* them; T-11 mockall in 2 crates contradicts "no mocks" constitution (policy drift).

**#1 testing lever:** add `security_invariants_e2e` + `hook_dispatch_p99_guard` (T-01+T-03) — in-process MCP tests asserting file_ops containment / transcript redaction / sandbox env boundary + an hdrhistogram P99 budget on the real `post_edit` dispatch. They should *fail today*, run CI-safe (no subprocess), converting Phase 2's Criticals from "unguarded" to "regressions fail the build." Pair with T-02's timeout harness + `cargo llvm-cov --fail-under` floor.

## Documentation Findings (3B) — 2 Critical / 6 High / 7 Medium / 4 Low
**Verdict: 6 of 8 user-facing docs contradict the code.** Docs *look* elite (Diátaxis, badges, doc-as-code gates) but the headline numbers in the two most-read files describe a workspace that no longer exists.

- **🔴 DOC-01 — 3-way version contradiction.** README badge `30.0.0` (`README.md:5`), ARCHITECTURE `v30.3.6` (`:3`), but the binary prints **`0.1.0`** (`Cargo.toml:142` → `health.rs:236`). `touring --version` → 0.1.0 destroys trust in every other number.
- **🔴 DOC-02 — ARCHITECTURE.md body still describes the pre-decomposition architecture (scopes A3).** Header was synced (46 crates, session G-1) but the body (`:155-835`) lists **4 phantom crates** (touring-core/-index/-vfs/-semantics), still says **touring-hooks LOC: 127,575** (`:167,806` — contradicting its own header), totals **"38 crates / 476,728 LOC"** (`:835`), links to non-existent per-crate ARCHITECTURE files. **`sync_metrics.py` only checks the *first* `\d+ crates` match (the header), so body drift is invisible — a gap in the G-1 gate itself.**
- **[High] DOC-03 — SECURITY.md misleads on credentials (confirms SEC-04).** Claims credentials "never in ENV_ALLOWLIST" (`SECURITY.md:31`) — true of that constant, but the sandbox uses a *separate* `CREDENTIAL_ENV_WHITELIST` (`sandbox_executor.rs:542`) forwarding GITHUB_TOKEN/AWS_*/ANTHROPIC_API_KEY/OPENAI_API_KEY into the child.
- **[High] DOC-04/05 — README + generated MCP catalog wrong.** README "36 crates/~428k LOC" (real 46/~499k), "88 MCP tools", "198 hooks" while its own footer says 218. Generated `mcp-tools.md` says 164 because `gen_reference.py` extracts string literals, not the **184–194 `#[tool]` macros**; documented "22-tool mcp-curated" is a dead flag.
- **[High] DOC-06 — No usable SDK reference.** `missing_docs` enforced on 8 clean crates (real progress) but NOT on the biggest public surfaces: touring-server, touring-intelligence (**1,756 pub items**), touring-cli, touring-ceg. `cargo doc` would leave the most important crates undocumented.
- **[High] DOC-07/08 — Broken README links + no external-contribution model.** README→`docs/CONTRIBUTING.md` 404 (file at root); CONTRIBUTING never reconciles the no-git internal rule with inviting public PRs.

**#1 documentation lever:** make the doc-as-code gates cover the trust-breaking claims — version, README counts, ARCHITECTURE *body* topology — then add `cargo doc --no-deps -D warnings` and ratchet `missing_docs` onto touring-ceg → touring-server → touring-intelligence. Turns "claims to be auto-synced" into "cannot drift from code."

## Cross-cutting signal for Phases 4-5
- **The dogfooding gates exist but under-check** (sync_metrics first-match-only; gen_reference string-literal-only; cargo-fuzz/llvm-cov present-but-unrun). The elite pattern is already invented here — it just isn't *binding*. Phase 4 (CI/CD) must close the loop: every claimed invariant → a gate that fails the build.
- **Version `0.1.0` (DOC-01)** is also a release-engineering finding for Phase 4 (release.yml tags `v*` but the crate version is 0.1.0).
