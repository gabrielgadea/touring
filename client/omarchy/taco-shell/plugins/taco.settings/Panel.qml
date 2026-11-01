// Panel.qml — taco.settings: the suite's Configure surface (panel kind).
//
// Dev-gallery contract: root Item with open(payloadJson) / close() /
// requestClose() + host-injected `shell`, hosting a FloatingWindow. Summon:
//   omarchy-shell shell summon taco.settings "{}"
// Persistence: FileView + JsonAdapter — every edit writeAdapter()s back to
// ~/.config/omarchy/taco/settings.json on the spot (Quickshell v0.3.0 contract).
// The surface itself is the design-system showcase: colors come ONLY from the
// vendored ColorRoles/Theme singletons (import "taco" — L1 self-containment
// via scripts/sync-taco-vendored; zero hue literals, L4).

import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "taco"

Item {
  id: root
  property var shell: null
  property bool closingFromHost: false

  function open(payloadJson) {
    closingFromHost = false
    window.visible = true
  }
  function close() {
    closingFromHost = true
    window.visible = false
    closingFromHost = false
  }
  function requestClose() {
    if (shell && typeof shell.hide === "function") shell.hide("taco.settings")
    else window.visible = false
  }

  // ── Persistence (FileView + JsonAdapter; writes create the file) ─────────
  FileView {
    id: settingsFile
    path: Quickshell.env("HOME") + "/.config/omarchy/taco/settings.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onAdapterUpdated: writeAdapter()

    JsonAdapter {
      id: cfg
      property int pollLearningMs: 5000
      property int pollKpiMs: 15000
      property int cacheStatusMs: 120000
      property int backoffFactor: 2
      property string colorProfile: "semantic"
      property bool reducedMotion: false
      property bool ambienceEnabled: false
    }
  }

  // ── Window ───────────────────────────────────────────────────────────────
  FloatingWindow {
    id: window
    title: "TACO OS — Settings"
    color: ColorRoles.field
    implicitWidth: 560
    implicitHeight: 640
    minimumSize: Qt.size(480, 480)
    visible: false

    Shortcut { sequences: ["Esc"]; onActivated: root.requestClose() }

    Column {
      anchors {
        fill: parent
        margins: Style.space(20)
      }
      spacing: Style.space(16)

      Text {
        text: "TACO OS Shell Suite"
        color: ColorRoles.ink
        font.family: Style.font.family
        font.pixelSize: Style.font.heading
        font.bold: true
      }
      Text {
        width: parent.width
        wrapMode: Text.WordWrap
        text: "Polling da camada de dados (lei L7 — medido 2026-08-24: status -j ≈ 28,5s nunca é polado por widgets; só via cache em background)."
        color: ColorRoles.whisper
        font.family: Style.font.family
        font.pixelSize: Style.font.bodySmall
      }

      NumberField {
        label: "learning status poll (ms) — barato, 13ms"
        from: 1000; to: 60000; stepSize: 500
        value: cfg.pollLearningMs
        onModified: function(v) { cfg.pollLearningMs = v }
      }
      NumberField {
        label: "kpi poll (ms) — barato, 69ms"
        from: 2000; to: 120000; stepSize: 1000
        value: cfg.pollKpiMs
        onModified: function(v) { cfg.pollKpiMs = v }
      }
      NumberField {
        label: "status cache refresh (ms) — pesado, background"
        from: 30000; to: 900000; stepSize: 15000
        value: cfg.cacheStatusMs
        onModified: function(v) { cfg.cacheStatusMs = v }
      }
      NumberField {
        label: "backoff factor (falhas consecutivas)"
        from: 2; to: 8; stepSize: 1
        value: cfg.backoffFactor
        onModified: function(v) { cfg.backoffFactor = v }
      }

      Dropdown {
        label: "Color profile (widgets de barra)"
        options: ["semantic", "colorful"]
        value: cfg.colorProfile
        onChanged: function(v) { cfg.colorProfile = v }
      }
      Dropdown {
        label: "Reduced motion (remove tempo, não estrutura)"
        options: [{ "value": "false", "label": "off" }, { "value": "true", "label": "on" }]
        value: String(cfg.reducedMotion)
        onChanged: function(v) { cfg.reducedMotion = (v === "true") }
      }
      Dropdown {
        label: "Ambience overlays (P4 — ainda não instalados)"
        options: [{ "value": "false", "label": "off" }, { "value": "true", "label": "on" }]
        value: String(cfg.ambienceEnabled)
        onChanged: function(v) { cfg.ambienceEnabled = (v === "true") }
      }

      Text {
        width: parent.width
        wrapMode: Text.WordWrap
        text: "Gravação imediata em ~/.config/omarchy/taco/settings.json — taco.state hot-reloada na hora."
        color: ColorRoles.seam
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }
  }
}
