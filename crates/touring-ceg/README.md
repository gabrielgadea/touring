# touring-ceg — Code Execution Gateway

The **X0..X9 execution-gating pipeline** plus the Deno-style deny-by-default
**capability model**, extracted from `touring-hooks` on 2026-06-10 (Fronteira 1
of the monolith decomposition — plan `recursive-cuddling-blossom`, Sessions A+B).

## Purpose

Every code-bearing action (a `Bash` command, a `ctx_execute` body, a generated
script) is driven through a typestate pipeline **before** any real execution:

| Stage | Name | What it does |
|-------|------|--------------|
| X0 | CAPTURE | Admit code-bearing tool calls (`gateway::capture`) |
| X1 | CLASSIFY | Surface + language detection (`gateway::classify`) |
| X2 | STATIC | AST forbidden-call detector (`gateway::static_stage`) |
| X3 | VGP | Symbol verification against the index (`gateway::vgp_stage`) |
| X3.5 | PROVE | Optional SMT claim check (`gateway::offensive_integration`) |
| X4 | PREDICT | Beta success-probability estimator (`gateway::predict`) |
| X5 | SANDBOX | Dry-run under a capability profile (`gateway::sandbox_stage`) |
| X6 | CAPABILITY-GATE | Deny-by-default capability match (`gateway::gate`) |
| X7 | DECISION | Composite score → Allow / Warn / Deny (`gateway::decision`) |
| X8 | SUPERVISED-EXEC | Real run under landlock + rlimit (`gateway::supervised`) |
| X9 | LEARN | RL reward + gotcha + drift reconciliation (`gateway::learn`) |

The `Execution<S>` typestate (`gateway::typestate`) makes X3 and X5
structurally unskippable.

## API entry points

- `gateway::pre_exec::run_gateway(tool, payload, intent, &GatewayDeps) -> Result<GatewayOutcome, GatewayError>` — the pure driver.
- `gateway::pre_exec::observe(tool, body)` — observe-only entry (counters, fail-open).
- `gateway::pre_exec::run_gateway_speculative(..)` — S-12 batch speculative driver.
- `capability::{Capability, CapabilityProfile, builtins::*}` — profile construction; `CmdScope` / `HostScope` / `KeyScope` / `PathScope` re-exported at the crate root.

## IoC contract (how X9 LEARN stays host-agnostic)

`gateway::learn` is generic over `CegRuntime` (supertrait of
`touring-contracts::LearnRuntime`). The host runtime implements the traits;
this crate never names a host type. The Claude Code **hook driver** lives in
the parent (`touring_hooks::ceg_adapter`) and maps `GatewayOutcome` to the
hook protocol.

## Consumers

`touring-hooks` re-exports `gateway` and `capability` at its root, so
`touring_hooks::gateway::*` (3 touring-server consumers, the `ceg_e2e` suite,
and every parent module) keeps resolving unchanged.

## Feature flags

- `txn_lock_enforcement` (default **off** here; **default-on** via the
  parent's forward `touring-hooks/txn_lock_enforcement = ["touring-ceg/txn_lock_enforcement"]`)
  — gates the `ExecPool` txn-permit defense-in-depth path (28 `cfg` sites).
  Both configurations compile and pass tests (506 with, 4 doctests either way).

## Caveats

- X5 defaults to `deferred_dry_run` (non-executing) — see the
  `gateway::pre_exec` module docs for the safety rationale.
- The staging area writes under `~/.claude/touring/ceg-staging/`
  (`gateway::staging`); GC via `gc_staging`.
- Safety invariant: **fail-open** — the gateway never blocks a session on its
  own malfunction; hard-deny requires `CEG_ENFORCE=1` at the hook driver.

## Build

```bash
# Default (txn_lock_enforcement OFF — matches standalone build)
cargo build -p touring-ceg

# With txn_lock_enforcement (ExecPool txn-permit defense-in-depth, 28 cfg sites)
cargo build -p touring-ceg --features txn_lock_enforcement

# Release
cargo build -p touring-ceg --release
```

The library crate name is `touring_ceg`. There is no standalone binary; the
crate is consumed by `touring-hooks` which re-exports `gateway` and
`capability` at its root.

## Tests

```bash
cargo test -p touring-ceg                            # 506 tests (default features)
cargo test -p touring-ceg --features txn_lock_enforcement  # same suite + txn paths
cargo clippy -p touring-ceg -- -D warnings
```

The `ceg_e2e` suite exercises the full X0..X9 pipeline end-to-end. Each
gateway stage (`capture`, `classify`, `static_stage`, `vgp_stage`, `predict`,
`sandbox_stage`, `gate`, `decision`, `supervised`, `learn`) carries its own
unit fixtures. Doctests cover the 4 built-in capability profiles.

## Contributing

`touring-ceg` co-evolves with its host adapter in `touring-hooks`
(`ceg_adapter`) and the operational reference in
`~/.claude/rules/code-execution-gateway.md`. When adding a new gateway stage
or capability kind: (1) add the module under `gateway/` or `capability/`, (2)
wire it into the typestate pipeline in `gateway/typestate.rs`, (3) update the
`GatewayDeps` / `GatewayOutcome` types in `gateway/pre_exec.rs`, (4) add
tp/fp test fixtures, (5) run
`touring-quality score crates/touring-ceg --fail-below 0.80`.

The crate must never name a concrete host type — X9 LEARN stays host-agnostic
via `CegRuntime` (supertrait of `touring-contracts::LearnRuntime`).

## License

Part of the Touring workspace; see the workspace root for licensing.
