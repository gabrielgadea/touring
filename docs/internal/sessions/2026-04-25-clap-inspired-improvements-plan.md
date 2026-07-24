# Plano: Clap-Inspired CLI Enhancements for Touring

> **Data**: 2026-04-25 | **Autor**: TACO (Claude Code) | **Status**: DRAFT
> **Versão**: v1.0 | **Prioridade**: P1-P5 conforme ranking de impacto/esforço

---

## Objetivo

Potencializar a CLI do Touring (41 subcommands em `touring-server/src/cli/`) adotando padrões
provenientes do ecossistema Clap (clap, clap_derive, clap_builder). Cinco gaps foram identificados
e analisados na sessão 2026-04-25 via Context7 (2.495 snippets, 154 exemplos). O plano cobre todas
as 5 melhorias com deliverables atômicos, sizes estimados, dependências explícitas e riscos.

**Resultado esperado**: CLI Touring com sugestões de typo (did you mean), derive-based dispatch,
help templating DRY, TypedValueParser pipeline unificado, e feature gate modularity.

**Metric gates** (TACO GATE mandatory antes de aceitar cada fase):
- `cargo check --workspace` exit code 0
- `cargo test -p touring-server` 100% PASS (baseline: 502 tests)
- `cargo clippy -p touring-server -- -D warnings` 0 warnings
- Zero regressions em `touring e2e -j`

---

## Contexto da Análise

### Touring CLI现状 (Ground Truth verificado)

| Dimensão | Valor |
|---------|-------|
| Subcommands CLI | 45 arquivos em `touring-server/src/cli/` |
| Handler files | 12 `cli_handlers*.rs` em `touring-hooks/src/` |
| Dispatch mechanism | `match subcommand.as_str()` manual em `cli/mod.rs:263-274` (~400 LOC) |
| Help output | Strings literais hardcoded em cada handler |
| Error suggestions | ZERO — apenas mensagens genéricas |
| ValueParser | Pattern-based via `pre_tool_validator.rs` (regex/dangerous patterns) |
| Feature gates | Monolítico — todas features em `default` em `touring-server/Cargo.toml` |
| Daemon health | 1.097.890 símbolos indexados, 204.474 orphans, EMA 0.53 |

### Clap patterns identificados (Context7, 2026-04-25)

| Pattern Clap | Aplicação Touring | Gap |
|---|---|---|
| `#[derive(Subcommand)]` enum dispatch | Elimina match arms manuais | P2 |
| `ValueParser` composable (range, enum, NonEmpty, custom fn) | Typed parser pipeline com validação baked-in | P4 |
| `help_template` com placeholders (`{name}`, `{usage}`, `{all-args}`) | DRY em 45 arquivos | P3 |
| Error com `suggest` (did you mean) via edit distance | UX de erro premium | P1 |
| Feature gates modulares (`derive`, `builder`, `wrap-help`) | Builds otimizados, compile times menores | P5 |
| `ArgGroup` para flags mutuamente exclusivas | Wiring flags (`--json`, `--detailed`, `--explain`) agrupadas | P3 |
| `Styles::styled()` com ANSI customization | Help theming consistente | P3 |
| `external_subcommand` passthrough | Generators Touring invocam subprocessos sem API formal | P1 |

### Dependência externa reutilizável

**`stringzilla` já integrado** (Wave 2026-04-25, T2.1):
- `sz_edit_distance` (~2125× mais rápido que brute-force Vec)
- `stringzilla::hash` AES-NI como blake3 pre-filter
- `simd_edit_distance::Compute::edit_distance` SIMD-accelerated
- Destination: `touring-analysis/src/quality/` (BkTreeFuzzyAdapter)

Para P1 (suggest), usar `stringzilla::simd_edit_distance` como quick-filter
(mesma estratégia do `fast_content_hash` em `touring-analysis/quality/fast_hash.rs`).

---

## Deliverables

### D1 — Módulo `suggest` para correção de typos

**Deliverable**: `touring-core/src/error/suggest.rs` — módulo standalone com
função `suggest_similar(input, candidates, threshold) -> Option<String>`.

**Inspiração**: Clap error engine (did you mean 'X'?) + Clap suggestion built on
edit distance sobre valores válidos.

**API proposta**:

