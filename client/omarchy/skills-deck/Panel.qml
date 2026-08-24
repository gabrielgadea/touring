// Panel.qml — Skills deck panel: renders tiles from deck.json
//
// File loading strategy:
//   Quickshell.Io.FileView (watched) via Qt.resolvedUrl("deck.json").
//   This resolves relative to the *installed* plugin directory
//   (~/.config/omarchy/plugins/gabriel.skills-deck/deck.json) at runtime, and
//   to the source tree during development.
//   FileView (Quickshell) was considered but omitted: it is not documented in
//   the Quattro first-party source examined and its availability is uncertain.
//
// TODO(P0V): validar no shell real — Panel/PanelCard components from qs.Ui
//   are first-party and not available here; using conservative QtQuick primitives
//   (Item, Column, Repeater, Rectangle, Text, MouseArea) instead.
//
// Imports qs.* generate "module not found" warnings under qmllint without the
// Quickshell runtime — expected and accepted.

import QtQuick
import Quickshell.Io  // FileView — TODO(P0V): validar no shell real
import qs.Commons  // TODO(P0V): validar no shell real
import qs.Ui       // TODO(P0V): Panel/PanelCard shapes — using Item fallback below

Item {
    id: root

    // ── Shape contract expected by BarWidget.injectPanel ────────────────────
    property bool opened: false
    property var  bar:    null
    property var  settings: null
    property var  anchorItem: null
    property var  hostWidget: null
    readonly property bool popoutSwitchClosing: false

    function open()  { root.opened = true  }
    function close() { root.opened = false }
    function toggle() { root.opened = !root.opened }
    function openFromHotkey() { root.opened = true }
    function closeForPopoutSwitch() { root.opened = false }

    // ── Deck data ────────────────────────────────────────────────────────────
    property var deckItems: []

    // Quickshell ships its own file reader (Quickshell.Io.FileView); XMLHttpRequest
    // on file:// is gated in Qt 6 behind QML_XHR_ALLOW_FILE_READ and a SYNC xhr
    // is refused outright, so the previous `xhr.open(…, false)` left the panel
    // silently empty (critic-robustness P1, 23/08/2026). FileView reloads when
    // deck.json changes on disk, which is the hot-reload the deck wants anyway.
    // TODO(P0V): confirm `FileView.text()` + `watchChanges` against the shell's
    //   Quickshell version; fallback is an ASYNC xhr with onreadystatechange.
    FileView {
        id: deckFile
        path: Qt.resolvedUrl("deck.json")
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.parseDeck(text())
        onLoadFailed: function(error) {
            console.warn("gabriel.skills-deck: could not read deck.json:", error)
        }
    }

    function parseDeck(raw) {
        try {
            var items = JSON.parse(raw)
            root.deckItems = Array.isArray(items) ? items : []
        } catch (e) {
            console.error("gabriel.skills-deck: failed to parse deck.json:", e)
            root.deckItems = []
        }
    }

    // ── Panel surface ────────────────────────────────────────────────────────
    // TODO(P0V): replace with Panel { } + PanelCard { } when validating on the
    //   actual shell — first-party components handle anchor, shadow, and
    //   keyboard dismiss automatically.

    visible: root.opened
    width:   300
    height:  tileColumn.implicitHeight + 24

    Rectangle {
        anchors.fill: parent
        color:        "#1e1e2e"   // TODO(P0V): use Style.panel.background when available
        radius:       10
        border.color: "#313244"
        border.width: 1

        Column {
            id: tileColumn
            anchors {
                top:    parent.top
                left:   parent.left
                right:  parent.right
                margins: 12
            }
            spacing: 6
            topPadding: 12
            bottomPadding: 12

            Repeater {
                model: root.deckItems

                // ── Skill tile ───────────────────────────────────────────────
                delegate: Rectangle {
                    width:  parent.width
                    height: 52
                    radius: 6
                    color:  tileArea.containsMouse ? "#313244" : "#2a2a3e"

                    // Model / effort badge strip
                    Rectangle {
                        id: badge
                        anchors {
                            right:          parent.right
                            verticalCenter: parent.verticalCenter
                            rightMargin:    8
                        }
                        width:  badgeText.implicitWidth + 12
                        height: 20
                        radius: 4
                        color:  "#45475a"

                        Text {
                            id: badgeText
                            anchors.centerIn: parent
                            text:  (modelData.model || "") + " · " + (modelData.effort || "")
                            color: "#cdd6f4"
                            font { pixelSize: 10; family: "monospace" }
                        }
                    }

                    // Skill label
                    Text {
                        anchors {
                            left:           parent.left
                            verticalCenter: parent.verticalCenter
                            leftMargin:     12
                            right:          badge.left
                            rightMargin:    8
                        }
                        text:           modelData.label || modelData.skill || ""
                        color:          "#cdd6f4"
                        font.pixelSize: 13
                        elide:          Text.ElideRight
                    }

                    MouseArea {
                        id:          tileArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape:  Qt.PointingHandCursor

                        onClicked: {
                            if (!root.bar) return
                            var tile  = modelData
                            var herdr = tile.herdr ? "--herdr " : ""
                            var cmd   = "omarchy-skill-run " + herdr
                                      + tile.skill    + " "
                                      + tile.model    + " "
                                      + tile.effort   + " "
                                      + tile.mode     + " "
                                      + tile.cwd
                            root.bar.run(cmd)
                            root.close()
                        }
                    }
                }
            }
        }
    }
}
