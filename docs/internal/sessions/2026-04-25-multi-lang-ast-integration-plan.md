# Multi-Language AST Integration Plan for Touring

> **Date**: 2026-04-25 | **Author**: TACO (Touring Agentic Code Orchestrator)
> **Status**: PLANNED | **Priority**: P1 (tree-sitter) → P2 (pattern) → P3 (highlight) → P4 (index) → P5 (derive)
> **Estimated Duration**: 8-9 weeks sequential, 5-6 weeks with parallelism after P1

---

## Executive Summary

This document details a comprehensive plan to integrate multi-language AST capabilities into the Touring ecosystem by leveraging existing high-quality Rust crates: **ast-grep**, **parsel**, **syntect**, and **ast-index**. The goal is to extend Touring's current Rust-only parsing (via `syn`) to support Python, JavaScript, TypeScript, Go, and other languages, while adding pattern matching, syntax highlighting, and cross-language indexing capabilities.

**Key Differentiators to Protect:**
- Daemon actor pattern (mpsc 128 + panic-safe)
- RL loop LinUCB (8 arms × 25 dims, EMA 0.44)
- health_delta streak tracking
- wiring F1/F2 (BFS impact + Tarjan SCC cycles)
- query cache moka (4096 cap, 60s TTL)
- rkyv IPC (zero-copy default ON)
- VGP symbol verification
- TDG grades A+..F

**Not Change:**
- Touring daemon architecture
- Existing hook registry (172 hooks)
- Rust-only parsing core (syn fallback maintained)

---

## Crate Analysis

### 1. ast-grep (v0.42.1, Abr/2026) — 13.6k ★

| Aspect | Detail |
|--------|--------|
| **Function** | CLI tool for structural search, linting, and AST rewriting |
| **Stack** | Rust (94.3%) + tree-sitter + NAPI + Python bindings |
| **Languages** | 29+ via tree-sitter (JS, TS, Python, Rust, Go, etc.) |
| **Pattern Language** | `$VAR` (single node), `$NAME` (identifier), `$$$` (rest), `$ARG:expr` (typed) |
| **API** | CLI + Node.js NAPI + Python package |
| **Benchmark** | Context7 score: 73.6, High reputation |

**Key Pattern Example:**
```bash
# Search console.log with any argument
ast-grep --pattern 'console.log($MATCH)' --lang js

# Find functions with body
ast-grep --pattern 'fn $NAME($$$ARGS) { $$$BODY }' --lang rust

# YAML lint rules
# .ast-grep/rules/no-console-log.yml
id: no-console-log
message: Avoid using console.log in production
severity: warning
language: JavaScript
rule:
  pattern: console.log($$$)
fix: console.debug($$$)
```

**Relevance for Touring**: HIGHEST — tree-sitter foundation + pattern language

---

### 2. parsel (v0.16.0)

| Aspect | Detail |
|--------|--------|
| **Function** | Zero-code parser generator via derive macros |
| **MSRV** | 1.77.0 |
| **License** | MIT |
| **Dependencies** | ordered-float, parsel_derive, proc-macro2, quote, syn |

**Derive Example:**
```rust
#[derive(Parse, ToTokens)]
enum Value {
    Null(kw::null),
    Bool(LitBool),
    Array(#[parsel(recursive)] Bracket<Punctuated<Value, Comma>>),
}
```

**Key Features:**
- `LeftAssoc` / `RightAssoc` for left-recursive grammars without infinite loops
- `#[parsel(recursive)]` breaks constraint cycles in recursive ASTs
- Helper modules: Bracket, Brace, Punctuated, Paren
- Span info for error reporting

**Relevance for Touring**: MEDIUM — useful for declarative grammar parsing

---

### 3. syntect (v5.3.0, Set/2025) — 2.3k ★, 12.2k dependents

| Aspect | Detail |
|--------|--------|
| **Function** | Syntax highlighting via Sublime Text definitions |
| **Stack** | Rust pure (fancy-regex mode) or C-regex |
| **Performance** | 9200 lines/247kb in 600ms (vs Atom 6s, VS Code ~2s) |
| **Startup** | ~23ms with pre-compiled binary dump |
| **Notable Users** | bat, xi-editor (Google), Zola, delta, mdcat, Typst |

**API Modules:**
- `syntect::easy::HighlightLines` — simple highlighting
- `syntect::parsing::SyntaxSet` — syntax definitions
- `syntect::highlighting::{ThemeSet, Style}` — themes
- `syntect::html` — HTML output (inline styles or CSS classes)

