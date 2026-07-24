# Code Execution Gateway (CEG) - Pln2

> Pln2 = (Pln1)^2 - Sandbox-First Execution Protocol
>
> Generated: 2026-05-17 | Phases: 8 | Deliverables: 42 | Effort: ~34.5 engineer-days | Est. tests: 765

_Pln2 = (Pln1)^2 is interpreted as depth-squared: every one of the nine rigor dimensions of Pln1 is lifted to a higher order, not a literal squaring of deliverable count. Concretely: 4 phases -> 8 phases, 13 -> 42 deliverables, each carrying the full nine-dimension rigor._

_Rendered by generate_ceg_pln2_plan.py from ceg_pln2_data.json (code/data split, stdlib-only)._

## 1. Pln1 Critique - Nine Rigor Dimensions

| Dim | Name | Pln1 state | Gap | Pln2 lift |
|-----|------|------------|-----|-----------|
| a | Precisao / confiabilidade / fontes | 7 gaps with real file:line evidence. | No VGP of to-be-created symbols; a single external source (Deno); gaps never cross-checked with wiring impact / ast blast. | Mandatory VGP for every new symbol; context7 per critical dependency; each gap measured with wiring impact + ast blast; sizing grounded in real blast_radius. |
| b | Escalabilidade | Not addressed. | No concurrency model (daemon is an actor, mpsc 128, one actor per project); no per-project profiles; no resource caps under load. | Explicit bounded subprocess pool + backpressure; per-project capability profiles; cgroup/rlimit caps. |
| c | Performance e desempenho | Mentioned blake3 cache + skip-whitelist. | No latency budgets; ignored that the sandbox dry-run adds a full second execution (~2x latency per command). | P50/P99 budget per X0..X9 stage; static-only fast path for provably pure code; criterion benchmarks as a regression gate. |
| d | Maximizacao da aplicabilidade | Covered Bash + ctx_execute. | Missed inferlets run, jobs spawn, subagent Bash, heredoc temporal-split (write now / run later), MCP tools that execute. | Execution-surface matrix: 6+ surfaces x 11 languages, every cell with explicit coverage. |
| e | Excelencia / qualidade do codigo | Not specified. | No TDG target, no typestate, no error taxonomy, no test counts, no invariants stated. | Execution<S> typestate (8 states, compile-time enforced); TDG grade A target; exhaustive GatewayError enum; zero unwrap in prod; test count per deliverable; exit-0 fail-open invariant preserved. |
| f | Detalhamento e especificacoes | Deliverables were one-liners. | No exact files, signatures, acceptance criteria or LOC estimate. | Each deliverable is a full spec: crate, files, VGP-verified symbols, signatures, test count, measurable acceptance criterion. |
| g | Integracao sistemica / mapa de relacoes | Named crates loosely. | No formal relationship map; no blast radius per change; no list of existing symbols to reuse instead of recreate. | Systemic map: deliverable -> crate -> reused symbols (VGP) -> new symbols -> blast_radius -> wired consumers; wiring chains per file. |
| h | Atualizacao / compatibilidade de dependencias | Zero dependency audit. | ast-grep 0.36->0.42 upgrade pending; real Linux sandboxing (landlock/seccomp) absent from the design. | Dedicated P0 dependency-audit phase; context7-verified latest versions; prefer modern, reputable crates (landlock, rustix, ast-grep 0.42.x). |
| i | Potenciacao do projeto como um todo | Mentioned only the RL loop. | No synergy map - how the CEG amplifies the whole system. | Explicit synergy map: CEG <-> RL, memory/gotcha, generator (speculate reuse), transcript miner, synergy WIRED_PAIRS, cli_suggester enrichment. The CEG is a new organ of the nervous system. |

## 2. Execution Pipeline X0..X9 (typestate-modelled)

