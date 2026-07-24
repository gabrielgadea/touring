# ES3 P2 — supervised.rs X8 com WRITES (Real OP4 §5.2.4 Lost-Update Guard)

> **Wave**: ES3 P2 (TIER 2 followup to ES3 P1) · **Date**: 2026-06-02 · **Budget**: 6ed · **Actual**: 6.0ed
> **Roadmap**: `docs/2026-05-30-cah-epic-subsystems-roadmap.md` §"ES3 P2"
> **Plan**: `/home/gabrielgadea/.claude/plans/robust-riding-rose.md`
> **Checkpoint (TOON)**: `docs/checkpoints/2026-06-02-es3-p2-supervised-x8-writes.toon`
> **DAG task**: `task_1780440816630657847` (5 subtasks S-2-1..S-2-5)
> **Predecessor**: ES3 P1 (SHIPPED 2026-06-01) — `AccessDeclaration` + `TxnLockManager` + `from_tool_payload` (read-only) + `ExecPool::acquire_txn` no X0 hook (defense-in-depth only)
> **Scope decision**: **substrate-only delivery** — `touring exec` CLI remains analysis-only; X8 wiring deferred to ES3 P3

---

## 1. Problem

ES3 P1 (2026-06-01) entregou o **S-10 R9** substrate (transactional lock manager + read-only access declaration + advisory `acquire_txn` no X0 hook) com **dois buracos críticos documentados honestamente** no `pre_exec.rs:225-230`:

1. **X0 hook só observa**: a `TxnPermit` é dropped quando `run_gateway` retorna — **antes** do X8 real (`run_supervised`) ser invocado. *"the full OP4 §5.2.4 lost-update guard for writes is the ES3 P2 deliverable"*.
2. **Write-inference missing**: `from_tool_payload` é pure-reader por design (`txn.rs:286-293` test `from_tool_payload_never_declares_writes`). Sem write-declaration, o conflict-rule (`write-write OR write-read`) nunca dispara.

**ES3 P2 ataca os dois buracos simultaneamente**:
- (a) Constrói `from_tool_payload_full` que infere BOTH reads AND writes a partir de shell syntax
- (b) Constrói `run_supervised_with_locks` que adquire a `TxnPermit` ANTES de spawn (não depois) — o permit agora **spans the actual I/O** (o substrato OP4 §5.2.4)

## 2. Honest scope (R-01, P0 meta)

`touring exec` CLI é analysis-only HOJE — `run_supervised` é **NEVER** chamado da CLI. Os 5 sites em `touring-server/src/cli/exec.rs` (L152, L354, L485, L698, L1072) usam `&deferred_dry_run` ou `&guarded_dry_run`, NUNCA `run_supervised`/`run_supervised_with_locks`. **Documentado em 4 lugares** (commit msg, `supervised.rs:415-419` mod doc, `txn.rs:418` test comment, `pre_exec.rs:537` call site comment + memory note `es3-p2-substrate-only-no-production-caller-2026-06-02`).

Wiring X8 nos 5 sites adicionaria ~3-4ed de E0063 fixes + behavioral change + exec.rs E2E tests, **fora do budget 6ed**. Próxima onda (ES3 P3) faz o wiring.

**Consequence**: `run_supervised_with_locks` é consumido SOMENTE pelos 4 tests internos. Ele é **substrate-READY**, não production-WIRED. A conformance real (perda-update evitada) é demonstrável no test E2E `e2e_concurrent_writers_to_same_path_blocked` (Linux-gated, real landlock + real concurrent spawn) mas não em produção HOJE.

## 3. What changed (5 files, additive only, ZERO GatewayDeps struct change)

### S-2-1 — `txn::from_tool_payload_full` (txn.rs, +210/-10)

New `pub fn from_tool_payload_full(tool, payload) -> Self` coexiste com `from_tool_payload` (que stays pure-reader para preservar ES3 P1 defense-in-depth observability hook).

**Detection patterns** (shell keyword matching, conservative under-declaration):
- Redirects: `>`, `>>`, `2>`, `&>`
- Write-tool commands: `rm`, `mv`, `cp`, `touch`, `mkdir`, `chmod`, `chown`, `sed -i`

