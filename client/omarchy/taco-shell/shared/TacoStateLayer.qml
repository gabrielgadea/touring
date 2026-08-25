import QtQuick
import "."

// TacoStateLayer — the recess interaction state (L4: recess is an alpha STATE
// over a surface, never a hue). Layers hover/press as sinking-in over the
// parent surface, animated with tokens. Usage:
//   TacoStateLayer { roles: colorRoles; tokens: motionTokens; state: tileArea.containsMouse ? "hover" : "normal" }
Rectangle {
  id: root
  required property var roles
  required property var tokens
  // "normal" (transparent) | "hover" (recess) | "press" (deeper recess) | "focus" (seam ring)
  property string state: "normal"

  anchors.fill: parent
  radius: parent.radius || 0
  color: {
    if (!roles) return "transparent"
    switch (root.state) {
      case "hover":  return roles.recess(roles.plate, 0.06)
      case "press":  return roles.recess(roles.plate, 0.12)
      default:       return "transparent"
    }
  }
  border.width: root.state === "focus" ? 1 : 0
  border.color: roles ? roles.seam : "transparent"

  Behavior on color {
    TacoColorAnim { tokens: root.tokens }
  }
}
