# Touring — Master Plan "Premium de Elite de Mercado"

> **Data**: 2026-06-04 | **Versão**: 1.0 | **Autoridade**: Gabriel Gadea | **Operador**: TACO
> **Base factual**: [Diagnóstico verificado in loco](2026-06-04-touring-diagnostico-elite-mercado.md) (score **5.8/10**, gap 3.3) — §8 Verificação In Loco
> **Hierarquia**: Master Plan → Roadmaps → Plans (A-E) → Waves → Phases → Tasks → Subtasks · **QuickActions** transversais
> **Método**: elaboração multi-agente (6 agentes: 1 macro + 5 tracks) + VGP + sequential-thinking, ancorada nos achados FACT [1.0]

---

## PARTE I — MASTER PLAN

### 1. Visão

> O Touring se torna **o kernel open-source de referência para geração de código agentica**: um harness _typed, auditável e multi-modelo_ sobre o qual qualquer time constrói agentes de código com garantias formais de corretude (VGP), segurança de execução (CEG X0-X9 + landlock), aprendizado contínuo (RL LinUCB) e observabilidade determinística (198 hooks, índice local, wiring Tarjan).

Hoje o Touring **audita código alheio com um rigor que não aplica a si mesmo** — drift documental, wiring DB com crates-fantasma, família `cli_handlers` de 18.804 LOC sem remediação, `.gitignore` ausente. A transformação é o **fechamento do loop de dogfooding**: auto-gerar docs do próprio índice, decompor o monólito com o próprio generator, validar-se com a própria suíte de gates. O score médio passa de **5.8 → 9.0+**: de infra pessoal _single-user Claude-only_ para **plataforma adotável por terceiros** com binário distribuível, multi-modelo e SDK público.

### 2. Tese Estratégica

**"A cura está dentro."** O Touring já possui todos os instrumentos cirúrgicos para sua própria remediação (taco-forge, wiring, generator typestate, CEG, RL). O caminho para elite **não exige novas capacidades funcionais** — exige aplicar esses instrumentos a si mesmo. O sistema que promete ser _auditável_ deve ser o **primeiro objeto auditado**; o drift entre o que o Touring declara e o que o Touring é constitui o principal bloqueador de credibilidade de mercado, _antes de qualquer gap funcional_.

**Posicionamento**: _"Touring é kernel, não distro"_ — a infra-harness sobre a qual os "Cursors" futuros são construídos. Não compete com editores; é o substrato determinístico por baixo deles.

### 3. North Star & Metas por Horizonte

**North Star**: `composite_health_score ≥ 0.95` (atual **0.59-0.69**) + score médio das 9 dimensões **5.8 → 9.0+**.

| Horizonte      | Janela      |   Score-alvo   | `composite_health` | Marcos de saída                                                                                                                                                                          |
| -------------- | ----------- | :------------: | :----------------: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **H0 — Now**   | Semanas 1-4 | 5.8 → **6.5**  |       ≥ 0.72       | Zero drift documental (ARCHITECTURE auto-gerado); `.gitignore` presente; SCC-683 fantasma eliminado; bug `resolve-def` corrigido                                                         |
| **H1 — Next**  | Meses 1-3   | 6.5 → **7.8**  |       ≥ 0.78       | `cli_handlers` decomposto (<3k LOC/módulo); `lifecycle.rs` <200 LOC; ciclo `hooks↔server` quebrado via `HookRuntime` trait; LLM multi-provider (≥2); binário Linux + `touring init` <60s |
| **H2 — Later** | Meses 3-9   | 7.8 → **9.0+** |       ≥ 0.90       | `touring-sdk` + RFC-006; LSP bridge cross-file; salsa incremental; SWE-bench publicado; distribuição completa (brew/Docker/cargo-binstall); ≥1 adotante externo                          |

### 4. Estado Atual vs Alvo (FACT [1.0] — ground truth do plano)

| Métrica                   |                      Atual (medido) | Alvo H2                         |
| ------------------------- | ----------------------------------: | ------------------------------- |
| Score médio (9 dims)      |                              5.8/10 | 9.0+                            |
| LOC Rust                  | 472.579 (src) / 540.046 (workspace) | estável, modularizado           |
| Crates                    |      35 members (36 dirs; 13 shims) | ~27 reais + shims removidos     |
| Maior arquivo de produção |         `cli_handlers.rs` 9.077 LOC | nenhum > 1.500 LOC              |
| Maior família             |          `cli_handlers*` 18.804 LOC | módulos coesos `src/cli/`       |
| Ciclo wiring              |  SCC-683 (fantasma + capnp depth-2) | só depth-2 real, resolvido      |
| `.gitignore`              |                         **ausente** | presente, enforçado             |
| Drift doc (ARCHITECTURE)  |       45/429k/5100 vs 35/472k/10686 | auto-gerado, drift <5%          |
| LSP                       |      0 deps (sintático, intra-file) | `touring-lsp-bridge` cross-file |
| `LlmProvider` impls       |                    1 (NoopLlm stub) | ≥3 (Anthropic/OpenAI/Ollama)    |
| Instalabilidade           |                     compilar ~30min | binário <60s, brew/Docker       |
| RFC                       |                             001-005 | + RFC-006 Extension Contract    |

### 5. Princípios de Execução

1. **Dogfooding primeiro** — o Touring é o primeiro objeto auditado pelo Touring. Toda wave usa `taco-forge perfect-edit` (decomposição), `touring wiring` (blast), `touring inferlets` (contagem contextual, **não grep bruto**), `touring index find` (VGP). _A v1 do diagnóstico foi vítima do grep bruto (A08: 38→0); a v2 prova que VP-Scout Cadeia 3b existe para isso._
2. **No-regression estrutural** — cada split exige `cargo test --workspace` antes/depois + `wiring orphans` baseline + `composite_health` não-decrescente. Modelo: W8/W9/W10 provaram viabilidade com checkpoints `.toon`.
3. **Incrementos atômicos validados** — nenhuma wave toca >1 eixo estrutural simultâneo. DoD binário: CI green + `touring doctor` 5/5 + `composite_health ≥ baseline`.
4. **Metrics-as-code** — toda métrica em docs tem gerador determinístico (`sync-metrics.sh`) no CI. Drift >5% = gate failure. "Auditável" é vazio sem o próprio sistema ser o primeiro auditado.
5. **Moat-preservação** — CEG X0-X9, VGP V1-V4, RL LinUCB, 198 hooks, generator 36 kinds são o diferencial verificado; nenhuma remediação pode degradá-los. Decomposições usam as próprias boundaries (`gateway/`, `saga/`) como unidades.
6. **Evidência antes de afirmação** — todo débito exige evidência CLI (`file:line`/output) antes de virar item de roadmap. (VP-Scout Cadeia 3b: ler o corpo, não o nome.)