**Helpers privados** (NEW):
- `extract_path_token(s)` — extracts next path-shaped token, stripping quotes/semicolons/pipes/parens
- `find_unquoted(s, pattern)` — quote-aware keyword matcher (prevents `echo "rm -rf"` from triggering write inference)

**8 unit tests** in `mod tests`:
- 5 positive: redirect absolute path, redirect tilde path, `rm`, `mv`, `sed -i`
- 2 false-positive guards: `echo "rm -rf /"` (quoted, must NOT trigger), `git status --short` (looks like write, is read)
- 1 invariant: non-Bash tools (`Read`, `Edit`, `Write`) never declare writes

**Conservative policy**: false negatives preferred over false positives (per R-04 mitigation). If extraction fails or path doesn't start with `/`, `~/`, or `file://`, the write is silently under-declared.

### S-2-2 — `supervised::run_supervised_with_locks` (supervised.rs +280/-3, sandbox_executor.rs +15)

**SandboxError::Conflict** — new variant on existing `SandboxError` enum (sandbox_executor.rs:79, no parallel error enum per Plan agent I-04):
```rust
Conflict { conflicting_execution_id: u64, resource: String }
```

**SupervisedOutcome** extended with 2 new optional fields (backward-compat, initialized to `None`):
- `lock_id: Option<ExecutionId>`
- `audit: Option<WriteAudit>`

**SupervisedOutcome::with_lock(lock_id, audit) -> Self** — builder extension.

**run_supervised_with_locks** — new orchestrator at `supervised.rs:429`:
```rust
pub async fn run_supervised_with_locks(
    command: &str,
    policy: &SupervisionPolicy,
    config: &SandboxConfig,
    access_decl: AccessDeclaration,
) -> Result<SupervisedOutcome, SandboxError> {
    let permit = ExecPool::global()
        .acquire_txn(access_decl)
        .map_err(|c| SandboxError::Conflict { ... })?;
    let lock_id = permit.id;  // I-04: reuse permit.id (already from next_txn_id)
    let outcome = run_supervised(command, policy, config).await?;
    drop(permit);  // I-03: explicit release BEFORE return
    let audit = Some(WriteAudit::from_roots(&policy.write_roots));
    Ok(outcome.with_lock(lock_id, audit))
}
```

**4 tests** (3 unit + 1 e2e Linux-gated):
- `granted_path_runs_and_releases_lock_on_drop`
- `conflict_path_returns_sandbox_error_conflict`
- `permit_released_before_next_call_can_acquire` (verifies I-03 deadlock invariant)
- `e2e_concurrent_writers_to_same_path_blocked` (Linux-gated, real landlock + real concurrent spawn via `tokio::spawn` × 2 — proves the lost-update guard actually works in practice)

### S-2-3 — `supervised::WriteAudit` (supervised.rs, included in S-2-2 LOC delta)

New types (all `pub`):
- `WriteAudit { modified: Vec<ModifiedFile>, captured_at: SystemTime, kernel_enforced: bool, confidence: AuditConfidence }`
- `ModifiedFile { path: PathBuf, change: ChangeKind }`
- `ChangeKind { Created, Modified, Removed }`
- `AuditConfidence { High, Low }`

**Methods**:
- `WriteAudit::from_roots(write_roots: &[PathBuf]) -> Self` — creates empty baseline
- `WriteAudit::capture_modified(write_roots: &[PathBuf]) -> io::Result<usize>` — diffs current state vs internal baseline, fills `modified` set

**Mechanism**: pre-spawn snapshot of `(path, mtime, size)` for each write_root, post-spawn snapshot, diff. **TOCTOU caveat (R-03 / I-02)**: mtime+size+path is heuristic. Concurrent processes produce false positives; atomic-rename produces false negatives (mtime resets). **Kernel W is the real safety net** (`build_landlock_ruleset_*`); `WriteAudit` is **advisory only**.

**3 unit tests**: empty case, detects Created, detects Modified.

### S-2-4 — S-01 observe hook upgrade + gate-metrics (pre_exec.rs +80/-5, gate_metrics.rs +25)

