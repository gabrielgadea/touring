// Service.qml — taco.state: the suite's background cache service (L7).
//
// Heavy Touring composites (`status -j` ≈ 28.5s measured) are refreshed HERE,
// in background, atomically — widgets FileView the JSON caches and never block
// the shell. Cheap commands use short intervals; every feed has a freshness
// stamp, consecutive-failure backoff (interval × factor, capped), and
// last-valid retention (a stale-but-valid cache beats an absent one; a
// degraded daemon must never blank a widget — fail-loud via `degraded`).
//
// Intervals come from ~/.config/omarchy/taco/settings.json (taco.settings),
// hot-reloaded on save. The cache write path is shared/TouringService.qml,
// vendored into taco/ by scripts/sync-taco-vendored (L1 self-containment).

import QtQuick
import Quickshell
import Quickshell.Io
import "taco"

Item {
  id: service
  property var shell: null

  // ── Settings (taco.settings writes; we hot-reload) ───────────────────────
  property int intervalLearningMs: 5000
  property int intervalKpiMs: 15000
  property int intervalStatusMs: 120000
  property int backoffFactor: 2

  FileView {
    id: settingsFile
    path: Quickshell.env("HOME") + "/.config/omarchy/taco/settings.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: service.loadSettings(text())
    onLoadFailed: service.loadSettings("")
  }

  function loadSettings(raw) {
    if (!raw || raw.length === 0) return   // defaults stay
    try {
      var s = JSON.parse(raw)
      if (s.pollLearningMs > 0) intervalLearningMs = s.pollLearningMs
      if (s.pollKpiMs > 0) intervalKpiMs = s.pollKpiMs
      if (s.cacheStatusMs > 0) intervalStatusMs = s.cacheStatusMs
      if (s.backoffFactor > 1) backoffFactor = s.backoffFactor
    } catch (e) { /* keep last valid — same retention law as the caches */ }
  }

  // ── Feeds: cheap polled often, heavy rarely (L7 tiers) ───────────────────
  property int failsLearning: 0
  property int failsKpi: 0
  property int failsStatus: 0
  readonly property bool degraded: failsLearning >= 3 || failsKpi >= 3 || failsStatus >= 3

  function intervalFor(base, fails) {
    var factor = Math.min(Math.pow(backoffFactor, fails), 10)
    return Math.min(Math.round(base * factor), 600000)   // cap: 10min
  }

  Timer {
    id: learningTimer
    interval: service.intervalFor(service.intervalLearningMs, service.failsLearning)
    running: true; repeat: true; triggeredOnStart: true
    onTriggered: TouringService.execToCache("learning", ["touring", "learning", "status"])
  }
  Timer {
    id: kpiTimer
    interval: service.intervalFor(service.intervalKpiMs, service.failsKpi)
    running: true; repeat: true; triggeredOnStart: true
    onTriggered: TouringService.execToCache("kpi", ["touring", "kpi", "-j"])
  }
  Timer {
    id: statusTimer
    interval: service.intervalFor(service.intervalStatusMs, service.failsStatus)
    running: true; repeat: true; triggeredOnStart: true
    onTriggered: TouringService.execToCache("status", ["touring", "status", "-j"])
  }

  // ── Freshness stamps → failure counters (backoff + degraded) ─────────────
  function noteStamp(feed, stampText) {
    var failed = String(stampText || "").trim() === "FAIL"
    if (feed === "learning") failsLearning = failed ? failsLearning + 1 : 0
    if (feed === "kpi")      failsKpi      = failed ? failsKpi + 1 : 0
    if (feed === "status")   failsStatus   = failed ? failsStatus + 1 : 0
  }

  FileView { id: learningStamp; path: TouringService.stampPath("learning"); watchChanges: true; printErrors: false; onFileChanged: reload(); onLoaded: service.noteStamp("learning", text()) }
  FileView { id: kpiStamp;      path: TouringService.stampPath("kpi");      watchChanges: true; printErrors: false; onFileChanged: reload(); onLoaded: service.noteStamp("kpi", text()) }
  FileView { id: statusStamp;   path: TouringService.stampPath("status");   watchChanges: true; printErrors: false; onFileChanged: reload(); onLoaded: service.noteStamp("status", text()) }

  // ── IPC ──────────────────────────────────────────────────────────────────
  IpcHandler {
    target: "taco.state"
    function refresh(): string {
      TouringService.execToCache("learning", ["touring", "learning", "status"])
      TouringService.execToCache("kpi", ["touring", "kpi", "-j"])
      TouringService.execToCache("status", ["touring", "status", "-j"])
      return "ok"
    }
    function degraded(): string { return service.degraded ? "1" : "0" }
  }
}
