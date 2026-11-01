# touring-compact-evict

A Claude Code mod (hooks module) that, at the compaction boundary only, replaces tool results the
conversation has already superseded with a short stub that points at the raw output on disk. It calls no
model and sends nothing off the machine. It does nothing unless `TOURING_COMPACT_EVICT=on`.

Tested against Claude Code **2.1.311** (the mod API is early access and moves between releases; the
`claude-code.d.ts` of the running build is the contract).

## What it does

On `session.compact` (triggers `manual`, `auto`, `plugin`; never `precompute`, the ahead-of-time trigger,
which would break the cached prefix mid-session), the mod reads the transcript being compacted and stubs a
tool result when it is one of:

| Rule | When |
|---|---|
| superseded Read | the file was edited or written later (`Edit`, `MultiEdit`, `Write`, `NotebookEdit`), or read again later with the same `offset`/`limit`/`pages` |
| repeated command | the same `Bash` command ran again later and succeeded |
| already persisted | Claude Code saved the full output itself (`<persisted-output> ... Full output saved to: <path>`) and the file still exists |

The stub names what it replaced and where the raw is:

```
[touring-compact-evict] Read result of /repo/a.ts evicted at compaction (7.0 KiB, 120 lines; edited later). Raw output: /home/<you>/.claude/touring/evicted/<sha256-32>.txt - read it only if the old output is needed.
```

The raw is written (directory `0700`, files `0600`) before the result is replaced; if it cannot be written,
the result stays as it was. For the persisted rule the pointer is Claude Code's own file and nothing is copied.

## What it never evicts

- failures: an error result, a tool call that errored (hook denials included), and a Bash result whose text
  shows a failure even with exit 0 (red tests, tracebacks, `command not found`, `denied` ...);
- the first message and the last `TOURING_COMPACT_EVICT_KEEP_LAST` messages (default 6);
- any user or assistant text (only `toolResults` entries are read or rewritten, assistant messages are never rebuilt);
- results below `TOURING_COMPACT_EVICT_MIN_BYTES` (default 2048, floor 512);
- a message that also holds an image/pdf result, a persisted test-runner output (its verdict is in the
  tail, which the preview does not show), a re-read of a different range.

## Configuration (environment of the Claude Code process)

| Variable | Default | |
|---|---|---|
| `TOURING_COMPACT_EVICT` | unset (off) | `on`, `1`, `true` or `yes` arms it; anything else is a pass-through |
| `TOURING_COMPACT_EVICT_KEEP_LAST` | `6` | messages at the tail that are never touched |
| `TOURING_COMPACT_EVICT_MIN_BYTES` | `2048` | smallest result worth a stub |

A hook that throws is skipped and the compaction goes on unchanged (`.catch` re-runs the chain).

## Try it (nothing is installed by this repo)

```bash
TOURING_COMPACT_EVICT=on claude --plugin-dir tools/mods/touring-compact-evict
# then /compact, or let the auto-compaction fire; /plugin lists the mod
```

## Verify

```bash
claude plugin validate tools/mods/touring-compact-evict      # hooks, calls ($.env/$.fs/$.process.run), env reads
claude plugin test tools/mods/touring-compact-evict          # 30 tests, synthetic fixtures only
```

## Known limits (read before arming it for real)

- The messages handed to `next` are a rebuilt copy: a rewritten message loses its engine `handle`, and the
  d.ts only says it is "read as built from its role, text and tool blocks". Whether a session resumed
  afterwards (`--resume`) keeps the stubs is not established: the 08/10/2026 probe on CC 2.1.294 found a
  mod that answered with its own `{ messages }` was undone by `--resume`; this mod calls `next`, which the probe
  did not exercise. Verify with a real compaction + `--resume` before enabling it by default.
- The raw files are never cleaned up (`~/.claude/touring/evicted/`).
- Failure and test-runner detection are text heuristics; a miss costs a kept result, never a lost one.
