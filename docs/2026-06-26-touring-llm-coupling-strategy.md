# Estratégia de Acoplamento LLM ↔ Infraestrutura Touring

> **Da indução semântica cara para a afordância estrutural barata via Code Mode**
>
> **Data**: 2026-06-26 | **Sessão**: `e0f553d0` | **Autor**: TACO (Opus 4.8 1M) sob ordem de Gabriel Gadea
> **Documento-base (medição quantitativa)**: `2026-06-26-touring-token-footprint-diagnosis.md`
> **Insumo conceitual**: `~/.claude/downloads/Gemini-Orquestração Agêntica*.md` (2 monografias)
> **Método**: 2 docs lidos integralmente + 3 subagentes forenses (cli_suggester.rs, MCP server,
> status/sccache) com evidência file:line + síntese via sequential-thinking (5 passos).
> Tudo `[FACT]` é medido/lido do código; `[INFER]` é derivado com confiança indicada.

---

## 0. Sumário executivo — a virada

O diagnóstico de tokens da rodada anterior era o **sintoma**. A **doença** é a que Gabriel
nomeou: *as LLMs não têm prior de treino para se acoplarem à infraestrutura de código do
ambiente* — têm prior fortíssimo para `bash/grep/glob/cat` e **zero** para `touring index find`.

O Touring respondeu a isso com **indução semântica** em 3 frentes (rules massivas + cli-suggest +
158 MCP tools) que tentam *empurrar* a LLM para a infraestrutura. As 3 frentes **falham
estruturalmente** — e os documentos Gemini explicam por quê numa frase:

> **"Induzir proatividade não é problema de persuasão semântica via prompts verbosos. É um
> desafio de arquitetura de sistemas cibernéticos: o harness restringe ações inválidas por gates
> determinísticos e provê feedback estruturado."** — *Gemini, Orquestração Agêntica de Infraestrutura*

**A inversão proposta**: parar de gastar ~160K tokens *empurrando* a LLM (rules + hooks + MCP) e
investir em **tornar o touring a ferramenta de menor atrito** — saída densa, composável,
descobrível. A LLM adota o que tem **maior utilidade** `U(a)=P(sucesso)·V − C(tokens)`, não o que
é ordenado. Hoje `grep` tem U alto (barato, prior forte) e `touring wiring orphans -j` tem U
**negativo** (173K tokens de custo). Nenhuma quantidade de *"MUST touring"* vence essa economia.

**Três princípios operacionais** que amarram todas as frentes:
- **P1 — Afordância > Indução**: tornar touring barato/composável/descobrível faz mais que 110K de rules.
- **P2 — Alto sinal > Volume**: injeção rara, precisa, acionável. Volume genérico → *banner blindness* → ignorado.
- **P3 — Aproveitar o prior, não combatê-lo**: a LLM quer escrever scripts → fazer touring ser a *lib* natural desses scripts (Code Mode/PTC sobre a sandbox CEG), não forçar MCP tools sem prior.

---

## 1. O problema real — gap de acoplamento (a tese formalizada)

```
        PRIOR DE TREINO DA LLM                    INFRAESTRUTURA TOURING
   ┌──────────────────────────┐            ┌────────────────────────────────┐
   │ bash · grep · glob · cat  │   ⟍       │ touring index/ast/wiring (CLI) │
   │ Read · Edit · Write       │    ⟍ GAP  │ 160 MCP tools · taco-forge     │
   │ (function-calling atômico)│    ⟋       │ CEG sandbox · RL · memory      │
   │ U alto, prior forte       │   ⟋       │ U baixo, sem prior, alto atrito│
   └──────────────────────────┘            └────────────────────────────────┘
```

O doc Gemini ("Scripts vs Ferramentas") nomeia o mecanismo: **Desvio Atômico**. A LLM é
fine-tuned para mapear necessidade → 1 chamada de função e reagir; sob *greedy decoding* otimiza a
**menor distância cognitiva imediata** → dispara `grep` genérico em vez de orquestrar. Resultado:
*Poluição de Contexto* (schemas redundantes + logs brutos + histórico cumulativo).