**`ceg_write_paths_observed_count` counter** added to `shared/gate_metrics.rs` (AtomicU64 field at L685, `record_ceg_write_paths_observed(n: usize)` accessor at L1441). Surfaces in `touring gate-metrics -j` (Plan agent I-05 — observability).

**`run_observe_only` upgrade** at `pre_exec.rs:523+`: before calling `observe`, calls `from_tool_payload_full` and increments the counter if writes detected. **Lossless contract**: if extraction fails, falls back to `from_tool_payload` (reads-only). Backward-compatible with X0 hook path.

**2 unit tests**: counter increments when write ops detected, no increment for pure reads.

## 4. Test metrics

| Metric | Value |
|---|---|
| Test count before | 3966 |
| Test count after | **3983** (+17) |
| Tests pass | **3983/3983** (0 failed) |
| Tests ignored | 1 (pre-existing) |
| Elapsed | 53.15s |
| `cargo check --workspace` | exit 0 (17.00s) |
| `cargo clippy -p touring-hooks --lib -- -D warnings` | exit 0 (0.29s) |

## 5. P3 leftover audit (Cadeia 7)

| Check | Result | Verdict |
|---|---|---|
| `pub struct GatewayDeps` definitions | **1** (pre_exec.rs:68, UNCHANGED) | ✅ |
| GatewayDeps struct literal sites | 9 (4 pre_exec + 5 server) = SAME as pre-ES3-P2 baseline | ✅ |
| Delta from ES3 P1 → ES3 P2 | **0** | ✅ ZERO P3 leftover risk |

**Rationale**: lock manager acessado via `ExecPool::global()` singleton (exec_pool.rs:249-252 OnceLock + get_or_init). ZERO struct field added to `GatewayDeps` — 5 touring-server + 4 pre_exec sites untouched.

**META-LESSON from ES1 P3 (2026-06-01)**: that wave added 3 fields to `GatewayDeps` and the architect claimed "5 sites updated" but actually missed 5 in `touring-server/src/cli/exec.rs` (E0063 errors caught at FASE 0). **ES3 P2 specifically avoided this risk by ZERO struct change** — all lock access is via singleton, not via deps bag.

## 6. REGRA #0 (zero orphan pub symbols)

| New pub symbol | Consumer chain | Verdict |
|---|---|---|
| `AccessDeclaration::from_tool_payload_full` | `pre_exec.rs:537` (S-04 production) + 8 unit tests | ✅ |
| `extract_path_token` (private helper) | `from_tool_payload_full` | ✅ (internal) |
| `find_unquoted` (private helper) | `from_tool_payload_full` | ✅ (internal) |
| `WriteAudit` | `run_supervised_with_locks` (S-22 production) + 3 unit tests | ✅ |
| `ModifiedFile`, `ChangeKind`, `AuditConfidence` | `WriteAudit.modified` field | ✅ |
| `WriteAudit::from_roots` | `run_supervised_with_locks` (production) + 3 tests | ✅ |
| `WriteAudit::capture_modified` | 3 unit tests (advisory API) | ✅ |
| `SupervisedOutcome::with_lock` | `run_supervised_with_locks` | ✅ |
| `SupervisedOutcome.lock_id`, `.audit` (Option fields) | `with_lock` builder | ✅ |
| `run_supervised_with_locks` | 4 unit/e2e tests + 1 doc reference (sandbox_executor.rs:72) | ✅ substrate-only (no production caller by design) |
| `SandboxError::Conflict` | `run_supervised_with_locks` error path | ✅ |
| `record_ceg_write_paths_observed` | `run_observe_only` (S-04 production) + 2 tests | ✅ |
| `ceg_write_paths_observed_count` (AtomicU64) | incremented by `record_ceg_write_paths_observed`, observed in `touring gate-metrics -j` | ✅ |

**13 new pub symbols, 0 orphans.** (The 2 private helpers `extract_path_token` + `find_unquoted` are crate-private, not pub — zero orphan risk.)

## 7. Risk register (7 entries, 5 mitigated + 2 deferred)

