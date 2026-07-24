# Phase 2: Security & Performance Review

> Touring workspace · 2026-06-13 · agents: security-auditor (2A) + performance-engineer (2B)
> Full detail: `02a-security.md` · `02b-performance.md`

## Security Findings (2A) — 1 Critical / 5 High / 5 Medium / 3 Low

### CEG verdict: the crown jewel HOLDS — but it only governs `touring exec`, NOT the MCP surface
Verified directly: capability deny-by-default holds (empty profile denies all `profile.rs:135`; deny-wins `:108`); X8 landlock is real and **fail-closed** (`supervised.rs:373-385`) with E2E tests proving kernel-blocked writes outside granted roots (`:627-646`) + kernel-denied TCP bind on Linux 6.7+ (`:716`); deny-wins can't be overridden by a high score (`decision.rs:373`); daemon panic isolation two-layer (`daemon.rs:772`,`:242`); rkyv on untrusted bytes validated (`check_archived_root`, `daemon.rs:1003`). **This component is genuinely elite.**

- **🔴 [CRITICAL] SEC-01 — `touring_file_ops` is an unrestricted arbitrary FS read/write/delete primitive over MCP, bypassing the CEG entirely.** `tools_core.rs:1050-1370`, always-on, no canonicalize / root-check / capability check. Prompt-injection → read `~/.ssh/id_rsa`, overwrite `authorized_keys`, `delete_dir --force ~`. CWE-22/CWE-73. **The gateway proves Touring CAN enforce containment; this tool ignores it.**
- **[High] SEC-02** — `touring_ctx_execute` runs code with forbidden-call scanner defaulting to **Warn (executes anyway)** + **fail-open on panic** (`ctx_execute_tools.rs:144,170,245`).
- **[High] SEC-03** — `cargo deny check advisories` is **RED**: 6 vulns incl. **postgres-protocol RUSTSEC-2026-0179 CVSS 8.7** (SCRAM DoS) + pyo3 (0.24→0.29) + tokio-postgres; deny.toml ignore-list stale; CI gate not enforcing.
- **[High] SEC-04** — runtime sandbox passes **all cloud credentials** (GITHUB_TOKEN, AWS_*, ANTHROPIC_API_KEY…) into the child by default (`sandbox_executor.rs:542`), **contradicting SECURITY.md** which claims exclusion.
- **[High] SEC-05** — transcript miner persists raw tool errors + resolution commands to the searchable memory store **with no redaction** (`transcript_miner.rs:763-773`); `redact_secrets()` exists but isn't applied.
- **[High] SEC-06** — `unsafe impl Send for HookRuntime` over 10+ `RefCell` fields, no SAFETY comment (`hook_runtime.rs:695`); latent data-race UB masked only by the single-actor mpsc model.
- **[Med]** SEC-07 socket in `/tmp` without explicit `0o600`; SEC-08 arbitrary file-read tools; SEC-09 SSRF in `fetch_remote_wasm`; SEC-10 CEG doc/landlock self-contradiction; SEC-11 unbounded local reads.

**#1 security lever:** put the MCP tool surface behind the CEG capability model. The gateway already proves deny-by-default + landlock + root containment; ~169 always-on MCP tools bypass it. Routing every path-taking / code-running tool through a `CapabilityProfile` + canonicalize/root-guard neutralizes SEC-01, SEC-02, SEC-08 at once. The dead `mcp-curated` flag (only *adds* 3 tools, `mod.rs:442`) is the natural vehicle — the intended 22-tool curation never shipped.

## Performance Findings (2B) — 2 Critical / 3 High / 4 Medium / 2 Low (+ build F11-F13)

### Gate-metrics tail verdict (FACT, measured 2026-06-13 via `touring gate-metrics -j`)
`hook_dispatch_latency` (1361 samples): **p50=239µs, p90=28.3ms, p99=488ms, p999=1.30s, max=1.34s** — a **118× p50→p90 cliff** on the user-perceived hook plane. `rkyv_dispatch_latency`: p90=253ms, max=571ms. **`ann_search_latency` now uniformly 1µs** — the earlier p99≈4.4s was a cold-start artifact (lazy first-query index build), not a hot-path problem. Daemon RSS = 1477 MB.

- **🔴 [CRITICAL] F1 — Inline full-workspace E2E scan after every post_edit/post_write.** `daemon.rs:297-302` runs `cli_e2e(depth=quick)` inline on the serial actor thread → `phase_index → count_code_files`, a synchronous recursive `std::fs::read_dir` walk of the whole project (`cli_e2e.rs:1483-1505`). On the serial actor (`daemon.rs:220`) it convoys every subsequent hook. **This is the tail.**
- **🔴 [CRITICAL] F2 — Heavy handlers run inline on the serial per-project actor** (`daemon.rs:242-250`); "heavy" hooks only get a longer timeout, never offload → head-of-line blocking.
- **[High] F3** — no execution budget bounding hook latency; timeout abandons the result but the actor keeps running the slow work.
- **[High] F4** — rkyv "zero-copy" does a full `serde_json::from_slice` + 3× `String` copy per request (`ipc.rs:56-62`, daemon ~1014-1033) — negates rkyv's purpose; the rkyv p90 tail is real.
- **[High] F5** — global `Mutex<Histogram>` on the dispatch path (`gate_metrics.rs:36-37`); O(1)-held so a *secondary* convoy-drain serializer, not the primary 28ms cause (first-pass over-attribution explicitly contested).
- **[High] F11** — `touring-foundation` fan-in 22 + `incremental=false` → masterplan 9× rebuild claim aspirational; ~6-7× realistic.
- **[Med]** F6 cold-start lazy index; F7-F10 allocation/lock/cache items; **F12-F13** build-time.

### Already elite (don't regress)
Fail-open panic-guarded actor; CEG 100% fast-path (166/166); **async DB write offload already done** (`post_edit.rs:555`) — proves the exact pattern F1/F2 need; moka-bounded DryRunCache.

**#1 performance lever:** F1 — move the post_edit/post_write E2E workspace scan off the hook-response path (debounced fire-and-forget; the codebase already does this with `AsyncFileKnowledgeDB`). Should collapse p90 from 28ms → toward 239µs p50 and erase the 488ms p99 / 1.3s tail. F2+F3 then make latency *structurally bounded* — the real elite bar.

## Critical Issues for Phase 3 Context (testing/docs)

1. **SEC-01/SEC-02/SEC-04/SEC-05** need security regression tests: a test that asserts `touring_file_ops` cannot escape a root; that credentials are NOT in the sandbox child env; that the transcript miner redacts. Today these invariants are unenforced.
2. **F1/F2** need a perf regression guard (hdrhistogram P99 budget test on the hook path) so the tail can't silently return.
3. **SECURITY.md is inaccurate** (claims credential exclusion that SEC-04 disproves; CEG self-contradiction SEC-10) — Phase 3 docs must reconcile claim ↔ code.
4. **cargo-deny RED** must become a CI gate (Phase 4 CI/CD), with tests/docs noting the advisory policy.
5. Coverage of the CEG is strong (E2E proofs exist) — Phase 3 should confirm the MCP surface and untrusted-input paths have equivalent coverage (they likely don't).