| Stage | Name | Purpose | Reuses | Adds |
|-------|------|---------|--------|------|
| X0 | CAPTURE | Intercept every code-bearing action: PreToolUse Bash/Write, MCP ctx_execute, inferlets run, jobs spawn. | settings.json hook wiring; cli_suggester.rs classify(). | pre-exec hook entry point. |
| X1 | CLASSIFY | Detect execution surface + language; extract the code body; resolve heredoc temporal-split via the staging registry. | bash_ast_validator::command_shape; SandboxLanguage enum. | ExecSurface enum; CodeBody extractor. |
| X2 | STATIC | Static analysis of the code body across all languages. | bash_ast_validator::validate_command; AstGrepRiskSignalLayer; detect_forbidden_calls; ast tdg; gotcha match. | 11-language AST-based forbidden detector. |
| X3 | VGP | Verify symbols cited inside the code body against the index. | touring index find; generate verify. | code-body symbol extractor. |
| X4 | PREDICT | Speculative validation + RL outcome prediction. | generator/speculate/bridge.rs; action_signature.rs. | ExecutionOutcomePredictor. |
| X5 | SANDBOX | Dry-run inside SandboxExecutor under a CapabilityProfile. | sandbox_executor::execute_in_sandbox; credential whitelist. | dry_run_in_sandbox with capability binding. |
| X6 | CAPABILITY-GATE | Match required capabilities against the granted profile (Deno model: read/write/net/run/env allow/deny/ignore). | (none - new subsystem). | Capability, CapabilityProfile, Decision. |
| X7 | DECISION | Composite score -> Allow / Warn / Deny + reason + canonical fix. | pre_bash HookResponse::Deny. | GateDecision; composite scorer. |
| X8 | SUPERVISED-EXEC | Real execution under the granted profile (landlock + rlimit applied). | tokio::process::Command. | landlock binding; rlimit caps. |
| X9 | LEARN | Feed the outcome to RL reward + memory + transcript miner + synergy. | learning reward; memory store; transcript_miner. | ceg gate-metrics counters. |

## 3. Capability Model (Deno deny-by-default, typed)

### 3.1 Capability kinds

| Kind | Scope | Note |
|------|-------|------|
| FsRead | PathScope | Read access to a path subtree. |
| FsWrite | PathScope | Write access to a path subtree. |
| Net | HostScope | Outbound network to host:port. |
| Run | CmdScope | Spawn a subprocess by command name. |
| Env | KeyScope | Read an environment variable key. |


### 3.2 Built-in profiles

| Profile | Default | Grants | Use case |
|---------|---------|--------|----------|
| ReadOnly | Deny | FsRead(workspace); Env(allowlist). | Static analysis, classification, dry-run of pure code. |
| StagedWrite | Deny | FsRead(workspace); FsWrite(staging dir only); Env(allowlist). | Generated scripts that legitimately produce artifacts. |
| Trusted | Allow | All, minus Run(rm,sudo) and Net(*) which stay Deny. | taco-forge / touring / cargo - first-party tooling. |
| Sandboxed | Deny | FsRead(workspace); Env(allowlist); everything else Deny. | Default profile for any generic / unverified script. |

## 4. Phases & Deliverables

### P0 - Foundations & Evidence (deps: none) [~3.5d]

_Forensic measurement and dependency modernization before any code is written - dimensions (a), (c), (g), (h)._

#### P0.1 - Forensic blast-radius map of all touched files (deps: none) [size=M]

- **Crate**: (analysis only)
- **Files**: docs/2026-05-17-ceg-blast-map.md
- **Reused symbols (VGP)**: touring ast blast; touring ast meta
- **New symbols**: (none)
- **Tests**: n/a (measurement)
- **Acceptance**: Every file the CEG will touch has a recorded blast_radius + quality_score + TDG grade.
- **Rigor dimensions**: a, g

#### P0.2 - wiring impact per target symbol (deps: P0.1) [size=S]

- **Crate**: (analysis only)
- **Files**: docs/2026-05-17-ceg-wiring-impact.md
- **Reused symbols (VGP)**: touring wiring impact; touring wiring chains
- **New symbols**: (none)
- **Tests**: n/a (measurement)
- **Acceptance**: Each existing symbol the CEG reuses has a transitive consumer count and chain map.
- **Rigor dimensions**: a, g

#### P0.3 - context7 best-practices dossier (deps: none) [size=M]

- **Crate**: (research only)
- **Files**: docs/2026-05-17-ceg-best-practices.md
- **Reused symbols (VGP)**: context7 resolve/query-docs
- **New symbols**: (none)
- **Tests**: n/a (research)
- **Acceptance**: Documented best practices for: Deno permission model, Linux landlock LSM, seccomp, ast-grep, tokio process isolation.
- **Rigor dimensions**: a, h

