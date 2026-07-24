# Phase 3A: Test Coverage & Quality Review

> Touring workspace · 2026-06-13 · agent: test-automator (3A)
> Lens: Rust · Read-only · North star: what blocks Touring's test strategy from Premium, Elite-of-Market
> Evidence: real `file:line` + CLI. No mutation in this pass.

## Verdict (one line)

The test **count** is elite-tier (~13,942 fns); the test **strategy** is not. Coverage is
**broad-and-shallow**: a deep, well-structured unit corpus over an **inverted pyramid** (82%
in-`src` unit, F1 server integration layer unrunnable in CI), with **zero coverage of the
elite-critical security invariants** the Phase 2 audit flagged, **no perf regression gate on
the real hook-dispatch tail**, and **no coverage floor anywhere in CI**. The number is a
comfort blanket over specific, named holes.

---

## Severity counts

| Severity | Count | IDs |
|---|---|---|
| 🔴 Critical | 3 | T-01 (security invariants untested), T-02 (graph_service_e2e unrunnable / server integration untested in CI), T-03 (no perf regression gate on hook tail) |
| 🟠 High | 4 | T-04 (no coverage measurement/floor in CI), T-05 (fuzz exists but never runs in CI), T-06 (inverted pyramid + subprocess-as-"E2E"), T-07 (37 `#[ignore]` incl. 18 daemon-gated never run in CI) |
| 🟡 Medium | 4 | T-08 (19,296-LOC single test file), T-09 (`redact_secrets` substring-only — weak even where applied), T-10 (no `--tests` in CI = integration rot), T-11 (mock claim vs reality drift) |
| 🟢 Low | 2 | T-12 (bench P99 gates not run in CI), T-13 (nextest configured but unused by CI) |

---

## Untested elite-critical invariants (the table that matters)

These are security/perf invariants where the *code* exists but **no test asserts the invariant**.
Each is a regression waiting to happen silently.

| Invariant | Status in code | Test today | Risk |
|---|---|---|---|
| **SEC-01** `touring_file_ops` cannot read/write/delete outside a root | **No containment at all** — `tools_core.rs:1066` calls `tokio::fs::read_to_string(path)` on raw `p.path`; `write`/`remove_file` likewise. `FileTools::validate_path` (canonicalize+`starts_with` root, `file_tools.rs:153-167`) exists but is on the *CLI* path, **not** the MCP tool. | **NONE.** `test_path_outside_root` (`file_tools.rs:752`) tests the *unused* guard, giving false assurance. | Prompt-injection → arbitrary FS R/W/delete. CWE-22/73. |
| **SEC-05** transcript miner redacts secrets before persisting to searchable memory | `redact_secrets` exists (`sandbox_executor.rs:599`) but per Phase 2 (`transcript_miner.rs:763-773`) is **not applied** to mined errors/commands. | **NONE.** 20+ miner tests (`transcript_miner.rs:801-1196`) cover parsing/state-machine; **zero** assert redaction of persisted output. | API keys/tokens from tool errors land in a queryable store. |
| **SEC-04 boundary** non-whitelisted env vars are excluded from sandbox child | `CREDENTIAL_ENV_WHITELIST` (`sandbox_executor.rs:540-567`) is a deliberate allowlist. | **Partial/none.** No test asserts a *non*-whitelisted var (e.g. `MY_SECRET`) is **absent** from the child env. | Whitelist regression (adding `*`) goes uncaught; also doc-drift vs SECURITY.md. |
| **SEC-02** `ctx_execute` forbidden-call scanner blocks (not just warns) | Defaults to Warn+executes, fail-open on panic (`ctx_execute_tools.rs:144,170,245`). | `ctx_execute_e2e.rs` exists but no test asserts a forbidden call is *denied execution*. | Untrusted code runs despite a flagged forbidden call. |
| **F1/F2 perf** hook-dispatch P99 stays within a budget | Measured **p99=488ms, p999=1.30s** (`touring gate-metrics -j`, Phase 2). | **NONE.** No test or in-CI bench exercises `hook_dispatch_latency`. `latency_p99_guard.rs` covers touring-ast parse only; `ceg_baseline.rs` gates the CEG fast-path only — neither touches the actor dispatch tail. | The 488ms tail can silently return after any fix; nothing catches it. |
| **rkyv** untrusted-bytes deserialize is bounded | `fuzz_rkyv_deserialize.rs` exists. | Fuzz target present but **never run in CI**. | Malformed IPC bytes panic/OOM the daemon; not continuously fuzzed. |

---

## 🔴 Critical findings