A consequência prática que Gabriel observa: **mesmo com toda a infraestrutura, a LLM nem explora
um path com as ferramentas touring, ou as usa de forma incompleta, e recai no conjunto restrito
bash/grep/glob** — que nem sempre é efetivo/eficiente/eficaz.

---

## 2. Framework conceitual — os 2 docs Gemini destilados

### 2.1 CLI vs API/MCP — a matriz de acoplamento (doc 2, §3)

| Vetor | CLI (bash/grep) | API/MCP (JSON-RPC) |
|---|---|---|
| Densidade info | Muito alta, **não-estruturada** (parsing) | Alta, **estruturada** |
| Fricção cognitiva | **Elevada** (parser de strings/ANSI) | Mínima |
| Determinismo | Baixo | Máximo |
| **Descoberta/proatividade** | **Infinita** (encadeia, scripts ad-hoc) | **Limitada ao escopo do servidor** |
| Sobrecarga de contexto | **Grave** (stdout prolixo) | Otimizada |
| Risco | Crítico (injeção, loops) | Controlado (ACL, tipos) |

### 2.2 Veredito do doc: Híbrido Assimétrico Orquestrador-Trabalhador

1. **Orquestrador** (planner) opera via **API estruturada** — metadados limpos (grafos, schemas).
2. **Executores** (workers) operam via **CLI em sandbox** — compiladores/linters/testes nativos.

### 2.3 Os 3 protocolos de Engenharia de Contexto (doc 1, resposta 2)

- **Divulgação Progressiva** (*progressive disclosure*): não carregar schemas *upfront*; expor sob
  demanda via file-system virtual + `search_tools`. **O Claude Code já faz isso** com deferred
  tools + `ToolSearch` — mas o MCP touring expõe 160 de uma vez (não-deferred) quando ativo.
- **Compactação de Payload Local (PTC/Code Mode)**: a LLM escreve um script que processa/filtra na
  sandbox e retorna **só o sumário**. *"Redução de até 98,7% no consumo de tokens"*. Resolve a
  verbosidade (status 42K, orphans 173K) na raiz.
- **Estratificação de Memória** (working/episodic/semantic). O Touring já tem (`.remember`,
  `memory.db`, `MEMORY.md`).

### 2.4 Mecanismos de harness coercitivo (doc 2, §2) — força crescente

`afordância da ferramenta` > `Code Mode/PTC` > `gate coercitivo (PreToolUse <10ms)` >
`circuit breaker` > `progressive disclosure` > `redirect cirúrgico` > **`rules estáticas (mais fraco)`**.

> **O Touring investiu invertido**: pesado na camada **mais fraca** (110K de rules) e na **mais
> cara/ruim** (160 MCP atômicas), leve nas camadas fortes (Code Mode, afordância).

O que o Touring **já possui** e está subaproveitado: **CEG X0..X9** (sandbox + policy gate — é
exatamente o "Anti-Atomic Interceptor" do doc), **circuit breaker**, **memória multicamada**. A
infraestrutura coercitiva existe; o **modo de acoplamento** é que está errado.

---

## 3. Diagnóstico forense por frente (evidência file:line)

### 3.1 `cli-suggest` — persuasão semântica genérica + ruído fixo

`crates/touring-cli/src/cli_suggester.rs` (1855 linhas; lib `touring_hooks`). `[FACT]`
- **Arquitetura**: heurístico (regex + tabelas estáticas). `classify()` L349-361 → `classify_bash`
  (if-ladder 8 regex, L498-764), etc. `conf=0.85` é **literal hardcoded** por branch (L559), não
  calculado; o cálculo conformal (τ, L121-157) só decide **emitir/suprimir** (gate L1785).
- **Past-failures (57% da injeção)**: `retrieve_and_render_lessons` L1656-1679, montado em `run`
  L1823. SQL `key LIKE "outcome:{tool_class}:%:failure" LIMIT 15` (L1560-1566) — casa **qualquer**
  intent/arquivo, **sem filtro de relevância**. `memory_entries` sem timestamp → `recency=1.0`
  (L1582) → empate → **sempre os mesmos 5 transcripts fixos**. Budget 800 chars (L1661).