**Output Formats:**
```rust
// Inline styles
highlighted_html_for_string(code, &ss, syntax, &theme)

// CSS classes
css_for_theme_with_class_style(theme, ClassStyle::Spaced)
```

**Relevance for Touring**: HIGH — syntax highlighting for docs and diff output

---

### 4. ast-index (defendend/claude-ast-index-search) — Benchmark 87.5

| Aspect | Detail |
|--------|--------|
| **Function** | Fast code search CLI for 29+ languages |
| **Stack** | Rust native |
| **Commands** | class, symbol, search, implementations, callers, outline, usages, imports |

**Workflow:**
```bash
ast-index rebuild                    # Index project
ast-index stats                     # Index statistics
ast-index search "struct" --kind class
ast-index implementations "Repository"
ast-index outline "src/lib.rs"
ast-index callers "handle_request"
```

**Relevance for Touring**: HIGH — multi-language index pattern

---

### 5. python-ast / ts-typed-ast

**Status**: ❌ NOT FOUND on crates.io — crate does not exist or is abandoned

---

## Architecture Design

### Current touring-ast Structure

```
crates/touring-ast/src/
├── lib.rs
├── rust_semantic.rs    # syn-based, Rust only
├── surgery.rs          # format_rust_code (prettyplease)
├── wiring.rs           # WorkspaceInfo (cargo_metadata)
├── error.rs           # TracedAstError
└── tests/
    ├── parametric_multilang.rs  (58 rstest cases)
    ├── latency_p99_guard.rs     (5 P99 guards)
    └── benches/
```

### Proposed New Structure

```
crates/touring-ast/src/
├── tree_sitter/           # NEW: P1 - multi-language parsing
│   ├── mod.rs             # LazyLock parser pool, exports
│   ├── parser.rs          # Parser struct wrapping tree_sitter::Parser
│   ├── query.rs           # tree-sitter::Query pattern matching
│   ├── languages.rs       # Language loaders (python, js, go, ts, etc)
│   └── fallback.rs       # syn-based fallback when lang == Rust
│
├── pattern/               # NEW: P2 - ast-grep pattern matching
│   ├── mod.rs             # Pattern struct with metavariables
│   ├── matcher.rs         # $VAR, $NAME, $$$ matching logic
│   ├── finder.rs          # Find all matches in AST
│   └── replacer.rs        # AST rewriting with captures
│
├── highlighting/          # NEW: P3 - syntect integration
│   ├── mod.rs             # HighlightOutput struct
│   ├── html.rs            # Inline styles output
│   ├── ansi.rs            # 24-bit terminal ANSI colors
│   └── theme.rs           # Theme loading
│
├── index/                 # NEW: P4 - multi-language index
│   ├── mod.rs             # MultiLangSymbolIndex struct
│   ├── multi_lang.rs       # Cross-language symbol index
│   └── rust_index.rs       # Keep existing Rust index (separate namespace)
│
├── parsing/               # NEW: P5 - parsel derive
│   └── derive.rs          # #[touring_parse] derive macro
│
├── rust_semantic.rs       # EXISTING - syn-based, Rust only
├── surgery.rs             # EXISTING - format_rust_code
├── wiring.rs              # EXISTING - WorkspaceInfo
└── error.rs               # EXISTING - TracedAstError
```

### Feature Gates (Cargo.toml)

```toml
[features]
default = []
tree-sitter-parsing = ["dep:tree-sitter", "dep:tree-sitter-python", "dep:tree-sitter-javascript", "dep:tree-sitter-typescript", "dep:tree-sitter-go"]
pattern-matching = ["tree-sitter-parsing"]  # depends on tree-sitter
highlighting = ["dep:syntect"]              # syntect is heavy, default OFF
multi-lang-index = ["tree-sitter-parsing"]  # depends on tree-sitter

# Optional dependencies
tree-sitter = { version = "0.24", optional = true }
tree-sitter-python = { version = "0.23", optional = true }
tree-sitter-javascript = { version = "0.23", optional = true }
tree-sitter-typescript = { version = "0.23", optional = true }
tree-sitter-go = { version = "0.23", optional = true }
syntect = { version = "5.3", optional = true }
parsel = { version = "0.16", optional = true }
```

### CLI Integration (touring-server/src/cli/ast.rs)