```rust
// touring-core/src/error/suggest.rs

use stringzilla::simd_edit_distance::Compute;

/// Given an input token and a list of valid candidates, return the closest
/// match using SIMD-accelerated edit distance.
///
/// Returns `None` if no candidate is within `threshold` edits of `input`.
pub fn suggest_similar<'a>(
    input: &str,
    candidates: &'a [&str],
    threshold: usize,
) -> Option<&'a str> {
    let mut best: (usize, &'a str) = (usize::MAX, "");
    for candidate in candidates {
        let dist = Compute::edit_distance(input, candidate);
        if dist < best.0 && dist <= threshold {
            best = (dist, *candidate);
        }
    }
    if best.0 == usize::MAX {
        None
    } else {
        Some(best.1)
    }
}

/// Suggest a subcommand name given a malformed input.
pub fn suggest_subcommand(input: &str, subcommands: &[&str]) -> Option<&str> {
    suggest_similar(input, subcommands, 3)
}

/// Suggest an argument name given a malformed flag.
pub fn suggest_argument(input: &str, arguments: &[&str]) -> Option<&str> {
    suggest_similar(input, arguments, 2)
}
```

**Localização**: `touring-core/src/error/suggest.rs` (NOVO)
**Testes**: 8 unit tests em `touring-core/tests/suggest.rs`
**Dependências**: `stringzilla` (já em `touring-analysis` dev-deps, promover para
`touring-core` dependency via `simd-fuzzy` feature existente)
**Tamanho**: S (~150 LOC + tests)
**Refinement level**: L1 (bugfix-like, feature additive pura)
**Riscos**: LOW — purely additive, zero alterações em código existente

**Validação**:
```bash
cargo test -p touring-core -- suggest
cargo check -p touring-core
```

---

### D2 — Derive-based subcommand dispatch

**Deliverable**: Refactor de `touring-server/src/cli/mod.rs` para usar
`#[derive(Subcommand)]` enum — elimina ~400 LOC de `match` arms manuais.

**Inspiração**: Clap `#[derive(Subcommand)]` + `#[command(subcommand)]` derive macros.

**Estado atual** (ground truth verificado — `cli/mod.rs:263-274`):

```rust
match subcommand.as_str() {
    "ast" => ast::run(args)?,
    "wiring" => wiring::run(args)?,
    "index" => index::run(args)?,
    // ...40+ arms similares
}
```

**Arquitetura proposta**:

```rust
// touring-server/src/cli/mod.rs

use clap::{Parser, Subcommand};

// ── Subcommand enum (replace 40+ match arms) ──────────────────────────────

#[derive(Debug, Subcommand)]
enum CliSubcommand {
    /// Run E2E health check
    E2e,
    /// Analyze AST
    Ast {
        #[arg(short, long)]
        depth: Option<String>,
        file: Option<String>,
    },
    /// Wiring analysis
    Wiring {
        #[arg(long)]
        orphans: bool,
        #[arg(long)]
        audit: bool,
        #[arg(long)]
        cycles: bool,
    },
    /// ...todos os 41 subcommands com suas Args
}

// ── Unified run dispatcher (replace match) ────────────────────────────────

fn run(args: &[String]) -> anyhow::Result<()> {
    let subcmd = CliSubcommand::try_parse_from(args)?;
    match subcmd {
        CliSubcommand::E2e => e2e::run(&[])?,
        CliSubcommand::Ast { depth, file } => ast::run_depth_file(depth, file)?,
        CliSubcommand::Wiring { orphans, audit, cycles } => wiring::run_wiring(orphans, audit, cycles)?,
        // ...
    }
    Ok(())
}
```

**Benefício colateral**: compile-time verification de todos os subcommands —
se um subcommand falta no enum, o compilador alerta (missing match arm safety).

**Tamanho**: XL (~400 LOC refatorados, 41 subcommands mapeados)
**Refinement level**: L3 (refactoring, mesmo comportamento, full test suite)
**Dependências**: D1 (sugestão integrada no enum derive error handling)
**Riscos**:
- MEDIUM/HIGH — 41 subcommands com argumentos diferentes precisam mapear para
  o enum sem perder funcionalidade existente
- Mitigação: fase 1 (scout) identifica TODOS os argumentos de cada subcommand
  antes de refatorar; fase 4.5 (audit) valida cada mapping

**Validação**:
```bash
cargo build -p touring-server
./target/debug/touring --help  # verifica help output
./target/debug/touring ast --help
./target/debug/touring wiring --help
# ... todos os 41 subcommands
cargo test -p touring-server
cargo clippy -p touring-server -- -D warnings
```

---

### D3 — Help template system

**Deliverable**: Macro `#[touring_help_template(TEMPLATE)]` + 45 arquivos migrados.

**Inspiração**: Clap `help_template("{name} ({version})\n{usage}\n{all-args}")`.

**Estado atual**: help strings hardcoded em cada handler:

```rust
// touring-server/src/cli/wiring.rs (exemplo típico)
.about("Show wiring information")
.long_about("Shows orphan symbols, module connections...")
```

**Arquitetura proposta**:

```rust
// touring-core/src/cli/help.rs

pub const DEFAULT_HELP_TEMPLATE: &str = r#"
{name} {version}
{author}
{about-with-newline}
{usage-heading} {usage}

{all-args}
"#;

// Macro para aplicar template
#[proc_macro_derive(TouringCommand, attributes(help_template, example))]
pub fn derive_command(input: TokenStream) -> TokenStream { ... }
```

