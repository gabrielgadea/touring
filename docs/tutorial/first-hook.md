# Your First Lifecycle Hook

> A **tutorial** (Diátaxis): learning-oriented, honest about the contract.
> Master Plan D.W4.P2. For the full hook catalog see `docs/reference/hooks.md`;
> for the *why* see `docs/explanation/architecture.md`.

Touring participates in an agent session through **lifecycle hooks** — small,
fast handlers that the harness invokes around tool calls and session events.
A hook reads a JSON event on stdin, optionally prints a JSON result on stdout,
and **always exits 0** (fail-open). Its job is to *enrich* the session
(inject context, record an outcome, learn), never to block it.

By the end of this tutorial you will understand the contract, trace a real
hook end-to-end, and register a handler of your own.

## The contract (read this first)

Every Touring hook obeys four rules:

1. **stdin is a JSON event.** The harness pipes the tool-call (or session)
   payload as a single JSON object.
2. **stdout is an optional JSON result.** To inject context, print
   `{"hookSpecificOutput": {"additionalContext": "..."}}`. Print nothing to
   stay silent.
3. **Exit 0, always.** A non-zero exit (or a crash) would interrupt the user's
   session. Hooks are *advisory*: on any internal error they degrade to a
   no-op. This is the single most important invariant — verify it for anything
   you write.
4. **Be fast (≤ a few ms typical, ≤ 5s hard).** Hooks run on the interactive
   path. Heavy work belongs in the daemon, which the hook queries over a Unix
   socket, not inline.

## 1. See the hooks that already run

Touring hooks are registered in `~/.claude/settings.json` under `PreToolUse`,
`PostToolUse`, and the session lifecycle events. Each entry maps a *matcher*
(which tool/event) to a *command*:

```json
{
  "matcher": "Bash|Edit|Write|Read|Grep|Glob",
  "hooks": [
    { "type": "command", "command": "$HOME/.claude/hooks/touring-hook cli-suggest" }
  ]
}
```

That one line is why, before an edit, you see the `[TOURING SUGGEST]` block with
`MUST`/`SHOULD` commands and live enrichment (blast radius, gotchas). The
handler is `touring-hook cli-suggest`.

List the events a build supports:

```bash
touring-hook --help              # all lifecycle subcommands
```

## 2. Trace one hook end-to-end

The `cli-suggest` hook (PreToolUse) is the canonical read-path example. Its flow:

```
harness ──(JSON tool-call on stdin)──▶ touring-hook cli-suggest
                                          │
                                          ├─ classify the tool + target (C01..C12)
                                          ├─ query the daemon over the socket
                                          │   (symbol index, blast radius, gotchas)
                                          └─ print {"hookSpecificOutput":
                                               {"additionalContext": "<MUST/SHOULD ...>"}}
harness ◀──(stdout JSON, exit 0)──────────┘   the model sees additionalContext
```

The handler lives in `crates/touring-hooks/src/cli_suggester.rs`. Note what it
*doesn't* do: it never returns a non-zero exit, and if the daemon socket is
down it emits a smaller (or empty) result instead of failing. That is the
fail-open contract in practice.

Inspect the live result yourself (the daemon answers in <10ms):

```bash
echo '{"tool_name":"Edit","tool_input":{"file_path":"crates/touring-hooks/src/lib.rs"}}' \
  | touring-hook cli-suggest
```

You should get a JSON object whose `additionalContext` names the file's blast
radius and the MUST/SHOULD commands for an edit. Empty output is also valid —
that is the hook staying silent, not failing.

## 3. Write your own (minimal)

A hook is just a program that follows the contract, so you can prototype one in
any language before promoting it into the `touring-hooks` crate. Here is a
fail-open PreToolUse hook in shell that logs the tool name and stays silent:

```bash
#!/usr/bin/env bash
# my-first-hook.sh — fail-open PreToolUse logger
set -uo pipefail               # NOTE: no `-e` — a hook must not abort the session
payload="$(cat)"               # the JSON event arrives on stdin
tool="$(printf '%s' "$payload" | python3 -c \
  'import sys,json; print(json.load(sys.stdin).get("tool_name","?"))' 2>/dev/null || echo '?')"
printf '%s %s\n' "$(date -Is)" "$tool" >> "$HOME/.claude/touring/my-hook.log"
exit 0                         # ALWAYS 0
```

Register it in `~/.claude/settings.json`:

```json
{
  "matcher": "Bash|Edit|Write",
  "hooks": [
    { "type": "command", "command": "$HOME/.claude/hooks/my-first-hook.sh" }
  ]
}
```

Now every Bash/Edit/Write appends a line to `my-hook.log`, and the session is
never interrupted because the script returns 0 even when `jq`/`python3` is
missing.

> **To inject context** instead of just logging, print on stdout before exiting:
> ```bash
> printf '{"hookSpecificOutput":{"additionalContext":"seen tool: %s"}}\n' "$tool"
> ```

## 4. Promote it into Touring (the real path)

Shell is fine for a spike; production hooks are Rust handlers in the
`touring-hooks` crate so they share the daemon connection, the capability
gateway, and the RL reward loop. The shape:

1. Add a handler function in `crates/touring-hooks/src/` (e.g. a new module or
   an arm in an existing dispatcher).
2. Register the subcommand name in the hook registry
   (`crates/touring-hooks/src/hook_registry.rs`) so `touring-hook <name>`
   resolves.
3. Keep the fail-open contract: return `String` (possibly empty), never panic
   on the hot path — use `?`, `.unwrap_or_default()`, or an explicit
   `.expect("reason")` only where truly impossible.
4. Rebuild + reinstall symlinks + restart the daemon in one shot:
   ```bash
   update-touring          # build release + dual-target symlinks + daemon restart + verify
   ```
   (Editing `~/.claude/hooks/touring-hook` directly does nothing useful — it is
   a symlink into `target/release/`.)

## Verify (the only test that matters)

A hook that can crash the session is worse than no hook. Confirm the invariant:

```bash
# 1. Malformed input must still exit 0 and not hang
echo 'not json' | touring-hook cli-suggest; echo "exit=$?"     # expect exit=0

# 2. Daemon down must degrade, not fail
touring daemon-ctl status        # observe state; the hook still returns
```

If either case produces a non-zero exit or a stack trace on stdout, the hook
violates the contract and must be fixed before it ships.

## Where to go next

- **Reference** (`docs/reference/hooks.md`) — the full event catalog (PreToolUse,
  PostToolUse, SessionStart/Stop, PreCompact, Task*, decompose*, RL*).
- **How-to** (`docs/how-to/debug-ceg.md`) — when a hook routes code execution
  through the Code Execution Gateway (X0–X9).
- **Explanation** (`docs/explanation/architecture.md`) — why hooks are
  fail-open and how the daemon/hook/CLI processes relate.

## Known gaps (honesty first)

Writing a *Rust* hook today requires rebuilding the workspace (~6 min) and
editing the hook registry by hand; there is no dynamic hook-plugin ABI yet
(the public extension contract is RFC-006, planned — Master Plan B.W3). Until
then, shell/Python prototypes are the fast path and Rust handlers are the
production path.