- **Verbosos `-j`**: literais por branch — `status -j`/`doctor -j` L545-559, `wiring orphans -j`
  L391/L752. Único que já filtra com `jq`: **L741** (o modelo a replicar).
- **O que já é BOM**: `classify_grep` roteia símbolo→`touring index find` (L835-863) e
  free-text→`touring tantivy search` (L867-887) — **o padrão certo**: pega a ação atômica e oferece
  a composável superior.

### 3.2 `prompt-enhance` — ~70% boilerplate, parte Python-stale

`crates/touring-hook-runtime/src/prompt_enhance.rs` (1689 linhas). `[FACT]`
- **Estático**: `TEMPLATES[6][8]` const (L838) indexado por (técnica × intent). **Não varia com o
  conteúdo do prompt**. CoT idêntico em 5 dos 8 modos.
- **Conteúdo errado para o ambiente**: `ConstitutionalConstraints` é **Python-centric** (PEP 8,
  type hints, bare except — L881-914) num workspace **Rust**. `action_directives` apontam para
  `python scripts/discover.py`, `mcp__gitnexus__*`, `mcp__serena__*` (L558-711) — **tools de outras
  skills, não o touring CLI**.
- **Inteligência touring real existe mas está ENTERRADA**: `touring_cli_hints_for_intent` (L506,
  comandos TIER-1 corretos por intent) e `taco_phase_for_cila` (L475, FASE 1→7) são emitidos **só
  como campos JSON** (`touring_cli_hints` L404), **não no `additionalContext`** que a LLM lê.

### 3.3 MCP exposure — curadoria travada em W1

`crates/touring-server/src/server/mod.rs`. `[FACT]`
- **160 tools sempre expostas** (163 `#[tool(` definidas). Assembly em `mod.rs:426-460` faz
  **merge INCONDICIONAL** dos 12 legacy routers (L434-445, **sem `cfg` guard**).
- **Gate de curadoria quebrado**: features `mcp-legacy`/`mcp-curated` declaradas (Cargo.toml:93-94)
  mas **nenhuma no `default`**. `mcp-legacy` é **no-op** (zero `#[cfg(feature="mcp-legacy")]` no
  código). `mcp-curated` é **aditivo** (só soma 3 tools → 162, não 22). A curadoria 102→22 parou no
  scaffolding (W1).
- Doc-strings stale: `get_info` diz "42 tools" (mod.rs:514); CLAUDE.md do crate diz "86 tools".

### 3.4 `status -j` — 95KB são a rede neural PPO serializada

`crates/touring-server/src/cli/status.rs::run` (não o `health.rs`). `[FACT]`
- **Culpado #1 (~60%, ~95-100KB)**: `learning.agentic_rl_state.policy` — a `PolicyNetwork` PPO
  inteira (**3810 f32**: hidden 25×64 + policy 64×32 + biases) flatten, cada elemento numa linha
  indentada. Vem de `export_state()` (`touring-hooks-rl/src/lib.rs:954-963`, `self.policy.clone()`).
- **#2**: `gate_metrics` 142 campos (~5-8KB). **#3 multiplicador**: `to_string_pretty` (L87) dobra
  o tamanho. **Não há flag `--brief`**. Já existe shape lean pronto em `health.rs:374-379`.

### 3.5 sccache — 2,72% é estrutural, não misconfig

`[FACT]` Configs lidas: `rustc-wrapper="sccache"` ativo, `25G`, `incremental=false` ✓ (correto),
**mold 2.30.0 é o linker ativo** (verificado por `readelf` — o comentário sobre gold-fallback é
stale/falso). O 2,72% é **inerente ao workload**:
1. *Churn* first-party: editar qualquer crate invalida ele + dependents; sccache só acerta em
   recompile idêntico → código mudado sempre erra.
2. Fragmentação multi-tool: clippy usa `clippy-driver` (**nunca cacheado**), llvm-cov/nextest =
   namespaces disjuntos.
3. Proc-macros + build scripts não-cacheáveis (4134 calls). 36 cache errors.

→ O valor real do sccache é **CI/clean/cross-worktree**. O 75% prometido (Cargo.toml:548) é
otimista. **Ortogonal a tokens.**

---

## 4. A inversão estratégica — afordância > indução

