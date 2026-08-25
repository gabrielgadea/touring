pragma Singleton
import QtQuick
import Quickshell.Io
import qs.Commons

// Theme — the suite's hue bridge (L4: Omarchy owns hue, TACO owns form).
// Primary source: qs.Commons.Color (pushed transactionally on theme change —
// same clock as the native bar, never file-watching for the core palette).
// Extended hues (red/green/cyan/magenta/amber…) come from the active theme's
// colors.toml with last-valid retention on transient read failure (the file is
// atomically swapped during `omarchy theme set`; a read mid-swap must never
// blank the palette).
QtObject {
  id: root

  // ── Core palette (transactional) ─────────────────────────────────────────
  readonly property color foreground: Color.foreground
  readonly property color background: Color.background
  readonly property color accent: Color.accent
  readonly property color urgent: Color.urgent
  readonly property color muted: Color.muted
  readonly property var popups: Color.popups
  readonly property var menu: Color.menu
  readonly property var bar: Color.bar

  // ── Helpers ──────────────────────────────────────────────────────────────
  function withAlpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }

  // Parse "#RRGGBB[AA]", "rgba(r,g,b,a)" and hyprland "0xAARRGGBB" (lacuna contract).
  function parseColor(value, fallback) {
    var s = String(value || "").trim()
    if (s.match(/^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$/)) return s
    if (s.match(/^0x[0-9A-Fa-f]{8}$/)) {
      var a = s.substring(2, 4), r = s.substring(4, 6), g = s.substring(6, 8), b = s.substring(8, 10)
      return "#" + r + g + b + a
    }
    return fallback
  }

  // ── Extended hues (colors.toml, last-valid retained) ─────────────────────
  property color hueRed:     Color.urgent
  property color hueGreen:   foreground
  property color hueCyan:    accent
  property color hueMagenta: accent
  property color hueAmber:   accent
  property color hueYellow:  accent

  property var _lastValid: ({})

  function _loadHues(raw) {
    if (!raw || raw.length === 0) return  // transient swap: keep last valid
    var parsed = {}
    var lines = String(raw).split("\n")
    for (var i = 0; i < lines.length; i++) {
      var m = lines[i].match(/^\s*([A-Za-z0-9_-]+)\s*=\s*["']?(#[0-9A-Fa-f]{6})/)
      if (m) parsed[m[1]] = m[2]
    }
    if (Object.keys(parsed).length === 0) return  // garbage read: keep last valid
    root._lastValid = parsed
    if (parsed.red)     hueRed = parsed.red
    if (parsed.green)   hueGreen = parsed.green
    if (parsed.cyan)    hueCyan = parsed.cyan
    if (parsed.magenta) hueMagenta = parsed.magenta
    if (parsed.amber)   hueAmber = parsed.amber
    if (parsed.yellow)  hueYellow = parsed.yellow
  }

  property FileView colorsFile: FileView {
    path: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme/colors.toml"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: root._loadHues(text())
    onLoadFailed: root._loadHues("")   // keep last valid
  }
}
