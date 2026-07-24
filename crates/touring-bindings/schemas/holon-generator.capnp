# THSF holon-generator.capnp — Phase 5 COMBO F schema.
#
# Package: holon:generator@0.1.0 (separate from holon:core — signals
# that this is an opt-in Touring-producer capability, not a foundational
# contract). Consumers of `holon:core` do NOT automatically inherit
# `generator-health`.
#
# Transport path: capnp RPC over Unix socket for real-time subscribe
# (latency ~50 µs). JSON fallback available via
# `touring health-delta status -j` when capnp is unavailable.

@0x8d59c3ccb270ce4b;

# ============================================================================
# Scalar enums
# ============================================================================

enum DeltaOutcome {
  # new_health within the neutral band around old_health (no material change).
  neutral @0;
  # new_health exceeds old_health beyond the improvement threshold.
  improvement @1;
  # new_health falls below old_health beyond the regression threshold.
  regression @2;
}

# ============================================================================
# Event payload
# ============================================================================

# A single health delta event — the payload delivered to every active
# HealthDeltaListener.onDelta call. Mirrors touring-core::HealthDeltaEvent.
struct HealthDeltaEvent {
  # Absolute path of the tracked file.
  filePath @0 :Text;

  # Health score before the edit, in [0.0, 1.0].
  oldHealth @1 :Float32;

  # Health score after the edit, in [0.0, 1.0].
  newHealth @2 :Float32;

  # newHealth - oldHealth. Signed.
  delta @3 :Float32;

  # Classification of this delta.
  outcome @4 :DeltaOutcome;

  # Cumulative consecutive regressions for this path (reset on improvement).
  regressionStreak @5 :UInt32;

  # Cumulative consecutive improvements for this path (reset on regression).
  improvementStreak @6 :UInt32;

  # Wall-clock UNIX epoch milliseconds at event creation.
  timestampMs @7 :UInt64;
}

# ============================================================================
# Subscription filter
# ============================================================================

struct SubscriptionFilter {
  # When non-empty, only events whose filePath starts with one of these
  # prefixes are delivered. Empty list means "all paths".
  pathPrefixes @0 :List(Text);

  # Minimum absolute delta magnitude. Events with |delta| < threshold are
  # filtered out. 0.0 means "deliver everything".
  minAbsDelta @1 :Float32;

  # When true, deliver only events whose outcome is Regression.
  regressionsOnly @2 :Bool;
}

# ============================================================================
# RPC interfaces
# ============================================================================

# Client-provided capability invoked by the server for each delivered event.
# Implement this on the consumer side and pass it to
# `GeneratorHealth.subscribe`. When the client drops the returned handle
# the server stops calling `onDelta`.
interface HealthDeltaListener {
  # Called once per event that matches the subscription filter.
  # The server does not wait for this promise to resolve — slow listeners
  # will NOT block the producer (events are fanned out non-blocking).
  onDelta @0 (event :HealthDeltaEvent) -> ();
}

# Lifecycle handle for an active subscription. Dropping the capability
# reference on the client side terminates the subscription server-side.
interface SubscriptionHandle {
  # Explicit early cancellation. Alternative to dropping the reference.
  close @0 () -> ();
}

# The main entry point for the `holon:generator@0.1.0` capability.
interface GeneratorHealth {
  # Subscribe to the live stream of health delta events. The server
  # invokes `listener.onDelta(event)` for every event matching `filter`
  # that is published while the returned handle remains live.
  subscribe @0 (
    listener :HealthDeltaListener,
    filter :SubscriptionFilter,
  ) -> (handle :SubscriptionHandle);

  # Return aggregated counter snapshot (non-subscribed fallback).
  # Cheap call — reads touring-hooks gate_metrics atomics via subprocess
  # shim. Latency ~50 ms (subprocess fork+exec). Prefer `subscribe` for
  # real-time consumers.
  getCounters @1 () -> (counters :HealthDeltaCounters);

  # Spec version string this server implements.
  specVersion @2 () -> (version :Text);
}

# Aggregate counter snapshot — mirrors the 6 record_health_delta_* atomics
# in touring-hooks::shared::gate_metrics.
struct HealthDeltaCounters {
  recordCount @0 :UInt64;
  computeCount @1 :UInt64;
  regressionCount @2 :UInt64;
  improvementCount @3 :UInt64;
  streakAlertCount @4 :UInt64;
  recoveryCount @5 :UInt64;
  # Pass-through of the gate_metrics alert_threshold (default 3).
  alertThreshold @6 :UInt32;
}
