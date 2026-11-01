---
type: Strategy
title: "Estratégia — Touring próxima geração: harness modular com reinjeção medida"
description: "OUTER (Yetzirah) da criação Briah de 02/09/2026: veredito sobre o estado atual das 5 estruturas (telemetria padrão, telemetria estratégica, memória, processamento, reinjeção), decisões do grilling (contrato de módulo fora-de-processo + built-ins Rust; outcome v1 da régua), ondas medição-primeiro e o Pronto medido."
plan_id: 2026-09-02-touring-proxima-geracao
okf_version: 0.1
tags: [strategy, harness, modularizacao, reinjecao, telemetria, sistema-1-2, dsh, analise]
timestamp: 2026-09-02T06:50:00-03:00
---

# Estratégia — Touring próxima geração: harness modular com reinjeção medida

Parte do [bundle](/index.md). Concepção validada (Briah, ratio 1,0): [`criacao.md`](/criacao.md).
Diagnóstico: [`diagnostics/touring-20260902T062959.md`](/diagnostics/touring-20260902T062959.md).
Ledger CCE: `.touring-explore/evoluir-o-touring-a-proxima-geracao-de-harness--.ledger.json`
(3 rodadas, 11 achados, `converged: true`, lente `external` = dsh lido no clone).

## 1. A intenção (uma frase)

Fazer do Touring o **Sistema 1** do agente: uma plataforma de módulos em que **toda reinjeção de
contexto carrega um id, respeita um teto por turno aplicado no executor e é medida pelo efeito no
turno seguinte**, em que um módulo novo entra por um **contrato público fora-de-processo com diff
zero no núcleo**, e cuja primeira filha é o módulo `analise-regulatorio` do projeto analise.

Ordem obrigatória (Cadeia da concepção): **medição-primeiro**. A régua por injeção nasce antes do
primeiro módulo, porque o único fracasso que Gabriel teme é "morreu de contexto", e esse só é
detectável no número.

## 1.1 Cadeia causal (Yetzirah: os elos da concepção, medição-primeiro)

1. Se a telemetria PADRÃO registra cada evento agente↔harness↔usuário com identidade (sessão,
   turno, hook, módulo, bytes injetados) → então existe o corpus bruto que qualquer módulo consome (W0).
2. Se cada reinjeção carrega um id e o outcome do turno seguinte é atribuído a esse id → então a
   efetividade é mensurável POR INJEÇÃO, nunca por sensação (W1).
3. Se existe um contrato de módulo (registrar → tratar → reinjetar → medir) com interface única →
   então um módulo novo entra sem tocar o núcleo (W2; modularização exponencial à maneira do dsh).
4. Se contrato + régua existem → então o primeiro módulo (analise/lexhub/regulatório) nasce já medido (W3).
5. Se a régua mostra efeito baixo → então o módulo é refinado ou desligado por evidência; o
   aprendizado fica registrado (laço Sistema 1/Sistema 2 fechado; W1 + W5).
6. Se o laço fecha em um módulo → então o harness de próxima geração é o portfólio de módulos, cada
   um com KPI de efetividade (W4 + W5).

## 2. Evidência medida (02/09/2026)

