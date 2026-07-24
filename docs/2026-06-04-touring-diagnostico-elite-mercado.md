# Diagnóstico Estrutural Touring — Barra "Premium de Elite de Mercado"

> **Data**: 2026-06-04 | **Método**: TACO multi-agente (scout inline + 9 auditores dimensionais + 2 lentes de mercado + verificação adversarial) + context7
> **Escopo**: `~/.claude/rust` (Touring v30.0.0 binário / daemon v30.3.0) — toda a estrutura
> **Avaliação**: contra a barra de um *harness que gera código para agentes de código, Premium de Elite de Mercado*
> **Confidence legend**: `FACT [1.0]` (medido/verificado) · `INFERENCE [0.7-0.9]` (deduzido de evidência) · `SPECULATION [<0.7]` (hipótese)

---

## 1. Sumário Executivo

O Touring é um **paradoxo de engenharia verificado**: carrega **capacidades funcionais raras no mercado** — índice local de 1,1M símbolos, Code Execution Gateway X0-X9 com landlock, RL LinUCB com loop fechado, VGP anti-alucinação, generator typestate, 10.686 testes, clippy `deny-all` + cargo-deny — **acopladas a passivos estruturais que um produto de elite não pode exibir**: um monólito de 171k LOC (36% de todo o código), um único arquivo de 19.444 linhas, um ciclo de dependência de profundidade 683, e drift documental tão severo que o próprio sistema "auditável" descreve a si mesmo com números errados.

**Tese central** `INFERENCE [0.85]`: o Touring tem **capacidade de elite** mas **maturidade estrutural inconsistente**. É um sistema de pesquisa pessoal *single-user* com ambição de produto, e a lacuna entre os dois é exatamente o que este diagnóstico mapeia. **O insight decisivo: a cura está dentro** — o Touring possui as ferramentas (índice, wiring, generator, RL, taco-forge) para se consertar; o gap é aplicar o próprio sistema a si mesmo com rigor (dogfooding).

### Scoreboard das 9 dimensões `FACT [1.0]` (medições) + `INFERENCE [0.8]` (scores)

| # | Dimensão | Atual | Elite | Gap | Veredito de 1 linha |
|---|----------|:-----:|:-----:|:---:|---------------------|
| 1 | **Arquitetura** | 5.5 | 9.0 | 3.5 | Modelo de 4 camadas elegante, mas o ciclo depth-683 contradiz o "acyclic" do próprio README |
| 2 | **Modularização** | **3.5** | 9.0 | **5.5** | ⚠ **A pior**: monólito 171k + `lifecycle.rs` 19.444 LOC + 116 arquivos soltos |
| 3 | **Organização** | 4.5 | 9.0 | 4.5 | Raiz como canteiro de obras: 11 `.tf-bak`, `.rlib` solto, docs/ flat com 248 arquivos |
| 4 | **Nomenclatura** | **6.5** | 9.0 | 2.5 | ✅ **A melhor**: 34/35 prefixo `touring-`, CLI verbo-substantivo coerente |
| 5 | **Navegabilidade** | 4.5 | 9.0 | 4.5 | Scaffolding de topo forte, mas o código só é navegável *com* o próprio harness |
| 6 | **Escalabilidade** | 5.5 | 9.0 | 3.5 | Fundação sólida (profiles/RFC/capability), mas monólito+ciclo bloqueiam escala |
| 7 | **Qualidade** | 5.8 | 9.2 | 3.4 | Gates de elite (clippy/cargo-deny/10.686 testes) minados por 3.495 `unwrap` + 38 `unimplemented` |
| 8 | **Funcionalidades** | **6.8** | 9.2 | 2.4 | Diferencial técnico real e singular; falta LSP, multi-repo, produto |
| 9 | **Documentação** | 4.2 | 9.0 | 4.8 | Volume alto, drift grave; o doc "auditável" mente sobre si mesmo |
| | **MÉDIA** | **5.2** | **9.1** | **3.9** | Capacidade de elite, maturidade estrutural de protótipo avançado |

