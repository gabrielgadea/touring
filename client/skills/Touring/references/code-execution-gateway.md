# Code Execution Gateway (CEG) — Operational Reference

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-05-19
> **Status**: COMPLETE through P6 | **Plan**: `~/projects/touring/docs/2026-05-17-ceg-pln2-plan.md`
> **Source**: `crates/touring-ceg/src/gateway/` (20 modules) + `crates/touring-ceg/src/capability/` (7 modules)
> **Note (2026-06-26)**: CEG extraído para o crate dedicado **`touring-ceg`** (era `touring-hooks/src/` até o crate-split). Paths corrigidos nesta data via exploração estrutural.

---

## What the CEG Is

The **Code Execution Gateway** is a ten-stage typestate pipeline that intercepts every
code-bearing action (Bash, Write, ctx_execute, inferlets run, jobs spawn, MCP tools)
before any real execution. No stage can be skipped — the `Execution<S>` typestate
enforces stage order at compile time. X3 (VGP) and X5 (SANDBOX) are structurally
unskippable.

**Safety invariant**: fail-open (exit 0) — the gateway never blocks a session; it
emits warnings and records outcomes for RL learning.

---

## Pipeline: X0..X9

| Stage | Name | What it does | Key symbol |
|-------|------|-------------|------------|
| **X0** | CAPTURE | Intercepts Bash/Write/MCP/inferlets PreToolUse hooks | `capture_tool_call`, `ExecSurface` |
| **X1** | CLASSIFY | Detects surface + language; extracts code body; resolves heredoc staging | `sniff_language`, `Classification`, `CodeBody` |
| **X2** | STATIC | AST-based forbidden-call detector (11 languages); gotcha match; TDG check | `StaticReport`, `StaticSeverity` |
| **X3** | VGP | Verifies symbols cited in the code body against the Touring index | `extract_symbols`, `VgpReport` |
| **X4** | PREDICT | Speculative validation + RL outcome prediction | `ExecutionOutcomePredictor`, `PredictionReport` |
| **X5** | SANDBOX | Dry-run inside SandboxExecutor under a CapabilityProfile | `dry_run_in_sandbox`, `SandboxOutcome` |
| **X6** | CAPABILITY-GATE | Matches required capabilities against the granted profile (deny-by-default) | `gate_capabilities`, `GateReport` |
| **X7** | DECISION | Composite score → Allow / Warn / Deny + canonical fix hint | `composite_score`, `GateDecision`, `Verdict` |
| **X8** | SUPERVISED-EXEC | Real execution under granted profile (landlock LSM + rlimit) | `run_supervised`, `SupervisionPolicy` |
| **X9** | LEARN | Feeds outcome to RL reward + memory + transcript miner | `emit_gate_reward`, `persist_forbidden_as_gotcha` |

**Entry point**: `run_gateway(deps: GatewayDeps) -> GatewayOutcome`
(`crates/touring-ceg/src/gateway/pre_exec.rs`)

**Fast path**: provably pure code (read-only, no side effects) bypasses X5-X8 via
`is_provably_pure` → `pure_skip_outcome` (marker: `FAST_PATH_PURE_MARKER`).

---

## Capability Model (Deno deny-by-default)

Executed code never gets ambient power. It declares the `Capability` set it needs;
a `CapabilityProfile` resolves each request to `Allow / Deny / Prompt`. Deny always
wins over Allow; an empty profile denies everything.

### Capability kinds (`crates/touring-ceg/src/capability/mod.rs`)

| Variant | Scope type | Resource |
|---------|-----------|----------|
| `FsRead(PathScope)` | path subtree | Read filesystem |
| `FsWrite(PathScope)` | path subtree | Write filesystem |
| `Net(HostScope)` | host:port | Outbound network |
| `Run(CmdScope)` | command name | Spawn subprocess |
| `Env(KeyScope)` | env var key | Read environment |

### Four built-in profiles (`capability/builtins.rs`)

| Profile | Default | Grants | Use case |
|---------|---------|--------|----------|
| **ReadOnly** | Deny | FsRead(workspace) + Env(allowlist) | Static analysis, classification, dry-run of pure code |
| **StagedWrite** | Deny | FsRead(workspace) + FsWrite(staging dir) + Env(allowlist) | Generated scripts that produce artifacts |
| **Trusted** | Allow | All minus Run(rm,sudo) and Net(*) | First-party tooling: touring, cargo |
| **Sandboxed** | Deny | FsRead(workspace) + Env(allowlist) | Default for any generic or unverified script |

