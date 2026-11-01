// Panel.qml — taco.deck: skills deck bar-widget + popup, single QML entry.
//
// P0 hardening (2026-08-24, strategy v2 bundle 2026-08-24-omarchy-deep-shell):
//   * Root is qs.Ui.Panel — the first-party popout machinery. The pre-P0 widget
//     hand-rolled a plain Item inside the 26px bar that never became a popout
//     (P0V finding 23/08: `shell summon` returned ok and nothing appeared).
//   * Structure mirrors the stock omarchy.agents plugin [verified]: BarIconButton
//     owns the bar slot; KeyboardPanel + PanelKeyCatcher own the popup surface,
//     keyboard navigation and dismiss; IpcHandler exposes open/close/toggle.
//   * L4 theme law: every color derives from qs.Commons.Color / qs.Commons.Style.
//     Zero literals — the old Catppuccin hexes (#1e1e2e/#313244/#cdd6f4) are gone,
//     so an `omarchy theme set` re-tints this panel on the shell's own clock.
//   * deck.json via Quickshell.Io.FileView with watchChanges — the hot-reload the
//     deck wants (same pattern qs.Commons.Color itself uses for shell.toml).

import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "taco.deck"
  ipcTarget: "taco.deck"
  // The Ui.Panel base registers its own IpcHandler for ipcTarget; we register
  // ours below (it adds refresh()). manageIpc=false keeps exactly one handler
  // per target — the stock omarchy.agents pattern (duplicate-target WARN otherwise).
  manageIpc: false

  // Theme-derived roles (agents/Panel.qml pattern: bar-injected with Color fallback)
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color accent: Color.accent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property color tileBase: alpha(foreground, 0.05)
  readonly property color tileHover: Style.selectedFillFor(foreground, accent)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  function alpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
  function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)) }

  // ── Deck data ─────────────────────────────────────────────────────────────
  property var deckItems: []
  property int selectedIndex: -1
  property bool cursorActive: false

  FileView {
    id: deckFile
    path: Qt.resolvedUrl("deck.json")
    watchChanges: true
    onFileChanged: reload()
    onLoaded: root.parseDeck(text())
    onLoadFailed: function(error) {
      console.warn("taco.deck: could not read deck.json:", error)
    }
  }

  function parseDeck(raw) {
    try {
      var items = JSON.parse(raw)
      root.deckItems = Array.isArray(items) ? items : []
    } catch (e) {
      console.error("taco.deck: failed to parse deck.json:", e)
      root.deckItems = []
    }
  }

  function refreshDeck() {
    deckFile.reload()
  }

  function selectTile(index) {
    if (root.deckItems.length === 0) return
    var wrapped = ((index % root.deckItems.length) + root.deckItems.length) % root.deckItems.length
    root.selectedIndex = wrapped
    ensureVisible(wrapped)
  }

  function ensureVisible(index) {
    if (!panelFlick) return
    var rowH = Style.space(52) + Style.space(6)
    var y = index * rowH
    if (y < panelFlick.contentY)
      panelFlick.contentY = y
    else if (y + rowH > panelFlick.contentY + panelFlick.height)
      panelFlick.contentY = y + rowH - panelFlick.height
  }

  function runTile(tile) {
    if (!root.bar || !tile) return
    var herdr = tile.herdr ? "--herdr " : ""
    var cmd = "omarchy-skill-run " + herdr
            + tile.skill  + " "
            + tile.model  + " "
            + tile.effort + " "
            + tile.mode   + " "
            + tile.cwd
    root.bar.run(cmd)
    root.close()
  }

  function activateSelection() {
    if (root.deckItems.length === 0) return
    var index = root.selectedIndex >= 0 ? root.selectedIndex : 0
    root.runTile(root.deckItems[index])
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onOpenedChanged: if (opened) {
    cursorActive = false
    selectedIndex = -1
    refreshDeck()
    if (panelFlick) panelFlick.contentY = 0
    Qt.callLater(function() { keyCatcher.forceActiveFocus() })
  }

  IpcHandler {
    target: root.ipcTarget
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function refresh(): string { root.refreshDeck(); return "ok" }
  }

  // ── Bar slot ──────────────────────────────────────────────────────────────
  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󱁤"
    tooltipText: "TACO skills deck"

    onPressed: function(buttonCode) {
      if (!root.bar) return
      if (buttonCode === Qt.RightButton)
        root.bar.run("omarchy-launch-or-focus-tui \"tail -n 50 -f ~/Work/runs.log\"")
      else
        root.toggle()
    }
  }

  // ── Popup surface (first-party: owns anchor, scrim, keyboard dismiss) ─────
  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(340))
    contentHeight: panel.fittedContentHeight(tileColumn.implicitHeight, Style.space(520))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent

      onMoveRequested: function(dx, dy) {
        if (dy !== 0) {
          root.cursorActive = true
          root.selectTile(root.selectedIndex + dy)
        }
      }
      onActivateRequested: root.activateSelection()
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Flickable {
        id: panelFlick
        anchors.fill: parent
        contentWidth: width
        contentHeight: tileColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
          id: tileColumn
          width: panelFlick.width
          spacing: Style.space(6)
          topPadding: Style.space(12)
          bottomPadding: Style.space(12)

          Text {
            visible: root.deckItems.length === 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "deck.json vazio ou ilegível"
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }

          Repeater {
            model: root.deckItems

            delegate: Rectangle {
              width: tileColumn.width
              height: Style.space(52)
              radius: Style.radius ? Style.radius(6) : 6
              color: (root.cursorActive && index === root.selectedIndex) || tileArea.containsMouse
                     ? root.tileHover : root.tileBase

              // Model / effort badge strip
              Rectangle {
                id: badge
                anchors {
                  right: parent.right
                  verticalCenter: parent.verticalCenter
                  rightMargin: Style.space(8)
                }
                width: badgeText.implicitWidth + Style.space(12)
                height: Style.space(20)
                radius: Style.radius ? Style.radius(4) : 4
                color: root.alpha(root.accent, 0.18)

                Text {
                  id: badgeText
                  anchors.centerIn: parent
                  text: (modelData.model || "") + " · " + (modelData.effort || "")
                  color: root.accent
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                }
              }

              // Skill label
              Text {
                anchors {
                  left: parent.left
                  verticalCenter: parent.verticalCenter
                  leftMargin: Style.space(12)
                  right: badge.left
                  rightMargin: Style.space(8)
                }
                text: modelData.label || modelData.skill || ""
                color: root.foreground
                font.family: root.fontFamily
                font.pixelSize: Style.font.body
                elide: Text.ElideRight
              }

              MouseArea {
                id: tileArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: { root.cursorActive = false; root.selectedIndex = index }
                onClicked: root.runTile(modelData)
              }
            }
          }
        }
      }
    }
  }
}
