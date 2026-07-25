<!--
  TACO-cross-audit report template — Phase 7 fills the {{PLACEHOLDERS}}.
  Discipline: every claim carries an EXECUTED command and its output.
  No executed output -> the row is UNVERIFIED, never a pass.
-->
# Cross-Audit Report — {{TARGET}}

> Date: {{DATE}} · Auditor: TACO-cross-audit · Daemon: {{ok | degraded}}

## Verdict

**{{FULFILLS | PARTIAL | VIOLATES}}** — {{one-line summary of whether the code
fulfills its documented purpose as an orchestration instrument}}

## Phase 1 — Relation map

{{Module graph, public API surface, entry points. High-blast-radius files
(blast_radius > 10) listed first. Source: touring ast workspace-info / blast /
wiring chains.}}

## Phase 2 — Purpose audit

| Unit | Documented purpose | Verdict | Evidence (command + output) |
|------|--------------------|---------|------------------------------|
| {{symbol/module}} | {{purpose}} | {{FULFILLS\|PARTIAL\|VIOLATES\|UNDOCUMENTED}} | {{what was run + what it showed}} |

## Phase 3 — Debt scan

{{scan_debt.py output — every dead-code / allow(unused) / TODO / FIXME /
unimplemented! / pending marker. Each row paired with its Phase 5 resolution.}}

| Debt item | Location | Resolution |
|-----------|----------|------------|
| {{kind}} | {{file:line}} | {{wired / implemented / fixed — see Phase 5}} |

## Phase 4 — Harmony

{{harmony_map.py output — orphan pub symbols, dependency cycles, modules scoring
< 1.0, unused imports. Each paired with its resolution.}}

## Phase 5 — Fixes & potentialization

Every correction took the potentializing direction (REGRA #0) — integrate, wire,
implement; never delete capability.

| Finding | Fix applied | How it potentialized |
|---------|-------------|----------------------|
| {{finding}} | {{fix, via Edit tool}} | {{the capability it realized}} |

## Phase 6 — E2E proof

The integrated flow, exercised end to end and **run**:

```
{{the actual test run — command invoked, pass/fail lines, exit code}}
```

## Invariants

| Invariant | Proven by (command) | Result |
|-----------|---------------------|--------|
| exit 0 always | {{command}} | {{exit code observed across normal + edge input}} |
| {{other invariant}} | {{command}} | {{output}} |

## Remaining UNVERIFIED

{{Anything that could not be executed — environment, missing dependency. Listed
honestly, never folded into a pass count. Empty section = nothing unverified.}}

## Persisted

- `touring memory store` — {{key}}
- `touring learning reward` — {{value + context}}
- `touring diary write taco-cross-audit` — {{entry}}
