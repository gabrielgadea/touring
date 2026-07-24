# Fifth Synergy Wave — `touring ast highlight` Command + syntect Module

**Date**: 2026-04-26 | **Session**: TACO L4+ | **Skill**: Touring v4.17.0

## Objetivo

Análise profunda de 5 crates crates.io (python-ast, ts-typed-ast, ast-grep-py, syntect, parsel)
+ extração de insights + implementação das integrações de alto valor para Touring.

## Análise de Crates (FASE 1 — VP-Scout 3 scouts paralelos)

| Crate | Decisão | Razão (VP-Scout chains) |
|-------|---------|-------------------------|
| `syntect 5.3` | **INTEGRATE** | Zero conflict (onig 6.5.1 já no lockfile via candle-transformers); ativa rendering de código colorido em terminal |
| `python-ast 1.0.2` | **SKIP** | Hard conflict pyo3 0.24↔0.25 (workspace-wide blocker) + redundante (tree-sitter-python já parsea Python) + requer CPython runtime |
| `ts-typed-ast 0.1.0` | **SKIP** | Hard conflict tree-sitter 0.24↔0.25 (afetaria 11 grammar crates) + abandonment risk (única release 1 ano atrás, 0% docs, 701 downloads) |
| `ast-grep-py 0.33.1` | **SKIP** | Python bindings via PyO3; workspace já consome `ast-grep-core 0.36.0` natively em touring-ast-polyglot — redundância arquitetural |
| `parsel 0.16` | **SKIP** | Proc-macro-only (só parsea Rust token streams); 22 meses sem commit; zero use case no workspace (regex já cobre monetary_parser, FromStr cobre enum variants) |

**Total**: 4 SKIPs (FALSE_POSITIVES preventidos pelo VP-Scout) + 1 INTEGRATE.

## Sumário Executivo

