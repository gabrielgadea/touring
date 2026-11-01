import QtQuick
import "."

// TacoColorAnim — the only ColorAnimation a suite component may use (L5).
// Uses the dedicated `color` duration token (160ms) with OutCubic.
ColorAnimation {
  id: root
  required property var tokens
  duration: tokens ? tokens.dur(tokens.color) : 160
  easing.type: tokens ? tokens.easeOut : Easing.OutCubic
}