A economia que governa a escolha da LLM (doc 2): **`U(a) = P(sucesso|a)·V(sucesso) − C(t·tokens)`**.

| Ação | P(sucesso) | C(tokens) | U(a) hoje |
|---|---|---|---|
| `grep -r "Foo"` | alto | baixo | **alto** ✅ |
| `touring wiring orphans -j` | alto | **173K** | **negativo** ❌ |
| MCP `touring_*` (1 de 160) | médio | 38K schema + paralisia | baixo ❌ |
| script que chama `touring` + filtra (Code Mode) | alto | **baixo** (só sumário) | **alto** 🎯 |

A LLM já está fazendo a escolha **economicamente racional** ao preferir `grep`. Para mudar o
comportamento, muda-se a **economia**, não o **sermão**:

1. **Reduzir C(tokens) do touring**: `status --brief` (42K→<2K), saídas densas default, MCP curado.
2. **Aumentar P(sucesso) via composição**: Code Mode — 1 script que orquestra N comandos touring e
   retorna sumário (aproveita o prior "escrever bash").
3. **Afiar a indução restante** para alto-sinal: cli-suggest vira *redirect anti-atômico*
   cirúrgico (grep→`index find`), não banner genérico.

> A reconciliação do paradoxo de Gabriel ("MCP não funciona") com o doc ("orquestrador via API"):
> **ambos certos em escopos diferentes**. MCP **atômico de 160** = ruim. MCP **curado de ~15
> metadados estruturados** (ast meta, wiring impact, index find) serve ao cérebro-orquestrador. O
> **grosso** do trabalho deve ser **Code Mode** sobre a CLI na sandbox CEG — nem MCP, nem
> comando-a-comando.

---

## 5. Redesign por componente (concreto, file:line)

### 5.1 `cli-suggest` → "redirector anti-atômico de alto sinal"

| # | Movimento | Onde (file:line) | Efeito |
|---|---|---|---|
| 1 | **Cortar past-failures** | gatear `run` L1823-1829 | −57% da injeção; mata ruído fixo |
| 2 | **Cortar banners que ignoram operando** (cargo→doctor, git, pgrep, exec, task, webfetch) | confidence < τ em L537/L671/L702/L732/L369/L432 → gate L1785 suprime | mata *banner blindness* |
| 3 | **Afiar o redirect específico** (grep/sed/find/cat → `index find`/`tantivy search`) | manter+expandir L835-887, Pattern 1 L498-534 | alto-sinal: ação atômica → composável superior |
| 4 | **Nunca `-j` cru** — anexar `\| jq '{proj}'` (modelo L741) | L545/549/551/554/391/752 | corta picos de 42-173K |
| 5 | **Reduzir matchers 9→3** (Bash/Edit/Write) | `settings.json` PreToolUse | menos disparos, mais relevância |
| 6 | **SALTO — Code Mode redirect**: ao detectar varredura múltipla (2º+ grep, `for f in`, Read em loop), oferecer **snippet de script** (`for f in $(touring index files '*.rs'); do touring ast meta $f -j; done \| jq -s …`) | novo branch + usar o circuit-breaker/CEG que já detecta loops | materializa o PTC; induz 1 script macro em vez de N atômicas |

### 5.2 `prompt-enhance` → "scaffold de workflow" (não boilerplate)

- **Cortar**: CoT genérico + `ConstitutionalConstraints` Python-stale (L881-914) + `action_directives`
  que apontam gitnexus/serena/python (L558-711).
- **Promover ao texto** (hoje só JSON): `touring_cli_hints_for_intent` (L506) + `taco_phase_for_cila`
  (L475) → o `additionalContext` passa a carregar **os 3-4 comandos touring TIER-1 do intent + o
  esqueleto de decomposição** (FASE 1→7 / C01-C12). É exatamente o que Gabriel pediu: *decomposição
  de tarefas, alinhamento com rules/objetivos, etapas/workflows*.
- **Frequência**: só no 1º prompt da sessão ou quando o intent muda (não a cada mensagem).

### 5.3 MCP exposure → curado ~15

- Gatear os 12 legacy routers (`mod.rs:434-445`) com `#[cfg(feature="mcp-legacy")]` (hoje
  incondicional) — **o fix central**.
