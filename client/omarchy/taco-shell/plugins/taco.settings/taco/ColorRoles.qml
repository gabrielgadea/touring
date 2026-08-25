pragma Singleton
import QtQuick
import qs.Commons
import "."

// ColorRoles — the suite's derived color roles (L4). ALL roles are derivations
// of Theme's hue, never literals: "fallbacks are a floor, not a palette".
// recess is deliberately NOT a color — it's an alpha state over a surface
// (interaction renders as sinking-in, not added light).
QtObject {
  id: root

  readonly property color field:       Theme.background                 // page / deepest background
  readonly property color void:        Theme.withAlpha(field, 0.18)     // intentional absence: insets, wells, scrims
  readonly property color plate:       Theme.popups.background          // raised surfaces (panel, flyout)
  readonly property color ink:         Theme.popups.text                // primary text on plate
  readonly property color whisper:     Theme.muted                      // muted/secondary text (4.5:1 target)
  readonly property color soft:        Theme.withAlpha(ink, 0.78)       // de-emphasized foreground
  readonly property color seam:        Theme.withAlpha(ink, 0.18)       // edges, dividers, strokes
  readonly property color frameBorder: Theme.popups.border              // frame reveal / outlines (opaque)
  readonly property color accent:      Theme.accent                     // the single theme accent

  // retro-82 signature: the orbital core is amber — which IS the theme accent.
  // Named role so surfaces say "amberCore" semantically, and a future theme
  // whose accent isn't amber still lights the nucleus correctly (hueAmber).
  readonly property color amberCore:   Theme.hueAmber

  readonly property color danger:      Theme.hueRed                     // user destructive intent
  readonly property color warning:     Theme.hueYellow                  // caution
  readonly property color urgent:      Theme.urgent                     // red system states

  // The load-bearing unified rule (lacuna): only `danger` changes the hue.
  function toneAccent(tone) {
    return tone === "danger" ? danger : accent
  }

  // recess: alpha interaction state over a surface — never a standalone hue.
  function recess(surface, strength) {
    return Theme.withAlpha(Theme.foreground, strength === undefined ? 0.06 : strength)
  }
}