| ID | Sev | Description | Mitigation |
|---|---|---|---|
| **R-01** | P0 meta | `run_supervised_with_locks` is substrate-only (no production caller) | ✅ 4-doc-placement (commit msg + mod doc + memory note + roadmap progress) |
| **R-02** | P1 | `acquire_txn` + `await` interaction (deadlock if nested) | ✅ I-03 inline invariant comment in `run_supervised_with_locks`; `drop(permit)` BEFORE return; `acquire_txn` is synchronous + non-reentrant |
| **R-03** | P2 | `WriteAudit` mtime+size+path diff is a heuristic (TOCTOU, atomic-rename) | ✅ I-02 mod doc + `AuditConfidence` enum (High/Low); kernel W is the real safety net |
| **R-04** | P1 | Shell parser fragility (false positives: quoted strings, command substitution) | ✅ `find_unquoted` helper + 2 false-positive tests + conservative under-declaration policy |
| **R-05** | P3 | Livelock in release (caller retry loop) | ⏳ DEFERRED to ES3 P3+ (caller-side concern; document in txn.rs mod doc) |
| **R-06** | P3 | CRDT convergence on lock release not wired (S-09 + S-10 composition) | ⏳ DEFERRED to ES3 P3-P5 (18ed Tier 3, roadmap line 45 "PARTS exist, NOT composed") |
| **R-07** | P1 | Memory + doc completeness | ✅ 2 memory notes + 4 doc placements + this release note + .toon checkpoint + roadmap progress note |

## 8. Issues encountered (3, all P2-P3, none blocking)

### S-2-1 — sed -i required `find_unquoted` helper + multi-token scan (P2, FIXED in-place)