```rust
// Existing subcommands (preserve):
// ast rust-semantic <file.rs>
// ast format-rust <file.rs>
// ast workspace-info [<dir>]
// ast tdg <file>
// ast tdg <file> --grade-only

// New subcommands:
enum AstSubcommand {
    // ... existing ...
    Parse {
        lang: String,
        file: PathBuf,
    },
    Pattern {
        pattern: String,
        lang: String,
        file: PathBuf,
    },
    Highlight {
        theme: Option<String>,
        lang: String,
        file: PathBuf,
    },
    Index {
        lang: String,
        path: PathBuf,
    },
}
```

---

## Deliverables

### Phase 1: tree-sitter Parser (P1) — XL (3-4 weeks)

| ID | Deliverable | Size | Description |
|----|--------------|------|-------------|
| D1.1 | Cargo dependencies | S | Add tree-sitter crates to touring-ast/Cargo.toml (feature-gated) |
| D1.2 | Parser module | M | Create tree_sitter/mod.rs with LazyLock parser pool |
| D1.3 | Parser wrapper | M | Implement tree_sitter/parser.rs - Parser struct |
| D1.4 | Language loaders | M | Implement tree_sitter/languages.rs (python, js, go, ts) |
| D1.5 | Rust fallback | S | Implement tree_sitter/fallback.rs - syn fallback for Rust |
| D1.6 | Query wrapper | M | Implement tree_sitter/query.rs - tree-sitter::Query |
| D1.7 | CLI handler | M | Add `ast parse --lang <lang> <file>` |
| D1.8 | Unit tests | M | 30+ cases for Python, JS, Go, TypeScript parsing |
| D1.9 | P99 guards | S | hdrhistogram <50ms parse, <200ms query |

**Acceptance Criteria:**
- `touring ast parse --lang python file.py` returns parsed AST
- `touring ast parse --lang javascript file.js` returns parsed AST
- `touring ast parse --lang rust file.rs` uses syn fallback (same as before)
- All tests pass with feature-gated compilation

---

### Phase 2: Pattern Matching Engine (P2) — M (2 weeks) [PARALLEL after D1.4]

| ID | Deliverable | Size | Description |
|----|--------------|------|-------------|
| D2.1 | Pattern struct | M | Create pattern/mod.rs with metavariables |
| D2.2 | Matcher logic | M | Implement pattern/matcher.rs ($VAR, $NAME, $$$) |
| D2.3 | Finder | M | Implement pattern/finder.rs - find all matches |
| D2.4 | Replacer | M | Implement pattern/replacer.rs - AST rewriting |
| D2.5 | CLI handler | M | Add `ast pattern --pattern <p> --lang <lang> <file>` |
| D2.6 | Wiring integration | S | Find consumers of pattern-matched symbols |
| D2.7 | Unit tests | M | 20+ cases across languages |
| D2.8 | E2E test | S | Find all console.log calls across Python/JS/Rust |

**Acceptance Criteria:**
- `touring ast pattern --pattern 'console.log($ARG)' --lang js file.js` finds all matches
- Pattern replacement works (capture groups → rewrite)
- Integration with touring-wiring for symbol consumers

---

### Phase 3: Syntax Highlighting (P3) — S (1 week) [PARALLEL with P2]

| ID | Deliverable | Size | Description |
|----|--------------|------|-------------|
| D3.1 | Syntect integration | S | Add syntect to Cargo.toml (feature-gated, default OFF) |
| D3.2 | HighlightOutput struct | S | Create highlighting/mod.rs |
| D3.3 | HTML output | S | Implement highlighting/html.rs (inline styles) |
| D3.4 | ANSI output | S | Implement highlighting/ansi.rs (24-bit terminal) |
| D3.5 | CLI handler | S | Add `ast highlight --theme <name> --lang <lang> <file>` |
| D3.6 | Generator integration | S | Artifact preview with syntax colors |
| D3.7 | Unit tests | S | Output validation, ANSI escape sequences |
| D3.8 | Performance test | S | 10k lines <100ms |

**Acceptance Criteria:**
- `touring ast highlight --theme base16-ocean.dark --lang rust file.rs` produces HTML
- ANSI output renders correctly in terminal
- Performance: 10k lines highlighted in <100ms

---

### Phase 4: Multi-Language Index (P4) — M (2 weeks) [PARALLEL with P2]

