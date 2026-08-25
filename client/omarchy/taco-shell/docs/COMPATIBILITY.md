# COMPATIBILITY — TACO OS Shell Suite

Host compatibility record (Shibumi law: pin exact revisions, re-verify on update).

## Pinned host (measured 2026-08-24, machine `storm`)

| Component | Version | Source |
|---|---|---|
| Omarchy | **4.0.0-1** | `pacman -Q omarchy` |
| Quickshell | **0.3.0.r20.g28771c7-2** (quickshell-git) | `pacman -Q quickshell-git` |
| Shell | single `omarchy-shell` process, 33 first-party `qs.Ui` components | `/usr/share/omarchy/shell/` |

Same host generation the reference suites pin (Shibumi pins `0.3.0.r20.g28771c7-1`;
lacuna tests against `0.3.0.r18`) — contract-compatible.

## Verified contracts used

| Contract | Evidence |
|---|---|
| Plugin kinds `bar / bar-widget / bar-option / service / overlay / panel` | stock manifests + `services/PluginRegistry.qml` |
| `qs.Ui`: Panel, KeyboardPanel, PanelKeyCatcher, BarIconButton, PanelHero | `ls /usr/share/omarchy/shell/Ui/` |
| `qs.Commons.Color` singleton (transactional theme push; surface groups bar/popups/menu/…) | `Commons/Color.qml` |
| `qs.Commons.Style` (font/space/selectedFillFor) | stock widgets (`panels/monitor`, `bar/widgets/*`) |
| Single-entry bar-widget pattern (root `Panel` = button + popup) | stock `plugins/agents/Panel.qml`, 3rd-party `stappmus.activity-monitor` |
| `FileView` watched JSON | `Commons/Color.qml` itself + Context7 Quickshell v0.3.0 |
| `omarchy plugin validate` / `omarchy-shell shell rescanPlugins` | `omarchy plugin --help` |
| `omarchy-shell shell summon <id> '<json>'` | `/usr/share/omarchy/shell/plugins/README.md` |

## Re-verification

A `post-update.d` Omarchy hook re-runs `tests/validate_all.sh` after each
`omarchy update` (planned P1 — mirrors `browser-policy-recommended.sh`).
On contract drift: fail loud, pin the new revision here, repair via `taco-suite repair`.

## Reference suites (external lens)

- `OldJobobo/lacuna-shell` — theme split law + design system (color/motion tokens)
- `HANCORE-linux/Shibumi-Shell` — self-contained plugins, vendored shared, transactional lifecycle