**Tamanho**: M (~200 LOC macro, migração de 45 arquivos)
**Refinement level**: L2 (optimization, mesma funcionalidade)
**Dependências**: D2 (derivação do enum precisa do help template)
**Riscos**: LOW — purely visual, não muda comportamento funcional

**Validação**:
```bash
cargo build -p touring-server
./target/debug/touring --help | diff - <(antes do refactor)
touring e2e -j  # composite score não degrada
```

---

### D4 — TypedValueParser pipeline

**Deliverable**: Trait `TypedValueParser` em `touring-core/src/cli/parser.rs`
unificando `pre_tool_validator.rs` + validação em handlers.

**Inspiração**: Clap `value_parser` composable chain:
`value_parser(RangedI64ValueParser::new().range(1..))`, `NonEmptyStringValueParser`,
`PossibleValuesParser`, `Fn(&str) -> Result<T, E>`.

**Arquitetura proposta**:

```rust
// touring-core/src/cli/parser.rs

/// Typed value parser with validation baked in.
pub trait TypedValueParser: Send + Sync {
    type Output: Send + Sync + 'static;
    fn parse(&self, s: &str) -> Result<Self::Output, String>;
    fn validate(&self, val: &Self::Output) -> Result<(), ValidateError> { Ok(()) }
}

// Implementations
pub struct NonEmptyString;
pub struct Ranged<T: Parseable>(T::Range);
pub struct Enum<E: ValueEnum>(E);
pub struct Custom<F, T>(F) where F: Fn(&str) -> Result<T, String>;
pub struct FilePath;
pub struct GlobPattern;

// Composition
impl<T: TypedValueParser, U: TypedValueParser<Output = T::Output>> TypedValueParser for (T, U) {
    // chain: parse THEN validate
}
```

**Migração**: `pre_tool_validator.rs` (~50 patterns) migram para composable
`TypedValueParser` chain — validators se tornam `pub fn`s exportadas.

**Tamanho**: L (~600 LOC novo trait + impls + migração)
**Refinement level**: L3 (refactoring estrutural)
**Dependências**: D2 (dispatch refactored precisa do parser pipeline)
**Riscos**:
- MEDIUM — validators existentes têm edge cases; cada pattern migrado
  precisa de teste de regressão
- Mitigação: 100+ E2E tests existentes cobrem validators

**Validação**:
```bash
cargo test -p touring-hooks -- pre_tool
cargo test -p touring-server
touring pre-edit  # score >= 0.8 (validação unchanged)
```

---

### D5 — Feature gate modularity

**Deliverable**: Split de `touring-server` em crates menores com feature gates ortogonais.

**Inspiração**: Clap feature gates (`derive`, `builder`, `unstable-doc` separados).

**Arquitetura proposta**:

```toml
# touring-server/Cargo.toml — CRATES ATUAL
[package]
name = "touring-server"

# PROPOSTO — split em:
# touring-cli-core = parser + dispatch + base subcommands (zero heavy deps)
# touring-cli-derive = #[touring_subcommand] derive macros
# touring-cli-completion = shell completion generation
# touring-cli-profile = heap-dump + flamegraph (heavy deps: jemalloc_pprof, pprof)
# touring-cli-wasm = WASM plugin runner
```

**Tamanho**: XL (~1000 LOC restructure, Cargo.toml rewrite)
**Refinement level**: L4 (arquitetura, mudança fundamental de estrutura)
**Dependências**: D2 + D3 + D4 (prefere esperar pipeline estabilizar)
**Riscos**:
- HIGH — quebra compatibilidade de API pública de touring-server
- Mitigação: manter `touring-server` como meta-crate que re-exporta tudo,
  feature gates apenas controlam compile-time, não runtime API
- Timeline: Gabriel decide se benefício de compile time justifica risco

**Validação**:
```bash
cargo build -p touring-server --no-default-features --features derive
cargo build -p touring-server --features wasm-plugins,l7b-alpha
# ambos builds devem compilar e tests passarem
```

---

## Timeline e Sequenciamento