| ID | Deliverable | Size | Description |
|----|--------------|------|-------------|
| D4.1 | MultiLangIndex struct | M | Create index/multi_lang.rs |
| D4.2 | Rust index separation | S | Keep existing Rust index separate |
| D4.3 | Python extraction | M | Implement Python symbol extraction |
| D4.4 | JS/TS extraction | M | Implement JS/TS symbol extraction |
| D4.5 | Go extraction | M | Implement Go symbol extraction |
| D4.6 | CLI handlers | M | `ast index --lang <lang> <path>` + `touring index find --lang` |
| D4.7 | Cross-language find | M | Rust symbol → Python usages |
| D4.8 | Unit tests | M | Symbol extraction per language |
| D4.9 | Wiring integration | S | Cross-language impact analysis |

**Acceptance Criteria:**
- `touring index find User --lang python` finds Python class User
- `touring index find User --lang any` finds across all languages
- Cross-language: `touring wiring impact User --lang python --depth 3` works

---

### Phase 5: Derive Macro Parsing (P5) — S (1 week) [LOWEST PRIORITY]

| ID | Deliverable | Size | Description |
|----|--------------|------|-------------|
| D5.1 | parsel integration | S | Add parsel to Cargo.toml |
| D5.2 | Derive macro | S | Create parsing/derive.rs - #[touring_parse] |
| D5.3 | Grammar conversion | S | Grammar-to-parser conversion |
| D5.4 | Config parsing | S | YAML, TOML DSL parsing |
| D5.5 | Unit tests | S | Grammar parsing tests |

**Acceptance Criteria:**
- `#[touring_parse(grammar = "expression")]` generates Parse impl
- Config files (YAML, TOML) can be parsed

---

## Dependency DAG

```
P1 (tree-sitter) ──────────────────────────────────────────────────────────┐
    │                                                                    │
    ├── D1.1 ─ D1.2 ─ D1.3 ─ D1.4 ─ D1.5 ─ D1.6 ─ D1.7 ─ D1.8 ─ D1.9    │
    │                                                                    │
    ├─────────────────────────────────────────────────────────► P2       │
    │                                                             │       │
    │                                                             ├── D2.1-9
    │                                                             │
    ├─────────────────────────────► P3                             │
    │                                 │                            │
    │                                 ├── D3.1-8                   │
    │                                                             │
    ├────────────────────► P4 ───────────────────────────────────────┐   │
    │                       │                                          │   │
    │                       ├── D4.1-9                                 │   │
    │                                                                    │
    └─────────────────────────────────────────────────────────────────┐   │
                                                                        │   │
                                                                        ▼   │
                                                                  P5 (lowest) │
                                                                  D5.1-5     │
```

**Parallel Execution After P1:**
- P2, P3, P4, P5 can run in parallel once P1 (D1.4) is complete
- P3 (highlighting) needs D1.3 (parsing exists)
- P4 (multi-lang index) needs D1.4 (language loaders) + D1.6 (query)
- P5 (derive) is independent but lowest priority

---

## Timeline

| Week | Phase | Deliverables | Notes |
|------|-------|--------------|-------|
| 1-2 | P1 | D1.1-D1.9 | tree-sitter foundation |
| 3-4 | P2 | D2.1-D2.8 | Pattern matching engine |
| 3-5 | P3 | D3.1-D3.8 | Syntax highlighting |
| 3-5 | P4 | D4.1-D4.9 | Multi-language index |
| 5-6 | P5 | D5.1-D5.5 | Derive macro (lowest priority) |

**Total Duration:**
- Sequential: 9 weeks
- With parallelism (P2-P5 after P1): 6 weeks

---

## Risk Analysis

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **R1**: tree-sitter bindings unstable | MEDIUM | HIGH | Feature-gated, fallback to syn, trait abstraction (ParserTrait) |
| **R2**: syntect binary bloat | HIGH | MEDIUM | `highlighting` default OFF, LazyLock for SyntaxSet |
| **R3**: ast-grep API breaking | LOW | MEDIUM | Pin version, implement pattern matching ourselves |
| **R4**: Performance regression | HIGH | HIGH | hdrhistogram P99 guards, benchmark before/after |
| **R5**: Conflict with existing index | LOW | MEDIUM | Separate namespace (multi_lang_index), compose pattern |
| **R6**: MSRV compatibility | MEDIUM | LOW | All crates require Rust 1.65-1.77, compatible |
| **R7**: Cross-crate dependency conflicts | LOW | HIGH | Centralize via package.metadata.tree-sitter |