### 6. Governança

| Eixo                          | Regra                                                                                                                                                                                                                   |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Gates por wave**            | `touring doctor -j` 5/5 + `cargo test --workspace` + `composite_health ≥ baseline` + `wiring orphans` não aumentou. Falha bloqueia próximo item.                                                                        |
| **Cadência**                  | H0 = sprints semanais (gate `sync-metrics.sh`); H1 = waves quinzenais (checkpoint `.toon` + `memory store --tier semantic`); H2 = releases mensais (CHANGELOG + semver).                                                |
| **Anti-drift enforcement**    | CI job `sync-metrics.sh` falha se LOC drift >5% ou crate-count diverge; mesmo job verifica `wiring cycles` sem crates-fantasma. Roda em cada push.                                                                      |
| **Circuit breaker de escopo** | Item que cresce para >2 tracks dispara revisão com Gabriel. (Modelo: W8 planejou 8 crates, entregou 3 pragmático.)                                                                                                      |
| **Memória institucional**     | Cada wave fecha com `memory store` + `learning reward` + `.toon`. `health-delta` por arquivo ativado para `lifecycle.rs`/`cli_handlers.rs` como gate de regressão histórica.                                            |
| **Ownership**                 | A (Eng) = Gabriel + TACO; B (Produto) = Gabriel prioriza, TACO executa; C (Qualidade) = gates automatizados; D (Docs) = `sync-metrics.sh` é fonte única; E (GTM) = Gabriel decide, whitepaper+SWE-bench como evidência. |

---

## PARTE II — ROADMAPS

### Roadmap H0 — Now (semanas 1-4): **Credibilidade**

> Fecha os P0 baratos. Nenhum toca código de produção crítico. Resultado visível e imediato.

| #    | Item                                                                                                         | Achado  | Track | Size | Impacto                                                                |
| ---- | ------------------------------------------------------------------------------------------------------------ | ------- | ----- | :--: | ---------------------------------------------------------------------- |
| H0-1 | Criar `.gitignore` na raiz (`target/`, `fuzz/`, `*.tf-bak.*`, `*.rlib`, `mutants.out/`, `reports/`, `pln2/`) | N02     | C     |  S   | Safety net contra `git add` de 4.3GB                                   |
| H0-2 | Auto-gerar `ARCHITECTURE.md` + README badges via `sync-metrics.sh` (cargo metadata + workspace-info + tokei) | A03     | C+D   |  S   | Elimina o maior risco de credibilidade (3 números p/ crates: 45/36/35) |
| H0-3 | Ativar `workspace_root` filter no CLI wiring (fix PLT-2026-06-02 já existe, não ativado)                     | A05     | A/C   |  S   | Elimina SCC-683 fantasma                                               |
| H0-4 | Corrigir bug format string `cli_handlers_semantics.rs:179` + teste de regressão                              | N07     | A     |  S   | Bug FACT afetando output de `resolve-def`                              |
| H0-5 | Higiene de raiz: remover 11 `.tf-bak` + `.rlib` + `debug_js.js`; mover `ARCHITECTURE*`/`PLAN-*` p/ `docs/`   | A10     | D     |  S   | Raiz limpa, navegável                                                  |
| H0-6 | Auditar e documentar os 122 erros de teste de `touring-server` (declarados em `ARCHITECTURE.md:771`)         | N03     | C     |  M   | Desbloqueia credibilidade do claim "10.686 tests"                      |
| H0-7 | Montar pipeline de release de binário pré-compilado (CI matrix Linux/macOS)                                  | mercado | B     |  M   | Pré-requisito de existência no mercado                                 |

### Roadmap H1 — Next (meses 1-3): **Estrutural**

| #    | Item                                                                                                           | Achado    | Track | Size |
| ---- | -------------------------------------------------------------------------------------------------------------- | --------- | ----- | :--: |
| H1-1 | Decompor família `cli_handlers` (18.804 LOC) em `src/cli/` com dispatch tipado (`enum CliCommand`)             | N01       | A     |  L   |
| H1-2 | Migrar 1.211 testes inline de `lifecycle.rs` → `lifecycle/` (FIX-3 Fase B); arquivo <200 LOC                   | A02       | A     |  M   |
| H1-3 | Quebrar ciclo `hooks↔server` via trait `HookRuntime` (IoC, modelo tower-lsp) em crate leaf `touring-contracts` | A01/A05   | A     |  XL  |
| H1-4 | `LlmProvider` concreto OpenAI + Ollama (hoje só `NoopLlm` stub)                                                | A12/N05   | B     |  L   |
| H1-5 | `touring init` zero-config (autodetect Cargo/pyproject/package.json, <60s)                                     | mercado   | B+C   |  M   |
| H1-6 | Reestruturar `docs/` em Diátaxis; gerar `mcp-tools.md`; CHANGELOG Keep-a-Changelog                             | A03       | D     |  M   |
| H1-7 | Campanha `unwrap` cirúrgica (~300-600 reais, **não** 3.495); `clippy::unwrap_used` no gateway/L1               | qualidade | C     |  M   |
| H1-8 | Corrigir os 122 erros de teste de `touring-server`                                                             | N03       | C     |  M   |

### Roadmap H2 — Later (meses 3-9): **Plataforma & Mercado**