| Sinal | Valor | Fonte |
|---|---|---|
| composite_health | 0,6864 | diagnóstico OKF |
| quality 50-dim | 0,9371 (Platinum), 0 blockers, warnings F1_1 F1_2 F1_3 F4_5 | diagnóstico OKF |
| orphans (baseline REGRA #0) | 5.530 no diagnóstico das 06:29 → **1.759 medidos às 11:00**, após a wave 14 de outra sessão corrigir 4 furos do detector de wiring | diagnóstico OKF + `touring wiring orphans -j` |
| hook_dispatch_latency | p50 0,65 ms · p90 133 ms · max 1,18 s (n=4.364) | `touring gate-metrics -j` |
| enrichment emitido | 81 emissões · 105.422 bytes · média 1.301 B/emissão | `touring gate-metrics -j` |
| KPI hooks_complement | disponível; `post_bash_delivery_ratio` medido por origem | `touring kpi -j` |
| KPI code_mode_signal_use | ratio 0,625 (5/8 hooks canônicos usados) | `touring kpi -j` |
| KPI code_mode_adherence | success_rate 0,918 (6.527 runs) | `touring kpi -j` |
| régua por injeção | **não existe** (bytes só em agregado; sem id; sem atribuição por injeção) | gate-metrics + kpi |
| contrato de módulo | **não existe** (signal layers são lista estática in-process) | ver §2.1 |

### 2.1 Chão de verdade do substrato (scout read-only + VGP nativa, 02/09)

Todas as linhas abaixo foram reverificadas contra o código por leitura direta (o relatório do scout chegou 40 min após o OUTER; dois números dele estavam errados e foram corrigidos aqui).

| Estrutura | Existe hoje | Evidência (file:line) | Lacuna |
|---|---|---|---|
| **Telemetria padrão** | sim, em **4 sinks desconexos** | `HookTraceLine` hook_trace.rs:32-55 (`ts_ms, pid, ppid, hook, route, exit_reason, stdin_state, stdin_bytes, elapsed_ms, session_id?`; opt-in `TOURING_HOOK_TRACE_FILE` :28, grava no `atexit` :174) · `JournalEntry` journal.rs:82-99 (`ts, language, exit_code, failure_kind, duration_ms, code_hash_stdout_bytes, bytes_elided`) · `HookCallEntry` sdk_signal_mirror.rs:51-67 (`ts, hook_name, duration_ms, success, origin`) · `hook_dispatch_by_name` gate_metrics.rs:1894 + `record_enrichment_emitted(bytes)` gate_metrics_snapshot.rs:1386 | nenhum sink carrega **turno**; só o trace tem `session_id`; o journal não tem nem sessão nem hook; os 4 só se cruzam em kpi.rs:541-585 por **janela temporal**, nunca por identidade |
| **Telemetria estratégica / reinjeção** | sim, mas **anônima** | `SignalLayer` signal_layer.rs:130-145 · `SignalContext` :48-64 (v2 com `tool_name`, `proposed`) · registro **estático por builder**: pre_write.rs:214-240 (9 `add_layer`), pre_edit.rs:518-538 (8), pre_read.rs:947-962 + `build_graph_pipeline` signal_pipeline.rs:338-369 · assembly e emissão: signal_pipeline.rs:99-153, `advisory_response` cli_suggester.rs:3765, hook_rewrite.rs:156 | nenhuma injeção carrega id de instância (`grep injection_id\|inject_id\|Uuid::new\|nanoid` nos 3 crates de hook = **0 linhas**); módulo novo exige editar o builder, isto é, **diff no core** |
| **Identidade que já existe** | classe, não instância | `ActionSignature::to_key()` action_signature.rs:139-146 → `outcome:<tool_class>:<intent_class>:<qualifier>`, colada como `sig=` em cli_suggester.rs:5727 | é o **molde** do id, mas agrega N injeções distintas na mesma chave |
| **Atribuição de efetividade** | parcial, **por arquivo** | `compute_context_bonuses` post_tool_rl.rs:58-76 lê `__context_injection_file__` e soma 0,1 se o caminho bate e não houve erro; reward `context_injection_quality` :264 · indução: `pillar_induction_{emitted,followed}` gate_metrics_snapshot.rs:706/712 | granularidade é **caminho de arquivo**, último gravador vence; **`pre_edit` não grava o marcador** (só pre_read.rs:362 e pre_write.rs:279) — logo **edição não é atribuível hoje** |
| **Teto de bytes** | sim, **por chamada de hook, graduado por CILA** | `cila_budget_read` cila.rs:33-51 = 800/2000/4000 e `cila_budget_edit`/`write` = 1200/3000/6000 (override por env) · aplicado no executor: signal_pipeline.rs:139 e :195 truncam antes de emitir · pressão observada: `record_enrichment_metrics` pre_read.rs:392-401 emite `ctx_budget_warning` a 75% e `ctx_budget_alert` a 90% · `DEFAULT_CONTEXT_BUDGET`=3.200 (pre_read.rs:974) é o fallback de `compose_high_signal_context` e dos hooks de compactação/sessão | nada **acumula bytes ao longo de um turno**, nem recusa o (N+1)-ésimo enriquecimento do mesmo turno |
| **Proxy de STR** | sim | `mean_enrichment_bytes = enrichment_context_bytes_total / enrichment_emit_count` gate_metrics_snapshot.rs:665/1368 | é média global, não por módulo nem por injeção |
| **Memória** | sim, facetada | `enum Facet` tags.rs:189-205 (7 facetas: Kind, Purpose, Lang, Domain, Process, Artifact, Status) | nenhuma lição indexada por módulo ou por injeção |
| **Taxonomia de eventos** | sim, em **três vocabulários incompatíveis** | (a) `ALL_DAEMON_HOOK_NAMES` hook_registry.rs:401 — **239 nomes**; (b) tabela de despacho no mesmo arquivo — **245 entradas distintas** (199 são `cli-*` de telemetria; lifecycle real: 9 `pre-*`, 9 `post-*`, 12 `task-*`, 3 `subagent-*`, 2 `session-*`, 2 `teammate-*`, 1 `ceg-*`); (c) 27 tipos de evento do Claude Code registrados em `~/.claude/settings.json` | nenhum enum tipado, nenhum **modo de despacho** declarado, e os três vocabulários não se reconciliam |

> **Correção de doc encontrada de passagem** (não aplicada — arquivo constitucional de Gabriel): a rule `~/.claude/rules/tool-combination-patterns.md:34` cita o counter `enrichment_signal_to_token_*`, que **não existe** (`grep signal_to_token crates/` = 0). O nome real é `mean_enrichment_bytes`.

### 2.2 Lente externa — DeepSeek Harness (dsh), recon read-only do clone

Fonte: `~/references/code-mode-2026-08-23/deepseek-harness` (nota de arquitetura `.agents/notes/implemented/architecture/2026-06-11-microkernel-event-taxonomy.md` e `docs/cookbook/extension-cookbook.md`).

- **68 eventos tipados em 4 modos de despacho** declarados em **pacotes de contrato, nunca no loop**: *waterfall* (around-middleware com `next()` e curto-circuito: `agent/pre-step` runtime-types.ts:231, `tools/pre-execute` core/tools/src/index.ts:152, `tools/post-execute` :175, `system-prompt/assemble` core/system-prompt/src/index.ts:31, `llm/stream`, `fs/write-intent`, `approval/request`, `session-telemetry/record`), *serial* (`agent/turn-stopping` :278), *parallel* (`session/flush` core/session/src/index.ts:85, o checkpoint de durabilidade), *emit* (`tools/result` :197, congelado e contido). O `dsh-agent-loop` é o único loop concreto e nada fora dele pode depender dele.
- **Plugin = módulo ES com 4 exports**, sem classe base e **sem registry central**: `name`, `inject` (serviços requeridos), `Config` (schema — config inválida falha o load) e `apply(ctx, config)` que só faz `ctx.on('<evento>', handler)`. Toda registração é `ctx.effect`, então HMR e disposal saem de graça.
- **Carga por manifesto**: `cordis.yml`, lista plana de `{id, name (pacote npm), config}`. Trocar um backend é trocar uma linha do YAML.
- **A injeção carrega identidade e prioridade**: `ctx.systemPrompt.section({ name, order, text })` — `name` é ID único (duplicata lança) e `order` é prioridade (−100 identidade, 0 persona, 100-199 guia de tools). Mensagens injetadas carregam proveniência tipada `MessageSource { kind:'plugin', plugin, form }` com `form ∈ instructions|catalog|snapshot|notice|relay|recall`.
- **O orçamento vive nas bordas, não no prompt**: o `system-prompt` só ordena e concatena; os tetos estão no *spill* (`maxInlineBytes`, 50 KB em produção — preview head/tail + locator com `retrievalHint` que ensina a recuperar), na compactação (`thresholdRatio 0.8`, `retainRatio 0.16`) e no pruner de resultado (`thresholdChars 8192`).
- **O mapa feature→mecanismo é obrigação de prova declarada e mantida atualizada** (cookbook: *"No row modifies the loop"*), reforçada por ~30 scripts `verify-*` no CI (`verify-package-invariants`, `verify-runtime-closure`).
- **O achado decisivo: o dsh NÃO mede eficácia de contexto injetado.** `grep -rn "effectiveness" packages/` = 0 e `grep -rn "reward" packages/` = 0; `attribution` e `correlat` só aparecem em identidade de SDK e IDs de RPC. Ele tem **proveniência** (`source.plugin`, `SpillSource.callId`) e tem **desfecho** (`tools/result` congelado, `severity`), mas **nada junta os dois**.

> Consequência estratégica: a **forma** (taxonomia tipada, plugin por manifesto, injeção com id e ordem) importamos do dsh; a **régua por injeção** é exatamente o que ele não tem, e é o que a Learning Memory do Touring torna possível. O diferencial da próxima geração não é ser modular como o dsh — é ser modular **e medida**.

| dsh mecanismo | Análogo Touring (existente → alvo) |
|---|---|
| taxonomia tipada com modo declarado, em pacote de contrato | 239 nomes em `ALL_DAEMON_HOOK_NAMES` + 27 eventos do settings.json, sem tipo nem modo → **HarnessEvent** + **DispatchMode** (S-11) |
| waterfall com `next()` e curto-circuito | pipeline sequencial de layers, sem short-circuit → modo declarado por inscrição (S-11) |
| parallel `session/flush` como checkpoint de durabilidade | `session-stop` + PreCompact → inscrição parallel (S-11) |
| plugin = `{name, inject, Config, apply(ctx)}` | 16 `impl SignalLayer` estáticos → trait **Module** + adapter (S-11) |
| carga por manifesto `cordis.yml` | listas `add_layer` nos builders dos hooks → **ModuleRegistry** + `.touring/modules/*.toml` (S-12) |
| seção com `name` único + `order` | injeção anônima, score interno → **InjectionId** + score já existente (S-1, S-6) |
| `MessageSource {kind, plugin, form}` (proveniência) | `ActionSignature::to_key()` (classe) → **InjectionRecord** com módulo e id (S-6) |
| spill: teto → preview + locator + `retrievalHint` | `touring run` spill + `--brief` (já existe, paridade) |
| `session-telemetry/record` waterfall como ponto de redação | nenhum ponto de redação antes do sink → considerar na W2 (fora do escopo atual) |
| companion `./invariant` por pacote + `verify-*` no CI | guards por script → `zero_core_diff` + mapa feature→mecanismo (S-12, S-14) |
| **medição de eficácia da injeção** | **não existe no dsh** → é a W1 inteira (S-6..S-10): o diferencial |

## 3. Diagnóstico por camada (lacunas que a criação fecha)

| # | Camada | Lacuna medida | Consequência hoje |
|---|---|---|---|
| L1 | Telemetria padrão | 4 sinks sem esquema comum de identidade; cruzam-se só por janela temporal (kpi.rs:541-585) | não há corpus atribuível; cada régua nova reinventa o join |
| L2 | Reinjeção | `additionalContext` sai sem id; bytes contados só em agregado | efetividade por injeção impossível; "morreu de contexto" invisível |
| L3 | Atribuição | atribui ao ARQUIVO (post_tool_rl.rs:58-76), último gravador vence, e **pre_edit sequer grava o marcador** — edição é inatribuível; `pillar_induction_followed` mede adesão só dos nudges de pilar | régua parcial, por família, cega a edições |
| L4 | Contrato | signal layers são lista estática Rust; toda camada nova = diff no núcleo + rebuild + propagação de toolchain | modularização linear, não exponencial; terceiros impossíveis |
| L5 | Executor | há teto POR CHAMADA graduado por CILA (800-6.000 chars, cila.rs:33-51, truncado em signal_pipeline.rs:139/195) e alertas a 75%/90%, mas **nada por TURNO** nem desligamento por evidência | o antídoto do risco principal não existe (D8: texto sem executor não enforça) |

## 4. Decisões (grilling do passo 6, Gabriel, 02/09/2026)

| Decisão | Escolha | Descartadas e porquê |
|---|---|---|
| Onde vive um módulo | **C**: contrato PÚBLICO fora-de-processo (JSON-RPC sobre stdio, manifest `.touring/modules/*.toml`, carregado por um `BridgeModule`) + built-ins do núcleo implementando o MESMO `trait Module` em Rust | A (só Rust in-process): viralização exigiria Rust de quem chega · B (tudo fora-de-processo): IPC nos hooks quentes, beira reescrita |
| Outcome v1 da régua | (1) sucesso/falha da próxima tool · (2) adesão: o agente seguiu o injetado · (3) STR por injeção: bytes injetados vs bytes referenciados depois | "ausência de correção em N turnos" adiada para v2 |
| Ordem | medição-primeiro (Cadeia A) | módulo-primeiro e contrato-primeiro: sem régua, "morreu de contexto" não é detectável |
| Intensidade Briah | I2 executivo; I3 recusado para a criação inteira | — |

Invariantes herdados (não negociáveis): fail-open dos hooks (nunca bloquear a sessão) · segredos
nunca entram em processo de módulo (A11: env_clear + whitelist) · enforcement no executor, texto e
predicado da mesma fonte (D8) · densidade de injeção (diretriz 29/06: derivar valor real, nunca
placeholder) · peer crates via subprocess, nunca import direto (padrão 12/04 e 31/08).

## 5. Ondas (medição-primeiro; cada onda com gate exit 0)

| Onda | Entrega | Gate medido |
|---|---|---|
| **W0 Identidade** | esquema único de evento `{session_id, turn_id, hook, module, injection_id?, bytes}` aplicado aos 4 registros existentes (hook trace, run_journal, sdk_signal_mirror, gate-metrics); `turn_id` derivado do par PreToolUse/PostToolUse | `touring kpi -j` expõe `telemetry_identity.coverage ≥ 0.95` das linhas com os 4 campos |
| **W1 Régua** | `InjectionId` determinístico (REGRA #17: hash de sessão+turno+módulo+conteúdo) em TODA emissão de `additionalContext`; atribuição do outcome v1 (tool result · adesão · STR) ao id no PostToolUse; KPI `reinjection_effectiveness{module}` com n mínimo; **teto de bytes/turno no executor** + desligamento por evidência (`module.disabled_by_evidence`) | KPI vivo com n≥30 para os built-ins atuais; teste que injeta acima do teto e prova o corte; mutação 0→1→0 na atribuição |
| **W2 Contrato** | `trait Module` (subscribe · observe · process · inject · measure = os 5 estágios), registry, taxonomia de eventos com modo de despacho (waterfall/serial/parallel/emit) documentada, `BridgeModule` (spawn, health, timeout, JSON-RPC stdio, manifest), 2 signal layers migradas como prova; `touring module {list,new,explain}`; mapa feature→mecanismo gerado | `git diff --stat` do núcleo vazio ao registrar um módulo de teste; p50/p90 por módulo medidos; guard CI feature→mecanismo |
| **W3 Filha** | `analise-regulatorio` em Python via bridge no projeto analise: inscreve UserPromptSubmit/pre_read/pre_edit, consulta lexhub + memória facetada, injeta contexto do domínio (regulatório/legal/contratual/administrativo; pipeline de documentos, relatórios, apresentações, GeoAI) com id | Pronto #1: n≥30 injeções atribuídas em sessão real, efeito acima do piso do módulo, bytes/turno abaixo do teto, ≥1 injeção desligada por evidência |
| **W4 Publicação** | spec do contrato, cookbook feature→mecanismo, template `touring module new`, 2º módulo (prova de diff zero por terceiro simulado), README e exemplos para quem chega de fora | Pronto #2/#3 instrumentados: `gh repo view --json stargazerCount,forkCount` no KPI; contadores de módulos de terceiros |
| **W5 Contínua** | medir → refinar → desligar por evidência; KPIs de adoção por módulo | `loop_converged.py` exit 0 por fase; régua nunca regride |

## 6. O Pronto (da concepção, medido)

1. **Primeira filha viva no analise** — `touring kpi -j` expõe `reinjection_effectiveness.analise-regulatorio` com n ≥ 30 e efeito acima do piso do contrato.
2. **Repositório hypado no GitHub** — `gh repo view --json stargazerCount,forkCount`; alvos PROPOSTOS (Gabriel calibra): ≥ 1.000 stars, ≥ 100 forks, ≥ 10 contribuidores externos.
3. **Viralização mundial** — ≥ 3 módulos de terceiros pelo contrato, contribuidores de ≥ 5 países, ≥ 10.000 instalações (PROPOSTOS).

Gates de processo em toda onda: `loop_converged.py --task <id> --scope <path>` exit 0 ·
`cargo check` + clippy verdes · `touring e2e -j` sem regressão · orphans ≤ 5.530.

## 7. Riscos e antídotos

| Risco | Antídoto (no executor) |
|---|---|
| **Morreu de contexto** (o único que Gabriel teme) | W1: teto de bytes/turno aplicado no executor + STR por injeção + desligamento por evidência; W0 dá a identidade que o torna visível |
| IPC nos eventos quentes (pre_read/pre_edit) | built-ins seguem in-process; bridge só para módulos externos; p50/p90 por módulo no KPI; timeout fail-open |
| Régua que mente (sinal ausente lido como zero) | fail-closed: injeção sem outcome atribuído conta como "não medida", nunca como efeito 0 |
| Juiz gravável pelo julgado | `judge_attest.py` no gate; KPI computado no daemon, não pelo módulo |
| Segredos no processo do módulo | env_clear + whitelist (A11); manifest declara capacidades deny-by-default (perfil CEG) |
| Reescrita disfarçada | fronteira: 2 layers migradas como prova, não todas; diff do núcleo medido por onda |

## 8. Próximo passo (HUMAN GATE, passo 9) — aprovado 02/09; plano: [`plan.md`](/plan.md), DAG `task_1788344667602060021`

Aprovação desta estratégia → `touring adw run plan-excellence --var intent="<§1>" --var
bundle=docs/plans/2026-09-02-touring-proxima-geracao --var level=L4` (autor ligado à craft
`taco-planning`) → DAG registrada → marker `active` → INNER por onda.

## 9. Artefatos

- [`criacao.md`](/criacao.md) · [`topic.txt`](/topic.txt) · [`adw-strategy-loop.log`](/adw-strategy-loop.log)
- [`diagnostics/touring-20260902T062959.md`](/diagnostics/touring-20260902T062959.md)
- memórias: `criacao:touring-proxima-geracao:2026-09-02`, `grilling:touring-proxima-geracao:2026-09-02`,
  `decisao:touring-proxima-geracao:fronteira-modulo:2026-09-02`
