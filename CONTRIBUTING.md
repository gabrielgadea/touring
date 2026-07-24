# Contributing to Touring

Thanks for your interest. Touring is an agentic code harness written in Rust; it
holds itself to the same bar it enforces on the code it analyzes. This guide
describes the contribution workflow and the quality gates every change must pass.

## Quality Gates (must pass before a change is accepted)

```bash
cargo check --workspace                          # 0 errors
cargo clippy --workspace -- -D warnings          # 0 warnings (deny-all)
cargo test --workspace --exclude touring-python  # green (pyo3 crate excluded by design)
python3 docs/sync_metrics.py --check             # ARCHITECTURE.md must not drift
touring doctor -j                                # daemon/index health
```

A change that grows a file past the file-size budget, introduces a new `unwrap`
in a gateway/L1 path, or leaves a new orphan `pub` symbol (`touring wiring orphans -j`)
will be flagged.

## Principles

1. **No drift** — docs are generated from the index where possible. If you change
   crate count / LOC / test count, `docs/sync_metrics.py --check` keeps
   `ARCHITECTURE.md` honest.
2. **No regression** — run the full test suite before and after; structural
   changes must keep `composite_health` non-decreasing.
3. **Atomic, reviewable changes** — one logical transformation per change, with a
   test that proves it (see the `n07_*` regression tests in
   `crates/touring-hooks/src/cli_handlers_semantics.rs` for the expected style).
4. **Fail loud** — prefer `?`/typed errors over `.unwrap()`/`panic!` in runtime paths.

## Code Style

- Rust 2021, formatted (`cargo fmt` / `touring ast format-rust`).
- New public items carry doc comments; surface crates aim for `#![deny(missing_docs)]`.
- Tests live next to the code (`#[cfg(test)] mod tests`) or under `tests/`.

## Reporting & Proposing

- Bugs: minimal reproduction + `touring --version` + platform.
- Larger changes: open a short proposal first (an RFC under `docs/rfcs/` for
  cross-cutting or extension-contract work). The extension contract itself is
  tracked as RFC-006 (planned).

## Security

Security issues follow a private disclosure process — see `SECURITY.md`. Do not
file them as public issues.
