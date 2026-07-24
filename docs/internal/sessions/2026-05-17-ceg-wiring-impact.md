# CEG Pln2 — P0.2 Wiring Impact per Reused Symbol

> Generated: 2026-05-17 | Plan: `2026-05-17-ceg-pln2-plan.md` P0.2 | Read-only forensic measurement.
> Daemon health: `touring doctor -j` → all `ok`.

This document resolves every symbol the CEG REUSES and measures how dangerous it is to edit each one
by transitive consumer count. Every row is sourced from real CLI output. Resolution:
`touring index find <sym> -j` + `touring ast find <sym> -j` (file:line); consumer counts from
`touring wiring impact <sym> --depth 2` (`Direct consumers` / `Max depth`). Signatures were read
directly from source (`ast find` does not emit signature text in this build).

`touring wiring chains` / `wiring chains --rebuild` both returned `{"chain_count":0,"rebuilt":true}` —
the functional-chain graph is not populated for this workspace, so no chain extraction is possible.
Blast-radius coupling (P0.1) and per-symbol `wiring impact` are the available coupling metrics.

## Resolved reused symbols

| Symbol | file:line | Signature | direct_consumers | max_depth | verdict |
|---|---|---|---|---|---|
| `command_shape` | crates/touring-hooks/src/shared/bash_ast_validator.rs:294 | `pub fn command_shape(command: &str) -> Option<String>` | 3 | 1 | found |
| `validate_command` | crates/touring-hooks/src/shared/bash_ast_validator.rs:144 | `pub fn validate_command(command: &str) -> Verdict` | 3 | 1 | found |
| `detect_forbidden_calls` | crates/touring-server/src/tools/ctx_execute_tools.rs:71 | `fn detect_forbidden_calls(lang: SandboxLanguage, code: &str) -> Vec<String>` (private) | 0 | 0 | found |
| `AstGrepRiskSignalLayer` | crates/touring-hooks/src/shared/ast_grep_signal.rs:225 | `pub struct AstGrepRiskSignalLayer { project_root: Option<PathBuf> }` | 1 | 1 | found |
| `execute_in_sandbox` | crates/touring-hooks/src/sandbox_executor.rs:247 | `pub async fn execute_in_sandbox(tool_name: &str, original_args: Value, config: SandboxConfig) -> ...` | 0 | 0 | found |
| `SandboxLanguage` | crates/touring-hooks/src/sandbox_executor.rs:99 | `pub enum SandboxLanguage { JavaScript, TypeScript, ... }` | 0 | 0 | found |
| `SandboxConfig` | crates/touring-hooks/src/sandbox_executor.rs:28 | `pub struct SandboxConfig { timeout_ms: u64, max_output_bytes: u64, fallback_on_timeout: bool }` | 1 | 1 | found |
| `HookResponse` | crates/touring-hooks/src/hook_response.rs:87 (impl); type also in hook_runtime.rs:70 | `impl HookResponse { ... }` — public response type | **26** | 1 | found |
| `ctx_execute_impl` | crates/touring-server/src/tools/ctx_execute_tools.rs:137 | `pub async fn ctx_execute_impl(language: String, code: String, args: Option<Value>) -> ...` | 2 | 1 | found |
| `ActionSignature` | crates/touring-hooks/src/action_signature.rs:128 | `pub struct ActionSignature { tool_class: String, intent: ..., ... }` | 0 (index-stale; grep=3) | 0 | found |
| `ContextQualifier` | crates/touring-hooks/src/action_signature.rs:41 | `pub enum ContextQualifier { HiBlast, HiComplexity, ... }` | 0 (index-stale; grep=1) | 0 | found |
| `TranscriptMiner` | crates/touring-server/src/ingest/transcript_miner.rs:620 | `pub struct TranscriptMiner { state: MinerState, state_path: PathBuf }` | 0 (index-stale; grep=2) | 0 | found |
| `WIRED_PAIRS` | crates/touring-server/src/cli/synergy.rs:23 | `const WIRED_PAIRS: &[(&str, &str, &str, &str)]` (crate-private const) | 0 | 0 | found |
| `WIRED_PAIR_METRICS` | crates/touring-server/src/cli/synergy.rs:86 | `const WIRED_PAIR_METRICS: &[(&str, &str, &str)]` (crate-private const) | 0 | 0 | found |
| `classify` (cli_suggester) | crates/touring-hooks/src/cli_suggester.rs:274 | `fn classify(tool_name: &str, tool_input: &Value) -> Option<ClassifierOutput>` (private dispatcher) | n/a (private) | n/a | found |
| `tee_dir` | crates/touring-hooks/src/sandbox_executor.rs:582 | `pub fn tee_dir() -> PathBuf` | 1 | 1 | found |
| `cleanup_tee` | crates/touring-hooks/src/sandbox_executor.rs:629 | `pub fn cleanup_tee(retention_secs: u64) -> std::io::Result<u64>` | 2 | 1 | found |
| `classify_tool_class` | crates/touring-hooks/src/action_signature.rs:244 | `pub fn classify_tool_class(tool_name: &str) -> String` | 0 (index-stale; grep=1 self-crate) | 0 | found |

### Resolution corrections vs. the P0.2 input list

- **`classify`** — the index reported a `classify` def at `cli_suggester.rs:260`, but line 260 is a
  regex helper (`extract_command_short`). The real CEG-relevant dispatcher is `fn classify` at
  **cli_suggester.rs:274** (`fn classify(tool_name, tool_input) -> Option<ClassifierOutput>`), an 8-arm
  `match` over Bash/Task/WebFetch/Grep/Glob/Read/Edit/Write. It is private to the crate; the CEG
  should add a CEG arm here rather than call it externally. The plan should cite line 274, not 260.