- Definir o router curado real (~15 tools de metadados estruturados); add `mcp-curated` ao
  `default`, remover `mcp-legacy` do default (Cargo.toml:84-88 já previa, nunca implementado).
- Corrigir doc-strings stale (mod.rs:514 "42"; CLAUDE.md crate "86"). **−~33K tokens** + fim da paralisia.

### 5.4 `status -j` → denso por default

- **Dropar** `learning.agentic_rl_state.policy` do dashboard (status.rs) — a PPO não pertence ao
  at-a-glance; expor em `touring learning export`. **−95KB**.
- **`to_string`** em vez de `to_string_pretty` no `-j` (status.rs:87) — **−50%**.
- **`--brief`** roteando para o shape lean já existente (health.rs:374-379). Princípio: **toda saída
  densa por default, verbosa opt-in**.

### 5.5 sccache / build (frente separada, não-tokens)

- **Aceitar** que 2,72% é o teto realista de dev first-party; **reframe**: sccache só para
  CI/clean/cross-worktree; `fast-iter` (incremental) no inner loop.
- **Ganho real de dev-loop**: `cranelift` backend (`rustc-codegen-cranelift` + `[profile.dev]
  codegen-backend`) — ataca o codegen que o sccache **não pode** cachear; 2-5× mais rápido.
- Investigar os 36 cache errors; consolidar config de linker stale.

---

## 6. Plano de execução priorizado (ROI = tokens × adesão)

| Pri | Ação | Ganho | Risco | Toca código? |
|---|---|---|---|---|
| **P0** | D-rules → skill references (manter keystone) | −54K base/sessão | baixo | não (mover .md) |
| **P0** | Desinstalar plugins **recipe** + **gws** | −parte de 44K | baixo | não |
| **P0** | cli-suggest: cortar past-failures + banners genéricos (mov. 1-2) | −21-37K/sessão **+ fim do banner-blindness** | baixo | sim (Rust) |
| **P0** | `status -j`: dropar PPO + `to_string` + `--brief` | 42K→<2K/uso | baixo | sim (Rust) |
| **P1** | rules top-level gordas → references (taco-forge-canonical 25K, decision-matrix 17K, TACO-subagent 17K) | −15-20K base | baixo | não |
| **P1** | cli-suggest: `\| jq` nos `-j` + reduzir matchers 9→3 (mov. 4-5) | corta picos | baixo | sim |
| **P1** | prompt-enhance → scaffold + hints touring reais (corta Python-stale) | +adesão, −boilerplate | médio | sim |
| **P1** | MCP curado 160→~15 (cfg-gate legacy) | −33K (sessões c/ MCP) | médio | sim |
| **P2** | **cli-suggest Code-Mode redirect** (mov. 6) — o salto de adesão | +++adesão | médio | sim |
| **P2** | cranelift backend (dev-loop) | −tempo build | baixo | config |

**Sequência sugerida**: P0 tudo (recupera ~80-130K e mata banner-blindness sem risco) → medir →
P1 → P2 (Code Mode, o salto qualitativo de adesão).

---

## 7. Métricas de sucesso — como medir adesão (não só tokens)

A meta tem **dois eixos**; medir ambos antes/depois:

1. **Custo** — base estática (cache_creation turno 1) e dinâmico/sessão. Alvo: base 178K→~110K;
   dinâmico 50K→~15K.
2. **Adesão (o que importa de verdade)** — **% de ações que usam infraestrutura touring vs
   bash/grep cru**. Instrumentável: já existe `record_enrichment_emitted(context.len())`
   (cli_suggester.rs L1838) + telemetria de tool-mix. Proxy: razão
   `(touring_cmds + code_mode_scripts) / (grep + cat + find atômicos)` por sessão. Alvo: subir.
3. **Sinal-ruído da injeção** — chars de `additionalContext` aceitos/agidos vs ignorados.
   Banner-blindness cai quando a injeção fica rara e precisa.

---

## 8. Apêndice — paths e correções de drift detectadas

- `cli_suggester.rs` está em `crates/touring-cli/src/` (não `touring-hooks/src/`). **REGRA #18 no
  CLAUDE.md tem path desatualizado** — corrigir.
