# gabriel.skills-deck — Omarchy bar-widget plugin

Quick-launch Claude Code skills from the Omarchy bar.

## What it does

- Adds a robot icon (`󱁤`) to the right section of the bar.
- **Left-click**: opens/closes a tile panel showing all skills from `deck.json`.
- **Right-click**: tails `~/Work/runs.log` in a terminal.
- **Tile left-click**: runs `omarchy-skill-run` with the tile's `skill / model / effort / mode / cwd`.
- Tiles with `"herdr": true` pass `--herdr` (delegates to Herdr, no-wait).

## Install — local copy

```bash
# 1. Copy the plugin into the Omarchy plugins directory
cp -r client/omarchy/skills-deck ~/.config/omarchy/plugins/gabriel.skills-deck

# 2. Validate the manifest (requires jq)
omarchy plugin validate ~/.config/omarchy/plugins/gabriel.skills-deck

# 3. Enable and place in the right bar section
omarchy plugin enable gabriel.skills-deck --section right

# 4. (optional) Copy the runner to PATH if not already installed
cp client/omarchy/bin/omarchy-skill-run ~/.local/bin/
chmod +x ~/.local/bin/omarchy-skill-run
```

## Install — via git (once repo is published)

```bash
omarchy plugin add https://github.com/gabrielgadea/omarchy-skills-deck.git --enable --yes
```

## Customise — add a button

Edit `~/.config/omarchy/plugins/gabriel.skills-deck/deck.json` and append an entry:

```json
{
  "label":  "My skill",
  "skill":  "my-skill-name",
  "model":  "sonnet",
  "effort": "medium",
  "mode":   "acceptEdits",
  "cwd":    "~/projects/myproject",
  "herdr":  false
}
```

Valid values:
- `model`  ∈ `{fable, opus, sonnet, haiku}`
- `effort` ∈ `{low, medium, high, xhigh, max}`
- `mode`   ∈ `{plan, acceptEdits, auto, default}` — **`bypassPermissions` is refused**

The panel hot-reloads on Quickshell reload (`omarchy-shell reload`).

## Run log

All runs are appended to `~/Work/runs.log`:

```
ts=2026-08-23T15:04:00Z skill=TACO-cross-audit model=fable effort=high mode=acceptEdits cwd=/home/you/projects/touring exit=0 secs=42 via=direct artifact=/home/you/Work/artifacts/20260823T150400Z-TACO-cross-audit.log
```

- `exit` is **always numeric**. For `via=direct` it is the exit status of `claude`;
  for `via=herdr` it is the status of the *dispatch* (workspace create + agent
  start + agent prompt), because the herdr run is asynchronous by design (no
  `--wait`) and its outcome is not known when the line is written.
- `via` ∈ `{direct, herdr}`.
- `artifact` always points at a file that exists: the run transcript for
  `via=direct`, an `herdr agent read` snapshot of the agent terminal for
  `via=herdr`.

Override the path with `OMARCHY_SKILL_RUN_LOG`.

## Herdr mode (`--herdr`)

Requires a **running herdr server** (`herdr` opens the TUI and starts it; `herdr
server` runs it headless). With none, `herdr workspace create` exits 1 and the
button fails loudly rather than pretending to dispatch.

Two constraints of herdr 0.8.0 the runner handles for you — both found the first
time this button was ever executed, on 2026-08-23:

- **Agent names must be lowercase.** herdr rejects `TACO-cross-audit` with
  `invalid_agent_name` ("must start with a lowercase letter and contain only
  lowercase letters, digits, '-' or '_' (1-32 characters)"). The runner derives
  `taco-cross-audit` and prints it, so you know what to pass to `herdr agent
  explain`. The **workspace** keeps the readable label.
- **The pane id is not always `w1:p1`.** It is read from
  `.result.root_pane.pane_id` of the `workspace create` response; the second
  workspace of a server is `w2:p1`, and a hardcoded guess dies with
  `agent_pane_not_found`.

The runner also waits for the pane to reach its shell prompt before starting the
agent: `agent start --timeout` waits for *agent* readiness, not for the shell, so
a dispatch fired immediately after `workspace create` gets `agent_pane_busy`.

`model`, `effort` and `permission-mode` travel to the agent through herdr's `--`
passthrough. Verify with the `argv` field herdr returns:

```json
"argv":["claude","--model","fable","--effort","high","--permission-mode","plan"]
```

Without the passthrough that argv is just `["claude"]` — the three arguments are
dropped in silence and the `bypassPermissions` refusal becomes decorative.

## License

Dual-licensed under **MIT OR Apache-2.0**, inherited from the repository this
plugin is exported from (`gabrielgadea/touring`, `Cargo.toml: license = "MIT OR
Apache-2.0"`). Both texts travel with the subtree so the standalone repo carries
its own terms.
