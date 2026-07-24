## ARCHITECTURAL FIX (2026-03-29): Session ID Propagation to Circuit Breaker

### Problem
Session ID from Claude Code hook JSON was never propagated to `DaemonRequest` — it was hardcoded as `None`.
This meant the session-level circuit breaker dimension was dead code: all requests used a single global fault domain.

### Fix Applied
In `main.rs` (~line 92-103), extract session_id from the hook input JSON:

```rust
let session_id = input
    .get("session_id")
    .and_then(|v| v.as_str())
    .map(str::to_string);
```

Then pass it to `record_failure()` and `record_success()`:
```rust
cb.record_failure(req.session_id.as_deref());
cb.record_success(req.session_id.as_deref());
```

### Impact
Sessions now have independent fault isolation — a degraded session (e.g. a runaway agent loop) doesn't trip the circuit breaker for other sessions. Previously a single bad session could degrade the entire daemon.

### Rule
When a struct field exists for session-scoped context but is never populated at the call site, treat it as a P1 bug — the feature appears to exist but provides no protection.

### All tests: 3918 passing after fix.