| #    | Item                                                                                                        | Achado  | Track | Size |
| ---- | ----------------------------------------------------------------------------------------------------------- | ------- | ----- | :--: |
| H2-1 | `touring-lsp-bridge` (tower-lsp IoC) alimentando VGP V3; `find-references`/`rename` cross-file (TODO D.2.2) | A09     | A     |  XL  |
| H2-2 | `touring-index-salsa` — invalidação incremental `#[salsa::tracked]` (vs rebuild total)                      | escala  | A     |  XL  |
| H2-3 | `touring-sdk` público + RFC-006 Extension Contract + E2E `ToyLang`                                          | A12     | B+A   |  L   |
| H2-4 | `touring-eval` SWE-bench-lite Rust/Python + leaderboard público                                             | mercado | B+E   |  L   |
| H2-5 | Distribuição completa: brew tap, cargo-binstall, Docker oficial, npm shim                                   | mercado | B     |  L   |
| H2-6 | Migração real `SCHEMA_VERSION` V7→V8 com fixtures (hoje só canary test)                                     | A16     | C     |  M   |
| H2-7 | Ativar license tiers (free/standard/premium/enterprise) + GTM B2B + comunidade                              | negócio | E     |  XL  |

---

## PARTE III — QUICKACTIONS (transversais, <1 dia, máximo impacto)

> Executáveis **hoje**. Comando concreto. Risco ≈ zero. São a materialização do H0.

| ID        | QuickAction                                | Comando / passo                                                                                                                      | Fecha          | Tempo  |
| --------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ | -------------- | :----: |
| **QA-01** | Criar `.gitignore`                         | `printf 'target/\nfuzz/\n*.tf-bak.*\n*.rlib\nmutants.out/\nreports/\npln2/\ndebug_js.js\n' > ~/.claude/rust/.gitignore`              | N02            | <5min  |
| **QA-02** | Remover artefatos sujos da raiz            | `rm -f ~/.claude/rust/*.tf-bak.* ~/.claude/rust/libworkflow_templates.rlib ~/.claude/rust/debug_js.js`                               | A10            | <5min  |
| **QA-03** | Snapshot de métricas-âncora                | `touring ast workspace-info -j \| jq '{crates:(.packages\|length)}'` + `cargo metadata --no-deps \| jq '.workspace_members\|length'` | input p/ QA-04 | <5min  |
| **QA-04** | Corrigir cabeçalho `ARCHITECTURE.md`       | `taco-forge perfect-edit` linha 3: `45 crates/429k LOC/5100 tests` → `35 crates/472k LOC/10686 tests` (valores de QA-03)             | A03 parcial    | <15min |
| **QA-05** | Corrigir bug `resolve-def`                 | `touring index find resolve_definition` → ler `cli_handlers_semantics.rs:175-185` → `taco-forge perfect-edit` no parêntese extra     | N07            | <30min |
| **QA-06** | Confirmar fix wiring existe                | `grep -n 'workspace_root' crates/touring-hooks/src/cli_handlers.rs \| head` (PLT-2026-06-02, linha ~797)                             | preparar A05   | <15min |
| **QA-07** | Purgar nós-fantasma do wiring              | `touring index rebuild ~/.claude/rust` → remove rows legadas de `touring-rule-engine`/`touring-definitions`                          | A05 parcial    | <30min |
| **QA-08** | Criar estrutura Diátaxis                   | `mkdir -p ~/.claude/rust/docs/{tutorial,how-to,explanation,reference,internal/sessions}`                                             | base D         | <1min  |
| **QA-09** | Remover 5 paths duplicados do `Cargo.toml` | `grep 'crates/' Cargo.toml \| sort \| uniq -d` → editar (`.toml`, Edit OK)                                                           | N09            | <15min |
| **QA-10** | Baseline `touring-server`                  | `cargo check -p touring-server --tests 2>&1 \| grep -c '^error'` → confirma se 122 ainda valem                                       | N03            | ~2min  |

---

## PARTE IV — PLANS (Waves → Phases → Tasks → Subtasks)

> Numeração: `<Plan>.W<wave>.P<phase>.T<task>`. Tamanhos S/M/L/XL. Cada item cita o achado-âncora.

### PLAN A — Core Engineering & Architecture

**Objetivo**: eliminar os 4 maiores bloqueadores técnicos do score 9.0 — (1) decompor o hotspot real `cli_handlers` (N01, 18.804 LOC); (2) resolver staleness do wiring (A05, fix já existe); (3) LSP bridge real (A09); (4) salsa incremental — preservando o moat (CEG/VGP/RL/generator).

#### A.W1 — Higiene de Fundação `[S]` · dep: nenhuma · DoD: docs sincronizados, `.gitignore` presente, Cargo.toml sem duplicatas, `cargo check --workspace --tests` exit 0

- **A.W1.P1 — Higiene de docs e segurança**
  - `A.W1.P1.T1` Sincronizar `ARCHITECTURE.md` com `cargo metadata` `[S]` — subtasks: a) contar crates/LOC/tests reais; b) reescrever "Workspace Overview"; c) remover `45 crates/429k/5100`; d) sincronizar README · DoD: `grep '45 crates\|429k\|5100'` = 0 hits
  - `A.W1.P1.T2` Criar `.gitignore` raiz (N02) `[S]` — subtasks: a) entradas `target/`, `fuzz/corpus/`, `*.tf-bak.*`, `*.rlib`; b) cobrir `fuzz/` 4.3GB · DoD: `git check-ignore fuzz/` match
- **A.W1.P2 — Baseline `touring-server`**
  - `A.W1.P2.T3` Auditar os 122 erros de teste (N03) `[M]` — subtasks: a) `cargo check --workspace --tests` capturar saída; b) documentar baseline real (122 declarado em `ARCHITECTURE.md:771` — confirmar atual); c) smoke test CI que falha se `!= baseline` · DoD: `docs/2026-06-04-touring-server-test-baseline.md`

#### A.W2 — Decomposição `cli_handlers` (N01) `[L]` · dep: A.W1 · DoD: `cli_handlers.rs` residual <200 LOC, dispatch `enum CliCommand`, 0 regressão, clippy 0