### T-01 — The elite-critical security invariants have NO regression tests

The Phase 2 crown-jewel finding (CEG holds, but the MCP surface bypasses it) maps directly to a
**test vacuum**. Confirmed in code this pass:

- `touring_file_ops` (`crates/touring-server/src/server/tools_core.rs:1066-1130+`) performs
  `tokio::fs::read_to_string` / `write` / `OpenOptions().append` / `remove_file` on the **raw,
  un-canonicalized `p.path`**. No `validate_path`, no root guard. The only path-containment test
  in the crate (`file_tools.rs:752 test_path_outside_root`) exercises a **different, unused**
  guard — so the suite *looks* covered while the live MCP tool is wide open.
- `transcript_miner` has 20+ unit tests (`transcript_miner.rs:801-1196`) — all parsing /
  state-machine; **none** assert `redact_secrets` is applied to persisted memory.

**Why critical:** these are exactly the invariants a Premium system proves by test. The CEG
*demonstrates* Touring can write such tests (it has E2E kernel-enforcement proofs,
`supervised.rs:627-646,:716`). The MCP surface — the actual attack surface — has none.

**Concrete recommendation** (containment regression test — should FAIL today, proving the gap):

```rust
// crates/touring-server/tests/file_ops_containment_e2e.rs
#[tokio::test]
async fn file_ops_cannot_escape_project_root() {
    let root = tempfile::tempdir().unwrap();
    let server = TestServer::with_root(root.path()).await; // real server, real fs
    // craft an absolute escape path
    let escape = "/etc/passwd";
    let res = server.call_tool("touring_file_ops",
        json!({"operation": "read", "path": escape})).await;
    assert!(res.is_err(), "file_ops read of {escape} MUST be denied; got Ok — \
        SEC-01: tool bypasses root containment");
    // and a traversal escape from inside the root
    let trav = root.path().join("../../../../etc/passwd");
    let res2 = server.call_tool("touring_file_ops",
        json!({"operation": "read", "path": trav.to_str().unwrap()})).await;
    assert!(res2.is_err(), "file_ops MUST reject ../ traversal — SEC-01");
}
```

```rust
// crates/touring-server/src/ingest/transcript_miner.rs (tests mod)
#[test]
fn mined_errors_are_redacted_before_persist() {
    let raw = "Error: auth failed with GITHUB_TOKEN=ghp_DEADBEEF0123456789";
    let persisted = build_lesson_memory_value(raw); // the fn that feeds memory store
    assert!(!persisted.contains("ghp_DEADBEEF0123456789"),
        "SEC-05: raw token leaked into searchable memory store");
    assert!(persisted.contains("[REDACTED]"));
}
```

```rust
// crates/touring-ceg/src/gateway/sandbox_executor.rs (tests mod)
#[test]
fn non_whitelisted_env_is_excluded_from_child() {
    std::env::set_var("TOURING_TEST_SECRET_XYZ", "topsecret");
    let child_env = build_sandbox_child_env(); // the fn that filters via whitelist
    assert!(!child_env.iter().any(|(k,_)| k == "TOURING_TEST_SECRET_XYZ"),
        "SEC-04 boundary: only CREDENTIAL_ENV_WHITELIST may flow into sandbox");
}
```

---

### T-02 — `graph_service_e2e` is unrunnable; the server integration layer is effectively untested in CI

`crates/touring-server/tests/graph_service_e2e.rs` (26.5 KB) is **excluded from CI** (`ci.yml`
runs `cargo test --workspace --lib` only). Root-cause class confirmed by reading the file:

- It mixes pure unit tests (`#[test]` SymbolIndex, `:31-104`) with `#[tokio::test]` GraphService
  tests **and** with **subprocess integration tests that spawn the real `touring` binary against
  the real workspace**: `test_graph_svg_output` (`:474-535`) runs
  `Command::new(binary).args(["viz","workspace","--format","svg"]).current_dir("/home/gabrielgadea/.claude/rust")`,
  and a `run_touring` helper (`:540+`) does the same for many more. A spawned `touring` subprocess
  will attempt daemon connect/auto-spawn (REGRA #2.5) or perform a full-workspace walk
  (`viz workspace`) — a **process-level external dependency** that can block indefinitely. That is
  the hang class: not a logic deadlock in the test, but an **uncontrolled external process with no
  timeout** under `cargo test`.

**Consequence:** the *entire* server integration file is skipped, so the cross-project graph hot
path, blast-radius, focus tracker, and CLI viz are **integration-untested in CI**. This is a
permanent, structural coverage hole on the public-facing server layer.

**Elite fix (do all three):**
1. **Split** the file: pure `SymbolIndex`/`GraphService` tests are in-process and should move to
   `--lib` (or stay as a fast `graph_service_unit.rs` with no subprocess). The subprocess tests go
   to a clearly-named `graph_cli_subprocess_e2e.rs`.
2. **Bound every subprocess** with a timeout harness so a hang fails loud instead of stalling CI:
   ```rust
   fn run_touring_bounded(args: &[&str], timeout: Duration) -> (i32, String, String) {
       let mut child = Command::new(&binary).args(args)
           .current_dir(workspace).stdout(Stdio::piped()).stderr(Stdio::piped())
           .spawn().expect("spawn touring");
       // wait_timeout crate (already in the dep tree for tests) or a thread+recv_timeout
       match child.wait_timeout(timeout).unwrap() {
           Some(status) => { /* collect */ }
           None => { child.kill().ok(); panic!("touring {args:?} exceeded {timeout:?} — \
               the graph_service_e2e hang reproduced; fix the blocking call, not the test"); }
       }
   }
   ```
   Run subprocess tests against an **isolated temp workspace** with `TOURING_BINARY` pinned and a
   `--no-daemon`/offline flag so they never touch the real socket.
3. Add the split, timeout-bounded files to CI via `cargo nextest run` (which natively enforces
   `slow-timeout terminate-after`, already configured in `.config/nextest.toml`) — re-enabling the
   server integration layer in CI *with* a hang guard.

---

### T-03 — No perf regression gate on the real hook-dispatch tail (F1/F2)

Phase 2 measured `hook_dispatch_latency` p99=488ms / p999=1.30s — a real, user-perceived tail. I
confirmed **no test or in-CI bench exercises that path**:

- `crates/touring-code/tests/latency_p99_guard.rs` is a genuine hdrhistogram P99 gate — but it
  covers `extract_symbols` / `analyze_quality` (touring-ast parse), not dispatch. Budgets:
  100µs–300µs (`:91,:124,:145`). Good pattern, wrong target.
- `crates/touring-hooks/benches/ceg_baseline.rs` *does* panic on P99 budget breach (`:8,:32-35`)
  — but it's a **bench** (only `cargo bench`, never in `ci.yml`) and gates the CEG fast-path, not
  the actor dispatch.

So the exact tail Phase 2 names as the #1 perf lever has **zero guard**. After F1 is fixed
(debounced off-path E2E scan), nothing prevents it silently returning.

**Concrete recommendation** — promote the `latency_p99_guard.rs` pattern to the hook plane and run
it as a **test** (in CI), seeding from `gate-metrics` reality:

```rust
// crates/touring-dispatch/tests/hook_dispatch_p99_guard.rs
use hdrhistogram::Histogram;
#[test]
fn post_edit_dispatch_p99_under_budget() {
    let rt = test_runtime();              // real HookRuntime, real temp project
    let mut h = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
    for _ in 0..2_000 {
        let t = std::time::Instant::now();
        rt.dispatch_post_edit(&sample_edit());   // the real serial-actor path
        h.record(t.elapsed().as_micros() as u64).unwrap();
    }
    let p99 = h.value_at_quantile(0.99);
    // Budget BELOW today's 488ms so a fix is required to pass, and a
    // regression past it fails CI loudly. Start at the pre-edit hook UX budget.
    assert!(p99 < 50_000, "post_edit dispatch P99 {p99}µs ≥ 50ms — F1/F2 tail regressed");
}
```

This is the single test that makes the elite perf bar *enforceable* rather than aspirational.

---

## 🟠 High findings

### T-04 — No coverage measurement or floor anywhere in CI

No coverage artifact exists on disk (`find` for `*.profraw`/`lcov.info`/`tarpaulin-report` → none;
no `target/llvm-cov/`). `ci.yml` has **no llvm-cov / tarpaulin step** and **no coverage threshold**.
The prior measurements (touring-intelligence ~83%, touring-foundation ~78%) were one-off, manual,
and now **stale** (the workspace was decomposed 47→46 crates since). So the ~13,942 number is
**unverifiable as coverage** — it tells us tests exist, not what they cover. A Premium repo gates a
floor and trends it.

**Recommendation:** add a `coverage` CI job using `cargo llvm-cov` with a **floor that ratchets**:
```yaml
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: llvm-tools-preview }
      - run: cargo install cargo-llvm-cov --locked
      # --lib only (graph_service_e2e excluded until T-02), per-crate so the
      # giant crates don't mask small-crate gaps.
      - run: cargo llvm-cov --workspace --lib --fail-under-lines 70 --lcov --output-path lcov.info
      - uses: actions/upload-artifact@v4
        with: { name: coverage, path: lcov.info }
```
Start the floor at the *measured current* number minus a small margin; ratchet up. Add per-crate
floors for the security-critical crates (touring-ceg, touring-server) at a higher bar.

### T-05 — 8 cargo-fuzz targets exist but never run in CI

`fuzz/fuzz_targets/` has 8 targets (rkyv deserialize, syn parse, public-API, 5 polyglot
search/rewrite). The crate is detached from the workspace (`fuzz/Cargo.toml` `[workspace]`),
correctly. But `grep fuzz .github/workflows/` → **nothing**. Fuzzing that never runs catches
nothing; the W11.6 fuzz wave already *found 5 real bugs* (per memory) — that ROI evaporates without
continuous runs. This is the untrusted-input coverage layer (rkyv IPC, syn on arbitrary Rust,
polyglot on arbitrary source) — precisely the elite-critical parsing surface.

**Recommendation:** nightly scheduled CI job running each target for a bounded time, plus a fast
smoke (`cargo +nightly fuzz run <t> -- -runs=10000 -max_total_time=60`) on PRs touching
`touring-code`/`touring-rkyv`/`touring-ast-polyglot`. Persist a corpus artifact so it accretes.

### T-06 — Inverted test pyramid; subprocess tests miscategorized as E2E

Measured: **11,375** test fns in `crates/*/src` (unit) vs **2,566** in `crates/*/tests`
(integration) — ~82% unit. A healthy pyramid is fine to be unit-heavy, but here the *integration*
layer is both thin AND partly broken (T-02). Worse, several "E2E" files are really
**subprocess-spawning tests** (graph_service_e2e, binary_e2e, cli_smoke) that depend on a built
binary + daemon — fragile, slow, and the source of the hang. There is no clean,
in-process-service integration tier between "unit" and "spawn the whole binary". The
`touring-integration-tests` crate (208 fns across comprehensive_system_e2e, entity_mcp_e2e,
pln2_e2e, 6 wave_*_e2e) is the right home but is feature/wave-scoped, not a systematic integration
matrix of the MCP tool surface.

**Recommendation:** build an in-process MCP-server integration tier (instantiate the server struct,
call tools directly — no subprocess, no daemon) covering the ~169-tool surface for: happy path,
malformed params, and the containment/redaction invariants (T-01). This is fast, deterministic, and
CI-safe.

### T-07 — 37 `#[ignore]` tests; 18 require a daemon socket and never run in CI

`#[ignore]` breakdown: 18 `"requires daemon socket"`, 2 `"requires daemon"`, 1 daemon symbol-store,
**11 bare `#[ignore]`** (no reason), 2 SIMD-flaky, 2 model-download-gated, 1 Wave-8-collateral
known-broken. The CI runner has no daemon (the `health-delta` gate explicitly notes "touring binary
absent in this runner"), so **all 21 daemon-gated tests are permanently skipped in CI** — the entire
daemon RPC round-trip (the product's core) is integration-untested in the pipeline. The 11 bare
`#[ignore]` are the worst: no reason = forgotten, indefinitely. The Wave-8 one
(`cli_decompose_ready ... needs investigation`) is an openly-acknowledged broken contract parked
behind `#[ignore]`.

**Recommendation:** (a) every `#[ignore]` must carry a reason (lint/grep gate in the `gates` job);
(b) stand up a **self-hosted or service-container daemon** in CI so the 21 daemon tests actually run
(the nextest `daemon-db` test-group already exists for serialization); (c) triage the 11 bare
ignores and the Wave-8 broken contract — fix or delete, don't park.

---

## 🟡 Medium findings

### T-08 — 19,296-LOC single test file (`touring-dispatch/src/lifecycle/tests.rs`)

1,211 test fns, 309 section markers in one file. It is *organized* (sub-`mod`s, separators) and
real-data (tempdir-based), so not unmaintainable today — but it's a review/merge-conflict
chokepoint and a compile-time tax (it rebuilds wholesale on any change). Elite practice splits by
behavior into a `lifecycle/tests/` dir (`pre_edit.rs`, `post_edit.rs`, `dispatch.rs`, …). Lower
priority than the gaps above, but it's a maintainability debt that grows.

### T-09 — `redact_secrets` is substring-only — weak even where it IS applied

`sandbox_executor.rs:599-625` redacts a line only if it `contains` one of 8 hardcoded KEY *names*
(`GITHUB_TOKEN`, `AWS_SECRET_ACCESS_KEY`, …) and then blanks after the first `=`/`:`. It will **not**
redact a bare secret *value* in a stack trace (`AKIA...`, `ghp_...`, `sk-ant-...`) that doesn't sit
next to its key name, nor `password123` in free text. So even if SEC-05 wiring is fixed (T-01),
redaction is shallow. A test should pin both the success and the **known-miss** cases so the gap is
documented, and the redactor upgraded to value-pattern regexes (`ghp_[A-Za-z0-9]{36}`,
`AKIA[0-9A-Z]{16}`, `sk-ant-[A-Za-z0-9-]+`).

### T-10 — CI never compiles/runs integration tests (`--lib` only) → integration rot

`ci.yml` test job is `cargo test --workspace --lib`. The `check` job does `cargo check --workspace
--tests` (so they compile), but they are **never executed**. 175 integration test files can rot
behavior-wise while still compiling. Combined with T-02/T-07 this means the *entire* integration +
E2E + daemon tier is outside the green-bar. Fix is the timeout harness (T-02) + nextest in CI, which
makes `--tests` safe to run.

### T-11 — "No mocks" constitution vs reality (mockall in 2 crates)

The constitution forbids mocks (real-data testing). Reality: `mockall` is a dev-dep in
`touring-generator/Cargo.toml:99` and `touring-code/Cargo.toml:87`, used by
`touring-code/tests/mockall_observer.rs` (mocks `SymbolChangeObserver`). The usage is *defensible*
and *honest* — its doc comment (`:1-13`) explains it mocks only the observer side while the store
writes to a **real tempdir DB**, and explicitly calls out it "fills the gap flagged in SKILL.md". So
this is mostly a **doc/policy drift**: the constitution's blanket "no mocks" is stricter than
practice. Reconcile the policy ("real data for I/O; mocks only for narrow trait-contract isolation,
with the I/O side still real") so the rule matches the (reasonable) code. The broad `grep` for
`Mock`/`mock_` also hit production `impl_*.rs`/`circuit_breaker.rs` — those are failover/test-double
*implementations*, worth a glance that they're test-only-constructible.

---

## 🟢 Low findings

### T-12 — Bench-based P99 gates are real but invisible to CI
`ceg_baseline.rs` and the `latency_p99_guard.rs` *test* are good regression gates, but benches
don't run in `cargo test`/CI. Either convert the critical budget assertions to `#[test]` (as
`latency_p99_guard.rs` already does) or add a `cargo bench --no-run` + targeted bench run in a
nightly job.

### T-13 — nextest configured but CI uses plain `cargo test`
`.config/nextest.toml` is well-tuned (retries, `slow-timeout terminate-after`, `daemon-db`
serialization group, JUnit output for the `ci` profile). CI ignores it (`cargo test --workspace
--lib`). Adopting `cargo nextest run --profile ci` gives the slow-test guillotine (which would have
*contained* the T-02 hang), retries for flakes, and JUnit reporting — all already authored, unused.

---

## Coverage verdict

**Broad but unverified, and shallow exactly where it must be deep.** The ~13,942 fns are real,
mostly real-data (1,150 tempfile/tempdir uses), and the unit corpus on parsing/quality/RL is
genuinely strong (the `latency_p99_guard.rs` and `ceg_baseline.rs` patterns are elite-grade). But:
no coverage number is measured or gated in CI; the server integration layer and the entire daemon
RPC tier are skipped in CI; the named security invariants (SEC-01/02/05 + SEC-04 boundary) have
zero assertions; the measured 488ms hook tail has zero guard; 8 fuzz targets never run. The strategy
is **count-rich, gate-poor**.

## The #1 testing lever toward elite

**Make the elite-critical invariants enforceable, not aspirational — by adding the three regression
tests that should fail today and wiring CI to run them.** Concretely, the highest-leverage single
move is a small **`security_invariants_e2e` + `hook_dispatch_p99_guard`** test pair (T-01 + T-03):
an in-process MCP-server test asserting `touring_file_ops` containment + transcript redaction +
sandbox env boundary, plus an hdrhistogram P99 budget test on the real post_edit dispatch path. They
require no subprocess, run CI-safe, and convert Phase 2's biggest Critical findings from "known and
unguarded" into "regressions fail the build". Pair that with T-02's timeout harness (which
simultaneously re-enables the server integration layer in CI and proves the hang) and a
`cargo llvm-cov --fail-under` floor (T-04), and the test strategy crosses from broad-and-shallow to
Premium.
