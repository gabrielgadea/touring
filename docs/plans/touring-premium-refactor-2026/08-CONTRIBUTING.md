---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "contributing"
created: "2026-05-11"
---
# 08-CONTRIBUTING — How to Contribute to Touring Premium Refactor

> **Quem pode contribuir**: Apenas Gabriel Gadea (architect) e TACO (orchestrator) durante a
> refactor (W0-W11). Após W12 + 1.0.0 GA, contribuições externas via PR.

## 1. Dev setup (5 minutes)

```bash
# Pre-flight
rustup install 1.83
rustup default 1.83
rustup component add clippy rustfmt llvm-tools-preview

# Required tools
cargo install --locked cargo-deny cargo-machete cargo-mutants \
                        cargo-msrv cargo-semver-checks cargo-llvm-cov \
                        cargo-cyclonedx cargo-fuzz cargo-hack

# Touring local install
cd ~/.claude/rust
cargo build --release
ln -sf ~/.claude/rust/target/release/touring ~/.local/bin/touring
ln -sf ~/.claude/rust/target/release/touring-hook ~/.local/bin/touring-hook
ln -sf ~/.claude/rust/target/release/touring-daemon ~/.local/bin/touring-daemon

# Verify
touring doctor -j
touring status -j
```

## 2. Per-wave workflow

Para iniciar uma wave WX:

```bash
# 1. Read the wave file
cat docs/plans/touring-premium-refactor-2026/W<N>-*.md

# 2. Read the validator stub
cat scripts/touring_premium_refactor_2026/validate_W<N>.py

# 3. Create pre-wave snapshot (required for W3+, mandatory for W4/W6/W8/W12)
tar -czf docs/baselines/pre-W<N>.tar.gz crates/ Cargo.{toml,lock}

# 4. Implement subtasks W<N>.1, W<N>.2, ... in order
#    (blocking subtasks must complete before later ones)

# 5. After each subtask, validate:
cargo check --workspace
cargo test --workspace --no-fail-fast
cargo clippy --workspace -- -D warnings
touring wiring cycles --min-depth 2 --format json | jq '.cycle_count'

# 6. When all subtasks done, run wave validator:
python3 scripts/touring_premium_refactor_2026/validate_W<N>.py

# 7. If validator passes, run cross-audit smoke:
python3 scripts/touring_premium_refactor_2026/cross_audit_e2e.py

# 8. Persist learning:
touring memory store "wave:W<N>:completion-2026-MM-DD" \
  "Wave W<N> <name> completed. <summary>." --tier semantic
touring learning reward orchestrate 1.0 "W<N>-completion-success"

# 9. Document changes:
# - Update CHANGELOG.md with conventional commit-style entries
# - If new gotchas discovered, add via `touring gotcha add`
```

## 3. Non-negotiable quality gates

Every wave commit MUST pass:

| Gate | Command | Threshold |
|---|---|---|
| Type check | `cargo check --workspace` | exit 0 |
| Build test | `cargo test --workspace --no-run` | exit 0 |
| Unit tests | `cargo test --workspace --no-fail-fast` | 100% pass |
| Lints | `cargo clippy --workspace -- -D warnings` | clean |
| Cycles | `touring wiring cycles --min-depth 2` | monotonic non-increasing |
| Orphans | `touring wiring orphans -j` (REGRA #0) | new orphans = 0 |
| Doc | `cargo doc --workspace --no-deps --warnings-as-errors` | clean (from W13) |
| Supply | `cargo deny check && cargo audit && cargo machete` | clean (from W2) |
| Bench | criterion compare baseline | ≥ -5% (from W2) |
| Coverage | `cargo llvm-cov` per touched crate | ≥ 20% (from W11) |
| Mutation | `cargo mutants` per touched crate | ≥ 80% (from W11) |
| Memory | `touring memory store` per wave | ≥ 1 lesson |
| RL | `touring learning reward` per wave | ≥ 1 reward |

## 4. PR template (post-1.0)

```markdown
## Description

<One sentence: what this PR does>

## Wave / Subtask (if applicable)

- Wave: W<N>
- Subtask: W<N>.<M>

## Quality gates (check all)

- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace` 100% pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles` cycle count non-increasing
- [ ] `touring wiring orphans` new orphans = 0 (REGRA #0)
- [ ] Bench regression ≥ -5% (criterion compare)
- [ ] Test ratio ≥ 20% for touched crates
- [ ] CHANGELOG.md entry added
- [ ] Memory lesson persisted

## VGP — Verified Generation Protocol

- [ ] V1: Schema extracted from real source (no inferred fields)
- [ ] V2: Each `struct.field` verified via `touring index find`
- [ ] V3: Blast radius assessed for symbols with ≥ 2 callers
- [ ] V4: VGP cache hits documented

## DISCOVER protocol

- [ ] tantivy search done
- [ ] wiring impact assessed
- [ ] ast blast checked
- [ ] memory recall consulted

## Risks introduced

<Any new risks? Update 05-RISKS.md if needed.>

## Rollback plan

<How to undo this change if it breaks something downstream?>
```

## 5. Commit message convention

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `chore`, `bench`.
Scope: crate name (touring-foundation, touring-code, etc.) or `workspace`.
Footer: `BREAKING CHANGE: ...` if semver major.

Example:
```
refactor(touring-code): absorve touring-ast-polyglot via parser-ast-grep feature

Move 769 LOC from touring-ast-polyglot/src/ to touring-code/src/parsers/ast_grep/.
Feature `parser-ast-grep` (opt-in) gates the dependency on ast-grep crate.
Re-export shim `pub use touring_code::polyglot::*` mantém compat por 2 versões.

Validates W4.3. Passes:
- cargo check --workspace exit 0
- cargo bench parser-ast-grep delta < 5%
- touring wiring cycles monotonic non-increasing

Lesson stored: `wave:W4:ast-polyglot-fusion-2026-MM-DD` (tier semantic).
```

## 6. Code style

- **Rust**: rustfmt default + clippy::pedantic + clippy::nursery
- **Python (scripts)**: ruff format + pyright strict; no unused imports
- **No `unwrap()` in production code** (only in tests with `// SAFETY:` justification)
- **All errors via `thiserror`** + `Result<T, Error>`
- **`#![warn(missing_docs)]`** strict (from W13)
- **Conventional commit messages** (semver-relevant)

## 7. Forbidden operations (REGRA #11 + REGRA #14)

- ❌ `git stash` (destroyed 162 modules em 06/04/2026)
- ❌ `git reset --hard` (sem snapshot prévio)
- ❌ `rm -rf target/` (use safe-clean.sh)
- ❌ Edit em arquivo de código com blast_radius > 10 sem pre-edit gate
- ❌ `Write` tool em `.rs/.py/.ts/.tsx/.go` (use taco-forge perfect-create / perfect-edit)
- ❌ `cat > foo.rs <<EOF` heredoc (hook bloqueia)
- ❌ `sed -i` em arquivos de código (use perfect-edit --operation rewrite)

## 8. Approval matrix

| Action | Approval needed |
|---|---|
| Wave kickoff | Gabriel Gadea (architect) explicit approval |
| Subtask reordering | TACO (orchestrator) judgment (within wave) |
| Bench gate failure → continue anyway | Gabriel Gadea (architect) explicit override |
| Cycle re-introduction | Gabriel Gadea (architect) ABSOLUTELY NEVER |
| Risk register update | TACO (orchestrator) can add; remove requires Gabriel |
| ADR amendment | Gabriel Gadea (architect) only |

## 9. References

- Master constitution: `~/.claude/CLAUDE.md`
- taco-forge canonical: `~/.claude/rules/taco-forge-canonical-workflows.md`
- VP-Scout chains: `~/.claude/rules/VP-Scout.md`
- Touring Decision Matrix: `~/.claude/rules/touring-decision-matrix.md`
- Per-wave details: `WX-*.md`