```
SEMANA 1
├── D1 (S, ~150 LOC)
│   ├── touring-core/src/error/suggest.rs (NOVO)
│   ├── touring-core/tests/suggest.rs (8 tests)
│   └── cargo test -p touring-core -- suggest
│
└── SCOUT D2 — mapear todos os 41 subcommands + argumentos
    ├── Ler cada arquivo em touring-server/src/cli/*.rs
    ├── Listar todos os args de cada subcommand
    └── Output: tabela de mapping subcommand → args

SEMANA 2-3
└── D2 (XL, L3 refactor — MAIS ARRISCADO)
    ├── Criar enum CliSubcommand com todos os 41 variants
    ├── Mapear cada variant → handler::run() existente
    ├── Testar cada subcommand manualmente (--help)
    ├── 502 tests passarem
    └── clippy zero

SEMANA 4
├── D1 (sugestão integrada no error layer do D2)
│   └── Clap error → suggest_similar() injetado
│
└── D3 (M, migração help template)
    ├── Criar #[touring_help_template] macro
    ├── Migrar wiring.rs, ast.rs, index.rs como piloto (3 arquivos)
    ├── Verificar --help output unchanged
    └── Migrar restantes (42 arquivos)

SEMANA 5-6
└── D4 (L, TypedValueParser pipeline)
    ├── Trait TypedValueParser em touring-core/src/cli/parser.rs
    ├── Implementations: NonEmpty, Ranged, Enum, Custom, FilePath, Glob
    ├── Migração de pre_tool_validator.rs (~50 patterns)
    └── Full test suite

SEMANA 7-8 (se Gabriel aprovar)
└── D5 (XL, feature gate restructure)
    ├── touring-cli-core crate (parser + dispatch)
    ├── touring-cli-derive crate (#[touring_subcommand] macros)
    ├── touring-cli-profile crate (heap-dump, flamegraph)
    ├── touring-server meta-crate re-export
    └── CI jobs para cada feature gate combination
```

---

## Riscos e Mitigações

| ID | Risco | Prob | Impacto | Mitigação |
|----|-------|------|---------|-----------|
| R1 | D2 dispatch refactor quebra 41 subcommands | HIGH | HIGH | Scout semana 1 identifica todos args; auditor valida mapping antes de engineer |
| R2 | D4 TypedValueParser migração perde edge cases | MEDIUM | MEDIUM | 100+ E2E tests existentes como regression suite; TDD em cada pattern migrado |
| R3 | D5 feature gate quebra API pública | HIGH | MEDIUM | Meta-crate wrapper preserva API; feature gates só controlam compile |
| R4 | D3 help template muda formatação visual | LOW | LOW | Diff test antes/depois de --help; composite e2e score unchanged |
| R5 | D1 suggest retorna false positive (distância > threshold) | LOW | LOW | Threshold configurável (default 3 para subcommands, 2 para args) |

---

## Autovalidação do Plano

| Critério | Verificação |
|----------|-------------|
| **Cada deliverable é atômico e independently shippable** | D1-D5 cada um é módulo standalone, pode ser выпущен sem os outros |
| **Dependências são explícitas e acíclicas** | D1→D2→D3→D4→D5 (linear), D5 opcional após D2-D4 |
| **Estimativas são realistas (T-shirt sizing)** | D1=S, D3=M, D2+D4=XL+L, D5=XL — sizing baseado em LOC e scope |
| **Riscos têm mitigação** | Todos os 5 riscos têm mitigação concreta listada |
| **TACO GATE mandatory em cada fase** | cargo check + test + clippy + e2e antes de aceitar cada deliverable |

---

## Priorização Final

| Prioridade | ID | Razão |
|------------|-----|-------|
| **1** (primeiro) | D1 — suggest | Zero dependências, zero regressão, arquivo único, impacto UX imediato |
| **2** | D2 — derive dispatch | Remove 400 LOC boilerplate, type safety em compile-time |
| **3** | D3 — help template | DRY em 45 arquivos, melhoria visual sem risco funcional |
| **4** | D4 — ValueParser pipeline | Unifica validator layer, prepara terreno para D5 |
| **5** (último) | D5 — feature gates | Esforço mais alto, risco mais alto, benefício mais distante |

**Recomendação Gabriel**: iniciar por **D1** — menor risco, maior aprendizado
do codebase error layer. Após D1 completo, D2 pode rodar em paralelo
(independent scope, engineers diferentes se necessário).

---

## Aprovações

| Fase | Gate | Critério |
|------|------|----------|
| FASE 0 | 🚦 | `cargo check --workspace` EXIT:0 + `touring doctor -j` OK |
| D1 completo | 🚦 | `cargo test -p touring-core -- suggest` 8/8 PASS + 0 clippy |
| D2 completo | 🚦 | `cargo test -p touring-server` 502/502 PASS + clippy 0 |
| D3 completo | 🚦 | `./target/debug/touring --help` output unchanged + e2e PASS |
| D4 completo | 🚦 | `cargo test -p touring-hooks -- pre_tool` 100% PASS |
| D5 completo | 🚦 | Build com cada feature gate combination OK + 502 tests PASS |

---

*Plano gerado via TACO Phase Protocol v6.2 — FASE 4 (decompose) com
subtasks D1-D5, sequenciamento otimizado para risco mínimo e impacto máximo.*
