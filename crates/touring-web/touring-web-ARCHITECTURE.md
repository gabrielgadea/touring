# touring-web — Architecture

> **Version**: v0.2.0 | **Updated**: 2026-06-11 | **Constraints**: `#![forbid(unsafe_code)]`
> **Design system**: "Elite" (2026-06-11) — ported from Gabriel's UX exploration (`touring.zip` design canvas)

## Overview

Web UI for the Touring system — Leptos 0.8 frontend with Axum server. WASM-based
interactive interface for code analysis, wiring visualization, and session
management. This crate is a **thin shim**: the implementation lives in
`touring-bindings/src/web/` behind the `bind-web` feature; this crate exists so
Trunk has a dedicated wasm32 build target.

## Design System — "Elite" (2026-06-11)

Single stylesheet: `public/assets/styles/main.css` (~4.060 LOC after Wave 2).
Ported from the design-canvas exploration (3 metaphors → elite synthesis:
sidebar 232px + main hero + right rail). Tokens:

| Token group | Values |
|---|---|
| Ink/surfaces | `--el-ink #0a0a0d`, `--el-surface #111114/#16161b/#1c1c22`, hairlines `rgba(255,255,255,.06)` |
| Foreground | 5 levels `#fafafa → #52525b` |
| Accent | teal `#5eead4` (+soft/hair); pos `#84cc16`, neg `#f43f5e`, warn `#f59e0b` |
| Type | **Inter Tight** (exclusive display+body) + JetBrains Mono; `tabular-nums` |
| Structure | `.el-card .el-btn .el-tag .el-kbd .el-eyebrow .el-stat .el-row .el-prog-* .el-pulse .el-side .el-table` (26 classes) + page-level `ql-*` (quality) and `srch-*` (search) |

Legacy variable names are aliased for compat; light theme preserved via the
existing toggle.

**Elite coverage — Wave 1 (2026-06-11)**: **dashboard**, **quality**, **search**
(command-palette style).

**Elite coverage — Wave 2 (2026-06-11)**: ALL remaining routes ported from
their design-canvas artboards (`/tmp/touring-zip/artboards/hifi-*.jsx`), each
with a per-route CSS prefix appended to `main.css`:
**memory** (`mem-` ← hifi-elite-memory), **wiring** (`wir-` ← hifi-elite-wiring),
**health** (`hlt-` ← hifi-health, KPI strip reuses the shared `ScoreBar`
component), **orphans** (`orp-` ← hifi-orphans), **federation** (`fed-` ←
hifi-federation), **quality_diff** (`qd-`), **quality_rules** (`qr-`),
**workspace** (`ws-` ← hifi-elite-overview, full HUD + node-detail panel
restyle). Data layer (LocalResource / server functions / models) untouched —
only the `view!` layer changed. 12/12 routes now speak the elite design system.

## wasm32 build (fixed 2026-06-11)

`trunk build --release` works again (it had been broken since the
daemon-lib-rearch consolidated heavy native deps into `touring-bindings`):

- `touring-bindings`: the 8 `touring-*` internal crates + `tokio` + the Axum
  stack moved to `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`;
  `web::server` is `#[cfg(not(target_arch = "wasm32"))]`.
- `wasm-bindgen` declared explicitly (workspace `0.2`) and wired into `bind-web`.
- This crate: `.cargo/config.toml` sets `--cfg getrandom_backend="wasm_js"`;
  wasm32 deps enable `getrandom 0.3/wasm_js` + `getrandom 0.2/js`.
- `index.html` `data-trunk` CSS href points at `public/assets/styles/main.css`
  (the old `src/styles/` was removed in an earlier wave).

**Result**: WASM bundle shrank **6.3MB → 1.05MB** (native dep graph no longer
leaks into the client).

## Key Types

`UiError` | `AppState` | `WsState` | `Theme` | `WorkspaceQualitySignal`

## Web Stack

- **Frontend**: Leptos 0.8 — Rust WASM compiled to web interface
- **Backend**: Axum HTTP server (`touring-bindings::web::server`, native-only)
- **Communication**: JSON over REST (`/api/*`) + WebSocket (`/ws/quality`)
- **Build**: `trunk build --release` (dev: `trunk serve` on :3001, proxy → :3000)
- **Deployment**: `touring-web-server` serves `dist/` + the API

## Vendored JS (`vendor/`)

`three.min.js` v0.149 + `3d-force-graph` + `svg-pan-zoom` — self-hosted to
bypass Chrome ORB; load order is critical (three before 3d-force-graph).