**No blocking risks.** All risks have mitigations.

---

## Quality Gates

### Pre-Implementation
- [ ] `cargo check --workspace` exits 0
- [ ] `touring doctor -j` shows 5/5 healthy
- [ ] Existing test suite: 4007+ tests pass

### Per-Deliverable
- [ ] Feature-gated compilation (cargo check --features ...)
- [ ] Unit tests: 80%+ coverage on new modules
- [ ] hdrhistogram P99 guards on all hot paths
- [ ] Zero new clippy warnings

### Post-Implementation
- [ ] `touring e2e -j` composite >= 0.8
- [ ] Binary size increase <10% with all features enabled
- [ ] Performance: `touring ast parse --lang python` <50ms for 1k LOC

---

## Commands Reference

### New CLI Commands

```bash
# Phase 1: tree-sitter parsing
touring ast parse --lang python file.py        # Parse Python
touring ast parse --lang javascript file.js    # Parse JavaScript
touring ast parse --lang typescript file.ts    # Parse TypeScript
touring ast parse --lang go file.go            # Parse Go
touring ast parse --lang rust file.rs          # Uses syn fallback

# Phase 2: Pattern matching
touring ast pattern --pattern 'console.log($ARG)' --lang js file.js
touring ast pattern --pattern 'fn $NAME($$$ARGS) { $$$BODY }' --lang rust file.rs

# Phase 3: Syntax highlighting
touring ast highlight --theme base16-ocean.dark --lang rust file.rs
touring ast highlight --theme InspiredGitHub --lang python file.py

# Phase 4: Multi-language index
touring index find User --lang python           # Find Python symbols
touring index find User --lang any             # Find across all languages
touring wiring impact User --lang python --depth 3

# Phase 5: Derive parsing
touring parse --grammar expression file.expr   # Using #[touring_parse]
```

### Existing Commands (Preserved)

```bash
touring ast rust-semantic file.rs              # Unchanged
touring ast format-rust file.rs                 # Unchanged
touring ast workspace-info                    # Unchanged
touring ast tdg file                           # Unchanged
```

---

## File Locations

| File | Purpose |
|------|---------|
| `crates/touring-ast/src/tree_sitter/` | P1 tree-sitter module |
| `crates/touring-ast/src/pattern/` | P2 pattern matching module |
| `crates/touring-ast/src/highlighting/` | P3 syntect module |
| `crates/touring-ast/src/index/multi_lang.rs` | P4 multi-language index |
| `crates/touring-ast/src/parsing/derive.rs` | P5 parsel derive |
| `crates/touring-server/src/cli/ast.rs` | CLI handlers |
| `~/.claude/rust/docs/YYYY-MM-DD-multi-lang-ast-analysis.md` | This document |

---

## Success Metrics

| Metric | Target |
|--------|--------|
| New commands working | `touring ast parse --lang python` returns parsed AST |
| Pattern matching | `touring ast pattern --pattern` finds all matches |
| Syntax highlighting | `touring ast highlight` produces HTML/ANSI output |
| Multi-language index | `touring index find --lang python` finds Python symbols |
| Existing tests | 4007+ tests still pass |
| Binary size | <10% increase with all features enabled |
| Performance | Parse 1k LOC <50ms, highlight 10k lines <100ms |
| P99 guards | All new hot paths <200ms |

---

## Appendix: Crate Versions

| Crate | Version | MSRV | License | Stars |
|-------|---------|------|---------|-------|
| ast-grep | 0.42.1 | 1.75+ | MIT | 13.6k |
| parsel | 0.16.0 | 1.77.0 | MIT | - |
| syntect | 5.3.0 | 1.77.0 | MIT | 2.3k |
| ast-index | - | 1.65+ | MIT | - |
| python-ast | NOT FOUND | - | - | - |
| ts-typed-ast | NOT FOUND | - | - | - |

---

## Appendix: Touring Current State (v4.15.0)

- **CLI Commands**: 70 (table) + 24 hooks
- **MCP Tools**: 88
- **Hook Registry**: 172
- **Tests**: 4007+ passing
- **Index symbols**: 38,815
- **Wiring orphans**: 9,106
- **EMA reward**: 0.44
- **Daemon**: 5/5 healthy

---

*Document generated by TACO (Touring Agentic Code Orchestrator) on 2026-04-25*
*Next action: Await Gabriel's approval to begin Phase 1 (tree-sitter) implementation*