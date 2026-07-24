# Support

Touring is research-grade infrastructure under active development. This file
explains where to get help and what to expect.

## Getting Help

1. **Documentation** — start at `docs/` (Diátaxis layout: `tutorial/`, `how-to/`,
   `explanation/`, `reference/`). `ARCHITECTURE.md` is the high-level map.
2. **Health first** — most "it doesn't work" cases are environment/daemon issues.
   Run `touring doctor -j` and `touring status -j` before reporting.
3. **Self-diagnosis** — `docs/sync_metrics.py` reports the live workspace metrics;
   `touring e2e -j` gives a composite system score.

## Maturity Expectations (honest)

Touring is currently **single-user, Claude-oriented infrastructure** maturing
toward a multi-model platform. Today this means:

- installation requires compiling the workspace (a prebuilt binary is on the
  roadmap — see the Master Plan, `docs/2026-06-04-touring-elite-masterplan.md`);
- multi-model providers beyond the current default are in progress (`LlmProvider`);
- the public extension contract (RFC-006) is planned, not yet stable.

## Reporting Issues

For bugs, include `touring --version`, platform, and a minimal reproduction.
For security issues, follow `SECURITY.md` (private disclosure) instead.