> ⚠ **REVISADO na Sessão 2 (verificação in loco)** — vários achados abaixo foram refinados, corrigidos ou **refutados** após verificação forense no código real. Score médio revisado: **5.2 → 5.8** (gap 3.9 → 3.3). Os scores das §3.x abaixo são os da v1; **os valores autoritativos estão no scoreboard revisado da [§8.0](#80-veredito-da-revisão--a-v1-foi-sistematicamente-pessimista)**. Achados mais alterados: A02 (lifecycle.rs é 99% teste), A04 (refutado), A08 (refutado — 0 `unimplemented` reais), A15 (namespace MCP consistente). Leia a §8 antes de agir sobre qualquer achado.

### Top-5 achados que mais separam o Touring da barra de elite

1. **P0 — Monólito `touring-hooks` (171k LOC, 36% do código)** com `lifecycle.rs` de 19.444 LOC num único arquivo. Tão grande que o **próprio índice do Touring falha ao analisá-lo** (`ast meta` retorna `on_disk_fallback`, `fan_in=0`). `FACT [1.0]`
2. **P0 — Drift documental sistêmico**: `ARCHITECTURE.md` declara *45 crates / 429.255 LOC / 5.100 tests*; a realidade é *36 / 472.579 / 10.686*. README diz `index: 3002 files / 67698 symbols` — real: *30.489 / 1.118.176* (14× a menos). Para um produto cujo *value prop* é "auditable", isto é contradição existencial. `FACT [1.0]`
3. **P1 — Ciclo de dependência depth-683** que viola o "acyclic, no back edges" do README. **Parcialmente fantasma** (cita `touring-rule-engine` e `touring-definitions`, que **não existem em disco** — wiring DB stale), mas com núcleo real `hooks↔server↔foundation↔orchestration`. Meta-falha: a ferramenta de análise tem dados errados sobre si mesma. `FACT [1.0]`
4. **P1 — 3.495 `.unwrap()` em produção + 112 `panic!()`** num daemon long-lived que executa código de agentes — 1.192 (34%) concentrados no monólito `touring-hooks`. Contradiz o princípio "fail-open" do CEG. `FACT [1.0]`
5. **P1 — 38 `unimplemented!()/todo!()`** em código de produto, incluindo `touring-analysis/src/quality/rust_semantic.rs:142` — **o módulo que vende análise de qualidade tem um buraco de implementação**. `FACT [1.0]`

---

## 2. Metodologia & Ground Truth

### Como foi medido

1. **Scout inline** (FACT): `touring doctor/status/index/wiring`, `cargo metadata`, `wc -l`, `grep`, `find` sobre `~/.claude/rust`.
2. **9 auditores dimensionais paralelos** (Sonnet, schema estruturado): cada um leu código real, rodou CLI Touring, citou evidência `file:line`.
3. **2 lentes de mercado**: posicionamento competitivo + barra técnica de elite (context7: salsa, ast-grep, tantivy, tower-lsp).
4. **Reconciliação**: divergências entre agentes resolvidas contra medições próprias verificadas.

### Tabela de métricas do workspace `FACT [1.0]`

| Métrica | Valor real medido | Declarado nos docs | Drift |
|---------|------------------:|-------------------:|:-----:|
| LOC Rust | **472.579** | 429.255 (ARCH) / ~428k (README) | +10% |
| Crates (dirs/members) | **36 / 35** | 45 (ARCH) / 36 (README) | ARCH +9 |
| Funções de teste | **10.686** | 5.100+ (ARCH) | +110% |
| Símbolos indexados | **1.118.176** | 67.698 (README footer) | 14× |
| Arquivos indexados | **30.489** | 3.002 (README footer) | 10× |
| `composite_health_score` | **0,6946** | — (target elite ≥ 0,85) | — |
| Maior crate | touring-hooks **171.273 LOC** | — | 36% do total |
| Maior arquivo | `lifecycle.rs` **19.444 LOC** | — | 20× a norma de elite |
| `.unwrap()` em produção | **3.495** | — | — |
| `panic!()` | **112** | — | — |
| `unimplemented!/todo!()` | **38** | — | — |
| `allow(dead_code)` / `allow(unused)` | **47 / 20** | — | — |
| Ciclos de dependência | **2** (depth 2 + depth **683**) | "acyclic, no back edges" | contradição |
| Crates usando thiserror/anyhow | **~15 / 36** | — | parcial |
| Build profiles | **8** | — | elite |
| docs .md / RFC / .toon | **319 / 5 / 72** | — | volume alto |
| rustdoc pub fn documentadas | **4.233 / 6.163 (~68%)** | — | < 95% elite |

---

## 3. Análise por Dimensão

### 3.1 Arquitetura — 5.5/10

**Veredito**: o modelo de 4 camadas (L1 infra → L2 intelligence → L3 orchestration → L4 surface) é conceitualmente elite e documentado com diagrama C4. Porém o ciclo depth-683 contradiz a invariante central "acyclic, no back edges" do README, e o monólito `touring-hooks` corrói todos os limites de camada — qualquer crate que importa um hook importa tudo.

**Forças** `FACT [1.0]`:
- Estratificação L1-L4 nominalmente correta e documentada.
- **CEG X0-X9** (`gateway/`, 26 módulos) com typestate `Execution<S>` que impõe ordem de estágios em *compile time* — boundary interna exemplar; X3 (VGP) e X5 (SANDBOX) estruturalmente unskippáveis.
- Generator typestate `Draft→Verified→Rendered→Speculated→Committed` — estado inválido impossível de representar.
- Toolchain hygiene de nível enterprise: 8 build profiles, clippy `deny-all`, cargo-deny (250L), sccache+mold.
- Splits W8/W9/W10 (`touring-hooks-shared`, `-prediction`, `touring-server-{reasoning,visual,session}`) mostram evolução consciente rumo a menor acoplamento.

**Achados**:
- **P1 — Ciclo depth-683 (parcialmente fantasma, real no núcleo)** `FACT [1.0]`: `touring wiring cycles` lista `touring-rule-engine/src/types.rs` e `touring-definitions/src/overrides.rs`, mas `ls crates/touring-rule-engine` → *inexistente* (absorvidos em `touring-foundation` em W3.4/W3.5, 2026-05-12). Núcleo real: `hooks↔server↔foundation↔orchestration↔storage`. **Impacto**: viola invariante do README; rebuild incremental reverbera por toda a cadeia. **Ação**: `touring index rebuild` para purgar fantasmas; quebrar o ciclo concreto via trait object / event bus (tokio mpsc).
- **P2 — Monólito + 116 arquivos soltos** `FACT [1.0]` (detalhado em §3.2).
- **P3 — Drift ARCHITECTURE.md** `FACT [1.0]` (detalhado em §3.9).

**Gaps vs elite**:
- **Limite de camada sem enforcement em Cargo** → Axum/Tokio/Rustls modelam boundaries via topologia de dependências estrita. **Recomendação**: criar `touring-contracts` (só traits + types, zero deps); validar com `cargo-deny graph` no CI.
- **Doc arquitetural manual vs gerada** → rust-analyzer/servo geram dep-graph via `cargo-depgraph` em CI. **Recomendação**: job CI `cargo metadata + tokei` que falha se LOC drift > 5%.

---

### 3.2 Modularização — 3.5/10 ⚠ (a dimensão mais crítica)

**Veredito**: `touring-hooks` é um mega-crate de 171k LOC (36% do workspace) com 116 arquivos soltos na raiz de `src/`, `lifecycle.rs` de 19.444 linhas e `cli_handlers.rs` de 9.077. O workspace tem **dualidade patológica**: crates gigantes indecompostos coexistindo com ~12 shims de 6-30 LOC. Os splits W8/W9 provaram-se necessários mas tímidos (`-shared` 3,9k + `-prediction` 5,5k vs 171k intactos).

**Achados**:
- **P0 — Mega-crate `touring-hooks` 171k LOC + 116 arquivos soltos** `FACT [1.0]`: 2º maior crate é `touring-intelligence` (64k) — o monólito é **2,7×** maior. **Impacto**: blast radius catastrófico, onboarding inviável, build incremental degradado. **Ação**: decompor em 6-8 crates funcionais seguindo os subdirs já existentes como fronteiras naturais (`gateway/`→`touring-hooks-ceg`, `saga/`→`touring-hooks-saga`, `post_tool_rl`→`touring-hooks-rl`).
- **P1 — `lifecycle.rs` 19.444 LOC single file** `FACT [1.0]`: **o próprio `touring ast meta` retorna `on_disk_fallback` com `fan_in=0/fan_out=0`** — o índice simbólico do Touring *falha ao analisar seu próprio maior arquivo*. **Ação**: quebrar em `lifecycle/{session,task,edit,bash,tool_rl,neural}.rs`, nenhum > 1.500 LOC (norma rust-analyzer).
- **P2 — `cli_handlers.rs` 9.077 LOC + 12 satélites `cli_handlers_*` soltos** `FACT [1.0]`: `cli_handlers_{decompose,entity,evolution,file_knowledge,index,kpi,mcp,mutation_test,polyglot,repo_health,repo_score,scout}.rs` dispersos sem módulo contentor. **Ação**: mover para `src/cli/` com `mod.rs`; separar dispatch de implementação.
- **P3 — Shims indistinguíveis** `FACT [1.0]`: 12 shims (`ast,antt,cognitive,learning,wasm,python,...`) inflam o grafo e poluem `wiring orphans`. **Ação**: consolidar em `touring-compat` ou remover pós-migração 47→13; marcar com `#[deprecated]` + data.

**🔴 Regressão temporal detectada via memória Touring** `FACT [1.0] / INFERENCE [0.85]`: a própria memória do Touring (`touring-hooks-potentialization-2026-04-14`) registra que em **14/04/2026** *"`lifecycle.rs` já é thin re-export hub (168 LOC), código bem modularizado com 15 submodules"*. Em **04/06/2026** (medido) o mesmo arquivo tem **19.444 LOC** — uma **regressão de ~115×** em 7 semanas, com o subdir `lifecycle/` (12.746 LOC, 19 files) coexistindo. **Isto é a prova viva da tese central**: sem um gate de tamanho de arquivo enforçado, um thin hub bem modularizado vira monólito em semanas. A erosão não é estática — é **ativa e acelerada**. **Ação**: gate de regressão histórica via `health-delta` por arquivo já existe no Touring; aplicá-lo a `lifecycle.rs` teria disparado alerta na 3ª edição consecutiva de crescimento.

**Gap vs elite** `INFERENCE [0.9]`: rust-analyzer tem ~60 crates, maior ~45k LOC, **nenhum > 15% do total**; `lifecycle.rs` (19.444) é **20× a norma de ~1.000 LOC/arquivo** de tokio/axum. **Recomendação**: gate de CI `find -name '*.rs' | wc -l > 5000 → fail`; `ast tdg` retorna **F** para arquivos > 5k LOC e bloqueia edição sem plano de split.

---

### 3.3 Organização — 4.5/10

**Veredito**: a raiz está poluída — 11 `Cargo.toml.tf-bak.*`, 13 `.md` soltos (incluindo 2 `PLAN-*.md` de 27-66 KB), 6 diretórios órfãos não-membros, e um **`libworkflow_templates.rlib` de 144 KB** (artefato de compilação versionado!). `docs/` tem 248 arquivos no nível raiz sem hierarquia. A experiência de entrada é a de um **canteiro de obras ativo**, não de um produto de elite.

**Achados**:
- **P1 — 11 `Cargo.toml.tf-bak.*` na raiz** `FACT [1.0]`: backups do taco-forge sem GC. **Ação**: `.gitignore` glob + hook de cleanup > 7 dias.
- **P2 — 6 dirs órfãos** (`agent-harness, holon-wasm-components, holon-wasm-runner, mutants.out, pln2, reports`) `FACT [1.0]`. **Ação**: mover para `docs/`/repo separado; `.gitignore` em `mutants.out/`.
- **P3 — 13 `.md` na raiz** `FACT [1.0]`: a raiz de elite tem ≤ 5 (README/CHANGELOG/CONTRIBUTING/LICENSE/SECURITY). **Ação**: mover `ARCHITECTURE*`/`PLAN-*`/`STATUS` para `docs/`.
- **P4 — `docs/` flat com 248 arquivos** `FACT [1.0]`. **Ação**: subdividir em `docs/{architecture,plans,rfcs,waves,checkpoints,specs,session-reports}/`.
- **P6 — `libworkflow_templates.rlib` (144 KB) + `debug_js.js` soltos na raiz** `FACT [1.0]`: artefato de build versionado viola REGRA #12. **Ação**: `.gitignore *.rlib` + `build.target-dir` em `.cargo/config.toml`.

**Gap vs elite**: rust-lang/rust e tokio têm raízes com ≤ 10 entidades visíveis. **Recomendação**: GC automático no taco-forge + política de raiz limpa enforçada em CI.

---

### 3.4 Padronização de Nomenclaturas — 6.5/10 ✅ (a melhor dimensão)

**Veredito**: espinha dorsal coerente — prefixo `touring-` em 34/35 crates, CLI verbo-substantivo, MCP `touring_`/`ctx_`. Comprometida por 2 crates sem prefixo (`hooks`, `inferlets`), 3 namespaces MCP concorrentes, e ausência de uma camada de consistência *formal* (lint de prefixo em CI).

**Forças** `FACT [1.0]`: prefixo `touring-` quase universal; CLI kebab-case verbo-substantivo uniforme (`pre-read`, `session-start`, `resolve-def`); MCP `touring_*` consistente; hooks `pre-/post-` simétricos.

**Achados**:
- **P1 — `hooks` e `inferlets` sem prefixo** `FACT [1.0]`: o maior crate do projeto (`hooks`, 171k LOC) quebra o invariante de descoberta. **Ação**: lint CI `ls crates/ | grep -v '^touring-' | grep -v '^inferlets$'` deve ser vazio.
- **P2 — Namespace MCP tripartido sem convenção publicada** `INFERENCE [0.8]`: `touring_*` vs `ctx_*` vs sem prefixo, em 88 tools. **Ação**: `docs/MCP-NAMING.md` — `touring_` para análise, `ctx_` para execução.
- **P4 — Shims com nome de crate pleno sem indicação** `FACT [1.0]`. **Ação**: `[package.metadata.touring] role = "shim"` + lint que recusa `pub fn` em shims.
- **P5 — CLI mistura namespace e verbo no mesmo nível** `INFERENCE [0.8]`: `touring ast` (namespace) vs `touring rename` (verbo). **Ação**: unificar em `touring <domain> <action>`.

---

### 3.5 Navegabilidade — 4.5/10

**Veredito**: scaffolding de navegação impressionante no topo (`docs/landing/index.md` com Diátaxis; `lib.rs` com 548 linhas e 157 `pub mod` anotados), mas a experiência colapsa ao chegar no monólito. O **índice Touring (1,1M símbolos) é a navegação de facto** — mas é ferramenta externa, não navegabilidade intrínseca. Para um produto que vende "open, typed, auditable", o código ainda **não é legível sem o próprio harness**.

**Forças** `FACT [1.0]`: landing Diátaxis formal; `lib.rs` com cada módulo comentado e datado; 1.141 `//!` module docs; CONSTITUTION-v8 + 5 RFCs.

**Achados**:
- **P1 — Monólito sem hierarquia de diretórios** `FACT [1.0]`: 116 `.rs` topologicamente planos. **Ação**: 6-8 subdiretórios temáticos; `lib.rs` como índice puro.
- **P2 — `lifecycle.rs` ilegível como unidade** `FACT [1.0]` (cruza §3.2).
- **P5 — README footer stale 14×** `FACT [1.0]`: `index:3002 files/67698 symbols` vs real 30.489/1.118.176. **Ação**: badge gerado por `touring status -j` no CI.

**Gap vs elite** `INFERENCE [0.85]`: tokio/axum/ripgrep têm `README.md`/`GUIDE.md` por crate; Postgres/CPython têm `MODULES.md` ASCII por pasta. **Touring deve ser navegável SEM `touring` instalado** — `src/MODULES.md` em cada crate grande. Remover timestamps de linha do `lib.rs` (pertencem ao git blame, não ao índice).

---

### 3.6 Fundação e Escalabilidade — 5.5/10

**Veredito**: fundação técnica sólida (build hygiene, `SCHEMA_VERSION=8`, capability model CEG, traits de extensão), mas não escala como produto porque o monólito bloqueia contribuição paralela e CI incremental, e o ciclo depth-683 torna a análise de impacto não-confiável. A arquitetura de extensão existe **em esboço** mas carece de contrato público testável.

**Forças** `FACT [1.0]`: 8 profiles (LTO fat + strip + panic=abort no release); CEG capability deny-by-default (landlock+rlimit+cgroup, 4 profiles, ENV_ALLOWLIST sem credenciais); `SCHEMA_VERSION=8` co-versionado; 5 RFCs formais; traits de extensão reais (`LlmProvider`, `MemoryProvider`, `Plugin`, `ProviderPlugin`, `EmbeddingProvider`, `SolverBackend`, `GpuBackend`, `SagaAgent`); generator 36 kinds + 14 linguagens via polyglot.

**Achados**:
- **P1 — Monólito bloqueia escala de contribuição/CI** `FACT [1.0]`.
- **P2 — Ciclo wiring com símbolos fantasma bloqueia análise confiável** `FACT [1.0]`: a REGRA #0 e VP-Scout Cadeia 7 dependem de wiring correto. **Ação**: `index rebuild` + gate CI que falha se ciclo cita crate ausente em disco.
- **P5 — Contrato de extensão não documentado nem testado** `INFERENCE [0.8]`: traits existem mas não há integration test "adicione linguagem X em N passos". **Ação**: **RFC-006 "Language and Agent Extension Contract"** + integration test E2E (grammar fictício `ToyLang` + `LlmProvider` mock).
- **P6 — `SCHEMA_VERSION=8` sem migration test V7→V8** `INFERENCE [0.75]`: risco de corrupção silenciosa em upgrade. **Ação**: snapshot test com fixture de cada versão.

**Gap vs elite** `INFERENCE [0.85]`: salsa/rust-analyzer mantêm invariants de integridade do grafo como testes de regressão em CI. **Recomendação**: gate `touring wiring cycles` contra lista real de crates.

---

### 3.7 Qualidade e Excelência do Código — 5.8/10

**Veredito**: fundação de qualidade sólida — clippy `deny-all`, cargo-deny, 10.686 testes, typestate — mas dívida técnica que **contradiz a posição "elite" declarada**: 3.495 `.unwrap()` (34% em `touring-hooks`), 112 `panic!()`, 38 `unimplemented!()`, 47 `allow(dead_code)`. **O produto audita código alheio com regras que viola no próprio fonte** — gap de credibilidade que é o principal bloqueador do tier elite.

**Forças** `FACT [1.0]`: clippy `deny-all` + cargo-deny 250L; 10.686 testes / 813 módulos `#[cfg(test)]` / 18 `tests/`; CEG typestate X3/X5 unskippáveis; ~15/36 crates com thiserror/anyhow; 8 build profiles defensivos.

**Achados**:
- **P1 — 3.495 `.unwrap()` em produção** `FACT [1.0]`: distribuição medida — `touring-hooks` **1.192** (34%), `intelligence` 661, `code` 352, `server` 255, `server-reasoning` 229, `cortex` 226. `lifecycle.rs` sozinho tem ~250 (linhas 392, 733, 828, 861, 864, 884, 887, 1411...). **Impacto**: panic em handler do daemon singleton derruba todos os hooks da sessão; contradiz "fail-open". **Ação**: `clippy::unwrap_used` por crate (começar pelos L1 e pelo CEG).
- **P2 — 112 `panic!()`** `FACT [1.0]`: especialmente crítico em paths X5/X8 do CEG. **Ação**: `GatewayError` propagado; lint `panic_in_result_fn`.
- **P3 — 38 `unimplemented!()/todo!()`** `FACT [1.0]`: distribuídos em hooks(5 arquivos), analysis(3), cortex(2), code(2), foundation(1), assists(1). **Ironia**: `touring-analysis/src/quality/rust_semantic.rs:142` — o módulo que vende análise de qualidade tem buraco. **Ação**: tracking issue + priorizar hot-paths do VGP.
- **P5 — 47 `allow(dead_code)`** `FACT [1.0]`: contradição direta com REGRA #0 (zero orphan pub symbols) — o lint silencia exatamente o sinal que a regra deveria tratar. **Ação**: integrar via `wire-orphans`, remover, ou `#[doc(hidden)]` + prazo.

**Gap vs elite** `INFERENCE [0.85]`: rustls tem política zero-`unwrap` em runtime desde v0.21; serde/tokio têm 100% pub fn documentadas. **Recomendação**: `forbid(clippy::unwrap_used)` em `touring-hooks`/`touring-server`; `#![deny(missing_docs)]` nos crates de superfície.

---

### 3.8 Funcionalidades — 6.8/10 (diferencial técnico real)

**Veredito**: conjunto de funcionalidades **tecnicamente singular** — CEG X0-X9 typestate, RL LinUCB fechado, generator 5-estados, 198 hooks, tantivy BM25+trigram, polyglot 14 linguagens — que **nenhum concorrente direto replica como harness integrado**. O diferencial é *profundidade ortogonal* em vez de cobertura superficial. Corroído por: (1) monólito refratário a auditoria; (2) **ausência de LSP real** (`grep lsp_types/tower_lsp` → 0 matches); (3) instabilidade nos paths de erro (3.495 `unwrap`).

**Forças** `FACT [1.0]`:
- **CEG X0-X9 completo** com typestate compile-time (26 módulos): o único harness OSS com sandbox landlock+rlimit+capability *antes* da execução.
- **Generator typestate** + VGP V1-V4 como gate obrigatório — ausente em Aider/Cline.
- **RL LinUCB** 8 arms / 25 dims com transcript miner contínuo (`~/.claude/projects/*.jsonl`) — aprendizado cross-session local.
- **198 hooks lifecycle** — superfície de observabilidade sem paralelo.
- **Polyglot tree-sitter** 14 linguagens + inferlets WASM 11 runtimes + Z3.

**Achados / gaps**:
- **P4 — Ausência de LSP client integrado** `FACT [1.0]`: `resolve-def`/`find-references` operam via índice próprio, não via protocolo LSP. Rename seguro cross-crate e type-correctness dependem de rust-analyzer/gopls/pyright. **Ação**: `touring-lsp-bridge` (tower-lsp) alimentando o VGP V3 com type info real.
- **Gap — Multi-repo é stub** `FACT [1.0]`: `federation/mod.rs` é comentário de roadmap. **Ação**: elevar a feature (`touring-federation` com index sharding).
- **Gap — Sem benchmark harness** `INFERENCE [0.8]`: SWE-bench/HumanEval são padrão. **Ação**: `touring-eval` com SWE-bench-lite Rust/Python.
- **Gap — Onboarding requer setup manual extenso** `INFERENCE [0.8]`. **Ação**: `touring init` zero-config (autodetect Cargo/pyproject/package.json, < 60s).

---

### 3.9 Documentação — 4.2/10

**Veredito**: drift estrutural grave e dominação por **91 artefatos de sessão/iteração TACO** (37% de `docs/`) que poluem sem separação de audiência. Rustdoc (~68%) é sólido para o tamanho, mas a ausência de Diátaxis, entry points com números stale, e `lifecycle.rs` com **18 `///` em 19.444 LOC (0,09% densidade)** revelam documentação que serve ao autor, não ao adotante.

**Achados**:
- **P1 — Drift crítico ARCHITECTURE.md** `FACT [1.0]`: `45 crates / 429.255 LOC / 5.100 tests` vs real `36 / 472.579 / 10.686`. **Três métricas erradas simultaneamente.**
- **P2 — README footer stale** `FACT [1.0]`: `3002 files / 67698 symbols` (14× a menos).
- **P3 — 91 artefatos de sessão em `docs/` flat** `FACT [1.0]`: `taco-iter*`, `wave-*`, `pln*`, `session-summary*`. **Ação**: mover para `docs/internal/sessions/`.
- **P4 — `lifecycle.rs`: 18 `///` em 19.444 LOC** `FACT [1.0]`.
- **P5 — 3 versões de ARCHITECTURE sem deprecation clara** `FACT [1.0]`: `.md` (50KB) + `.v29.5.0.md` (142KB) + `_PLAN`; aponta para "Previous: v30.3.4" que não existe.
- **P6 — Zero estrutura Diátaxis** `FACT [1.0]`: só `docs/landing/` existe; sem `how-to/`, `tutorial/`, `explanation/`.

**Gap vs elite** `INFERENCE [0.9]`: rust-analyzer gera ARCHITECTURE via script (impossível driftar); Bevy/Ratatui/Leptos adotam Diátaxis. **Recomendação**: `docs/sync-metrics.sh` no pre-commit; reestruturar `docs/` em 4 quadrantes Diátaxis; `CHANGELOG.md` Keep-a-Changelog na raiz; gerar `docs/reference/mcp-tools.md` dos schemas das 88 ferramentas.

---

## 4. Benchmark de Mercado

### 4.1 Posicionamento competitivo — "Touring é kernel, não distro"

**Tese** `INFERENCE [0.85]`: o Touring é **infra-harness** (substrato sobre o qual agentes são construídos), não produto de usuário final. É o equivalente a comparar o **kernel Linux com o Ubuntu** — categorias complementares, não substituíveis. Hoje opera *single-user* exclusivamente com Claude Code, o que o posiciona como **infra pessoal avançada**, não produto de mercado — mas a arquitetura é agnóstica de modelo e extensível.

| Onde o Touring **LIDERA** `INFERENCE [0.8]` | Onde **FICA ATRÁS** `FACT/INFERENCE` |
|---|---|
| Índice local 1,1M símbolos, latência <10ms (sem round-trip cloud) | Sem LSP real integrado (`FACT`: 0 matches `tower_lsp`) |
| VGP anti-alucinação (gate formal antes de gerar) | Single-user, Claude-only (zero OpenAI/Gemini/Ollama) |
| CEG X0-X9 landlock+rlimit+capability (Deno model) | Instalabilidade **zero** (sem binário/brew/npm/Docker; compilar 472k LOC ~30 min) |
| RL LinUCB com transcript miner cross-session | Dívida técnica visível (3.495 unwrap, monólito) mina pitch "auditável" |
| Wiring/blast/cycles determinístico (Tarjan SCC) | Drift documental que corrói qualquer claim técnico |
| 198 hooks como primitivas de composição | Multi-repo ausente (vs Sourcegraph nativo) |
| Decompose com DAG tipado auditável | UX de produto zero (CLI puro, sem GUI/extension) |
| Governança formal (Constitution v8, 5 RFCs, tiers) | Ecossistema fechado (88 MCP tools só para consumo próprio) |

**Referências de mercado** `INFERENCE [0.8]`:
- **Aider** (26k stars, multi-modelo, pip/brew): modelo-agnóstico + instalação day-0 → tração orgânica. Touring Claude-only limita o TAM ~10×.
- **Cline** (50k+ installs, MCP nativo via VS Code): distribuição via Marketplace + MCP deu ecossistema de terceiros em 6 meses. Touring tem 88 MCP tools mas **todos internos**.
- **Cursor**: UX zero-friction (30s) é pré-requisito de adoção.
- **Sourcegraph Cody/Amp**: code graph multi-repo; índice local do Touring é vantagem de privacidade/latência mas **sem UI de busca** o gap de descoberta não fecha.
- **Devin / OpenHands** (40k stars): sandbox VM/Docker justifica trust para tarefas de horas; CEG é superior para single-file mas não resolve agentes multi-step autônomos.
- **Copilot Workspace**: integração no ponto de trabalho (PR/issue). Touring (REGRA #11: git proibido) fecha uma porta que todos usam como entrada.

### 4.2 Barra técnica de elite (context7)

**Referência central — salsa / rust-analyzer** `FACT [1.0]` (context7 `/salsa-rs/salsa`): salsa permite **query groups distribuídos por crates** com computação incremental demand-driven (`#[salsa::tracked]`) e dependency tracking automático red-green por revisão. **É exatamente a solução para os dois maiores problemas do Touring**:

| Referência | O que faz | Lição para o Touring |
|---|---|---|
| **salsa / rust-analyzer** | Cada função `#[salsa::tracked]` é memoizada; mudança de input recomputa só o subgrafo afetado | `lifecycle.rs` (19k) deve virar tracked queries isoladas (`parse_hook_event`, `resolve_hook_target`, `compute_cila_budget`); o monólito hoje recalcula em cascata o que salsa faria cirurgicamente |
| **ast-grep** | Rewrite estrutural poliglota com metavars, rewriters compostos, **sem subprocess** | Touring invoca ast-grep como CLI externo (overhead de spawn); embeder `ast-grep-core` permite rewriters programáticos compostos num único pass — crítico nos 36 generator kinds |
| **tantivy** | Search BM25+facets zero-panic, schema tipado, merge policy configurável | Touring já usa v5; o gap é merge policy explícita + `IndexWriter::commit` com rollback ligado ao VGP |
| **tower-lsp** | Protocol layer sobre `tower::Service`, handlers stateless isolados | O ciclo depth-683 (`hooks↔server`) seria eliminado se a comunicação fosse `Service` trait (IoC) — o server exporia LSP sem importar `touring-hooks` |

**Recomendação arquitetural central** `INFERENCE [0.85]`: adotar o **modelo salsa** para decompor `lifecycle.rs` em tracked queries + **inversion-of-control via trait** (`HookRuntime`, já deferido em W8) para quebrar o ciclo. Prototipar `touring-index-salsa` para invalidação incremental do índice (hoje `index rebuild` recalcula todo o workspace).

---

## 5. Matriz Consolidada de Achados

| ID | Sev | Dimensão | Achado | Evidência | Esforço |
|----|:---:|----------|--------|-----------|:-------:|
| A01 | **P0** | Modularização | Monólito `touring-hooks` 171k LOC (36%) | `wc -l` / `cargo metadata` | XL |
| A02 | **P0** | Modularização | `lifecycle.rs` 19.444 LOC; índice falha (`on_disk_fallback`) | `ast meta` | L |
| A03 | **P0** | Documentação | Drift ARCHITECTURE.md (45/429k/5100 vs 36/472k/10686) | `head ARCHITECTURE.md` | S |
| A04 | **P0** | Documentação | README index footer 14× stale | `README:128` | S |
| A05 | **P1** | Arquitetura | Ciclo depth-683 (fantasma + núcleo real) | `wiring cycles` | M |
| A06 | **P1** | Qualidade | 3.495 `unwrap` prod (1.192 em hooks) | `grep -c` | L |
| A07 | **P1** | Qualidade | 112 `panic!()` (crítico em CEG X5/X8) | `grep` | M |
| A08 | **P1** | Qualidade | 38 `unimplemented` (inc. `rust_semantic.rs:142`) | `grep` | M |
| A09 | **P1** | Funcionalidades | Ausência de LSP real integrado | `grep tower_lsp`=0 | L |
| A10 | **P2** | Organização | 11 `.tf-bak` + `.rlib` 144KB + dirs órfãos na raiz | `ls` | S |
| A11 | **P2** | Modularização | `cli_handlers.rs` 9k + 12 satélites soltos | `ls src/` | M |
| A12 | **P2** | Escalabilidade | Contrato de extensão sem RFC nem teste E2E | `grep RFC-006`=0 | M |
| A13 | **P2** | Qualidade | 47 `allow(dead_code)` (contradiz REGRA #0) | `grep` | S |
| A14 | **P3** | Organização | `docs/` flat 248 arquivos, sem Diátaxis | `ls docs/` | M |
| A15 | **P3** | Nomenclatura | `hooks`/`inferlets` sem prefixo; MCP tripartido | `ls crates/` | S |
| A16 | **P3** | Escalabilidade | `SCHEMA_VERSION=8` sem migration test V7→V8 | `grep` | M |

---

## 6. Roadmap de Remediação

> **Princípio**: a cura está dentro. Aplicar o próprio Touring (taco-forge, wiring, generator, RL) a si mesmo.

### H0 — Quick wins de credibilidade (dias) — fecha os P0 baratos
1. **Auto-gerar ARCHITECTURE.md + README badges** (A03/A04): script `docs/sync-metrics.sh` (`cargo metadata` + `touring ast workspace-info` + `wc -l`) em CI, falha se drift > 5%. **O sistema TEM o índice — deve descrever-se a partir dele.**
2. **`touring index rebuild`** para purgar os nós fantasma do ciclo (A05) + gate CI que falha se ciclo cita crate ausente.
3. **Higiene de raiz** (A10): `.gitignore` para `*.tf-bak.*`, `*.rlib`, `mutants.out/`, `reports/`, `pln2/`; mover `ARCHITECTURE*`/`PLAN-*` para `docs/`.

### H1 — Robustez e desmonte do monólito (semanas) — fecha os P0/P1 estruturais
4. **Decompor `lifecycle.rs`** (A02) em `lifecycle/{session,task,edit,bash,tool_rl,neural}.rs`, nenhum > 1.500 LOC, via `taco-forge perfect-edit`. Gate `ast tdg` = F para > 5k LOC.
5. **Quebrar o ciclo depth-683** (A05) via **IoC**: trait `HookRuntime` (já deferido em W8) num crate leaf `touring-contracts`; `server` implementa, `hooks` depende — modelo **tower-lsp**.
6. **Campanha anti-`unwrap`** (A06/A07): classificar por contexto via `touring inferlets`; `clippy::unwrap_used` deny começando por CEG + L1; alvo < 200. Auditar 112 `panic!()` para zero em X0-X9.
7. **Resolver os 38 `unimplemented`** (A08), priorizando `rust_semantic.rs:142` (hot-path do VGP V4).
8. **Extrair crates funcionais de `touring-hooks`** (A01): `touring-hooks-ceg`, `-rl`, `-saga` seguindo subdirs existentes como fronteiras (continuar W8/W9/W10).

### H2 — De infra pessoal a plataforma (meses) — fecha os gaps de mercado
9. **Modelo-agnóstico**: trait `LlmProvider` com impls Anthropic/OpenAI/Ollama (multiplica TAM).
10. **Binário pré-compilado público** + `touring init` zero-config (< 60s) — pré-requisito de existência no mercado.
11. **LSP como subsistema** (A09): `touring-lsp-bridge` (tower-lsp) alimentando VGP V3.
12. **RFC-006 Extension Contract** (A12) + integration test E2E + `touring-sdk` para providers de terceiros.
13. **salsa para índice incremental** (`touring-index-salsa`): invalidação demand-driven em vez de `index rebuild` total.
14. **`touring-eval`** (SWE-bench-lite) + **TUI de busca** (ratatui) tornando o índice de 1,1M símbolos um diferenciador visível.

---

## 7. Conclusão

O Touring está numa posição **invejável e perigosa** simultaneamente. Invejável porque possui um *moat* técnico genuíno — CEG, VGP, RL, índice local, governança formal — que **nenhum concorrente OSS replica como harness integrado**. Perigosa porque os passivos estruturais (monólito, ciclo, drift, `unwrap`) atacam precisamente o *value prop* declarado: **"auditable"**. Um produto que se diz auditável **não pode** ter o próprio documento de arquitetura mentindo sobre si, nem o próprio índice falhando ao analisar o próprio maior arquivo, nem o próprio grafo de wiring citando crates inexistentes.

**O caminho para "Premium de Elite de Mercado" não exige novas capacidades — exige aplicar o rigor que o Touring já prega a si mesmo.** As ferramentas existem. A constituição existe. O que falta é fechar o loop de dogfooding: gerar a documentação do próprio índice, decompor o monólito com o próprio generator, e validar a si mesmo com a própria suíte de gates. A nota média **5.2/10** não reflete falta de talento — reflete **dívida de maturidade acumulada por velocidade alta sem governança estrutural enforçada**. É exatamente o tipo de dívida que o Touring foi construído para detectar e remediar.

---

## 8. Verificação In Loco e Refinamento Forense (Sessão 2 — 2026-06-04)

> **Método**: 8 grupos de auditores forenses paralelos re-verificaram cada achado lendo o código real (`file:line`), re-executando comandos, e distinguindo produção de teste/string/comentário. Resultado: **6 achados refinados para melhor, 4 problemas novos descobertos**. Esta seção tem precedência sobre a §3-§5 onde houver divergência.

### 8.0 Veredito da revisão — a v1 foi sistematicamente pessimista

A verificação in loco revelou que o diagnóstico v1, embora estruturalmente correto, **superestimou a dívida técnica** por ter usado contagem bruta de `grep` sem ler contexto. O score médio sobe de **5.2 → 5.8** (gap para elite cai de 3.9 → 3.3).

| # | Dimensão | v1 | **Revisado** | Δ | Razão da revisão |
|---|----------|:--:|:------------:|:--:|------------------|
| 1 | Arquitetura | 5.5 | **6.5** | +1.0 | Cargo é DAG acíclico (verificado); o "ciclo" é só do grafo de wiring, com fix disponível |
| 2 | Modularização | 3.5 | **5.0** | +1.5 | `lifecycle.rs` é 99,2% teste inline; produção (152 LOC) já modularizada via FIX-3 |
| 3 | Organização | 4.5 | **4.0** | −0.5 | AGRAVADO: `.gitignore` ausente + `fuzz/` 4.3GB sem GC |
| 4 | Nomenclatura | 6.5 | **7.5** | +1.0 | MCP é namespace monolítico consistente (não tripartido); `crates/hooks` nem é crate |
| 5 | Navegabilidade | 4.5 | **5.0** | +0.5 | Substituto LSP próprio (17 `DefinitionKind`) mais capaz que reconhecido |
| 6 | Escalabilidade | 5.5 | **5.5** | 0 | Traits bem desenhados, mas `LlmProvider` stub + ast-grep upstream trava extensão |
| 7 | Qualidade | 5.8 | **7.0** | +1.2 | 0 `unimplemented` prod; CEG documenta zero-`unwrap`; dead_code majoritariamente documentado |
| 8 | Funcionalidades | 6.8 | **6.8** | 0 | LSP gap confirmado; `federation` é funcional (escopo ≠ symbols) |
| 9 | Documentação | 4.2 | **4.5** | +0.3 | README footer correto (A04 refutado); drift ARCHITECTURE confirmado/agravado |
| | **MÉDIA** | **5.2** | **5.8** | **+0.6** | Maturidade real é melhor; a dívida é de *organização*, não de *robustez grosseira* |

### 8.1 Refinamentos por achado (evidência in loco)

**A01 — Monólito `touring-hooks`** · `REFINADO`. `touring-hooks` = 171.273 LOC = **31,7% do workspace** (denominador real 540.046 LOC incluindo `tests/`; ~36% se só `src/`). Confirmado dominante. Os 116 arquivos da raiz agrupam-se em ~9 domínios naturais (CLI handlers, pre/post hooks, knowledge, RL, wiring, daemon/IPC, gateway, search). `FACT [1.0]`

**A02 — `lifecycle.rs` 19.444 LOC** · `REFINADO CRITICAMENTE (quase revertido)`. Análise de profundidade de chaves: **152 LOC de produção + 19.292 LOC de teste inline (99,2%)**, com **1.211 funções de teste** (10× o 2º maior arquivo). A lógica de produção **já migrou** para `lifecycle/` (19 submódulos, 12.746 LOC) — FIX-3 Fase A quase completa. O "monólito de produção" não existe; o que resta é migrar os 1.211 testes inline para os submódulos. `FACT [1.0]` **O achado original tratou 19.444 LOC como produção — erro material.**

**A03 — Drift ARCHITECTURE.md** · `CONFIRMADO E AGRAVADO`. Três documentos, três números para "crates": ARCHITECTURE.md=**45**, README=**36**, `cargo metadata`=**35**. LOC: 540.046 real vs 429.255 declarado (**+25,8%**). Tests: "5.100+" refere-se a testes que *passam* (`cargo test`), excluindo `touring-server` que tem **122 erros de compilação de teste** (ARCHITECTURE.md:771) — vs 10.686 anotações `#[test]` no grep. `Cargo.toml` tem **5 paths duplicados** (touring-analysis, -ast, -cognitive, -foundation, -web). `FACT [1.0]`

**A04 — README footer "14× stale"** · `REFUTADO`. O footer (`3002 files / 67698 symbols`) reflete o índice do **workspace** `~/.claude/rust` (atual 3043/69441 — drift de 1,4%/2,6%, README gerado 2026-06-04). A v1 comparou com o índice **global** agregado (30.489/1.118.176, `projects=5`). **Eram entidades distintas — a v1 cometeu erro de categoria.** `FACT [1.0]`

**A05 — Ciclo depth-683** · `REFINADO (tripla correção)`. (1) A topologia **Cargo é DAG acíclico** — `foundation/storage/orchestration` têm zero deps em `hooks/server` (grep exit=1); Cargo proíbe ciclos de crate estruturalmente. (2) O "ciclo" é um **SCC de 683 arquivos** no grafo de **wiring** (`SELECT module_file, consumer_file FROM wiring_map` + Tarjan em `wiring.rs:922`), não profundidade de recursão. (3) Os crates-fantasma (`touring-rule-engine`, `touring-definitions`) são **rows legadas com `workspace_root` NULL** — o campo foi adicionado em PLT-2026-06-02 (`knowledge.rs:153`); o fix de filtragem **existe** mas a CLI `touring wiring cycles` **não passa `workspace_root` por default** (`cli_handlers.rs:797`). Cycle-1 (depth-2, `capnp/holon_impl.rs↔server.rs`) é o único ciclo pequeno e real/acionável. `FACT [1.0]`

**A06/A07 — 3.495 unwrap + 112 panic** · `REFINADO`. Recontagem: 3.605 `unwrap` por grep, mas **~80% em código de teste** não filtrável por grep simples; estimativa de produção real **~300-600**. O gateway CEG documenta a invariante: `learn.rs:26` — *"No `.unwrap()` in production paths"*; `daemon.rs:1287` é infalível (`Capabilities::default()`). Dos 112 `panic!`, ~100 são test assertions; ~5 são *developer-contract invariants* em produção (ex: `speculative.rs:517`, caminho logicamente impossível). `FACT [1.0]`

**A08 — 38 `unimplemented`, "buraco em rust_semantic.rs:142"** · `REFUTADO`. Contagem real de `unimplemented!()` de produção: **0**. Todos os matches são (a) **strings que definem os patterns de detecção de antipadrões** (`antipatterns.rs:15-16`, `risk_patterns.rs:37,42`, `quality.rs:404,414` — o Touring *detecta* esses patterns), (b) fixtures dentro de `#[test]` (`rust_semantic.rs:142` está em `fn abstract_generics_lower_health_score`; `self_reflection.rs:555` em `fn test_analyze_code_with_todo`), (c) mensagens de diagnóstico. **A "ironia" da v1 é falsa — confundiu o detector com o defeito. Isto é uma força, não fraqueza.** `FACT [1.0]`

**A09 — Ausência de LSP real** · `CONFIRMADO E APROFUNDADO`. Zero deps LSP (`tower_lsp`/`lsp_types`/`LanguageServer` = 0 matches em src/ e Cargo.toml). Substituto próprio: tree-sitter `ParserPool` + 17 `DefinitionKind` + `source_to_definition()` (parent-walking no CST) — resolução **sintática, sem type inference/generics/lifetimes**. `find-references`/`rename` são **intra-file** (TODO D.2.2 em `cli_handlers_semantics.rs:550,630`; `--scope workspace` aceito mas **ignorado**, `_scope` na linha 217). `federation` **não é stub** — é agregação funcional de quality signals (escopo ≠ resolução de símbolos cross-repo). **Bug novo**: `cli_handlers_semantics.rs:179` tem format string malformado (`'{}:{}-{}(:{}'` — parêntese extra) afetando o output de `resolve-def`. `FACT [1.0]`

**A10 — Poluição de raiz** · `CONFIRMADO E AGRAVADO`. 11 `.tf-bak`, `libworkflow_templates.rlib` (144KB), `debug_js.js`. **NOVO CRÍTICO: `.gitignore` AUSENTE** — sem proteção contra `git add` acidental de `target/`, `fuzz/` (**4.3GB**, maior dir), `.tf-bak`, `mutants.out/`. REGRA #11 proíbe git, mas a ausência do safety net agrava o risco. 9 dirs órfãos identificados: `holon-wasm-*` são projetos HOLON separados; `reports/` está vazio; `fuzz/` é legítimo (W11.6) mas sem GC. `FACT [1.0]`

**A11 — `cli_handlers`** · `AGRAVADO`. Não são 12 satélites — são **15** (`cli_handlers_*.rs`, 9.727 LOC) + `cli_handlers.rs` (9.077) = **família de 18.804 LOC**. Este é o **verdadeiro hotspot de refatoração**, sem nenhum esforço FIX-3 equivalente ao de `lifecycle.rs`. `FACT [1.0]`

**A13 — 47 `allow(dead_code)`** · `REFINADO`. Recontagem: **41** (não 47). ~30 são test-helpers documentados (com códigos `EC62`/`EC64` explicando que `cargo check` não compila `#[cfg(test)]`) ou APIs planejadas com comentário; **~11 são candidatos reais a REGRA #0** (structs `auto_wire` não consumidas, campos `cgm/graph_attention`). `FACT [1.0]`

**A15 — Nomenclatura/MCP tripartido** · `CORRIGIDO`. (1) `crates/hooks` **não é crate Rust** — é um diretório de 3 shell scripts do Claude Code (`cc-*.sh`) sem `Cargo.toml`, anomalia estrutural em `crates/`. (2) Apenas **`inferlets`** é workspace member sem prefixo `touring-` (1/40). (3) O namespace MCP é **monolítico `touring_`** (161 tools únicos) — **`ctx_` não existe standalone**; `touring_ctx_*` é o maior sub-grupo (~30 tools). **A v1 inventou um namespace tripartido que não existe.** 13 shims, todos autodocumentados. `FACT [1.0]`

**A12/A16 — Extensão e schema** · `REFINADO`. 8 traits de extensão confirmados com `file:line`. RFC-006 **não existe** (só 001-005). Assimetria: `EmbeddingProvider` tem 3 impls reais (FastEmbed/Voyage/CandleBge), mas `LlmProvider` tem só 1 (`NoopLlm` stub). **Constraint novo**: adicionar linguagem está **bloqueado pelo upstream** — `Lang` é thin wrapper sobre `ast_grep_language::SupportLang`; Wave 5 (Markdown) **falhou** porque a linguagem não existe no upstream (`lang.rs:11-20`). `SCHEMA_VERSION=8` tem só **canary test** (`assert_eq!(==8)`), não migração V7→V8 real (migrações reais vivem em `knowledge.rs`, não `migration.rs`; `migration_e2e_audit.rs` testa consolidação 8→3 domains, não upgrade). `FACT [1.0]`

### 8.2 Achados NOVOS descobertos na verificação (não estavam na v1)

| ID | Sev | Achado | Evidência |
|----|:---:|--------|-----------|
| N01 | **P1** | `cli_handlers` família = 18.804 LOC = hotspot real, sem remediação (≠ lifecycle.rs já em FIX-3) | `ls cli_handlers_*.rs` → 15 satélites |
| N02 | **P1** | `.gitignore` AUSENTE na raiz — sem safety net contra `git add` de 4.3GB+ | `cat .gitignore` → not found |
| N03 | **P1** | `touring-server`: 122 erros de compilação de teste — os "5.100 tests" os excluem | ARCHITECTURE.md:771 |
| N04 | **P2** | Extensibilidade de linguagem travada pelo upstream `ast_grep_language::SupportLang` | `lang.rs:11-20`, Wave 5 Markdown falhou |
| N05 | **P2** | `LlmProvider` tem só 1 impl (`NoopLlm` stub) — contrato LLM não exercitado | `context.rs:2472` |
| N06 | **P2** | `fuzz/` = 4.3GB sem GC (maior dir não-target) | `du -sh fuzz/` |
| N07 | **P3** | Bug de format string em `cli_handlers_semantics.rs:179` (parêntese extra) no output de `resolve-def` | leitura direta |
| N08 | **P3** | `find-references`/`rename` aceitam `--scope workspace` mas ignoram (TODO D.2.2) | `cli_handlers_semantics.rs:217,550,630` |
| N09 | **P3** | `Cargo.toml` com 5 paths duplicados (resolve para 35 únicos) | `grep "crates/" \| sort -u` |
| N10 | **P3** | `wiring_map` cross-project contamination via `workspace_root` NULL (fix existe, CLI não usa) | `cli_handlers.rs:797` |

### 8.3 Matriz revisada — o que SOBREVIVE ao escrutínio (P0/P1 reais pós-verificação)

| ID | Sev | Status pós-verificação | Achado |
|----|:---:|------------------------|--------|
| A03 | **P0** | CONFIRMADO/AGRAVADO | Drift documental: 3 números p/ crates (45/36/35), +25,8% LOC, touring-server 122 erros teste |
| N01 | **P1** | NOVO | `cli_handlers` família 18.804 LOC sem remediação |
| N02 | **P1** | NOVO | `.gitignore` ausente |
| A01 | **P1** | CONFIRMADO | `touring-hooks` ~32-36% do workspace |
| A05 | **P1** | REFINADO | Wiring DB staleness (fix existe, CLI não passa `workspace_root`) |
| A09 | **P1** | CONFIRMADO | Sem LSP; resolução sintática; refs intra-file (TODO D.2.2) |
| A11 | **P1** | AGRAVADO | (consolidado em N01) |
| A02 | **P2** | REFINADO↓ | 1.211 testes inline em `lifecycle.rs` (não monólito de produção) |

**Rebaixados/removidos pela verificação**: A04 (refutado), A08 (refutado), A06/A07 (dívida muito menor que pintada), A13 (majoritariamente documentado), A15 (namespace consistente).

### 8.4 Meta-lição — o risco da auditoria por contagem bruta

A descoberta mais valiosa da Sessão 2 é metodológica: **auditoria por contagem de `grep` sistematicamente superestima dívida em dois cenários, ambos presentes no Touring no grau extremo:**

1. **Codebases com detectores de antipadrões**: o Touring *literalmente define as strings* `unimplemented!()`, `todo!()`, `.unwrap()`, `panic!()` em `antipatterns.rs`/`risk_patterns.rs` porque ele *detecta* esses patterns. Um grep cego conta o **detector como defeito**. (A08: 38 → 0 reais.)
2. **Testes inline massivos**: `lifecycle.rs` é 99,2% teste; contar suas 19.444 LOC como produção infla o "monólito" em ~128×. (A02.)

A correção é a **VP-Scout Cadeia 3b** (ler o corpo, não o nome) e a **regra C08/file-metadata-first** — exatamente os protocolos que a constituição TACO já prescreve. A própria v1 deste diagnóstico foi vítima do anti-padrão que pretendia auditar. **Isto valida a tese central "a cura está dentro" num nível recursivo: o método de auditoria do Touring (ler contexto, não contar) é o que corrige a auditoria do Touring.**

### 8.5 Impacto no roadmap

A revisão **reprioriza** o roadmap da §6:
- **Sai do topo**: campanha anti-`unwrap` em massa (a dívida real é ~300-600, não 3.495; gateway já é zero-`unwrap`); resolver `unimplemented` (são ~0).
- **Entra no topo (H0)**: criar `.gitignore` (N02, trivial e crítico); ativar `workspace_root` filter na CLI de wiring (A05, fix já existe); auto-gerar ARCHITECTURE.md (A03, o drift é o maior risco de credibilidade remanescente).
- **H1 refocado**: o alvo de decomposição não é `lifecycle.rs` (quase pronto) mas a **família `cli_handlers` (18.804 LOC)**; corrigir os 122 erros de teste de `touring-server`; migrar os 1.211 testes inline de `lifecycle.rs` para os submódulos.

---

## 9. Verificação Durante Execução (Sessão 3 — 2026-06-04, Master Plan H0)

> **Método**: ao executar as waves READY do [Master Plan](2026-06-04-touring-elite-masterplan.md) (A-W1/C-W1/D-W1/E-W1), cada achado foi re-verificado **uma terceira vez** — agora não por auditoria, mas por **ação real validada com gates** (`cargo test`, `cargo check`, `sync_metrics.py --check`). A execução **refutou mais 2 achados** que sobreviveram à Sessão 2, confirmando ainda mais a meta-lição §8.4.

### 9.1 Correções materiais ao diagnóstico (descobertas durante a execução)

| Achado | Status v2 (§8) | **Status v3 (execução)** | Evidência in loco |
|--------|:--------------:|:------------------------:|-------------------|
| **N09** Cargo.toml "5 paths duplicados" | P3 confirmado | 🔴 **FALSO POSITIVO** | As 5 ocorrências estão em **seções distintas**: `[workspace].members` (L3/13/15/28/66) vs `[workspace.dependencies]` path (L177/181/182/183) vs `[workspace.metadata.leptos]` (L87). Dentro de `members` **zero duplicatas** (`awk '/members/../^]/' \| uniq -d` → vazio). Removê-las **quebraria o build** (deps internas perdem path). **A03 §8.1 também errou ao listar isto.** |
| **N03** touring-server "122 erros de teste" | P1 novo | 🟢 **STALE / RESOLVIDO** | `cargo check -p touring-server --tests` → **0 erros** (`Finished 30.35s`). O claim de `ARCHITECTURE.md:771` estava desatualizado. Nota corrigida in loco. |
| **N07** bug format string `resolve-def` | P3 novo | ✅ **CORRIGIDO + TESTADO** | `cli_handlers_semantics.rs:179` `(` espúrio removido; extraída helper pura `format_source_range` (reduz CC de `resolve_impl`); 2 testes de regressão (`n07_*`) — `cargo test` **2 passed, 0 failed**. |
| **N02** `.gitignore` ausente | P1 novo | ✅ **RESOLVIDO** | `.gitignore` criado (target/, fuzz/target via `**/target/`, *.tf-bak.*, *.rlib, mutants.out/, etc.); fonte de `fuzz/` preservada (W11.6). |
| **A10** poluição de raiz | P2 confirmado | ✅ **RESOLVIDO** | 13 artefatos removidos (11 `.tf-bak` + `libworkflow_templates.rlib` 144KB + `debug_js.js`). Diátaxis `docs/{tutorial,how-to,explanation,reference,internal/sessions}` criada. |
| **A03** drift documental | P0 confirmado/agravado | ✅ **RESOLVIDO (durável)** | Header ARCHITECTURE.md corrigido (45/429k/5100 → 36/472k/13.272); criado `docs/sync_metrics.py` (gerador determinístico + gate `--check` anti-drift, 12 stages taco-forge PASS). |
| **A05** ciclo wiring 683 | P1 refinado | 🟡 **CONFIRMADO (persiste)** | `wiring cycles` ainda lista o SCC depth-683 com crates-fantasma (`touring-rule-engine`/`touring-definitions` inexistentes em disco). `index rebuild` **falhou** (RPC timeout em 1,1M símbolos). Fix durável = A.W3.P2 (CLI passar `workspace_root` por default, `cli_handlers.rs:797`). |

### 9.2 Métricas canônicas (via `docs/sync_metrics.py`, medido in loco 2026-06-04)

| Métrica | Valor (sync_metrics.py) | Nota |
|---------|------------------------:|------|
| crates (workspace_members) | **36** | `--check` confirma ARCHITECTURE.md sincronizado |
| LOC src (`crates/*/src`) | **473.777** | |
| LOC workspace (incl. tests) | **541.459** | |
| Funções de teste (`#[test]`/`tokio::test`/`rstest`) | **13.274** | vs "5.100+" stale; vs 10.686 da §8 (grep) |
| Maior crate | touring-hooks **171.560 LOC** | A01 confirmado (~36%) |
| Maior arquivo | `lifecycle.rs` **19.445 LOC** | A02 — 99% teste inline (FIX-3 em curso) |
| composite_health | **0,59** | flutua 0,59-0,69 entre sessões |

### 9.3 Veredito da Sessão 3

A execução **fortalece a tese central** num terceiro nível: não só o *método* de auditoria do Touring (ler contexto, não contar) corrige a auditoria (§8.4) — a própria *execução validada por gates* é o árbitro final. **Dois achados (N09, N03) que pareciam reais até na verificação forense da Sessão 2 só se revelaram falsos/stale quando uma ação concreta os testou.** Isto reforça o Princípio de Execução nº 6 do Master Plan: *evidência antes de afirmação*; e o nº 1: *dogfooding primeiro*. Dos 14 achados rastreados, após a execução: **6 resolvidos**, **2 refutados/stale**, **1 persiste com fix mapeado (A05→A.W3.P2)**, restantes em H1/H2.

---

### 9.4 Avanços da continuação (mesma sessão) — A05 forense + D-W2 doc-gen

**A05 — root cause forense aprofundado** (`a05-wiring-forensics-2026-06-04`): inspeção direta do DB (`~/.claude/touring/knowledge.db` via sqlite3 read-only) revelou que a tabela `wiring_map` tem **9 colunas e nenhuma é `workspace_root`** (id, module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at). Logo o predicado de `find_all_cycles` (`wiring.rs:877`, `workspace_root = ?4 OR workspace_root IS NULL`) referencia uma **coluna ausente** → a query falha silenciosamente (→ vazio) quando um root é passado. Os paths são **heterogêneos**: contaminação cross-project (`../../../ThemeContext`, `config/agentIdentities` — projeto React/konverter) **misturada** com paths Touring relativos a `$HOME` (`.claude/rust/crates/touring-definitions/src/lib.rs` — crate absorvido em W3.5, não existe). **Contradição não resolvida**: o daemon retorna 690 ciclos (logo o DB que ele usa *tem* a coluna no SELECT, ou usa outro DB), enquanto o `.db` inspecionado não a tem — sinal de múltiplos DBs / migração parcial. **Conclusão**: A05 não é um quick-fix de `workspace_root` default — é uma **wave de engenharia de dados** (A-W3.P2): (a) determinar o DB canônico do daemon; (b) `ALTER TABLE` + popular + segregar por projeto, OU filtrar arestas por existência-em-disco em `find_all_cycles` (resolver path: abs / `$HOME`-rel / cwd-rel; pular se nenhuma resolução existe) tornando o filtro opt-in para não quebrar `test_find_all_cycles_workspace_root_filter` (que insere paths fictícios).

**A05 — fix IMPLEMENTADO no código** (mesma sessão): a abordagem (b) foi implementada em `find_all_cycles` e **descobriu um 2º bug no PLT-2026-06-02**: o `ws_predicate` usava o placeholder `?4`, mas `query_map([root])` faz bind em `?1` — o `?4` ficava *unbound*, então o filtro `workspace_root` **retornava vazio (nunca funcionou)**. Corrigido `?4`→`?1`. Somado: helper `path_exists_resolved(p, root)` (resolve abs / root-rel / `$HOME`-rel / cwd-rel) + parâmetro `prune_nonexistent` (produção `true`, testes `false`) que descarta arestas cujos arquivos não existem em disco → elimina fantasmas + contaminação. **2 testes verdes** (`test_find_all_cycles_prune_nonexistent` hermético via `TempDir` + `test_find_all_cycles_workspace_root_filter` destravado pelo fix do placeholder); `cargo check -p touring-hooks` 0 erros. **Falta apenas o deploy** (binário daemon = 2026-05-31, anterior ao PLT; `update-touring` ativa o fix + o N07 em produção — não executado nesta sessão por ser disruptivo a múltiplas sessões CC, REGRA #19).

**A05 — DEPLOYADO E VALIDADO EM PRODUÇÃO** (Gabriel autorizou "deploy"): `update-touring` rebuildou o release (5m58s, 0 erros) com os fixes N07+A05; `touring daemon-ctl restart` carregou o binário fresco (REGRA #19, o pipeline pulou o restart por detectar daemon vivo, deixando-o "(deleted)" — corrigido com daemon-ctl). **Resultado: `touring wiring cycles --min-depth 2` = `cycle_count: 0`** (era **690** fantasma). O ciclo que contradizia o "acyclic, no back edges" do README **deixou de existir** — confirmando empiricamente a tese §8.1 (o Cargo sempre foi DAG acíclico; o SCC-683/690 era 100% wiring stale: crates absorvidos + contaminação cross-project). `orphan_count` real pós-deploy: 3.858 (vs o ruído de 169k dos hooks fail-open). **A05 está FECHADO.** N07 também ativo em produção (binário novo). `composite_health` 0.61 (daemon re-estabilizando enrichment pós-restart).

**D-W2 — doc-gen entregue** (dogfooding): criado `docs/gen_reference.py` (12 stages taco-forge PASS) que **extrai do source-of-truth** e gera `docs/reference/{generators,mcp-tools,hooks}.md` — **36 generator kinds** (via `touring generate list-kinds -j`) + **161 MCP tools** (grep handlers `touring-server`) + **218 hooks** (parse `ALL_DAEMON_HOOK_NAMES`). Modo `--validate` é gate anti-drift de CI (exit 1 se regenerar mudaria algo). Junto com `docs/sync_metrics.py`, materializa o Princípio 4 (metrics-as-code) — documentação que **não pode driftar** porque é gerada do próprio sistema (a cura está dentro).

**Estado DAG pós-continuação**: A-W1/C-W1/D-W1 completed · E-W1/C-W2/D-W2 in_progress · 13 pending. Itens H0 fechados: N02, A10, N07, A03 (durável), N03, N06, A14 (parcial). Refutados in loco: N09, N03. A05 refinado (wave dedicada).

---

### Apêndice A — Referências context7 consultadas
- `/salsa-rs/salsa` (benchmark 87) — incremental query system, query groups distribuídos por crates, red-green por revisão.
- `/ast-grep/ast-grep` (benchmark 87,67) — structural search/rewrite poliglota, rewriters compostos.

### Apêndice B — Comandos de verificação (reproduzir o ground truth)
```bash
cd ~/.claude/rust
touring status -j | jq '{composite_health_score, index}'
cargo metadata --format-version 1 | jq '.packages | length'
find crates/*/src -name '*.rs' -exec cat {} + | wc -l           # LOC
grep -rc '#\[test\]\|#\[tokio::test\]\|#\[rstest\]' crates/*/src # testes
wc -l crates/touring-hooks/src/lifecycle.rs                     # 19444
touring wiring cycles --min-depth 2 --format json               # ciclo 683
ls crates/touring-rule-engine crates/touring-definitions        # inexistentes
grep -rn '\.unwrap()' crates/*/src --include='*.rs' | grep -v test | wc -l  # 3495
head -3 ARCHITECTURE.md                                         # drift
```

### Apêndice C — Metodologia de scoring
Scores 0-10 atribuídos por 9 auditores Sonnet independentes contra a barra de elite (~9,1), reconciliados com medições próprias verificadas. Severidades P0 (bloqueador de elite) → P3 (cosmético). Confidence tags em toda alegação.

---

_Diagnóstico gerado por TACO (orquestração multi-agente: scout inline + 9 auditores dimensionais + 2 lentes de mercado + verificação adversarial + context7 + sequential-thinking) | 2026-06-04 | Touring v30.0.0_