- **A.W2.P1 — Inventário e mapa de dispatch**
  - `A.W2.P1.T4` Catalogar 170 `pub fn cli_*` por domínio `[M]` — subtasks: a) `grep 'pub fn cli_'` → tabela domínio→handler; b) 8-10 domínios coesos (ast, wiring, memory, session, decompose, index, kpi, evolution, polyglot, entity); c) desenhar `enum CliCommand { Ast(AstCommand), Wiring(..), .. }` · DoD: `docs/2026-06-04-cli-handlers-inventory.md`
- **A.W2.P2 — Extração zero-risco (satélites)**
  - `A.W2.P2.T5` Mover 5 satélites pequenos p/ `src/handlers/` `[M]` — kpi(414), evolution(578), repo_score(605), repo_health(417), polyglot(385); `cargo test -p touring-hooks` após cada · DoD: 5 migrados, 0 regressão
  - `A.W2.P2.T6` Mover `decompose`(2.665) + `index`(1.246) p/ `handlers/` `[M]` — resolver re-exports; `cargo check --workspace` · DoD: imports resolvidos, 0 erros
- **A.W2.P3 — Extração do core**
  - `A.W2.P3.T7` Quebrar `cli_handlers.rs` (9.077) em `handlers/{ast,wiring,memory,misc}.rs` + `handlers/dispatch.rs` `[L]` — façade `<200 LOC` com `pub use`; `taco-forge perfect-edit` por extração · DoD: residual <200 LOC, `enum CliCommand` tipado
- **A.W2.P4 — Validação**
  - `A.W2.P4.T8` Validação N01 `[S]` — `wiring orphans` 0 novos; clippy `-D warnings` 0; `cargo test --workspace`; atualizar `touring-cli-index.md`; `memory store` · DoD: gates green

#### A.W3 — IoC `HookRuntime` + Fix A05 + Migração testes `[M]` · dep: A.W2 · DoD: trait `HookRuntime` 3+ impls, `workspace_root` default, `lifecycle.rs` <200 LOC, 0 SCC-683 ghosts

- **A.W3.P1 — Trait `HookRuntime` (IoC)**
  - `A.W3.P1.T9` Extrair trait de `hook_runtime.rs` (3.002 LOC) `[L]` — subtasks: a) trait com `dispatch`/`register_hook`/`get_config`; b) `MockHookRuntime` p/ testes; c) `daemon.rs` recebe `Box<dyn HookRuntime>`; d) mitigação: `HookRuntimeAdapter` newtype p/ 23 call-sites · DoD: ciclo `hooks↔server` reduzido
- **A.W3.P2 — Fix A05 (`workspace_root`)**
  - `A.W3.P2.T10` Ativar `workspace_root` filter por default `[M]` — subtasks: a) sub-comandos `cycles/orphans/impact/chains`; b) `$PWD` default; c) flag `--all-workspaces` legado; d) test PLT-2026-06-02 · DoD: 0 ghost SCC para workspace
- **A.W3.P3 — Migração testes (A02/FIX-3)**
  - `A.W3.P3.T11` Migrar 1.211 testes inline `[L]` — subtasks: a) separar blocos `#[cfg(test)]`; b) `tests/lifecycle/` por módulo; c) `lifecycle.rs` produção ~150 LOC · DoD: arquivo <200 LOC, mesma contagem de testes passando

#### A.W4 — LSP Bridge (A09) + Salsa `[XL]` · dep: A.W3 · DoD: `find-references --scope workspace` cross-file, `touring-index-salsa` bench <50ms

- **A.W4.P1 — tower-lsp setup**
  - `A.W4.P1.T12` `touring-lsp` crate + trait `LspBackend` `[M]` — `tower_lsp` opt-in (feature `lsp-bridge`); `LspServer: LanguageServer`
- **A.W4.P2 — find-references cross-file (TODO D.2.2)**
  - `A.W4.P2.T13` `handle_references` via `symbol_store` scope=Workspace `[L]` — conectar VGP V3; comparar vs grep baseline
- **A.W4.P3 — rename cross-file**
  - `A.W4.P3.T14`-Rename workspace scope via `symbol_store` + `taco-forge perfect-edit`
- **A.W4.P4 — Salsa prototype**
  - `A.W4.P4.T15` `touring-index-salsa` `[XL]` — `#[salsa::tracked]` sobre `FileKnowledgeDB`; bench `incremental_reindex` <50ms vs full-rebuild ~5s

**Riscos-chave A**: SCC em `touring-hooks` impede split limpo (HIGH/HIGH → mitigação: façade `pub use` como W8 pivot); trait IoC quebra 23 call-sites (MED/HIGH → `HookRuntimeAdapter` newtype + cargo check por subtask); salsa version mismatch (MED/MED → alinhar `workspace = true`).
**Métricas A**: `cli_handlers.rs` <200 LOC; `wiring cycles` 0 ghost; `lifecycle.rs` <200 LOC; `find-references --scope workspace` retorna cross-file; bench salsa <50ms; `cargo test --workspace` ≥10.686 a cada wave.

---

### PLAN B — Product, Distribution & Extensibility

**Objetivo**: fechar os 4 gaps de mercado — instalabilidade zero (compilar ~30min), onboarding manual, Claude-only (`LlmProvider` só stub), ecossistema fechado — preservando o moat e expondo as superfícies certas.

#### B.W1 — Day-0 Installability `[S→M]` · dep: A.W1 (CI verde) · DoD: `curl … | sh && touring doctor` 5/5 em <60s em máquina limpa

- **B.W1.P1 — Release pipeline CI**
  - `B.W1.P1.T1` GitHub Actions matrix (Linux x86_64-musl + macOS aarch64) `[L]` — cross-compile, strip+compress, SHA256, smoke em container limpo · DoD: Release com 2 binários + checksums
  - `B.W1.P1.T2` Installer `curl|sh` `[M]` — detect OS/arch, baixa+valida SHA256, instala `~/.local/bin`, smoke `touring doctor` · DoD: funciona Ubuntu 22.04 + macOS 14 <30s