#### P0.4 - criterion latency baseline (deps: P0.1) [size=M]

- **Crate**: touring-hooks
- **Files**: benches/ceg_baseline.rs
- **Reused symbols (VGP)**: criterion (dev-dep)
- **New symbols**: ceg_baseline bench group
- **Tests**: 3 bench cases
- **Acceptance**: Recorded P50/P99 for current pre-bash, sandbox spawn, ast-grep scan - the regression floor.
- **Rigor dimensions**: c

#### P0.5 - dependency audit & modernization (deps: P0.3) [size=M]

- **Crate**: workspace Cargo.toml
- **Files**: Cargo.toml; docs/2026-05-17-ceg-deps-audit.md
- **Reused symbols (VGP)**: touring ast workspace-info
- **New symbols**: (none)
- **Tests**: cargo check --workspace
- **Acceptance**: ast-grep-core 0.36->0.42.x; landlock + rustix crates evaluated and pinned; tree-sitter-go ABI fix scheduled.
- **Rigor dimensions**: h

### P1 - Coverage Closure (deps: P0) [~3.5d]

_Close the acute gap - generic script execution running unvalidated. Pln1 FASE A, deepened to all execution surfaces._

#### P1.1 - pre-bash hook 'if' extension for all exec surfaces (deps: P0.1) [size=S]

