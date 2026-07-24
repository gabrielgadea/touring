# How to debug the Code Execution Gateway (CEG)

> A **how-to** (Diátaxis): task-oriented. A command was blocked, sandboxed, or
> behaved unexpectedly and you need to see why. Master Plan D.W4.P3. For the
> concepts see `docs/explanation/architecture.md` (L3).

## Goal

Observe what the CEG (X0..X9 pipeline) did to a code-bearing action and resolve
a `Deny` verdict without disabling the gate.

## See the gate activity

```bash
touring gate-metrics -j | python3 -c \
  'import sys,json;d=json.load(sys.stdin);print({k:v for k,v in d.items() if k.startswith("ceg_")})'
```

Key counters:

| Counter | Meaning |
|---|---|
| `ceg_captured_count` | X0 fired (an action was intercepted) |
| `ceg_fast_path_count` | provably-pure code skipped the sandbox (X5–X8) |
| `ceg_sandboxed_count` | X5 dry-run actually executed |
| `ceg_blocked_count` | X7 returned `Deny` |

A high `fast_path` / low `sandboxed` ratio is normal: read-only, side-effect-free
code takes the pure shortcut.

## When an action was blocked (X7 → Deny)

The pipeline is **fail-open by design** — it warns and records, it does not abort
your session. A `Deny` therefore shows up as a `GateDecision` with a `reason` and
a `canonical_fix` hint, not a crash. Read them:

1. **Re-read the result.** The block prints the failed capability and the
   suggested canonical form. Most denials are a missing capability grant, not a
   bug.
2. **Identify the capability.** Filesystem write outside the staging dir,
   outbound network, or a forbidden subprocess (`rm`, `sudo`) are the common
   ones. The profile denies by default; deny always beats allow.
3. **Use the canonical form.** If a script needs to produce an artifact, route
   it so it writes to the staging dir, or invoke it through the canonical tool
   (e.g. `taco-forge perfect-create-script` for new scripts) which carries the
   right profile.

## Trace a specific run

The staging area (heredoc write-now/run-later) lives under
`~/.claude/touring/ceg-staging/`. Stale entries are GC'd by retention; inspect
recent ones to see exactly what body was staged:

```bash
ls -lt ~/.claude/touring/ceg-staging/ | head
```

## Linux kernel enforcement

On Linux 6.7+ the sandbox is kernel-backed (landlock + rlimit). On older kernels
or non-Linux it degrades **loud** (the enforcement level downgrades and is
reported), never silently. If you expected isolation and did not get it, check
the kernel version — that is the honest limitation, not a config error.

## Do NOT disable the gate to "fix" a block

A persistent `Deny` means the action genuinely needs a capability it was not
granted. The fix is to grant it narrowly (per-project profile in
`capability/resolve.rs`) or to use the canonical workflow — not to bypass the
pipeline. The gate is the safety property; removing it removes the property.

## Verify

After adjusting, re-run the action and confirm `ceg_blocked_count` did not
increment and the work completed. `touring gate-metrics -j` is the ground truth.
