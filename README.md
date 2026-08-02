<div align="center">

# Touring

**The agentic code harness — open, typed, auditable.**

Code intelligence, execution sandboxing, and quality gates for AI coding agents.
One Rust binary. Local-first. No telemetry.

[![CI](https://github.com/gabrielgadea/touring/actions/workflows/ci.yml/badge.svg)](https://github.com/gabrielgadea/touring/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/gabrielgadea/touring?logo=github)](https://github.com/gabrielgadea/touring/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![MSRV](https://img.shields.io/badge/rustc-1.85%2B-orange?logo=rust)](https://www.rust-lang.org)

</div>

---

## Why

Coding agents fail in three predictable ways: they **edit code they don't
understand**, they **run commands nobody sandboxed**, and they **claim success
without proof**.

Touring is the layer that closes all three. Before an agent edits a file, it can
ask what breaks (`blast radius`). Before it runs a command, the gateway
classifies and sandboxes it. Before it declares victory, quality gates say yes
or no — fail-closed on security.

It is infrastructure, not an assistant: everything is a CLI call that returns
typed JSON in under 10 ms, so agents *and* humans use the same surface.

## Install

Download the release, verify the checksum, run it:

```bash
VERSION=v30.3.0
BASE="https://github.com/gabrielgadea/touring/releases/download/$VERSION"

curl -fsSLO "$BASE/touring-x86_64-unknown-linux-gnu.tar.gz"
curl -fsSLO "$BASE/touring-x86_64-unknown-linux-gnu.tar.gz.sha256"
sha256sum -c touring-x86_64-unknown-linux-gnu.tar.gz.sha256   # verify before running

tar -xzf touring-x86_64-unknown-linux-gnu.tar.gz
install -Dm755 touring ~/.local/bin/touring
```

Every release ships a **SHA-256 checksum** and a **CycloneDX SBOM**.
Prefer building it yourself? See [Building and testing](#building-and-testing).

## Usage

First queries — each returns JSON, each runs in milliseconds:

```bash
touring doctor                 # health of every component
touring index rebuild          # one-time index for the current project
touring ast meta src/main.rs   # blast radius, quality score, fan-in/fan-out
touring ast blast src/main.rs  # what breaks if I change this file
touring index find MyStruct    # does this symbol exist, and who consumes it
```

The rule that pays for itself: **`ast meta` before every edit.** A file with a
blast radius of 40 is not a file you refactor casually.

## Building and testing

Building from source requires Rust **1.85+**:

```bash
git clone https://github.com/gabrielgadea/touring
cd touring
cargo build --release          # binary at target/release/touring
```

Running the test suite — roughly 15.5k tests across the workspace:

```bash
cargo test --workspace                    # full suite
cargo test -p touring-quality             # a single crate
cargo clippy --workspace -- -D warnings   # the lint gate CI enforces
```

## What's inside

Touring is a Cargo workspace of **42 crates** (~654k lines of Rust across 1,714
files, ~15.5k tests). The capabilities below are the ones you actually invoke.

### Code intelligence

| Capability | Command | What it gives you |
|---|---|---|
| **Blast radius** | `touring ast blast <file>` | Full dependency tree — what breaks if you touch this |
| **File triage** | `touring ast meta <file>` | Blast radius, quality score, cognitive score, fan-in/fan-out |
| **Symbol lookup** | `touring index find <sym>` | Exact definition + consumers, indexed and constant-time |
| **Transitive impact** | `touring wiring impact <sym>` | BFS over consumers — the real reach of a change |
| **Cycle detection** | `touring wiring cycles` | Tarjan SCC over the dependency graph |
| **Orphan detection** | `touring wiring orphans` | Public symbols nobody consumes (dead surface) |
| **Full-text search** | `touring tantivy search "<q>"` | BM25-ranked, plus fuzzy and autocomplete |
| **Rust semantics** | `touring ast rust-semantic <f>` | Generics, trait bounds, lifetimes, unsafe/async counts (via `syn`) |
| **Structural search** | `touring ast grep <f> <pat>` | AST-level match *and* rewrite, polyglot |

Parsing covers **13 languages** via tree-sitter — Rust, Python, TypeScript,
JavaScript, Go, Java, Bash, HTML, CSS, JSON, YAML, TOML, Markdown.

### Execution safety

Every code-bearing action can be routed through the **Code Execution Gateway**
— a typestate pipeline `X0..X9` (capture → classify → sandbox → gate → learn)
where the sandbox and verification stages are structurally impossible to skip.

```bash
touring run --lang python --code 'print(sum(range(100)))'
```

Capabilities are **deny-by-default** (Deno-style). The sandbox runs 12
languages: Python, JS/Node, TS/Bun, Ruby, Go, Rust, Perl, R, Elixir, PHP,
Bash/sh. Credential environment variables are never on the allowlist.

### Quality gates — 50 dimensions

`touring-quality` scores code across **50 dimensions** in four families:
architecture (12), security & performance (13), testing & documentation (13),
ecosystem & operations (12).

**Six are fail-closed BLOCK gates** — they stop the write, they don't warn:

| Gate | Dimension |
|---|---|
| `F2.1` | OWASP Top 10 |
| `F2.4` | Cryptographic issues |
| `F2.5` | Dependency CVEs |
| `F2.6` | Configuration security |
| `F4.3` | Deprecated APIs |
| `F4.5` | Package management |

The remaining 44 are advisory (13 WARN, 31 log-only).

```bash
touring-quality list                                  # the full catalog
touring-quality check --gate F2.1 --target src/       # one gate
touring-quality score src/ --workspace --fail-below 0.80
```

### Learning and memory

Touring gets better at *your* codebase across sessions, rather than restarting
cold every time:

- **Persistent memory** — `touring memory store` / `recall`, with semantic tiers,
  FTS5 + vector recall. Lessons and gotchas survive the session.
- **Reinforcement learning** — a LinUCB contextual bandit (8 arms) learns which
  tools pay off in which context; `touring learning reward` closes the loop.
- **Gotcha database** — `touring gotcha match <file>` surfaces pitfalls
  previously hit on that file.
- **Drift detection** — `touring evolution drift` flags when learned patterns
  stop matching reality.

### Agent workflows

- **`touring adw`** — durable declarative agent workflows. Typed nodes
  (`code` / `agent` / `gate` / `loop` / `human`), an fsync'd journal, and
  `--resume-run` replay that survives `kill -9`.
- **`touring factory`** — routes a ticket to the right workflow,
  deterministic rules first, RL-fed.
- **`touring explore`** — loop-until-dry exploration with a persistent
  multi-lens ledger and an explicit convergence contract.
- **`touring generate`** — code generation through a typestate pipeline
  (Draft → Verified → Rendered → Speculated → Committed) with **36 templates**.
  Symbols are verified to exist *before* generation, not after.

### Agent integration

**218 lifecycle hooks** cover the full tool-use surface (`PreToolUse`,
`PostToolUse`, `Session*`, `Task*`), letting a harness enrich or gate every
action an agent takes. Touring also ships an **MCP server**, so any
MCP-compatible client can reach the same intelligence.

## Architecture

Four layers, acyclic — no back edges:

```
┌──────────────────────────────────────────────────────────┐
│  L4  SURFACE          CLI · MCP · hooks · dashboards     │
├──────────────────────────────────────────────────────────┤
│  L3  ORCHESTRATION    workflows · agents · tasks · RL     │
├──────────────────────────────────────────────────────────┤
│  L2  INTELLIGENCE     code · storage · reasoning · learn  │
├──────────────────────────────────────────────────────────┤
│  L1  INFRASTRUCTURE   types · errors · config · identity  │
└──────────────────────────────────────────────────────────┘
```

A daemon holds the index in memory; the CLI is a thin client over a Unix
socket, which is why read-only queries answer in under 10 ms.

## What Touring is not

- **Not an editor.** It is harness infrastructure — pair it with yours.
- **Not a hosted service.** Local-first. Nothing leaves your machine.
- **Not an agent.** It is the substrate agents run *on*.
- **Not a linter.** Linters check style; Touring answers structural questions
  (*what breaks, who consumes this, is this safe to run*).

## Documentation

| | |
|---|---|
| 🚀 [Getting started](docs/landing/index.md) | Install and first queries |
| 🏛️ [Constitution v8.0](docs/CONSTITUTION-v8.md) | The master contract |
| 🏗️ [Architecture](docs/explanation/architecture.md) | Layers, daemon, security model |
| 🍳 [How-to guides](docs/how-to/) | Task-oriented recipes |
| 🔒 [Security policy](SECURITY.md) | Reporting vulnerabilities |
| 🤝 [Contributing](CONTRIBUTING.md) | Development setup and gates |

## Contributing

Contributions are welcome. Every PR runs the same gates the project holds
itself to: `cargo check`, `clippy -D warnings`, tests, e2e, dependency cycles,
orphan symbols, and TDG scoring.

Start with [CONTRIBUTING.md](CONTRIBUTING.md); please also read the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

Licensed under either of

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you state otherwise, any contribution you intentionally
submit for inclusion shall be dual licensed as above, without additional terms.