- **`detect_forbidden_calls`** is a **private** `fn` (not `pub`) inside `ctx_execute_tools.rs:71`. The
  CEG cannot import it across crate/module boundaries as-is — it must either be promoted to `pub`
  or its logic re-used via `ctx_execute_impl` (which already calls it internally).
- **`WIRED_PAIRS` / `WIRED_PAIR_METRICS`** are module-private `const` slices in `synergy.rs`. They
  cannot be imported; the CEG must add its own entries inside `synergy.rs` (same pattern the existing
  wave entries use) to register a CEG synergy pair.

### Index-staleness flag (Cadeia 7)

`ActionSignature`, `ContextQualifier`, `TranscriptMiner`, `classify_tool_class` returned
`count:0` from `touring index find` AND `0` direct_consumers from `wiring impact`. This is **index
staleness, not absence** — these are newer files (PreToolUse action-outcome learning + transcript
miner waves). Verified present via `grep`:

- `ActionSignature` — `pub struct` at `action_signature.rs:128`; consumers: `lib.rs`,
  `post_tool_rl.rs`, `cli_suggester.rs` (grep ≈ 3).
- `ContextQualifier` — `pub enum` at `action_signature.rs:41`; consumer: `cli_suggester.rs` (grep ≈ 1).
- `TranscriptMiner` — `pub struct` at `transcript_miner.rs:620`; consumers: `server/mod.rs`,
  `ingest/mod.rs` (grep ≈ 2).
- `classify_tool_class` — `pub fn` at `action_signature.rs:244`; grep finds usage only inside
  `transcript_miner.rs` (writer↔reader contract per its doc-comment). `ast meta` confirms it is an
  exported pub symbol of `action_signature.rs`.

All four are confirmed `found`. The plan's symbol names are correct; only the index is behind.

## Findings — which symbols are dangerous to edit vs. safe to extend

### High-impact — DANGEROUS to edit (signature changes ripple widely)

1. **`HookResponse` — 26 direct consumers (highest).** The shared hook response type. Any change to
   its public shape, fields, or `impl` methods ripples to 26 call sites across the hook surface. The
   CEG must CONSUME `HookResponse` (build instances, call existing methods) and must NOT alter its
   definition or method signatures. Treat as a frozen contract.

2. **`command_shape` — 3 consumers / `validate_command` — 3 consumers.** Moderate but real. Both are
   `pub fn` in `bash_ast_validator.rs` and are on the live bash-gating path. They are pure functions
   (`&str -> Option<String>` / `&str -> Verdict`); the CEG should reuse them as-is. Changing their
   signature would touch 3 sites each — avoid; extend behavior with new helpers instead.

3. **`ctx_execute_impl` — 2 consumers / `cleanup_tee` — 2 consumers.** Low-moderate. `ctx_execute_impl`
   is the core code-execution entry point the CEG wraps — reuse it, do not re-implement. `cleanup_tee`
   is shared GC; safe to call, risky to change signature.

> Cross-reference P0.1: `HookResponse`'s definition file (`hook_response.rs` / `hook_runtime.rs`) and
> the high-blast `pre_tool_validator.rs` (37) and `gate_metrics.rs` (55) form the editing danger zone.
> The CEG should treat all three as extend-only.

### Safe to extend / low-impact (1 or 0 consumers)

- `AstGrepRiskSignalLayer` (1), `SandboxConfig` (1), `tee_dir` (1) — single consumer; the CEG can
  reuse and even add fields/variants with low ripple, provided existing public surface is preserved.
- `execute_in_sandbox` (0), `SandboxLanguage` (0) — `wiring impact` shows 0 transitive consumers
  (the sandbox executor's own callers route through `cli_handlers_mcp.rs` / `tool_output_router.rs`
  per P0.1 blast). The CEG is effectively the new primary consumer of `execute_in_sandbox` — safe to
  build on; the function is purpose-built for sandboxed execution, exactly the CEG's need.
- `ActionSignature`, `ContextQualifier`, `TranscriptMiner`, `classify_tool_class` — low real coupling
  (grep ≤ 3, all within touring-hooks/touring-server). Safe to extend; add CEG-specific variants or
  fields additively.
- `detect_forbidden_calls` — private, 0 cross-module consumers. Safe to promote to `pub` if the CEG
  needs it directly (a `pub(crate)` or `pub` promotion is the lowest-risk wiring change in this set).
- `WIRED_PAIRS` / `WIRED_PAIR_METRICS` — module-private; the CEG adds new tuples, zero risk to
  existing entries.

### Top 3 most dangerous reused symbols to edit

1. `HookResponse` — 26 consumers.
2. `command_shape` — 3 consumers.
3. `validate_command` — 3 consumers.

(Tied at 3 with `command_shape` / `validate_command`; `ctx_execute_impl` and `cleanup_tee` follow at 2.)

**Bottom line for the CEG implementation:** every reused symbol resolves (`found`). No symbol from the
P0.2 list is missing — the plan needs only two precision corrections: cite `classify` at
`cli_suggester.rs:274` (not 260), and account for `detect_forbidden_calls`, `WIRED_PAIRS`,
`WIRED_PAIR_METRICS` being non-`pub` (require promotion or in-place extension). All editing should be
ADDITIVE: consume `HookResponse`/`execute_in_sandbox`/`ctx_execute_impl`, add new arms/counters/tuples,
never alter the public surface of the 26-consumer `HookResponse` or the high-blast files from P0.1.
