# touring-hooks

> Lifecycle hooks, the Code Execution Gateway, and the CLI handler surface of
> Touring. Master Plan D.W3.P2.

## Purpose

`touring-hooks` is the interactive-path crate: it implements the handlers that
the harness invokes around tool calls and session events, and it owns the
**Code Execution Gateway (CEG)** that sandboxes code-bearing actions. Everything
here obeys the **fail-open** contract — handlers exit 0 and degrade to no-ops on
error, never interrupting a session.

## Architecture (major modules under `src/`)

| Area | What |
|---|---|
| `cli_suggester.rs` | PreToolUse enrichment (`MUST`/`SHOULD` + blast radius, gotchas) |
| `gateway/` | CEG X0..X9 typestate pipeline (capture → classify → VGP → sandbox → gate → learn) |
| `capability/` | Deno-style deny-by-default capability profiles + Linux landlock/rlimit |
| `cli/` | Decomposed `cli_*` handlers (23 cohesive modules — Master Plan A.W2) |
| `cli_handlers*.rs` | Remaining core dispatch + shared helpers (A.W2.P4 finishes the split) |
| `lifecycle*.rs` | Session lifecycle (SessionStart/Stop, PreCompact, Task*) |
| `session_hooks.rs` | Session-start context injection + HarnessContract attestation |
| `hook_registry.rs` | The `touring-hook <event>` subcommand registry |
| `action_signature.rs` | Action→outcome learning keys (RL substrate) |

## Key entry points

- `run_gateway(deps)` — the CEG pipeline entry (`gateway/pre_exec.rs`).
- The `touring-hook <event>` binary — dispatches lifecycle handlers registered in
  `hook_registry.rs`.

## Example

```bash
# Drive the read-path enrichment hook directly
echo '{"tool_name":"Edit","tool_input":{"file_path":"src/lib.rs"}}' \
  | touring-hook cli-suggest

# CEG gate activity
touring gate-metrics -j   # ceg_captured_count, ceg_sandboxed_count, ceg_blocked_count
```

## Caveats

- **Largest crate in the workspace** (~167k LOC incl. inline tests). The
  `cli_handlers` family is being decomposed into `src/cli/` (A.W2; file-size gate
  tracks the residual). `lifecycle.rs` migration is A.W3.P3.
- **Fail-open is load-bearing.** Never add a hot-path `panic!`/`.unwrap()` that
  could abort a session; use `?` / `.unwrap_or_default()` / justified
  `.expect()`. The `clippy::unwrap_used` campaign (C.W3.P2) hardens this.
- Some handlers are **feature-gated** (`acp-protocol`, `mpatch-fuzzy`,
  `semantic-embeddings`, `tantivy-fts`); validate with `cargo check --all-features`.