`ENV_ALLOWLIST`: `PATH HOME USER LANG LC_ALL TERM TZ` — credential-bearing vars
(`AWS_*`, `GITHUB_TOKEN`, etc.) are never in the allowlist.

Linux enforcement: `apply_landlock` + `apply_rlimit` + optional cgroup v2 caps
(`ResourceLimits`, `ResourceCaps`, `CgroupStatus`).

---

## Staging Area

Heredoc temporal-split (write-now / run-later) is handled via `StagingArea` +
`StagingRegistry`. The staging root is under `~/.claude/touring/ceg-staging/`;
`gc_staging` removes entries older than `staging_retention_secs`
(default: `DEFAULT_STAGING_RETENTION_SECS`).

---

## Observability

Gate metrics (`gateway/metrics.rs`) — counters visible via `touring gate-metrics -j`:

| Counter | Meaning |
|---------|---------|
| `record_ceg_captured` | X0 fired |
| `record_ceg_fast_path` | Pure-skip fired |
| `record_ceg_sandboxed` | X5 ran |
| `record_ceg_blocked` | X7 → Deny |
| `record_workflow_antipattern_detected` | P8 anti-pattern hit |
| `record_workflow_advice_emitted` | P8 advice injected |
| `record_antipattern_converted` | Anti-pattern converted to canonical form |

Dry-run results are cached via `DryRunCache` (key: `dry_run_cache_key`; config:
`CacheConfig`; stats: `CacheStats`). Subprocess pool: `ExecPool` with bounded
concurrency (`PoolConfig`, `PoolStats`).

---

## TACO Integration Points

| When | What to do |
|------|-----------|
| Before running any script/command | Route through `run_gateway` (X0 captures PreToolUse automatically) |
| Checking gateway health | `touring gate-metrics -j` — inspect CEG counters |
| A command was blocked | `GatewayError` + `GateDecision::Deny` → read the `reason` + `canonical_fix` |
| Adding a new execution surface | Extend `ExecSurface` enum in `gateway/capture.rs` |
| Per-project capability tuning | `resolve_capability_profile` / `ProjectProfileRegistry` in `capability/resolve.rs` |
| Auditing staging entries | `staging_registry::RegistryEntry`; `content_hash` for dedup |

---

## Key Files (all in `crates/touring-ceg/src/`)

```
gateway/
  pre_exec.rs         ← run_gateway entry point + GatewayOutcome + GatewayDeps
  typestate.rs        ← Execution<S> typestate (Captured→…→Decided)
  capture.rs          ← X0: ExecSurface + capture_tool_call
  classify.rs         ← X1: sniff_language, Classification, CodeBody
  static_stage.rs     ← X2: StaticReport, StaticSeverity
  vgp_stage.rs        ← X3: extract_symbols, VgpReport
  predict.rs          ← X4: ExecutionOutcomePredictor, PredictionReport
  sandbox_stage.rs    ← X5: dry_run_in_sandbox, SandboxOutcome
  gate.rs             ← X6: gate_capabilities, GateReport, GatedCapability
  decision.rs         ← X7: composite_score, GateDecision, Verdict
  supervised.rs       ← X8: run_supervised, SupervisionPolicy
  learn.rs            ← X9: emit_gate_reward, persist_forbidden_as_gotcha
  fast_path.rs        ← Pure-code shortcut (is_provably_pure)
  dry_run_cache.rs    ← DryRunCache (blake3-keyed)
  exec_pool.rs        ← ExecPool (bounded subprocess pool)
  staging.rs          ← StagingArea, gc_staging
  staging_registry.rs ← StagingRegistry, content_hash
  metrics.rs          ← CEG gate-metrics counters
  error.rs            ← GatewayError taxonomy

capability/
  mod.rs              ← Capability enum + covers()
  profile.rs          ← CapabilityProfile + Decision
  builtins.rs         ← BuiltinProfile enum + 4 ready-made profiles
  scope.rs            ← PathScope, HostScope, CmdScope, KeyScope
  resolve.rs          ← resolve_capability_profile, ProjectProfileRegistry
  enforce_linux.rs    ← apply_landlock, apply_rlimit, EnforcementLevel
  limits.rs           ← ResourceLimits, ResourceCaps, cgroup_v2_status
```

---

_CEG v1.0 | CEG Pln2 P3-P6 complete | `touring gate-metrics -j` for live counters_
