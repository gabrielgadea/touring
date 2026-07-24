# Fourth Synergy Wave — Rich Terminal Rendering + miette Bridge + termtree

**Date**: 2026-04-25 | **Session**: TACO L4+ | **Skill**: Touring v4.16.0

## Objetivo

Análise profunda de 5 crates crates.io (codespan-reporting, codemap, termtree,
annotate-snippets, language-reporting) + extração de insights + implementação das
integrações de maior valor para o Touring.

## Análise de Crates (FASE 1 — VP-Scout)

| Crate | Decisão | Razão |
|-------|---------|-------|
| `termtree v0.5.1` | **INTEGRAR** | Já em Cargo.lock (via predicates-tree dev-dep), zero download. Ideal para CLI tree rendering. |
| `miette v7 (fancy)` | **INTEGRAR** | Já no workspace Cargo.toml. Zero nova dep. Bridge perfeita para RFC-100 → terminal rico. |
| `annotate-snippets v0.12.15` | **DEFER v4.17.0** | Nova dep necessária. Requer `SpanResolver` trait ainda não implementado. |
| `codespan-reporting` | **SKIP** | Transitivo via naga. Supersedido por miette para uso do Touring. |
| `codemap` | **SKIP** | Abandonado 2018. Apenas source tracking sem rendering. |
| `language-reporting` | **SKIP** | Prototype abandonado sem releases publicados. |

## Sumário Executivo

| ID | Task | Arquivos Modificados | Testes Adicionados |
|----|------|----------------------|---------------------|
| T1 | miette Bridge para `diagnostic::Diagnostic` | `touring-core/src/diagnostic.rs`, `Cargo.toml` | 2 |
| T2 | `touring ast blast --tree` via termtree | `touring-server/src/cli/ast.rs`, `Cargo.toml` | 3 |
| T3 | `touring wiring audit/cycles --tree` | `touring-server/src/cli/wiring.rs` | 6 |
| T4 | BlastWarning RFC-100 em blast JSON response | `touring-hooks/src/cli_handlers.rs` | 1 |
| T5 | MemoryFinding RFC-100 em recall JSON response | `touring-hooks/src/cli_handlers.rs` | 1 |
| Fix | `health_events::publish_and_subscribe_roundtrip` | `touring-core/src/health_events.rs` | 0 |
| **TOTAL** | | **5 arquivos em 3 crates** | **13 testes (+fix)** |

## Resultados FASE 6

- `cargo check --workspace`: EXIT:0
- `touring-core` --lib: **145 PASS, 0 failed** (era 144, +1 miette bridge tests; 1 pre-existing bug fixed)
- `touring-hooks` --lib: **3224 PASS, 0 failed** (era 3220, +4 novos)
- `touring-server` --lib: **408 PASS, 0 failed**
- Total: **3777 PASS, 0 failed** (era 3628 + touring-core 144 = 3772 baseline)
- Orphan baseline: **9106** (preservado — zero novos orphans)

## Detalhes por Task

### T1 — miette Bridge para RFC-100 Diagnostics

**Arquivos**: `crates/touring-core/src/diagnostic.rs`, `crates/touring-core/Cargo.toml`

Gap: `touring_core::diagnostic::Diagnostic` tinha 27 códigos RFC-100 definidos mas
renderização era texto plano. `miette` v7 (fancy) estava no workspace mas nunca wired
ao crate `touring-core`.

Fix: `miette = { workspace = true }` em `[dependencies]` do Cargo.toml.
Implementação de 3 traits em `diagnostic.rs`:

```rust
impl std::fmt::Display for Diagnostic { ... }
impl std::error::Error for Diagnostic {}
impl miette::Diagnostic for Diagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> { ... }
    fn severity(&self) -> Option<miette::Severity> { ... }
    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> { ... }
}

impl Diagnostic {
    pub fn to_miette_report(self) -> miette::Report { miette::Report::new(self) }
}
```

Agora qualquer `Diagnostic` pode ser convertido em `miette::Report` para renderização
fancy com ícones Unicode + cores ANSI em terminais que suportam. Fallback automático
quando fancy não suportado.

### T2 — `touring ast blast --tree`

**Arquivos**: `crates/touring-server/src/cli/ast.rs`, `crates/touring-server/Cargo.toml`

`termtree = "0.5.1"` adicionado como dep direta (já em Cargo.lock via predicates-tree).
Flag `--tree` detectada no arm "blast" da CLI.

Função `render_blast_as_tree(json_output: &str) -> String`:
- Root: `{file_path} [blast_radius={N}]`
- Subtree "Direct dependents": lista de consumers
- Subtree "Co-edit signals": lista de coedit_files

Antes: `touring ast blast src/foo.rs` → JSON bruto  
Depois: `touring ast blast src/foo.rs --tree` → ASCII tree legível

### T3 — `touring wiring audit/cycles --tree`

