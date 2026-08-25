import QtQuick
import "."

// TacoAnim — the only NumberAnimation a suite component may use (L5).
// Duration and easing come from MotionTokens; no hand-written literals.
//   TacoAnim { tokens: motionTokens; token: motionTokens.reveal }
NumberAnimation {
  id: root
  required property var tokens
  property int token: tokens ? tokens.quick : 150
  duration: tokens ? tokens.dur(token) : token
  easing.type: tokens ? tokens.easeOut : Easing.OutCubic
}