- **B.W1.P2 — `touring init` autodetect**
  - `B.W1.P2.T3` Subcomando `touring init` `[M]` — probe Cargo→pyproject→package.json; `.touring/config.toml`; `index rebuild`; `doctor` · DoD: 5/5 ok pós-init, <60s
  - `B.W1.P2.T4` Gate `<60s` `[S]` — modo `--fast` (só `src/`); progress bar; CI gate falha se >90s
- **B.W1.P3 — Distribuição**
  - `B.W1.P3.T5` Shell completion (`clap_complete`) + manpage (`clap_mangen`) `[S]`

#### B.W2 — Model Agnosticism `[M→L]` · dep: B.W1 + B.W3.P2 (touring-sdk) · DoD: `touring --llm {openai,ollama} ask` funciona; `LlmProvider` ≥3 impls

- **B.W2.P1 — Trait hardening** — `B.W2.P1.T6` mover `NoopLlm` p/ test-only; trait com `complete`/`complete_with_tools`/`embed`; features `llm-{anthropic,openai,ollama}` `[L]`
- **B.W2.P2 — Anthropic impl** — `B.W2.P2.T7` SSE streaming `/v1/messages` + prompt caching headers; tests `httpmock` `[M]`
- **B.W2.P3 — OpenAI + Ollama** — `B.W2.P3.T8` `/v1/chat/completions` + Ollama `localhost:11434`; `provider_from_env()`; CLI `--llm` `[M]`
- **B.W2.P4 — Integration tests** — suite com mock + 3 impls; E2E gated em CI

#### B.W3 — SDK & Extension Contract `[L]` · dep: B.W2 · DoD: RFC-006 + `touring-sdk` em crates.io + E2E `ToyLang`

- **B.W3.P1 — RFC-006** — `B.W3.P1.T9` `docs/rfcs/RFC-006-extension-contract.md`: stability tiers (S0/S1/S2), hook subscription, language plugin, provider registration, CEG isolation `[M]`
- **B.W3.P2 — `touring-sdk` crate** — `B.W3.P2.T10` re-exports seletivos (8 traits), `deny(missing_docs)`, `examples/`, publish workflow `[M]`
- **B.W3.P3 — `ToyLang` E2E** — `B.W3.P3.T11` grammar tree-sitter minimalista; plugin loader; integration test VGP+generator; **mitigação A12**: parse path alternativo (sem `ast_grep_language`) `[L]`
- **B.W3.P4 — How-to extensão** — guias "Add a Language in 5 Steps", "Build a Context Provider"

#### B.W4 — TUI & Discoverability `[M]` · dep: B.W1, B.W3 · DoD: `touring tui` busca live <200ms; Docker; brew

- **B.W4.P1 — TUI ratatui** — `B.W4.P1.T12` painel símbolos + preview + busca live tantivy (debounce 100ms); fallback offline; 20+ testes `[L]`
- **B.W4.P2 — Docker** — multi-stage distroless <50MB; GHCR
- **B.W4.P3 — Homebrew tap** — Formula baixa binário (não compila); `brew audit/install/test` CI
- **B.W4.P4 — SWE-bench** — `touring-eval` harness (ver Plan E)

**Riscos-chave B**: libc mismatch em binário (MED/HIGH → musl static + matrix); streaming diverge entre providers (HIGH/MED → `StreamChunk` normalizado no SDK); RFC-006 expõe internals instáveis (HIGH/HIGH → só re-exports estáveis + `#[doc(hidden)]` + stability tiers); `ast_grep` trava ToyLang (MED/MED → tree-sitter direto).
**Métricas B**: instalação <60s clean machine; binário <30MB; `LlmProvider` ≥3 impls reais; `touring-sdk` em crates.io 100% docs; Docker <50MB; SWE-bench publicado.

---

### PLAN C — Quality, CI & Self-Healing (Dogfooding)

**Objetivo**: materializar "a cura está dentro" — gates de CI que o Touring prega mas não pratica. Score-alvo: Qualidade 7.0→8.5, Doc 4.5→7.5, Org 4.0→7.0, Arq 6.5→8.0.

#### C.W1 — Foundation Safety Net `[S]` · dep: nenhuma · DoD: `.gitignore` presente, bug N07 corrigido, raiz limpa, CI mínimo

- **C.W1.P1 — Safety net imediata**
  - `C.W1.P1.T1` Criar `.gitignore` (N02) `[S]` — ≥10 entradas; smoke `git check-ignore` (sem usar git destrutivo)
  - `C.W1.P1.T2` Corrigir bug `resolve-def` (N07) + regressão `[S]` — `cli_handlers_semantics.rs:179` parêntese extra; teste que valida output
  - `C.W1.P1.T3` Limpar raiz `[S]` — remover `.tf-bak`/`.rlib`/`debug_js.js`; catalogar 9 dirs órfãos (mover `holon-wasm-*`)
- **C.W1.P2 — Cargo hygiene**
  - `C.W1.P2.T4` Remover 5 paths duplicados (N09) `[S]` — `cargo metadata` 35 únicos
  - `C.W1.P2.T5` CI mínimo `[M]` — `cargo check` + `clippy -D warnings` + prefix lint (N15) + `CONTRIBUTING.md`

#### C.W2 — Self-Describing Docs & Wiring Integrity `[M]` · dep: C.W1 · DoD: `sync-metrics.sh` no CI, 0 ghost-crates, migration test V7→V8

- **C.W2.P1 — `sync-metrics.sh` + anti-drift gate**
  - `C.W2.P1.T6` `sync-metrics.sh` (A03) `[M]` — `cargo metadata` + `tokei` + `grep #[test]`; gera seção ARCHITECTURE; gate falha se drift >5%; **fallback sem daemon** · DoD: ARCHITECTURE = 35/540k/10686
  - `C.W2.P1.T7` README badges auto via `touring status` `[S]`
- **C.W2.P2 — Fix wiring (A05/N10)**
  - `C.W2.P2.T8` `workspace_root` default no CLI `[M]` — `cli_handlers.rs:797`; gate `wiring cycles` = 1 (capnp depth-2), não 683; flag `--all-projects` legado
