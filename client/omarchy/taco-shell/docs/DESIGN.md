# DESIGN — TACO OS Shell Suite

The suite's visual and motion identity. Strategy v2 (full evidence):
`~/.touring/plans/2026-08-24-omarchy-deep-shell/strategy-2026-08-24-omarchy-deep-shell.md`.
Design canvas (7 artboards): claude.ai artifact `51b9a8a2-d391-46f5-a857-c5665e751694`,
sources `client/omarchy/design/taco-os/`.

## DNA — the HUD references (~/Downloads, mapped 1:1 onto retro-82)

| HUD element | retro-82 role |
|---|---|
| Deep navy field | `background` `#00172e` |
| Teal/cyan structural rings, trajectory arcs, hex frames | theme teals (structure) |
| **Amber incandescent orbital core** + data accents | `accent` `#faa968` |
| Glowing edges / seams | `seam` = alpha of ink, never a hue |

Typography: **Michroma** (display) + **Space Mono** (data/readouts) — applied at the
`taco.orbital`/`taco.ambience` surfaces (P4); bar widgets follow `Style.font.family`.

## Theme law (L4 — lacuna contract)

> Omarchy owns hue; TACO owns form, depth, and motion.

- Hue comes from `qs.Commons.Color` (transactional push on theme change) + extended
  hues from `~/.local/state/omarchy/current/theme/colors.toml` with last-valid retention.
- **No component hard-codes a hue.** Roles derive: field/void/plate/ink/whisper/soft/
  seam/accent/amberCore. Fallbacks are a floor, not a palette.
- `recess` is an alpha state over a surface, not a color.

## Motion law (L5 — lacuna contract)

> Reveal, don't appear.

- Geometry grows **from the attachment edge**; content withheld until the open
  threshold; closing is the reverse, slightly faster.
- Token scale (canonical home P1 `shared/MotionTokens.qml`, vendored):
  `instant 75 · quick 150 · color 160 · reveal 300 · settle 450 · ambient 750 ·
  pulse 900 · sweep 2400ms`.
- Easings: `OutCubic` default · `InOutSine` symmetric · signature Bézier
  `[0.20, 0, 0.32, 1]` for primary reveals. No bounce, no overshoot.
- Reduced motion removes time, not structure. No component hand-writes a duration.

## The signature surface — taco.orbital (P4)

The full-screen orbital nucleus: Touring reactor as the amber core, projects as
satellites on teal trajectory arcs, live DAG state per satellite, hex-framed data
panels (pulse/mission/forge/geo). Bar + frame + sidebar read as **one surface**
(lacuna's connected-shell pattern). Gate G2 (visual lock on the canvas) precedes it.

## Data law (L6/L7 — TACO differentiator)

- Every surface shows **real data** (Touring CLI, LexHub, kazuba-geo) — zero placeholders.
- **L7 latency**: widgets only poll sub-100ms commands (`learning status` 13ms,
  `kpi -j` 69ms); heavy composites (`status -j` ≈ 28.5s measured) refresh through
  `taco.state`'s background cache → JSON → `FileView`. Daemon degraded → visible
  "degraded" state with backoff, never an eternal spinner.
