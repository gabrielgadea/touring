// BarWidget.qml — gabriel.skills-deck: one icon on the bar; click opens the
// `skills` section of the Omarchy menu (the deck's working surface), right
// click tails ~/Work/runs.log.
//
// P0V finding (23/08/2026, VM storm): a hand-rolled Panel drawn as a plain
// Item inside the 26-px bar never becomes a popout — `shell summon` returns ok
// and nothing appears. The first-party popouts are built on qs.Ui's Panel
// machinery, whose API we can only exercise on a live shell. So the widget
// delegates its surface to the menu (which the shell owns and positions), and
// the rich tile panel (Panel.qml, kept beside this file) is a P5 refinement to
// be built against the real qs.Ui components.
import QtQuick
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "gabriel.skills-deck"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󱁤"
    tooltipText: "Skills deck (menu: Super+Space → skills)"

    onPressed: function (b) {
      if (!root.bar) return
      if (b === Qt.RightButton)
        root.bar.run("omarchy-launch-or-focus-tui \"tail -n 50 -f ~/Work/runs.log\"")
      else
        root.bar.run("omarchy menu summon skills")
    }
  }
}