| ID | Task | Arquivos | Testes |
|----|------|----------|--------|
| T1 | `syntect = "5.3"` workspace dep | `Cargo.toml`, `crates/touring-server/Cargo.toml` | — |
| T2 | Módulo `cli/highlight.rs` | `crates/touring-server/src/cli/highlight.rs` (novo, 230 LOC) | 8 |
| T3 | CLI command `touring ast highlight` | `crates/touring-server/src/cli/{ast.rs, mod.rs}` | — |
| Fix | Import órfão `rusqlite::params` (potencialização REGRA #0) | `crates/touring-hooks/src/hook_decompose_bridge.rs` | — |
| **TOTAL** | | **5 arquivos, 1 novo módulo** | **8 testes** |

## Resultados FASE 6

- `cargo check --workspace`: EXIT:0
- `cargo build -p touring-server`: 1m 18s OK
- `touring-core` --lib: **145 PASS, 0 failed**
- `touring-hooks` --lib: **3224 PASS, 0 failed, 1 ignored**
- `touring-server` --lib: **408 PASS, 0 failed**
- `touring-server` --bin highlight: **8 PASS, 0 failed**
- Total: **3785 PASS, 0 failed** (3777 baseline + 8 highlight)
- Orphan baseline: **9106** (preservado)

## Detalhes por Task

### T1 — Workspace dep `syntect = "5.3"`

**Arquivos**: `Cargo.toml:88` + `crates/touring-server/Cargo.toml:210`

```toml
# workspace
syntect = { version = "5.3", default-features = false, features = ["default-syntaxes", "default-themes", "dump-load", "parsing", "regex-onig"] }
# touring-server
syntect = { workspace = true }
```

**Decisões críticas**:
- `regex-onig` (não `regex-fancy`): `onig 6.5.1` já transitivo via candle-transformers — zero nova
  compilação C. `regex-fancy` é git-dep upstream que conflita com `fancy-regex 0.13.0` do workspace.
- `default-features = false`: evita `yaml-load` + `plist-load` features (não precisamos parsing de
  Sublime syntax customizadas, apenas os defaults embutidos).
- Sem `html` por enquanto: HTML rendering pode entrar em wave futura quando houver web UI.

### T2 — Módulo `cli/highlight.rs`

**Arquivo**: `crates/touring-server/src/cli/highlight.rs`

API pública:

```rust
pub fn is_terminal_color_enabled() -> bool;
pub fn detect_lang_from_path(file_path: &str) -> &str;
pub fn highlight_to_ansi(code: &str, lang_hint: &str) -> String;
pub fn highlight_range_to_ansi(code: &str, lang_hint: &str, start: usize, end: usize) -> String;
pub fn run(args: &[String]) -> anyhow::Result<()>;
```

**Design**:
- `Lazy<SyntaxSet>` + `Lazy<ThemeSet>` — load defaults frozen (~5–20ms cold, zero subsequente)
- Tema padrão `"Solarized (dark)"` com fallback ao primeiro tema disponível (defensivo)
- `NO_COLOR` env var honored (per [no-color.org](https://no-color.org/))
- `IsTerminal::is_terminal` — JSON pipes / CI runners → plain text automaticamente
- Unknown languages → `find_syntax_plain_text()` fallback (nunca panica)
- Output sempre termina com `\x1b[0m` (ANSI reset) — evita color bleed
- Range version: 1-indexed inclusive, clamp out-of-range, retorna empty se start > end

8 unit tests:
1. `detect_lang_from_path_handles_common_extensions` (rs, py, tsx, css)
2. `detect_lang_from_path_falls_back_for_extensionless` (Makefile, "")
3. `highlight_to_ansi_returns_empty_for_empty_input`
4. `highlight_to_ansi_emits_ansi_escapes_for_known_lang`
5. `highlight_to_ansi_unknown_lang_falls_back_to_plain_text_syntax`
6. `highlight_range_to_ansi_clamps_out_of_range`
7. `highlight_range_to_ansi_preserves_only_selected_lines`
8. `is_terminal_color_enabled_respects_no_color_env_var`

### T3 — CLI command `touring ast highlight <file> [--lang N] [--start N] [--end N]`

**Arquivo**: `crates/touring-server/src/cli/ast.rs:256` (novo arm)

```rust
"highlight" => {
    return super::highlight::run(args);
}
```

`mod.rs` adiciona `pub mod highlight;`. Error message do unknown subcommand atualizado.

**Comportamento por flag combo** (matriz dentro de `run()`):
| `--start`/`--end` | TTY/NO_COLOR | Saída |
|-------------------|--------------|-------|
| Provided | TTY ON | `highlight_range_to_ansi` (range + cor) |
| Provided | TTY OFF / NO_COLOR | `extract_lines_plain` (range, sem cor) |
| Absent | TTY ON | `highlight_to_ansi` (full + cor) |
| Absent | TTY OFF / NO_COLOR | `code.clone()` (full, sem cor) |

### Fix Collateral — Import órfão `rusqlite::params`

**Arquivo**: `crates/touring-hooks/src/hook_decompose_bridge.rs:24`

Erro pré-existente descoberto durante FASE 6 (`cargo test -p touring-hooks --lib` falhava).
Fix:
```rust
+use rusqlite::params;
```

REGRA #0 (Potencialização) aplicada — encontrar é corrigir.

## VP-Scout False Positives Avoided (5 totais)

| FP | Detecção | Evidência |
|----|----------|-----------|
| `python-ast provides unique Python AST features` | Chain 3 | `touring-ast/Cargo.toml` linha 12: `tree-sitter-python = { workspace = true }` + touring-python `PyAstSymbol` + `extract_symbols` already exposed |
| `pyo3 version conflict é resolvable` | Dep Conflict | Cargo.lock pinned 0.24.2; python-ast requer ^0.25 (semver major break) |
| `ts-typed-ast complementa touring-ast` | Dep Conflict | tree-sitter 0.24 vs ^0.25; afetaria 11 grammar crates |
| `ast-grep-py extende polyglot` | Chain 3 + Homonimia | `touring-ast-polyglot/Cargo.toml`: `ast-grep-core = "=0.36.0"` direct |
| `parsel parsea DSLs/queries Touring` | Chain 3 | parsel só parsea Rust token streams; `monetary_parser.rs` usa regex (461 LOC, incompatível com token-stream model) |

## Lições Aprendidas

1. **Análise de crates via 3 scouts paralelos**: VP-Scout aplicado a 5 crates em FASE 1 evitou 4 FPs
   antes de qualquer linha de código. Saving: ~3 dias de trabalho desperdiçado em integrações inviáveis.
2. **Discovery > assumption**: Scout B inicialmente identificou `cli_ast_skeleton/overview/find` como
   sites de integração para syntect, mas verificação via `touring ast skeleton <file>` mostrou que esses
   commands retornam APENAS metadata JSON, não code bodies. PIVOT salvou implementação errada.
3. **Standalone CLI command > intrusive integration**: criar `touring ast highlight` como utility command
   standalone é mais reusável e testável que tentar wire em todos os comandos existentes.
4. **`--start/--end` precisam respeitar NO_COLOR também**: bug detectado durante smoke test pós-build —
   matriz de decisão completa (Some/None × TTY ON/OFF) é necessária.
5. **Lockfile transitive deps são oportunidades grátis**: onig 6.5.1 já no lockfile = zero compilação
   C nova. termtree também (Wave 4). miette também (Wave 4). Pattern: sempre check Cargo.lock antes
   de assumir que adicionar dep "nova" custa.

## Touring CLI Changes

- **Novo**: `touring ast highlight <file> [--lang N] [--start N] [--end N]` — pure-library command
  (sem daemon), respeita NO_COLOR + isatty, output ANSI 24-bit ou plain text.
- **Total CLI commands**: 71 → 72.
- Help text atualizado em `cli/ast.rs:280`: `Use: ..., grep, highlight`.

## Deferred — v4.18.0

- **miette `SourceCode` integration**: estender RFC-100 `Diagnostic` da Wave 4 (T1 miette bridge) com
  `NamedSource` que retorna highlighted snippets via syntect. Requer adicionar `source: Option<Arc<NamedSource>>`
  + `span: Option<SourceSpan>` ao `Diagnostic` struct + `SpanResolver` trait.
- **pre_edit `code_snippets` coloring**: `context_compiler.rs` `CompactSummary.code_snippets: Vec<String>` — quando
  rendering for terminal display (não LLM injection), aplicar syntect. Nota: ANSI codes devem ser STRIPPED
  antes de enviar ao Claude (não consumir tokens com escape sequences).
- **`--theme <name>` flag**: permitir override do tema padrão Solarized. Útil para usuários com light terminals
  ou preferência de cor (base16 variants já bundled).
- **HTML rendering**: para futuro web UI / docs estáticos / blog.
- **bat-style line numbers + grid**: complemento visual quando full file é mostrado.
