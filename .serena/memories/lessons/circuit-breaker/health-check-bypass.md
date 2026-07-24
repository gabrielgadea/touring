# lesson:circuit-breaker:health-check-bypass

**Tier**: reference  
**Entry type**: lesson  
**Date**: 2026-03-29

## Rule (P2 fix)

Health check endpoints (`--daemon-health`) must NEVER flow through circuit breaker state.

## Root Cause

`try_daemon_request` called `is_open()` which blocked when the project circuit was open, and `record_failure` accumulated failures from health checks themselves (threshold=4).

## Fix

`try_daemon_health_direct()` connects directly to the Unix socket bypassing the circuit entirely — no `is_open()` check, no `record_failure`/`record_success` calls.

## Result

- Health checks work even when the project circuit is open.
- Health check failures do NOT pollute circuit state.
- Invariant: exit 0 always preserved.
- Regression: 3918 tests passing.

## Invariant

**NEVER route diagnostic probes through circuit protection logic.**