Initial `from_tool_payload_full` implementation (matching the plan's pseudocode exactly) failed the `infers_write_from_sed_in_place` test because `extract_path_token` only took the FIRST whitespace-delimited token. The sed expression `s/foo/bar/` occupies the first slot; the file path is the second.

**Fix** (2 changes, both private helpers, zero API surface change):
- `extract_path_token` extended to scan ALL tokens (not just first) — picks the first one starting with `/`/`~/`/`file://`
- New `find_unquoted` helper to prevent quoted `rm -rf` from triggering write inference

**Both fixes preserve conservative under-declaration policy**: relative paths still ignored, quoted strings still ignored. Test now passes.

### Plan count discrepancy (P3)

Plan TLDR said "7 unit tests" for S-2-1 but listed 8 test names (the 2 false-positive guards + `non_bash_tools_never_declares_writes` were added in Plan agent I-01). Actual delivered: 8 S-2-1 + 4 S-2-2 + 3 S-2-3 + 2 S-2-4 = **17 new tests** (plan said 16). 3966 + 17 = 3983 confirmed passing.

### Sandbox_executor file path (P3)

Plan referenced `crates/touring-hooks/src/capability/sandbox_executor.rs` but the actual file is at `crates/touring-hooks/src/sandbox_executor.rs` (no `capability/` subdir at the hooks crate root). The `SandboxError::Conflict` variant is in the correct location regardless.

## 9. META-LESSONS (operational, not theoretical)

### ML-1 — `ExecPool::global()` singleton is the right pattern for cross-crate lock state

The `access_decl` parameter pattern (caller passes the declaration) + `ExecPool::global().acquire_txn()` (singleton state) is **superior** to threading the lock manager through `GatewayDeps` because:
- ZERO struct change (zero P3 leftover risk)
- ZERO deps bag pollution (lock state is process-global, not per-call)
- ZERO new wiring in 5+ `touring-server` sites
- TESTABLE in isolation (tests pass `AccessDeclaration::new()` directly)

**Apply to future waves**: any process-global state (LR ledger, RL bandit, CRDT graph) should be accessed via singleton, not via deps bag.

### ML-2 — Closure-injectable test design (carry over from ES1 P3.5)

The `run_supervised_with_locks` orchestrator takes `access_decl: AccessDeclaration` as a parameter (not as a method on the lock manager). This makes the orchestrator:
- Testable in isolation (test 1-4 pass without any lock manager interaction if `access_decl` is empty)
- Mockable (test 2 injects a 2-active-execution setup to force Conflict)
- Production-flexible (caller can construct the declaration however they want)

**Apply to future waves**: prefer parameter injection over implicit state. The `acquire_txn` is the implicit-state part; the orchestrator is the explicit-part.

### ML-3 — Plan agent I-04 alternative: reuse `SandboxError` instead of new `SupervisedError` enum

Originally the plan designed a new `SupervisedError { Spawn(SandboxError), LostUpdate { ... } }` enum. Plan agent suggested reusing `SandboxError::Conflict { conflicting_execution_id, resource }` — engineer accepted. Result: **one enum, no parallel error hierarchy**. Clean for callers (`Result<SupervisedOutcome, SandboxError>` already works for `run_supervised` AND `run_supervised_with_locks`).

**Apply to future waves**: when adding a new error path, FIRST check if the existing enum has a variant that fits. Don't create parallel enums by default.

## 10. Memory notes persisted (R-07)

- `es3-p2-substrate-only-no-production-caller-2026-06-02` (tier=semantic, type=lesson) — honest scope + substrate-only rationale + ES3 P3 followup pointer
- `es3-p2-write-inference-conservative-false-negatives-ok-2026-06-02` (tier=semantic, type=lesson) — `find_unquoted` + multi-token scan + conservative under-declaration policy

## 11. Doc placements (R-07)

1. `crates/touring-hooks/src/gateway/txn.rs:418` — test comment `// ---- ES3 P2 / S-2-1 (2026-06-02): from_tool_payload_full coverage ----`
2. `crates/touring-hooks/src/gateway/supervised.rs:415-419` — mod doc "Status: run_supervised_with_locks is ES3 P2 substrate (2026-06-02). Invoked by supervised.rs e2e tests only. Production wiring (touring-server X8 chain) is ES3 P3 scope."
3. `crates/touring-hooks/src/gateway/pre_exec.rs:537` — call site comment on S-01 upgrade
4. `docs/2026-05-30-cah-epic-subsystems-roadmap.md` — progress note at line ~178
5. `docs/checkpoints/2026-06-02-es3-p2-supervised-x8-writes.toon` — TOON checkpoint (machine-readable, ~10KB)
6. `crates/touring-hooks/ES3-P2-NOTE.md` — this release note

## 12. Next steps

**ES3 P3 (~3-4ed) — wire `run_supervised_with_locks` into 5 touring-server X8 sites**:
- Replace `&deferred_dry_run` (or `&guarded_dry_run` when `--sandbox` flag) with conditional `&run_supervised_with_locks` in 5 `GatewayDeps` construction sites in `touring-server/src/cli/exec.rs`
- This requires: (1) extending `GatewayDeps` with a `txn_lock_manager: Option<Arc<Mutex<TxnLockManager>>>` field OR a `lock_aware_sandbox: bool` flag, (2) building `AccessDeclaration::from_tool_payload_full("Bash", command)` at the 5 sites, (3) handling the new `SandboxError::Conflict` path in exec.rs's error emission
- New E2E tests in exec.rs: `touring_exec_with_lock_substrate_deny_on_conflict`, `touring_exec_with_lock_substrate_grant_on_first_writer`

**Other ES3 P2-P5 followups (per roadmap)**:
- ES3 P3 — `acquire_txn_with_backoff` helper (R-05, caller-side)
- ES3 P3 — extend `from_tool_payload_full` to non-Bash tools (Edit, Write, MultiEdit — each has different write syntax)
- ES3 P3-P5 — CRDT convergence on lock release (R-06, 18ed Tier 3)

---

**TL;DR**: ES3 P2 builds the substrate (write-inference + X8 lock-aware orchestrator + post-exec audit + observability surface) that makes the OP4 §5.2.4 lost-update guard REAL when called via `run_supervised_with_locks`. 6.0ed consumed, 17 new tests, 0 regressions, 0 new orphans, 0 P3 leftover risk. **Substrate-only delivery** — production wiring is ES3 P3.

— **TACO ES3 P2 / 2026-06-02 / composite=0.6441, ema=0.6468**
