# E2E Proof — Tests That Prove Integration

Phase 6 of the audit. The goal is not "tests exist" — it is **executed evidence
that the integrated flow fulfills its purpose**. A test written but not run
proves nothing.

## Table of contents

- [What an E2E proof is](#what-an-e2e-proof-is)
- [Structure of an integration E2E](#structure-of-an-integration-e2e)
- [Invariants to prove](#invariants-to-prove)
- [Per-language entry points](#per-language-entry-points)
- [Written vs proven](#written-vs-proven)

## What an E2E proof is

A unit test exercises one function with mocked surroundings. An E2E proof
exercises the **real flow across real components** from an entry point to a
result — the way the code is actually used as an orchestration instrument.

For a cross-audit, the E2E suite must cover: every entry point, the main success
flow end to end, and the purpose-implied edge cases. Each test names the purpose
it proves — "proves the import flow produces a valid record from a malformed CSV"
beats "test_import_2".

## Structure of an integration E2E

```
1. ARRANGE  — set up real (not mocked) inputs and a clean state
2. ACT      — invoke the entry point exactly as a real caller would
3. ASSERT   — the result matches the documented purpose, not just "no exception"
4. ASSERT   — the invariants held (exit code, no partial state, contracts)
5. CLEANUP  — leave no state behind (itself an invariant worth asserting)
```

The ASSERT on purpose is the one that distinguishes a proof from a smoke test:
assert the *result is correct*, not merely that the call returned.

## Invariants to prove

| Invariant | How to prove |
|-----------|--------------|
| exit 0 always (or the documented exit contract) | run the entry point across normal + edge input, show every exit code |
| no partial writes on the error path | inject a failure mid-flow, assert state is unchanged |
| idempotence (where claimed) | run twice, assert the second run changes nothing |
| interface contract at each seam | assert the data crossing A→B→C keeps every field/type the contract promises |
| integration | assert the *whole flow* produces the documented result, not each part in isolation |

## Per-language entry points

| Language | Run the suite with | Exit-code proof |
|----------|--------------------|-----------------|
| Rust | `cargo test --workspace` | `cargo run -- <args>; echo $?` |
| Python | `pytest` | `python3 entry.py <args>; echo $?` |
| TypeScript / JS | `npm test` / `vitest run` | `node entry.js <args>; echo $?` |
| Shell | `bats` / direct run | `bash entry.sh <args>; echo $?` |

`scripts/prove_invariants.py` runs the entry points and captures every exit
code; `touring e2e -j` gives the composite system-health score on top.

## Written vs proven

This is the line the audit must not blur:

- **Written** — the test file exists, the assertions are coded. Worth nothing on
  its own.
- **Proven** — the test was *run*, in this audit, and its output is shown in the
  Phase 7 report.

The report carries the actual run: the command, the pass/fail line, the exit
code. If a test could not be run (environment, missing dependency), it is marked
`UNVERIFIED` in the report — never folded into a pass count. The user asked to be
shown, in practice, that it works. Showing means the run is in the report.