- **Crate**: settings.json
- **Files**: ~/.claude/settings.json
- **Reused symbols (VGP)**: touring-hook pre-bash
- **New symbols**: (config only)
- **Tests**: 4 hook-registry assertions
- **Acceptance**: python3/node/ruby/sh/bash + *.py/*.sh invocations trigger pre-bash so bash_ast_validator + PreToolValidator run.
- **Rigor dimensions**: d

#### P1.2 - .sh added to taco-forge-guard + Write/Edit matchers (deps: P0.1) [size=S]

- **Crate**: (hooks)
- **Files**: ~/.claude/hooks/taco-forge-guard.sh; ~/.claude/settings.json
- **Reused symbols (VGP)**: is_code_file()
- **New symbols**: (extends is_code_file)
- **Tests**: 6 guard tests
- **Acceptance**: Shell scripts can no longer be created via raw Write/Bash without the taco-forge gate.
- **Rigor dimensions**: d

#### P1.3 - detect_forbidden_calls -> 11 languages, AST-based (deps: P0.5) [size=M]

- **Crate**: touring-server
- **Files**: tools/ctx_execute_tools.rs
- **Reused symbols (VGP)**: detect_forbidden_calls; AstGrepRiskSignalLayer
- **New symbols**: ast_forbidden_scan
- **Tests**: 11 unit + 3 E2E
- **Acceptance**: Forbidden-call detection covers all 11 sandbox languages and uses ast-grep, not naive substring match.
- **Rigor dimensions**: d, e

#### P1.4 - ctx_execute forbidden_calls ENFORCED (deps: P1.3) [size=M]

- **Crate**: touring-server
- **Files**: tools/ctx_execute_tools.rs
- **Reused symbols (VGP)**: ctx_execute_impl
- **New symbols**: ForbiddenCallPolicy
- **Tests**: 5 unit + 2 E2E
- **Acceptance**: ctx_execute blocks (does not merely report) when forbidden calls are present, unless an explicit override is passed.
- **Rigor dimensions**: d, e

#### P1.5 - inferlets + jobs execution-surface coverage (deps: P1.1) [size=M]

- **Crate**: touring-server, touring-hooks
- **Files**: tools/*; sandbox_executor.rs
- **Reused symbols (VGP)**: inferlets run; jobs spawn
- **New symbols**: ExecSurface::Inferlet, ExecSurface::Job
- **Tests**: 4 unit + 2 E2E
- **Acceptance**: WASM inferlets and background jobs route through the same X0..X9 pipeline as Bash.
- **Rigor dimensions**: d

#### P1.6 - heredoc temporal-split detection (deps: P1.1) [size=M]

- **Crate**: touring-hooks
- **Files**: sandbox_executor.rs; staging registry stub
- **Reused symbols (VGP)**: extract_file_context
- **New symbols**: StagingRegistry (stub)
- **Tests**: 5 unit
- **Acceptance**: A script written in one turn and executed in a later turn is still gated at execution time.
- **Rigor dimensions**: d, f

### P2 - Capability Model (deps: P0) [~4.5d]

_The Deno-inspired deny-by-default capability layer - the heart of 'code never gets ambient authority'._

#### P2.1 - Capability enum + scope types (deps: P0.3) [size=M]

- **Crate**: touring-hooks
- **Files**: src/capability/mod.rs; src/capability/scope.rs
- **Reused symbols (VGP)**: (new module)
- **New symbols**: Capability, PathScope, HostScope, CmdScope, KeyScope
- **Tests**: 12 unit
- **Acceptance**: Capability is a total, serializable enum; scopes support subtree/glob matching.
- **Rigor dimensions**: e, f

#### P2.2 - Decision enum + CapabilityProfile (deps: P2.1) [size=M]

- **Crate**: touring-hooks
- **Files**: src/capability/profile.rs
- **Reused symbols (VGP)**: Capability
- **New symbols**: Decision, CapabilityProfile
- **Tests**: 10 unit
- **Acceptance**: A profile resolves any Capability to a Decision in O(grants) with deny-by-default.
- **Rigor dimensions**: e, f

#### P2.3 - four built-in profiles (deps: P2.2) [size=S]

- **Crate**: touring-hooks
- **Files**: src/capability/builtins.rs
- **Reused symbols (VGP)**: CapabilityProfile
- **New symbols**: ReadOnly, StagedWrite, Trusted, Sandboxed
- **Tests**: 8 unit
- **Acceptance**: Each built-in profile has a documented use case and a test proving its grant set.
- **Rigor dimensions**: d, e

#### P2.4 - landlock LSM binding + rlimit caps (deps: P2.3,P0.5) [size=L]

- **Crate**: touring-hooks
- **Files**: src/capability/enforce_linux.rs
- **Reused symbols (VGP)**: landlock crate; rustix
- **New symbols**: apply_landlock, apply_rlimit
- **Tests**: 6 unit + 3 E2E (Linux-gated)
- **Acceptance**: On Linux 5.13+ a Sandboxed profile is enforced by the kernel; non-Linux degrades to process-level isolation.
- **Rigor dimensions**: b, e, h

#### P2.5 - per-project capability profile resolution (deps: P2.3) [size=M]

- **Crate**: touring-hooks
- **Files**: src/capability/resolve.rs
- **Reused symbols (VGP)**: CapabilityProfile; project_db
- **New symbols**: resolve_profile_for_project
- **Tests**: 7 unit
- **Acceptance**: Each daemon project can declare its own default profile; resolution is deterministic.
- **Rigor dimensions**: b

### P3 - Gateway Core (deps: P1,P2) [~7.0d]

_The unified X0..X9 gateway, typestate-modelled so a stage cannot be skipped at compile time._

#### P3.1 - Execution<S> typestate (8 states) (deps: P2.2) [size=L]

- **Crate**: touring-hooks
- **Files**: src/gateway/typestate.rs
- **Reused symbols (VGP)**: (new module)
- **New symbols**: Execution<S>, 8 marker types
- **Tests**: 14 unit
- **Acceptance**: Compile-time proof that X3 (VGP) and X5 (sandbox) cannot be bypassed; mirrors the generator Draft->Committed typestate.
- **Rigor dimensions**: e

#### P3.2 - X0 CAPTURE + X1 CLASSIFY (deps: P3.1,P1.1) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/capture.rs; src/gateway/classify.rs
- **Reused symbols (VGP)**: command_shape; SandboxLanguage
- **New symbols**: ExecSurface, CodeBody, classify()
- **Tests**: 10 unit + 2 E2E
- **Acceptance**: Every code-bearing tool call is captured and its language + surface + code body extracted.
- **Rigor dimensions**: d, f

#### P3.3 - X2 STATIC + X3 VGP stages (deps: P3.2,P1.3) [size=L]

- **Crate**: touring-hooks
- **Files**: src/gateway/static_stage.rs; src/gateway/vgp_stage.rs
- **Reused symbols (VGP)**: validate_command; AstGrepRiskSignalLayer; index find
- **New symbols**: static_analyze(), vgp_verify()
- **Tests**: 16 unit + 3 E2E
- **Acceptance**: Code body passes structural + risk + forbidden + TDG + VGP checks; results attach to Execution<Analyzed>/<Verified>.
- **Rigor dimensions**: a, e

#### P3.4 - X4 PREDICT stage (deps: P3.3) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/predict.rs
- **Reused symbols (VGP)**: speculate/bridge.rs; action_signature.rs
- **New symbols**: ExecutionOutcomePredictor
- **Tests**: 8 unit
- **Acceptance**: RL predicts success probability; speculative validation runs before the real sandbox.
- **Rigor dimensions**: c, i

#### P3.5 - X5 SANDBOX dry-run integration (deps: P3.3,P2.4) [size=L]

- **Crate**: touring-hooks
- **Files**: src/gateway/sandbox_stage.rs; sandbox_executor.rs
- **Reused symbols (VGP)**: execute_in_sandbox; SandboxConfig
- **New symbols**: dry_run_in_sandbox; SandboxCapabilities field
- **Tests**: 12 unit + 4 E2E
- **Acceptance**: Code runs first in the sandbox under a CapabilityProfile; exit/stdout/stderr/forbidden captured before any real run.
- **Rigor dimensions**: b, c, e

#### P3.6 - X6 CAPABILITY-GATE + X7 DECISION (deps: P3.5) [size=L]

- **Crate**: touring-hooks
- **Files**: src/gateway/gate.rs; src/gateway/decision.rs
- **Reused symbols (VGP)**: CapabilityProfile
- **New symbols**: GateDecision, composite_score(), required_capabilities()
- **Tests**: 13 unit + 3 E2E
- **Acceptance**: Required capabilities are matched against the granted profile; composite score yields Allow/Warn/Deny with a canonical fix.
- **Rigor dimensions**: e, f

#### P3.7 - touring exec CLI + pre-exec hook + GatewayError (deps: P3.6) [size=L]

- **Crate**: touring-server, touring-hooks
- **Files**: src/cli/exec.rs; src/gateway/pre_exec.rs; src/gateway/error.rs
- **Reused symbols (VGP)**: HookResponse; cli dispatch table
- **New symbols**: cli_exec, pre_exec, GatewayError
- **Tests**: 11 unit + 5 E2E
- **Acceptance**: 'touring exec' and the pre-exec hook drive X0..X9 end to end; GatewayError is exhaustive; hook keeps the exit-0 invariant.
- **Rigor dimensions**: e, f, g

### P4 - Sandbox Completion & Hardening (deps: P3) [~5.5d]

_Finish the half-baked compiled-language path and make isolation real (kernel-enforced) and scalable._

#### P4.1 - compiled-language sandbox (Go, Rust) (deps: none) [size=M]

- **Crate**: touring-hooks
- **Files**: sandbox_executor.rs
- **Reused symbols (VGP)**: resolve_language_args; resolve_language_runtime
- **New symbols**: compile_and_run_go, compile_and_run_rust
- **Tests**: 8 unit + 2 E2E
- **Acceptance**: Go and Rust run via tempfile+compile; the comments admitting 'requires tempfile + compile' are resolved.
- **Rigor dimensions**: d, e

#### P4.2 - landlock enforcement in X8 SUPERVISED-EXEC (deps: P4.1) [size=L]

- **Crate**: touring-hooks
- **Files**: src/gateway/supervised.rs; sandbox_executor.rs
- **Reused symbols (VGP)**: apply_landlock; execute_in_sandbox
- **New symbols**: run_supervised()
- **Tests**: 7 unit + 4 E2E (Linux-gated)
- **Acceptance**: The real run executes under the granted profile, kernel-enforced; a Deny capability is unreachable at runtime.
- **Rigor dimensions**: b, e

#### P4.3 - rlimit / cgroup resource caps (deps: P4.2) [size=M]

- **Crate**: touring-hooks
- **Files**: src/capability/limits.rs
- **Reused symbols (VGP)**: rustix; apply_rlimit
- **New symbols**: ResourceCaps
- **Tests**: 6 unit
- **Acceptance**: CPU time, address space, file descriptors, and process count are capped per execution.
- **Rigor dimensions**: b, c

#### P4.4 - bounded subprocess pool + backpressure (deps: P4.2) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/exec_pool.rs
- **Reused symbols (VGP)**: tokio; JobRegistry pattern
- **New symbols**: ExecPool, Semaphore-bounded spawns
- **Tests**: 7 unit + 2 E2E
- **Acceptance**: Concurrent executions are bounded; the daemon never spawns more than N subprocesses; excess requests queue with timeout.
- **Rigor dimensions**: b, c

#### P4.5 - content-hash dry-run cache (deps: P4.4) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/dry_run_cache.rs
- **Reused symbols (VGP)**: blake3; moka
- **New symbols**: DryRunCache
- **Tests**: 6 unit + 1 bench
- **Acceptance**: Identical code bodies skip the dry-run via a blake3 cache; P99 of a cache hit < 5ms.
- **Rigor dimensions**: c

#### P4.6 - static-only fast path for pure code (deps: P4.5) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/fast_path.rs
- **Reused symbols (VGP)**: static_analyze; AstGrepRiskSignalLayer
- **New symbols**: is_provably_pure(), fast_path_decision()
- **Tests**: 8 unit + 1 bench
- **Acceptance**: Code with no I/O / net / subprocess skips X5; P50 of the fast path < 8ms.
- **Rigor dimensions**: c

### P5 - Managed Staging & Canonical Path (deps: P3,P4) [~3.5d]

_Replace ad-hoc /tmp with a managed, indexed staging area and make validated execution the canonical workflow._

#### P5.1 - managed staging area + GC (deps: none) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/staging.rs
- **Reused symbols (VGP)**: tee_dir; cleanup_tee
- **New symbols**: StagingArea, stage_path(), gc_staging()
- **Tests**: 8 unit
- **Acceptance**: ~/.claude/touring/staging/<session>/ holds transient scripts; GC removes entries older than the retention window.
- **Rigor dimensions**: d, g

#### P5.2 - staging registry (temporal-split resolution) (deps: P5.1) [size=M]

- **Crate**: touring-hooks, touring-server
- **Files**: src/gateway/staging_registry.rs
- **Reused symbols (VGP)**: StagingArea; index
- **New symbols**: StagingRegistry (full)
- **Tests**: 7 unit + 2 E2E
- **Acceptance**: A staged script is indexed; later execution resolves its origin and prior X2/X3 verdict (no re-analysis needed).
- **Rigor dimensions**: d, g

#### P5.3 - taco-forge perfect-run workflow (deps: P5.2,P3.7) [size=L]

- **Crate**: taco-forge
- **Files**: tools/taco-forge/workflows/perfect-run.sh
- **Reused symbols (VGP)**: perfect-create-script; touring exec
- **New symbols**: perfect-run workflow
- **Tests**: 1 E2E (dry-run)
- **Acceptance**: perfect-run does create -> validate -> sandbox -> run as one canonical command.
- **Rigor dimensions**: d, i

#### P5.4 - /taco-forge-run slash command (deps: P5.3) [size=S]

- **Crate**: (commands)
- **Files**: ~/.claude/commands/taco-forge-run.md
- **Reused symbols (VGP)**: perfect-run workflow
- **New symbols**: (slash wrapper)
- **Tests**: manual smoke
- **Acceptance**: '/taco-forge-run' invokes perfect-run with a clean argument surface.
- **Rigor dimensions**: d

### P6 - Systemic Integration (deps: P5) [~3.5d]

_Wire the CEG into the rest of the nervous system so it amplifies the whole project - dimension (i)._

#### P6.1 - CEG <-> RL reward loop (deps: none) [size=M]

- **Crate**: touring-hooks, touring-learning
- **Files**: src/gateway/learn.rs
- **Reused symbols (VGP)**: learning reward; QTable; LinUCB
- **New symbols**: emit_gate_reward()
- **Tests**: 6 unit
- **Acceptance**: Every X7 decision emits an RL reward; the gate tunes its own thresholds over time.
- **Rigor dimensions**: i

#### P6.2 - CEG <-> memory / gotcha DB (deps: P6.1) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/learn.rs
- **Reused symbols (VGP)**: memory store; gotcha match
- **New symbols**: persist_forbidden_as_gotcha()
- **Tests**: 6 unit
- **Acceptance**: A blocked execution persists its forbidden pattern as a gotcha, so the next session is pre-warned.
- **Rigor dimensions**: i

#### P6.3 - CEG <-> generator + transcript miner reuse (deps: P6.2) [size=M]

- **Crate**: touring-hooks, touring-server, touring-generator
- **Files**: src/gateway/sandbox_stage.rs; ingest/transcript_miner.rs
- **Reused symbols (VGP)**: speculate/bridge.rs; transcript_miner
- **New symbols**: shared dry-run bridge
- **Tests**: 5 unit + 2 E2E
- **Acceptance**: The generator Speculated stage reuses the CEG dry-run; failed executions are mined into lessons.
- **Rigor dimensions**: i

#### P6.4 - CEG <-> synergy WIRED_PAIR + cli_suggester (deps: P6.3) [size=M]

- **Crate**: touring-hooks
- **Files**: src/synergy/*; cli_suggester.rs
- **Reused symbols (VGP)**: WIRED_PAIRS; WIRED_PAIR_METRICS; cli_suggester
- **New symbols**: CEG WIRED_PAIR entry
- **Tests**: 5 unit
- **Acceptance**: The CEG registers as a synergy WIRED_PAIR; cli_suggest injects execution-gate enrichment for Bash.
- **Rigor dimensions**: g, i

### P7 - Observability, Docs & E2E Proof (deps: P6) [~3.5d]

_Make the gateway observable, documented, and proven end to end across every runtime and surface._

#### P7.1 - ceg_* gate-metrics counters (deps: none) [size=M]

- **Crate**: touring-hooks
- **Files**: src/gateway/metrics.rs
- **Reused symbols (VGP)**: gate-metrics; gate_metrics counters
- **New symbols**: ceg_captured/blocked/sandboxed/fast_path counters
- **Tests**: 6 unit
- **Acceptance**: 'touring gate-metrics -j' exposes CEG activity; counters are non-zero after a real execution.
- **Rigor dimensions**: c, g

#### P7.2 - rule code-execution-gateway.md (auto-load) (deps: P7.1) [size=S]

- **Crate**: (rules)
- **Files**: ~/.claude/rules/code-execution-gateway.md
- **Reused symbols (VGP)**: (doc)
- **New symbols**: (doc)
- **Tests**: wc -l < 400 (REGRA #16)
- **Acceptance**: An auto-load rule documents the gateway, the X0..X9 pipeline and the capability profiles.
- **Rigor dimensions**: f

#### P7.3 - Reflexo #9 Sandbox-First in CLAUDE.md (deps: P7.2) [size=S]

- **Crate**: (constitution)
- **Files**: ~/.claude/CLAUDE.md
- **Reused symbols (VGP)**: (doc)
- **New symbols**: (doc)
- **Tests**: CLAUDE.md hard-limit respected
- **Acceptance**: A ninth TACO reflex makes sandbox-validated execution the documented default behaviour.
- **Rigor dimensions**: f, i

#### P7.4 - E2E suite: 11 runtimes x 6 surfaces (deps: P7.3) [size=L]

- **Crate**: touring-hooks, touring-server
- **Files**: tests/ceg_e2e.rs
- **Reused symbols (VGP)**: execute_in_sandbox; touring exec
- **New symbols**: ceg_e2e test module
- **Tests**: 66+ E2E cases
- **Acceptance**: Every (language, surface) cell is proven: the gate blocks forbidden code and allows clean code, exit-0 invariant holds.
- **Rigor dimensions**: d, e

#### P7.5 - criterion regression gate + X9 closure (deps: P7.4) [size=M]

- **Crate**: touring-hooks
- **Files**: benches/ceg_baseline.rs; src/gateway/learn.rs
- **Reused symbols (VGP)**: criterion; gate-metrics
- **New symbols**: regression assertions
- **Tests**: 3 bench + 4 E2E
- **Acceptance**: P50/P99 stay within the P0.4 baseline budget; the X9 LEARN loop is proven to close (reward + memory + miner).
- **Rigor dimensions**: c, i


## 5. Risk Register

| ID | Risk | Prob | Impact | Mitigation |
|----|------|------|--------|------------|
| R1 | Gateway blocks legitimate work (fail-open invariant breach). | MEDIUM | HIGH | Phased WARN->ENFORCE rollout; hook never exits non-zero; TACO_CEG_DISABLED + TACO_CEG_WARN_ONLY opt-outs. |
| R2 | Dry-run doubles per-command latency. | HIGH | MEDIUM | Static-only fast path (P4.6); blake3 dry-run cache (P4.5); speculative parallelism; skip-whitelist for trusted commands. |
| R3 | False positives from naive forbidden detection. | MEDIUM | MEDIUM | AST-based detection (P1.3); WARN_ONLY default initially; RL threshold tuning (P6.1). |
| R4 | landlock unavailable on old kernels / non-Linux. | MEDIUM | MEDIUM | Graceful degradation to process-level isolation; capability decision still enforced in user space; Linux-gated E2E. |
| R5 | Concurrent executions exhaust daemon resources. | MEDIUM | HIGH | Bounded ExecPool with semaphore (P4.4); rlimit/cgroup caps (P4.3); per-project quotas. |
| R6 | ast-grep 0.42 upgrade breaks the polyglot grammar ABI. | MEDIUM | MEDIUM | P0.5 audits the ABI; staged upgrade with regression tests; tree-sitter-go ABI fix bundled. |
| R7 | Compiled-language sandbox compile cost is high. | LOW | MEDIUM | Per-artifact compile cache keyed by blake3; timeout-bounded; Go/Rust dry-run is opt-in. |
| R8 | Typestate refactor has a large blast radius. | LOW | HIGH | P0.1 measures blast first; new module, additive; the gateway is wired behind a feature flag until P7. |
| R9 | Heredoc temporal-split evades the gate. | MEDIUM | MEDIUM | Staging registry (P5.2) records origin + verdict; execution of an unregistered staged file forces full re-analysis. |
| R10 | Capability model is too coarse or too strict in practice. | MEDIUM | MEDIUM | Four tunable profiles + per-project resolution (P2.5); Prompt decision escalates to the user instead of hard-deny. |
| R11 | Credential leakage through sandbox stdout. | LOW | HIGH | Reuse redact_secrets on all captured output; env_clear + whitelist; net denied by default. |
| R12 | Scope creep - the CEG absorbs unrelated hook work. | MEDIUM | MEDIUM | Phase DAG is frozen; each deliverable has a measurable acceptance criterion; out-of-scope items go to a backlog. |

## 6. Synergy Map (dimension i - potentialization)

| Subsystem | Mechanism | Counter |
|-----------|-----------|---------|
| RL (touring-learning) | Each X7 gate decision emits a reward; the gate self-tunes its thresholds. | `ceg_reward_emitted_count` |
| Memory / gotcha DB | Blocked forbidden patterns persist as gotchas - the next session is pre-warned. | `ceg_gotcha_persisted_count` |
| Generator (touring-generator) | The generator Speculated stage reuses the CEG sandbox dry-run bridge. | `ceg_speculate_reuse_count` |
| Transcript miner | Failed executions are mined into error->resolution lessons. | `ceg_transcript_lesson_count` |
| Synergy WIRED_PAIRS | The CEG registers as a wired pair with live counter enrichment. | `WIRED_PAIR_METRICS[ceg]` |
| cli_suggester | cli-suggest injects execution-gate enrichment for Bash (MUST/SHOULD lines). | `ceg_enrichment_injected_count` |

## 7. Dependency Audit (dimension h - modernization)

| Dependency | Current | Target | Source | Rationale |
|------------|---------|--------|--------|-----------|
| ast-grep-core | 0.36 | 0.42.x | context7 / crates.io | Modern grammar ABI, bug fixes; required by P1.3 AST forbidden detection. |
| landlock | (absent) | latest stable | context7 / crates.io | Kernel-enforced filesystem capability sandbox (Linux 5.13+). |
| rustix | (workspace) | latest stable | crates.io | Safe rlimit / resource-cap syscalls without libc unsafe. |
| tree-sitter-go | ABI v15 (broken) | ABI-compatible release | memory: B-FUZZ-002 | Fix the broken Go grammar so the Go sandbox + forbidden scan work. |
| blake3 | (workspace) | current | crates.io | Already present; reused for the dry-run content cache. |
| moka | (workspace) | current | crates.io | Already present; reused for the dry-run + profile caches. |

## 8. Plan Self-Validation

- DAG validation: **PASS**
- T-shirt distribution: {'S': 7, 'M': 26, 'L': 9, 'XL': 0}
- Estimated effort: ~34.5 engineer-days
- Estimated test cases: 765

---
_Pln2 rendered from ceg_pln2_data.json. Re-run the generator to regenerate._