- MCP "curado 22" da memória (`MCP Curated Plan 2026-06-06`) **não foi implementado** — só
  scaffolding. Atualizar o registro de memória.
- Doc-strings de contagem de tools desatualizadas (42/86 vs 160 real).
- `.cargo/config.toml` comentário de linker (gold) é falso — mold é o ativo.

---

## 9. Status de execução — atualização fim-de-sessão (2026-06-26)

Progresso real desde a redação (mesma sessão `e0f553d0`). `[FACT]` tudo medido/validado em produção.

### 9.1 Entregue
| Item do plano §6 | Status | Evidência |
|---|---|---|
| **P0** D-rules → skill references | ✅ FEITO | `rules/quality/` (51 D-rules) → `skills/touring-elite/references/quality/`; rules base 327KB→142KB (**−54.332 tok/sessão**) |
| **P0** `status -j`: dropar PPO + `--brief` | ✅ FEITO + ESTENDIDO | `policy` PPO dropada **e** `hypergraph_cycles.detail` (59K) elidida via novo `slim_large_arrays` (status.rs). `-j` 70K→**14K** (−80%); `--brief` 68K→**2.5K** (−96%); `wiring` 59K→**384B** (−99,3%). 8/8 testes, clippy 0 |
| **P2** (adiantado) sccache | ✅ REVISADO + FEITO | ver §9.2 |

### 9.2 Revisão material do §3.5/§5.5 — sccache NÃO era "ortogonal a tokens"
O diagnóstico original tratou sccache como **perf-only** ("aceitar 2,72%, sccache só CI"). A execução **refutou**: sccache é **correctness hazard**. Em 2026-06-26 serviu um **objeto stale** — build exit 0 ("Compiling touring-server") com binário **sem a edição** (binário≠source — literalmente uma *Class D silent failure* de infra, ver insights §1). Root cause (context7 `mozilla/sccache docs/Rust.md`): crates `bin`/`proc-macro` são non-cacheable + proc-macros que leem o filesystem podem cachear errado.
- **Ação**: sccache **desativado no workspace** (`.cargo/config.toml rustc-wrapper=""`) + `[profile.dev] incremental=true`. Prova: rebuild de ~10 crates com contadores sccache **inalterados** (req/misses/compilations). Detalhe: `memory/project_sccache_daemon_status_fixes_2026_06_26.md`; REGRA #12 atualizada (exceção documentada).

### 9.3 Achado adjacente (REGRA #19) — update-touring matava MCP bridges
Fora do eixo de tokens, mas grave: o `update-touring` tinha `touring serve` no pattern `pgrep` → **matava os MCP bridges de todas as sessões CC** (cascading kill). Corrigido: delega a `daemon-ctl stop|restart` (deleted-aware, siblings intactos).

### 9.4 Pendente + refinamento externo
Pendente do plano §6: `cli-suggest` redesign (mov. 1-6), `prompt-enhance` scaffold, MCP curado 160→~15, cranelift. **Refinados pela análise externa** em **`2026-06-26-harness-architecture-insights.md`** (*LLM Agentic Harness Architecture*) — em especial: **Active Output Summarizer** (generaliza o `slim_large_arrays` de hoje para qualquer saída >threshold no CEG — gap confirmado vazio), **DADL/2-meta-tools** (refina o MCP curado: `search_tools`+`execute_code` sobre a CLI, 142× medido, em vez de curar para 15), **RGAO router** (formaliza o CILA L0-L5 via vetor de complexidade do `ast blast`/`wiring`), **Class-D detector** (narrativa-vs-exit real). Esse doc traz a **munição quantitativa** (WarpGrep +3,7pp/−17% tokens; harness +22pp) que justifica o redesign do cli-suggest.

---

_Estratégia produzida com 2 docs Gemini integrais + 3 subagentes forenses + sequential-thinking
(5 passos). A virada: deixar de **empurrar** a LLM com indução semântica cara e passar a **atraí-la**
tornando o touring a ferramenta de menor atrito — afordância, Code Mode, alto-sinal. Aproveitar o
prior de escrever scripts, não combatê-lo._