- **C.W2.P3 — Schema migration (A16)**
  - `C.W2.P3.T9` Migration test V7→V8 com fixtures `[M]` — fixture V7+V8; upgrade idempotente; guard se `SCHEMA_VERSION` muda sem fixture

#### C.W3 — Incremental Quality Gates (Dogfooding Elite) `[L]` · dep: C.W2 · DoD: file-size gate, `clippy::unwrap_used` gateway, fuzz GC, health-delta CI

- **C.W3.P1 — File-size gate + cli move**
  - `C.W3.P1.T10` Gate `>5k LOC → fail` `[S]` — whitelist temporária com data; detecta `cli_handlers.rs`
  - `C.W3.P1.T11` Mover família `cli_handlers_*` → `src/cli/` (N01 organização) `[L]` — `mod.rs` dispatch; `taco-forge perfect-edit`; símbolos preservados
- **C.W3.P2 — `clippy::unwrap_used` + fuzz GC**
  - `C.W3.P2.T12` `deny(clippy::unwrap_used)` em `gateway/` + `touring-server-reasoning` `[M]` — **inventariar antes** (dívida real ~300-600, não 3.495); CEG já documenta invariante (`learn.rs:26`)
  - `C.W3.P2.T13` `fuzz/gc.sh` + CI gate <200MB (N06) `[M]` — de 4.3GB; integra `safe-clean.sh` (REGRA #12)
- **C.W3.P3 — health-delta + DAG gate**
  - `C.W3.P3.T14` `health-delta` CI (warning) + RL reward loop `[M]` — `docs/reference/quality-gates.md`
  - `C.W3.P3.T15` No-back-edge Cargo gate + wiring integrity suite `[M]` — confirma DAG acíclico; badge

**Riscos-chave C**: `clippy::unwrap_used` revela >200 reais (MED/MED → inventory first, batches de 20/wave); `sync-metrics.sh` sem daemon em CI (MED/LOW → só `cargo metadata`+`tokei`, fallback); fuzz GC remove corpora valiosa (LOW/MED → dry-run + preservar seeds <1KB).
**Métricas C**: N02/N07/A03/A05/A16/N06 fechados; gateway 0 `unwrap_used`; `composite_health ≥ 0.78`; CI verde em 8 gates.

---

### PLAN D — Documentation, DX & Onboarding

**Objetivo**: eliminar drift via docs geradas do próprio índice (dogfooding); de infra inacessível → plataforma com onboarding <30min, Diátaxis completa, rustdoc enforçado.

#### D.W1 — Limpeza e Estrutura Base `[S]` · dep: nenhuma · DoD: raiz limpa, Diátaxis criada, session artifacts movidos, A03 eliminado

- `D.W1.P1.T1` Remover 11 `.tf-bak` + `.rlib` + `debug_js.js` `[S]`
- `D.W1.P2.T2` Criar `docs/{tutorial,how-to,explanation,reference,internal/sessions}/` + mover 89 session artifacts (iter/wave/summary) `[M]`
- `D.W1.P3.T3` Fix A03 cabeçalho `ARCHITECTURE.md` (35/540k/10686) `[S]`

#### D.W2 — Geração Automatizada de Referência `[M]` · dep: D.W1 · DoD: `docs/reference/*.md` gerados, rustdoc deny

- `D.W2.P1.T4` `touring-doc-gen.py` (stdlib) `[L]` — subcomandos: `mcp-tools` (161 tools), `hooks` (198), `generators` (36), `modules` (por crate); `--validate` hash estável
- `D.W2.P2.T5` CI gate anti-drift (`diff != 0 → fail`) `[S]`
- `D.W2.P3.T6` `#![deny(missing_docs)]` em `touring-hooks` + `touring-generator`; `#![warn]` <20 em intelligence/server `[XL]` — escalation warn→deny

#### D.W3 — CHANGELOG + Crate READMEs `[M]` · dep: D.W2 · DoD: CHANGELOG Keep-a-Changelog, 4 crate READMEs

- `D.W3.P1.T7` `changelog-synth.py` das 72 `.toon` → `CHANGELOG.md` `[M]` — ≥20 versões; idempotente
- `D.W3.P2.T8` `README.md` para touring-hooks/intelligence/server/generator (Purpose/API/Examples/Caveats) `[M]`

#### D.W4 — Tutorial Diátaxis + Onboarding `[L]` · dep: D.W2, D.W3 · DoD: usuário externo → `index status` em <35min

- `D.W4.P1.T9` `tutorial/getting-started.md` (honesto: ~30min compilação) `[M]`
- `D.W4.P2.T10` `tutorial/first-hook.md` (lifecycle walkthrough + ASCII) `[M]`
- `D.W4.P3.T11` 5 how-to recipes (add-lang com limitação A12, extend-generator, debug-ceg, build-mcp-tool, run-e2e) `[L]`
- `D.W4.P4.T12` `explanation/architecture.md` (doc vivo; `ARCHITECTURE.md` vira pointer; seção honesta "Known Gaps" A09/A12) `[L]`

**Riscos-chave D**: A03 re-aparece pós-refactor (HIGH/HIGH → gate `doc-gen` no CI); `deny(missing_docs)` bloqueia CI (HIGH/MED → warn→deny escalation, superfície primeiro).
**Métricas D**: A03 fechado (delta <2%); `reference/` completa (161+198+36); rustdoc limpo; onboarding ≤35min; CHANGELOG ≥20 versões.

---

### PLAN E — Go-to-Market, Business & Community

**Objetivo**: de sistema de pesquisa pessoal (GTM efetivo 1.5/10) → produto adotável. Posicionamento "kernel não distro", honestidade radical sobre maturidade, prova de valor por tier, benchmark público, multi-provider. **Depende da saúde estrutural dos Tracks A-D** — GTM sobre drift P0 é autodestruição de credibilidade.

#### E.W1 — Credibilidade Zero-BS `[M]` · dep: nenhuma (usa A03/B fundações) · DoD: docs auto-geradas, `touring init`, landing v0, CHANGELOG semântico

- `E.W1.P1.T1` `sync-metrics.sh` (compartilhado com C/D) — A03 fechado FACT
- `E.W1.P2.T2` CHANGELOG v31.0.0 + SECURITY.md + SUPPORT.md
- `E.W1.P3.T3` `touring init` + binário pré-compilado + installer público (compartilhado com B)
- `E.W1.P3.T4` Landing v0 + **atualizar whitepaper** (`docs/2026-06-04-touring-whitepaper.md` **existe** mas tem números stale 36/428k → 35/472k) + seção honesta de maturidade + comparativo Aider/Cline/Cursor

#### E.W2 — Prova de Valor por Tier `[L]` · dep: E.W1 + Tracks A+B parciais · DoD: SWE-bench baseline, demo por tier

- `E.W2.P1.T5` `touring-eval` SWE-bench-lite Rust (50 issues) `[XL]` — resolved%, tokens, false-positive VGP rate; comparativo Aider; CI semanal
- `E.W2.P2.T6` Matriz de features por tier + 4 screencasts (<5min) + exemplos por tier `[M]` — valida que feature gates batem com docs
- `E.W2.P3.T7` Refactor `cli_handlers` (N01) p/ onboarding viável (compartilhado com A.W2) `[L]`

#### E.W3 — Ecossistema & Multi-Provider `[XL]` · dep: E.W2 + LlmProvider ≥2 + N03 resolvido · DoD: 3 providers, RFC-006, 5 contributors externos, distribuição

- `E.W3.P1.T8` `LlmProvider` OpenAI + Ollama (compartilhado com B.W2) `[L]`
- `E.W3.P2.T9` RFC-006 + `touring-sdk` crates.io + E2E ToyLang (compartilhado com B.W3) `[M]`
- `E.W3.P3.T10` Comunidade: CONTRIBUTING + Code of Conduct + issue templates + brew/Docker/npm + 5 `good-first-issue` (dos 122 erros N03) `[M]`

#### E.W4 — Mercado & Monetização `[XL]` · dep: E.W3 + ≥10 usuários · DoD: 3 early adopters pagantes, case study, pricing validado

- `E.W4.P1.T11` 3 ICPs (platform teams / CI com código LLM / pesquisador de agents) + pitch deck + early adopter program (10 vagas, premium 90d grátis por feedback) `[M]`
- `E.W4.P2.T12` Benchmark expandido Python/TS + HumanEval Rust + leaderboard `touring.dev/eval` + submission SWE-bench oficial `[L]`
- `E.W4.P3.T13` Conferências (RustConf talk "CEG with Typestate+Landlock", AgentBench) + 3 blog posts (VGP anti-hallucination, CEG Deno-style, RL LinUCB) + awesome-lists `[S]`

**Riscos-chave E**: drift como credibility-killer (HIGH/HIGH → `sync-metrics.sh` P0 antes de qualquer GTM); single-user Claude-only limita TAM 10× (HIGH/HIGH → multi-provider); compilação 30min = barreira absoluta (HIGH/HIGH → binário pré-compilado); 122 erros minam claim "10.686 tests" (HIGH/MED → corrigir antes de publicar métricas; até lá claim honesto "10.686 anotações, suite parcialmente compilável"); SWE-bench abaixo de Aider na 1ª publicação (MED/HIGH → contextualizar harness+sandbox vs editor; medir false-positive rate além de resolved%).
**Métricas E**: A03 fechado; instalação <60s; SWE-bench publicado; 3 providers; RFC-006 + SDK; 5 PRs externos; 3 early adopters pagantes NPS≥7; GTM score 1.5→6.0.

---

## PARTE V — DAG de Dependências & Sequenciamento

```
H0 (paralelo, sem deps) ─── QuickActions QA-01..10
  ├─ C.W1 (.gitignore, bug fix, raiz)         ┐
  ├─ C.W2.P1 (sync-metrics → A03)             ├─ destrava credibilidade
  ├─ A.W3.P2 (workspace_root → A05)           │
  ├─ A.W1.P2 (baseline 122 erros)             ┘
  └─ B.W1.P1 (release pipeline)  ── pré-requisito de distribuição

H1 (depende de H0)
  ├─ A.W2 (decompor cli_handlers)  ← dep: CI verde + file-size gate (C.W3.P1)
  ├─ A.W3.P1 (HookRuntime IoC)     ← dep: A.W2
  ├─ A.W3.P3 (migrar testes lifecycle)
  ├─ B.W1.P2 (touring init)        ← dep: B.W1.P1 (binário)
  ├─ B.W2 (multi-model)            ← dep: B.W3.P2 (sdk crate)
  ├─ C.W3 (gates dogfooding)       ← dep: C.W2
  └─ D.W1-W2 (Diátaxis + doc-gen)

H2 (depende de H1)
  ├─ A.W4 (LSP + salsa)            ← dep: A.W3
  ├─ B.W3 (SDK + RFC-006)          ← dep: B.W2
  ├─ B.W4 (TUI/Docker/brew)        ← dep: B.W1
  ├─ E.W2 (SWE-bench)              ← dep: multi-model
  ├─ E.W3 (ecossistema)            ← dep: E.W2 + N03
  └─ E.W4 (GTM/monetização)        ← dep: E.W3 + binário + docs
```

**Caminho crítico de produto**: H0(CI verde + binário) → H1(decompor + multi-model) → H2(SDK + eval + GTM). ~9 meses.
**Caminho crítico de engenharia**: CI verde → decompor `cli_handlers` → `HookRuntime` IoC → salsa/LSP.

### Matriz de execução por horizonte

| Track           | H0 (sem 1-4)               | H1 (mês 1-3)                            | H2 (mês 3-9)                 |
| --------------- | -------------------------- | --------------------------------------- | ---------------------------- |
| **A** Eng       | A.W1 (higiene + baseline)  | A.W2 (cli_handlers) + A.W3 (IoC+testes) | A.W4 (LSP+salsa)             |
| **B** Produto   | B.W1.P1 (release pipeline) | B.W1.P2-3 + B.W2 (multi-model)          | B.W3 (SDK) + B.W4 (TUI/dist) |
| **C** Qualidade | C.W1 + C.W2.P1             | C.W2.P2-3 + C.W3                        | (gates mantidos)             |
| **D** Docs      | D.W1 (limpeza+Diátaxis)    | D.W2 (doc-gen) + D.W3                   | D.W4 (tutorial+explanation)  |
| **E** GTM       | E.W1 (credibilidade)       | E.W2.P1 (SWE-bench start)               | E.W3 + E.W4                  |

---

## Apêndice A — Rastreabilidade aos achados (diagnóstico verificado)

| Achado                        | Severidade | Tratado por                         | Horizonte |
| ----------------------------- | :--------: | ----------------------------------- | :-------: |
| A03 drift documental          |     P0     | C.W2.P1 + D.W1.P3 + sync-metrics.sh |    H0     |
| N02 `.gitignore` ausente      |     P1     | C.W1.P1.T1 / QA-01                  |    H0     |
| N01 `cli_handlers` 18.804 LOC |     P1     | A.W2 + C.W3.P1.T11                  |    H1     |
| A01 `touring-hooks` 32-36%    |     P1     | A.W3.P1 (splits)                    |    H1     |
| A05 wiring staleness          |     P1     | A.W3.P2 + C.W2.P2 / QA-06,07        |   H0-H1   |
| A09 sem LSP real              |     P1     | A.W4 (LSP bridge)                   |    H2     |
| A02 testes inline lifecycle   |     P2     | A.W3.P3                             |    H1     |
| A12 extensão sem RFC/teste    |     P2     | B.W3 (RFC-006+SDK)                  |    H2     |
| A16 schema sem migration test |     P2     | C.W2.P3                             |   H1-H2   |
| N03 122 erros touring-server  |     P1     | A.W1.P2 + C / QA-10                 |   H0-H1   |
| N05 LlmProvider só stub       |     P2     | B.W2 (multi-model)                  |    H1     |
| N06 fuzz/ 4.3GB               |     P2     | C.W3.P2.T13                         |    H1     |
| N07 bug resolve-def           |     P3     | C.W1.P1.T2 / QA-05                  |    H0     |
| N09 Cargo.toml duplicado      |     P3     | C.W1.P2.T4 / QA-09                  |    H0     |

## Apêndice B — Referências de elite (context7)

- **salsa** (`/salsa-rs/salsa`) — `#[salsa::tracked]` incremental demand-driven, query-groups por crate → modelo de `touring-index-salsa` (A.W4.P4) e decomposição.
- **tower-lsp** — `Service` trait IoC → quebra do ciclo `hooks↔server` via `HookRuntime` (A.W3.P1) e `touring-lsp-bridge` (A.W4).
- **ast-grep-core** — rewrite estrutural embedded (sem subprocess) → generator kinds.
- **rust-analyzer** — ~60 crates pequenos coesos, nenhum >15% → meta de modularização.

## Apêndice C — Comandos-âncora (verificar baseline antes de executar)

```bash
cd ~/.claude/rust
touring status -j | jq '{composite_health_score, index}'        # baseline saúde
cargo metadata --no-deps | jq '.workspace_members | length'      # 35 crates
ls crates/touring-hooks/src/cli_handlers*.rs | xargs wc -l | tail -1  # 18.804 família
ls .gitignore 2>/dev/null || echo "AUSENTE (N02)"
touring wiring cycles --min-depth 2 --format json | jq '.cycle_count'  # 2 (683 ghost)
grep -n 'workspace_root' crates/touring-hooks/src/cli_handlers.rs | head  # fix A05
head -3 ARCHITECTURE.md                                          # drift A03
```

---

_Master Plan gerado por TACO (6 agentes: macro + 5 tracks · 578k tokens) + VGP + sequential-thinking + context7 | ancorado no diagnóstico verificado in loco (score 5.8/10) | 2026-06-04 | Touring v30.0.0_

## DAG Executável (populado 2026-06-04)

Este plano foi convertido em **DAG nativo rastreável** via `touring decompose` — granularidade de **Wave** (as Tasks/Subtasks de cada Wave permanecem detalhadas neste `.md`).

- **Task raiz**: `task_1780622111986800900` (type `plan`, CILA L4)
- **19 subtasks** = 19 Waves, com dependências topológicas fiéis ao DAG da PARTE V
- **Validação**: `valid=true`, `has_cycles=false`, `subtask_count=19`
- **READY agora** (4, paralelo, 0 deps pendentes): `A-W1`, `C-W1`, `D-W1`, `E-W1` (= H0 Credibilidade)
- **BLOCKED** (15): liberadas conforme suas deps são concluídas

| Wave | Deps nativas | Wave | Deps nativas     |
| ---- | ------------ | ---- | ---------------- |
| A-W1 | —            | C-W3 | C-W2             |
| C-W1 | —            | B-W2 | B-W1             |
| D-W1 | —            | A-W2 | A-W1, C-W3       |
| E-W1 | —            | B-W3 | B-W2             |
| C-W2 | C-W1         | D-W4 | D-W2, D-W3       |
| B-W1 | A-W1         | A-W3 | A-W2             |
| D-W2 | D-W1         | B-W4 | B-W1, B-W3       |
| D-W3 | D-W2         | E-W2 | E-W1, A-W2, B-W2 |
| A-W4 | A-W3         | E-W3 | E-W2, B-W2       |
| E-W4 | E-W3         |      |                  |

> Ciclo `B.W2↔B.W3.P2` resolvido no nível de _phase_ (B-W2 depende de B-W1, não da Wave B-W3 inteira) para manter o grafo acíclico.

**Operação wave-a-wave**:

```bash
ROOT=task_1780622111986800900
touring decompose ready  $ROOT                              # waves executáveis agora
touring decompose update $ROOT::A-W1 --status in_progress   # iniciar uma wave
touring decompose update $ROOT::A-W1 --status completed      # concluir → libera dependentes
touring decompose status                                    # progresso global
```

_Memória Touring: `touring-elite-masterplan-dag-2026-06-04` (tier semantic). populate-dag não foi usado: o formato H4 das Waves é incompatível com o parser de H2 (`## S-NN`) do `taco-forge populate-dag`._
