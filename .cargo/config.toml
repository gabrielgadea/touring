# Workspace-wide cargo configuration.
# Docs: https://doc.rust-lang.org/cargo/reference/config.html
#
# NOTE: `--cfg tokio_unstable` is REQUIRED by `console-subscriber`
# (tokio-console) for task instrumentation. It is ABI-stable for our
# workspace — Tokio's unstable flag gates new APIs, not breakage.

[build]
# ── sccache DISABLED for this workspace (2026-06-26) ──────────────────
# Empirical: sccache Rust hit-rate here = 2.29% (measured `sccache --show-stats`),
# NOT the 37-75% the dev-profile comment once assumed. Worse, it is a
# CORRECTNESS hazard, not just a slow cache:
#   - `bin`/`proc-macro` crates (the `touring` binary is a `bin`) are
#     non-cacheable and run the system linker anyway (mozilla/sccache,
#     docs/Rust.md → "crates invoking the system linker ... cannot be cached").
#   - "Procedural macros reading from the filesystem may not cache properly"
#     (same doc) — this workspace is saturated with proc-macros (serde/rkyv/
#     syn/tokio). On 2026-06-26 sccache served a STALE object: the build
#     completed (exit 0, "Compiling touring-server") yet the binary did NOT
#     contain the edited code → binary ≠ source, a silent ship.
# This OVERRIDES the global ~/.cargo/config.toml rustc-wrapper, routing rustc
# directly. The local incremental cache (see [profile.dev] incremental = true in
# Cargo.toml) is the correct cache for iterative Rust dev. sccache stays ON
# globally for other projects (C/C++ hit-rate 47%).
#
# `/usr/bin/env` is an IDENTITY wrapper — it execs its argument unchanged, so
# rustc runs exactly as it would unwrapped, while still displacing the global
# `rustc-wrapper = "sccache"`. It is a real program ON PURPOSE: the previous
# value, an empty string, also disabled sccache but made `cargo llvm-cov`
# assemble the command `"" rustc -vV` and die with "No such file or directory".
# So NO LCOV artifact ever existed, and F3.1 silently reported its
# `#[test]`-presence PROXY instead of real line coverage, workspace-wide
# (root-caused 2026-08-02). `cargo build` tolerated the empty value; llvm-cov
# did not. Verify both halves after touching this line:
#   cargo build -p touring-license -v 2>&1 | grep -o 'Running `[^ ]*'   # no sccache
#   cargo llvm-cov --lcov --output-path /tmp/x.info -p touring-license  # artifact
rustc-wrapper = "/usr/bin/env"
rustflags = [
    "--cfg", "tokio_unstable",
    "-C", "link-arg=-fuse-ld=gold",
]

# Wave 9 Fix1 (2026-04-26): `split-debuginfo` is a profile-level setting,
# not a build-level one — placing it under `[build]` triggers the
# "unused config key `build.split-debuginfo`" warning on cargo ≥1.93.
# Disk-optimization wave (2026-04-26): `split-debuginfo = "unpacked"` is
# now properly configured at workspace root `Cargo.toml` under `[profile.dev]`.

# ── gold linker (Linux x86_64) — fallback from mold due to stdc++ path issue ──────
# Disk-optimization wave (2026-04-26). mold 2.30.0 could not resolve libstdc++ at
# /usr/lib/x86_64-linux-gnu. Using gold (GNU ld 2.42) as reliable fallback.
# NOTE: overrides ~/.cargo/config.toml [target.x86_64-unknown-linux-gnu] to avoid merge
[target.x86_64-unknown-linux-gnu]
# Using gcc as linker to bypass mold/gold fuse-ld conflict from ~/.cargo/config.toml
linker = "gcc"
rustflags = [
    "--cfg", "tokio_unstable",
    # REGRA #21 (zero-tolerância) — pyo3 0.29+ needs libpython at link time for
    # tests that pull in pyo3 transitively (e.g. touring-bindings lib tests).
    # gcc only resolves this symbol if pyo3 is actually in the link graph, so
    # crates without pyo3 are unaffected. Baseline issue surfaced during W0
    # of harness-consolidation-master-plan-v3.
    "-C", "link-arg=-lpython3.12",
]

# wasm32 uses rust-lld which doesn't understand -fuse-ld=gold
[target.wasm32-unknown-unknown]
rustflags = [
    "--cfg", "tokio_unstable",
]
[alias]
# Run all tests via nextest using the CI profile.
test-ci = "nextest run --profile ci --workspace"
# Fast local test loop — default profile.
t = "nextest run --workspace"
# Coverage run — matches the CI gate in .github/workflows/ci.yml.
# GOTCHA (2026-08-02): run it as
#     RUSTC_WRAPPER= cargo cov
# The `rustc-wrapper = ""` above is understood by cargo as "no wrapper", but
# cargo-llvm-cov concatenates it into the rustc command line, yielding a leading
# space and a path that cannot exist:
#     could not execute process ` /home/.../bin/rustc -vV` (never executed)
# An empty RUSTC_WRAPPER in the environment takes precedence and fixes it while
# keeping sccache off. A cargo alias cannot carry env vars, hence this note.
cov = "llvm-cov --workspace --lcov --output-path lcov.info"
# Deny check — full audit pass.
deny-all = "deny --all-features check"
# Disk guard — prune only duplicate binary artifacts (keeps current build)
disk-clean = "!find target -maxdepth 3 -name 'touring-????????????????' -not -name '$(cat target/.touring-hash 2>/dev/null)' -type f -delete && find target -name '*.rlib' -type f -size +100M -delete"
# Show target directory size
disk-size = "du -sh target"
