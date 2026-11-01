# taco.deck — TACO OS Shell Suite

Bar widget + popup panel that quick-launches Claude Code skills from the Omarchy bar.

Part of the **TACO OS Shell Suite** monorepo (`client/omarchy/taco-shell/`) — the
agentic Omarchy shell. Strategy v2: `~/.touring/plans/2026-08-24-omarchy-deep-shell/`.

Supersedes `gabriel.skills-deck` (v0.1.0) — the `taco-suite` installer migrates the
bar entry in `~/.config/omarchy/shell.json` automatically (decision D1).

## What it does

- Robot icon (`󱁤`) on the bar's right section.
- **Left-click the icon**: toggles the popup panel with skill tiles from `deck.json`.
- **Right-click the icon**: tails `~/Work/runs.log` in a terminal.
- **Tile click / Enter**: runs `omarchy-skill-run` with the tile's `skill / model /
  effort / mode / cwd`. Tiles with `"herdr": true` pass `--herdr`.
- **Keyboard**: ↑/↓ move, Enter runs, Tab switches panel, Esc closes.
- **IPC**: `omarchy-shell shell summon taco.deck` / `qs ipc call taco.deck toggle`.

## P0 hardening (vs gabriel.skills-deck v0.1.0)

- Root is first-party `qs.Ui.Panel` + `KeyboardPanel` + `PanelKeyCatcher` — the
  popout actually opens (the hand-rolled `Item` never became one; P0V finding).
- **Zero color literals**: everything derives from `qs.Commons.Color` /
  `qs.Commons.Style` (L4 theme law) — an `omarchy theme set` re-tints the panel.
- Single QML entry point (`Panel.qml`), the stock `omarchy.agents` pattern.
- Keyboard navigation via `PanelKeyCatcher`.

## deck.json

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

Valid values: `model` ∈ `{fable, opus, sonnet, haiku}` · `effort` ∈ `{low, medium,
high, xhigh, max}` · `mode` ∈ `{plan, acceptEdits, auto, default}` —
**`bypassPermissions` is refused**. `deck.json` hot-reloads on save (FileView).

## Install

```bash
taco-suite install            # validate → backup → stage → migrate shell.json → rescan
taco-suite install --dry-run  # print the plan only
taco-suite uninstall          # remove + restore previous state from backup
```

Run log: `~/Work/runs.log` (override with `OMARCHY_SKILL_RUN_LOG`). Herdr mode and
runner contract: see `client/omarchy/skills-deck/README.md` (unchanged semantics).

## License

Dual-licensed under **MIT OR Apache-2.0**, inherited from `gabrielgadea/touring`.
