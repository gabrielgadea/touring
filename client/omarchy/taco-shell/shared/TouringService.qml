pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// TouringService — the suite's bridge to the Touring CLI (L6 real data, L7 latency).
//
// L7 latency law (measured 2026-08-24): `touring status -j` ≈ 28.5s on a cold
// composite — NEVER polled synchronously by a widget. Cheap commands
// (`learning status` 13ms, `kpi -j` 69ms) may be polled directly under
// directBudgetMs; everything heavy goes through the background cache that
// taco.state maintains: execToCache writes JSON atomically + a freshness
// stamp, and widgets FileView the cache (hot-reload, zero shell blocking).
QtObject {
  id: root

  // Widgets may only poll commands cheaper than this directly (ms).
  readonly property int directBudgetMs: 100

  // Cache layout: ~/.local/state/taco-shell/cache/<name>.json + <name>.stamp
  readonly property string cacheRoot: Quickshell.env("HOME") + "/.local/state/taco-shell/cache"
  function cachePath(name) { return cacheRoot + "/" + name + ".json" }
  function stampPath(name) { return cacheRoot + "/" + name + ".stamp" }

  // Atomic background refresh: args (list) → <name>.json (+ .stamp UTC time,
  // "FAIL" on error). Fire-and-forget via execDetached — the shell never
  // blocks; consumers FileView the JSON and validate freshness via the stamp.
  // taco.state owns the timers; this is the single write path.
  function execToCache(name, args) {
    var json = cachePath(name), tmp = json + ".tmp", stamp = stampPath(name)
    var quoted = args.map(function(a) { return "'" + String(a).replace(/'/g, "'\\''") + "'" }).join(" ")
    var script = "mkdir -p '" + cacheRoot + "'"
      + " && " + quoted + " > '" + tmp + "' 2>/dev/null"
      + " && mv '" + tmp + "' '" + json + "'"
      + " && date -u +%FT%TZ > '" + stamp + "'"
      + " || echo FAIL > '" + stamp + "'"
    Quickshell.execDetached({ command: ["sh", "-c", script] })
  }

  // Freshness check (consumer-side): is the stamp younger than maxAgeS?
  // lastValid retention: a stale-but-valid cache beats an absent one — a
  // degraded daemon must never blank a widget (fail-loud via `degraded`,
  // never an eternal spinner).
  function stampFresh(stampText, maxAgeS, nowMs) {
    var t = String(stampText || "").trim()
    if (t === "" || t === "FAIL") return false
    var ms = Date.parse(t)
    if (isNaN(ms)) return false
    return ((nowMs || Date.now()) - ms) < maxAgeS * 1000
  }
}
