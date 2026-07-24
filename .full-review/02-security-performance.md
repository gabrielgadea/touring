# Phase 2: Security & Performance — Consolidated

> Detail: `02a-security.md` (F2.1–F2.6 + unsafe) · `02b-performance.md` (F2.7–F2.13). Both agents Read real source, ran `cargo deny` / `touring gate-metrics` (live histograms). Verdict: **mature for a local agentic dev tool; the real gaps are one opt-in network default and one synchronous hot-path scan.**

## Security (F2.1–F2.6) — 0 Critical · 1 High · 4 Medium · 3 Low

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| SEC-02 | **High** (CWE-306+942) | Web dashboard binds `0.0.0.0:3000`, **zero auth + CORS `allow_origin(Any)`** → unauth LAN/browser peer can `POST /api/mcp/call {dry_run:false}` (run whitelisted subcommands + read codebase/RL state) | `touring-bindings/src/web/server/mod.rs:2259, 2195-2200` | `127.0.0.1` default + CORS allowlist + bearer gate (3-line). **Mitigated today:** `required-features=["bind-web"]`, default-off; real MCP is stdio-only; cmd-injection blocked (whitelist+argv) |
| SEC-03 | Medium (CWE-276) | Daemon Unix socket bound with no `set_permissions` (umask-mode in shared `/tmp`) | `daemon.rs:633` | `fs::set_permissions(0o600)` after bind |
| SEC-04 | Medium | `find`/`tree`/`glob` jail the root but **follow in-jail symlinks out** (residual of SEC-01) | `file_tools.rs:598-651` | canonicalize each yielded path, re-check containment |
| SEC-05 | Medium | Error/abs-path disclosure + no security headers on web surface | web server | sanitize errors; add CSP/HSTS/X-Content-Type |
| SEC-06 | Medium | `cargo deny bans` FAIL (schemars dup) | = Phase-1 **A1** | `schemars = { workspace = true }` |

**Verified-safe (real, not asserted):** CEG landlock = genuine `landlock` v0.4 ABI-V6, `pre_exec` **fail-closed** (`supervised.rs:382-388` — must be `KernelEnforced` or spawn errs), deny-by-default net, rlimit, credential-stripped env *(caveat: the agent-Bash PreToolUse gate is advisory unless `CEG_ENFORCE=1`)* · **SEC-01 path-traversal remediated+wired+tested** (`enforce_path_within_roots` on every `touring_file_ops`, `tools_core.rs:1116-1120`, 5 tests, `/etc/passwd` denied) · **0 hardcoded secrets · 0 weak crypto · 0 CVEs** (`advisories ok`) · no SQL/FTS injection (bound params + `&'static` table names + FTS5 escape) · no command injection (vectorized argv) · `unsafe` = ELITE (0 high-risk, `#![forbid(unsafe_code)]` in 4 crates).

## Performance (F2.7–F2.13) — 0 Critical · 3 High · 5 Medium · 4 Low

Live `hook_dispatch_latency` (5,768 samples): p50=1.5ms · p90=25ms · **p99=199ms** · p99.9=566ms · max=992ms. Target <50ms on the editor critical path.

| # | Sev | Finding | Evidence | Fix |
|---|-----|---------|----------|-----|
| P-1 | **High** | `AnalysisPipeline::run()` does a **synchronous full-project wiring+quality scan** on every `post_edit` + `pre_read`; the declared `budget_ms: Some(40)` is **never consulted** — calls un-capped `run_wiring`/`analyze_wiring` (full-DB) not `analyze_wiring_incremental` (LIMIT 5000). **= the p99=199ms tail.** | `post_edit.rs:317`, `pre_read.rs:503`, `engine.rs:36`, `pipeline.rs:414` | Enforce the deadline in `run()` + offload via existing `handle.spawn` fire-and-forget → p99 → <50ms |
| P-2 | **High** | `cargo mutants` subprocess spawned on **every** non-test edit via `spawn_worker` with **no Semaphore/cap** | `post_edit.rs:269` | Cap concurrency (reuse CEG `ExecPool` Semaphore pattern) or gate behind opt-in |
| P-3 | **High** | `JOB_REGISTRY` unbounded; `gc` helper has **no daemon-level caller** → slow leak (each job holds stdout; ~1.3 GB live RSS) | `job_registry.rs:128` | Wire `gc` into a daemon tick + bound the registry |
| P-4..7 | Medium | redundant 2–3× full-file reads in pre_read · missing batch txn in decompose · regex-compile-in-loop in enrichment · histogram mutex contention | (see 02b) | memoize/batch/`Lazy` regex |

**Verified-fast (elite, do not regress):** CEG `ExecPool` (tokio Semaphore + timeout) · `DryRunCache` + centralized `moka_policies` (every cache capacity+TTL+TTI bounded) · `query_cache` single-flight `get_with` anti-stampede (16-thread test) · `LatencyHistogram` = real hdrhistogram · ANN HNSW bounded `ef_search` · CEG 100% pure-fast-path (974/974) · **0 lock-across-`.await`** (grep-confirmed) · DB connection reuse + indexed WHEREs + no `SELECT *`.

## Critical issues for Phase 3 (Testing & Documentation) context

- **P-1 needs a binding p99 budget test** — an `hdrhistogram` `hook_dispatch_p99_guard` (<50ms) so the regression can't reappear silently.
- **SEC-02 needs a security test** — assert the web bind defaults to loopback + rejects unauth `/api/mcp/call` (so the dangerous default is caught in CI).
- **Coverage gap on hot paths** (Phase-1 Q5: TDG coverage 0.40–0.52) — the enabler for safely doing P-1/P-2 and the file-size splits.
- **P-2 (`cargo mutants` on every edit)** is test-infra coupling — the test agent should assess whether mutation-on-edit belongs on the hot path at all.
- **Doc drift** (Phase-1 A2: ARCHITECTURE.md self-detected stale) → docs agent + wire `sync_metrics.py --check` into CI.

## Cross-phase convergence (same root, multiple lenses)

- **schemars dep pin** surfaces as both A1 (architecture) and SEC-06 (supply-chain) — single 1-line fix.
- **The hook tail** (P-1) is the same "response-path heavy work" the 2026-06-13 review flagged under F1+F2 — still synchronous; now pinpointed to the un-consulted budget in `pipeline.rs:414`.