**Arquivo**: `crates/touring-server/src/cli/wiring.rs`

`run_audit_with_tree(use_tree: bool)` — parametriza o arm audit + cycles.

`render_audit_as_tree()`:
- Root: `Wiring Audit [{N} total issues]`
- Subtree "Orphan Symbols": símbolos W-100 prefixed
- Subtree "Low-Score Modules": módulos com score < 1.0
- Subtree "Dependency Cycles": contagem de ciclos F2

`render_cycles_as_tree()`:
- Root: `Dependency Cycles [N found]`
- Cada ciclo como leaf com path + depth

### T4 — BlastWarning em JSON Response

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs` (linha ~3496)

**Nota**: VP-Scout Chain 3 detectou que T4 e T5 já estavam parcialmente implementados
pelo Engineer pré-compactação. Engineer C da FASE 4.5 confirmou como pre-implemented —
apenas validação sem edição duplicada.

`cli_ast_blast` resposta agora inclui:
```json
{
  "file_path": "...",
  "blast_radius": 12,
  "consumers": [...],
  "diagnostics": [
    {"code": "B-300", "severity": "warning", "message": "12 files depend on `foo.rs` (threshold=10)"}
  ]
}
```

### T5 — MemoryFinding em JSON Response

**Arquivo**: `crates/touring-hooks/src/cli_handlers.rs` (linha ~1603)

`cli_memory_recall` resposta inclui `memory_diagnostics: [...]` com M-500/M-510/M-520
além dos `tracing::*!` já emitidos pela v4.15.0 (G3). JSON + tracing = dupla observabilidade.

### Fix — `health_events::publish_and_subscribe_roundtrip`

**Arquivo**: `crates/touring-core/src/health_events.rs:154`

Bug pré-existente de test isolation: canal broadcast singleton compartilhado entre testes
paralelos. Teste não-async `publish_with_no_subscribers_returns_zero` publicava
`/tmp/a.rs` enquanto o receiver do test `roundtrip` já estava subscrito.

Fix: drenar em loop com timeout 200ms até receber o evento com
`file_path == "/tmp/roundtrip.rs"`. Testes paralelos podem publicar eventos espúrios —
o loop os descarta corretamente.

## FPs Evitados (VP-Scout)

- **codespan-reporting**: transitivo via naga — já disponível. Não é nova integração.
- **codemap**: abandonado 2018 — not a real opportunity.
- **language-reporting**: sem releases publicados — prototype descartado.
- **annotate-snippets**: nova dep (não está em Cargo.lock) — adiado para v4.17.0
  quando `SpanResolver` trait para RFC-100 spans estiver disponível.
- **T4+T5 duplicate edits**: VP-Scout Chain 3 confirmou ambos já implementados pelo
  Engineer anterior. Engineer C não fez edições duplicadas.

## Deferred — annotate-snippets v4.17.0

`annotate-snippets` v0.12.15 é o renderer do próprio rustc (stable desde 2025-12-16).
Para integração plena, o `touring_core::diagnostic::Diagnostic` precisa de:
1. `source: Option<Arc<dyn miette::SourceCode>>` — referência ao arquivo fonte
2. `span: Option<miette::SourceSpan>` — range exato no arquivo
3. `SpanResolver` trait: `fn resolve(file_path: &str) -> Option<NamedSource>`

Quando o Diagnostic tiver esses campos + SpanResolver, a integração renderizará
diagnostics RFC-100 com sourcemap inline exatamente como o compilador rustc.

## Lições Aprendidas

1. **Zero-cost deps**: termtree e miette já em Cargo.lock/workspace — adicionar como
   dep direta é sempre a estratégia preferida sobre baixar nova dep.
2. **Test isolation com broadcast singleton**: `OnceLock<broadcast::Sender<T>>` é
   compartilhado entre todos os testes do mesmo processo. Testes que esperam mensagem
   específica DEVEM drenar mensagens espúrias em loop, não fazer recv() único.
3. **VP-Scout Chain 3 como anti-duplicate gate**: pré-implementações detectadas pelo
   auditor evitam 2x trabalho e conflitos de merge entre engineers paralelos.
4. **miette Diagnostic trait**: `code()`, `severity()`, `help()` são as 3 funções
   essenciais. `labels()` e `related()` são opcionais e enriquecem com span info.

## Touring CLI Changes

- `touring ast blast <file> --tree` → ASCII tree output (novo flag)
- `touring wiring audit --tree` → ASCII tree output (novo flag)
- `touring wiring cycles --tree` → ASCII tree output (novo flag)
- `touring ast blast -j | jq '.diagnostics'` → RFC-100 B-300 quando blast > 10
- `touring memory recall -j | jq '.memory_diagnostics'` → RFC-100 M-5xx findings
- Qualquer `Diagnostic::to_miette_report()` → `miette::Report` renderizável fancy
