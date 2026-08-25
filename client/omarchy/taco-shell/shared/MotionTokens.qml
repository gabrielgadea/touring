pragma Singleton
import QtQuick

// MotionTokens — canonical motion scale for the TACO OS Shell Suite (L5).
// "Reveal, don't appear": geometry grows from the attachment edge, content is
// withheld until the threshold, closing is the reverse and slightly faster.
// No component hand-writes a duration — everything consumes these tokens
// (vendored into plugins by scripts/sync-taco-vendored).
// Scale + easings follow the lacuna design-system contract (03-motion.md).
QtObject {
  id: root

  // ── Duration scale (ms) ──────────────────────────────────────────────────
  readonly property int instant: 75     // micro-feedback, immediate state
  readonly property int quick:   150    // hover/press recess, small reveals
  readonly property int color:   160    // ColorAnimation transitions
  readonly property int reveal:  300    // standard panel/flyout disclosure
  readonly property int settle:  450    // large geometry changes, reflow
  readonly property int ambient: 750    // slow decorative drift
  readonly property int pulse:   900    // attention loops
  readonly property int sweep:   2400   // long decorative overlay sweeps

  // ── Easings ──────────────────────────────────────────────────────────────
  // OutCubic: default for opacity/position/geometry (soft arrivals).
  // InOutSine: symmetric (hovers, open+close in one gesture).
  // revealCurve: signature primary-disclosure Bézier [0.20, 0, 0.32, 1] —
  // quick commit out of closed, long graceful settle. No bounce, no overshoot.
  readonly property int easeOut:  Easing.OutCubic
  readonly property int easeInOut: Easing.InOutSine
  readonly property var revealBezier: [0.20, 0.0, 0.32, 1.0]

  // ── Reveal choreography ──────────────────────────────────────────────────
  // Content stays concealed until the open gesture crosses this progress;
  // closing reverses the mapping (content fades first, geometry collapses).
  readonly property real contentThreshold: 0.55
  readonly property real shellThreshold: 0.65

  // ── Reduced motion ───────────────────────────────────────────────────────
  // "Reduced motion removes time, not structure": durations collapse toward
  // instant, the attachment-edge origin is preserved. Central switch — no
  // component may special-case it. Wired to taco.settings in P1.
  property bool reducedMotion: false

  // The one accessor every component uses instead of a literal.
  function dur(token) {
    return root.reducedMotion ? root.instant : token
  }
}
