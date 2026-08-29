---
type: Strategy
title: "Paralelização e agentes especializados por fase — acoplar o motor parado aos caminhos que rodam"
description: "Onde e como paralelizar fases e especializar agentes no modus operandi TACO/Touring — diagnóstico por 3 evidências independentes + matriz read/write das fontes canônicas + 5 movimentos M0-M4"
tags: [strategy, loop-engineering, graph-engineering, paralelizacao, multi-agent, adw]
timestamp: 2026-08-29T10:15:00-03:00
plan_id: 2026-08-29-paralelizacao-agentes
---

# Estratégia — paralelização e agentes especializados por fase

> OUTER completo: diagnóstico ([/diagnostics/touring-20260829T093518.md](/diagnostics/touring-20260829T093518.md)),
> explore CCE convergido (7 lentes, external visitada com as 6 fontes originais lidas),
> 3 subagentes de pesquisa (fontes-teoria · inventario-adocao · lente-externa) + recall
> do feromônio. Este doc é o passo 7; o passo 9 (HUMAN GATE) é a apresentação a Gabriel —
> **nada de M0-M4 é implementado sem aprovação**.

## Diagnóstico (3 evidências independentes, todas medidas)

1. **A doutrina já está implementada; a adoção é ~zero.** O runner ADW tem diamante,
   sectioning (`concat`), voting (`tally`+quorum), pairing (worker-critic), fan-out
   dinâmico (Send), `max_branches` pré-checado, merge sem default, Class-D por ramo,
   personas com as 10 técnicas — e **zero dos 23 flows publicados contém um nó
   `parallel`**; `critic-panel` e `fanout-lenses` não são compostos por nenhum flow;
   `race` tem 0 runs; personas jamais executaram (40 dias de infra).
2. **O que roda é o que um executor dispara por construção.** 77 runs ADW em 7 dias,
   30% é `strategy-loop` — armado pelo Stop hook. Os 6 specs de 28/08 rodaram porque a
   estreia foi a entrega. Compliance: o OUTER inicia 434×/7d e conclui <50%.
   `plan_refine_iters = 1.0`. A tese ① (afordância muda U(a); persuasão não) confirma-se
   pela terceira vez, agora na topologia.
3. **A matriz das fontes vivas**: o eixo decisivo é **READ × WRITE** — leitura/exploração
   paraleliza quase de graça; escrita serializa ("actions carry implicit decisions",
   Cognition; "most coding tasks involve fewer truly parallelizable tasks than research",
   Anthropic). Custo: ~15× tokens vs chat; "token usage explains 80% of the variance".
   Especialização útil é por **função no loop** (executor ≠ verificador cego ≠ analisador
   de traces), não por persona. Delegação a subagente: ≥10 arquivos a ler ou ≥3 frentes
   independentes; razão nº 1 é isolamento de contexto. Effort scaling: 1 agente → 2-4 →
   10+ conforme a largura real da tarefa.

**Medições internas que calibram**: fan-out de fluxo não é ganho de relógio aqui (~15%,
`diagnose` 30,2s domina — 18/08); o daemon serializa handlers (5 recalls concorrentes →
3 mortos no budget de 15s, 29/08 — root-cause ANN corrigida na mesma sessão);
`touring.parallel` (SDK sandbox) nasceu 28/08 com o journal cego ao seu uso.
A própria sessão de 29/08 demonstrou o alvo: 3 subagentes read-only em paralelo
enquanto o contexto principal corrigia um P0 de infra.

## Os movimentos (M0-M4, ordem de execução)

| # | Movimento | O quê | Onde mora o enforcement (D8) | Tamanho |
|---|---|---|---|---|
| **M0** | **Instrumentar antes de esperar adoção** | `run_journal` ganha o sinal de `touring.parallel` por run (contagem de chamadas); o `parallel` do SDK aceita os nomes tipados do stub como alias dos hooks (`memory_recall` → `cli-memory-recall` — a fricção paga ao vivo em 29/08) | executor do sandbox (`run.rs`) + KPI novo | S/M |
| **M1** | **O juiz cego entra no caminho que já roda** | `[[use]] critic-panel` nos ADWs de auditoria que rodam por construção (`xaudit-gates`, 7 runs/7d) e no INNER 13 do loop-engineering; fecha P3 (gauntlet cego) e o evaluator-optimizer ("gate determinístico, não juiz" — o gate segue decidindo/L2, o painel fornece o feedback articulado que o retry precisa) | o runner ADW compõe; ninguém "lembra de usar" | M |
| **M2** | **Lentes do OUTER em fan-out** | `fanout-lenses` no `strategy-loop` (23 runs/7d por construção): cada lente do explore com contexto isolado — o breadth-first canônico da Anthropic; 6 lentes estáticas, custo teto conhecido | o spec do strategy-loop | M |
| **M3** | **Reflexo de delegação na sessão principal** | Regra na decision matrix + nudge no `cli_suggester`: exploração ampla (≥10 arquivos ou ≥3 frentes independentes, read-only) → sugerir subagente(s) com objetivo+formato+fronteiras; escrita NUNCA paralela; cadeia dependente fica no contexto único. Descriptions dos agentes TACO nomeiam o TRIGGER, não a persona | hook (a versão para SESSÃO do que o code-mode faz para comandos) + rules | S (regra) + M (nudge) |
| **M4** | **Doutrina do write-paralelo e do custo** | Documentar o gatilho de `race`/write-waves (write-sets disjuntos provados pelo `conflict-check`; unidades caras, independentes e verificáveis) e o teto de custo (15×; effort scaling) — prateleira com gatilho, não push de adoção | docs (code-mode.md / decision matrix) | S |

**Contrafactual declarado**: se em 30 dias `critic-panel`/`fanout-lenses` seguirem em
zero runs APÓS M1/M2, o problema não era afordância — era valor; a telemetria
(`touring.adw.*`, compliance.jsonl) decide remoção ou permanência.

## M5 — Quadro de especialização: QUAL agente exerce QUAL fase

Especializar por **função no loop**, nunca por persona decorativa (matriz externa +
Isenberg "checking is its own job" + tiering IndyDevDan "Scout+Planner: SOTA so nothing
gets missed"). O portador do ofício é o campo `skill` do nó ADW (craft) + persona
(postura) + tier (modelo). Quadro fase→agente com o estado de adoção medido:

### Loop-engineering (OUTER → INNER → CLOSE → META)

| Fase | Quem executa | Craft (`skill`) | Postura/Tier | Hoje | Move |
|---|---|---|---|---|---|
| OUTER 1-5 recall/diagnose | **código** (nós `code` do strategy-loop) | — | determinístico | roda por construção (23×/7d) | — |
| OUTER explore (lentes) | **N leitores**, 1 por lente, contexto isolado | — | persona com `lens` atribuída; light/workhorse | serial num só contexto | M2 |
| OUTER 6-8 estratégia | **orquestrador principal** (contexto pleno — decisão não se fragmenta, Cognition) | sequential-thinking | SOTA | correto hoje | — |
| OUTER 10 plano | **agente planner** | `taco-planning` (via plan-pack) | SOTA ("nothing gets missed") | fragment existe, 1 run (28/08) | M1b: plan-pack nos flows de criação |
| INNER 12 executar | **engineer** (nó worker OU contexto principal) | craft da fase | workhorse; escrita serial por write-set | contexto principal (ok p/ L0-L2) | M4 gatilha waves p/ L3+ |
| INNER 13 verificar | **painel de críticos CEGOS** (3, lentes distintas, `session="fresh"`, quorum por código) | `TACO-cross-audit` (audit-pack) | reject-by-default; workhorse | **NUNCA RODOU** — o maior furo | **M1** |
| INNER 14 phase-close | **código** (`loop_phase_close.py`) | — | determinístico | ok | — |
| CLOSE 16-17 docs | **scriber** | scriber + doc-link gate | light/workhorse | manual hoje | M1c: nó scriber no converge-close |
| META L4 traces | **analisador de traces** (o 4º loop da LangChain) | `skill-refine` (mine_transcripts) | workhorse | ADW existe desde 28/08, 2 runs | observar 30d |

### ADW (por tipo de nó) e TACO (fases 0-7)

| Nó/Fase | Agente | Estado medido | Move |
|---|---|---|---|
| ADW nó `agent` worker | persona worker + `skill` do ofício + tier | 24/47 specs têm nós agent; **0 declaram persona; `skill` em 9 declarações/6 flows (estreou 28/08, 5 runs)** | M5a: flows da library com nó agent ganham `skill` + tier |
| ADW nó crítico | persona reject-by-default, cega, fresh | só dentro de fragments; **0 execuções** | M1 |
| ADW `gate` | código FALANTE (a REASON ensina — A5) | ok desde 28/08 | — |
| ADW tiering | `tiers.toml` sota/workhorse/light (factory F5) | existe; adoção não medida | M0b: KPI de tier por run |
| TACO 1 scout / 2 architect / 5 engineer / 6 auditor / 7 scriber | os 5 agentes `touring-*` (~/.claude/agents/) | usados sob demanda; descriptions nomeiam persona, não TRIGGER | M5b: reescrever descriptions por gatilho ("Reviews X before Y" roteia melhor — medido no blog Claude Code) |

### Sessão principal (Claude Code)

| Situação | Agente | Critério (M3) |
|---|---|---|
| Exploração ≥10 arquivos / ≥3 frentes read-only | `Explore` / `general-purpose` em paralelo | isolamento de contexto é a razão nº 1 |
| Auditoria/review de entrega | `touring-auditor` ou critic-panel via ADW | verificador nunca é quem escreveu |
| Escrita | contexto principal, serial | "actions carry implicit decisions" |
| Papel recorrente novo | criar subagente dedicado SÓ quando o mesmo papel repetir (≥2 — aritmética, não entusiasmo) | skilling-pack |

**Agentes que NÃO criar** (aviso anti-superconstrução, 2 fontes independentes):
compressor de contexto dedicado (Cognition o quer, mas admite "hard to get right" — a
compaction do harness já cobre); persona standalone sem fragment (40 dias, zero
execuções — ela vive DENTRO do critic-panel/worker-critic-pair, e a adoção é derivada).

## Riscos

| Risco | Prob. | Mitigação |
|---|---|---|
| Custo de tokens do fan-out (15×) | MÉDIA | M1/M2 são read-only e curtos (painel de 3, lentes de 6); effort scaling documentado (M4) |
| Fan-out síncrono bloqueia no ramo lento | BAIXA | já documentado no runner; ramos curtos por desenho |
| Over-delegation (viés medido nos modelos atuais) | MÉDIA | gatilho numérico do M3 é o freio (≥10 arquivos/≥3 frentes) |
| Daemon serializa handlers sob fan-out | MÉDIA | anotado como limite; medir sob M0 (exec_pool default 4; `TOURING_CEG_MAX_CONCURRENT`) |

## Fontes

Internas: bundle 2026-08-18 (graph engineering — doutrina completa + ADRs 0002-0004),
2026-07-19 (factory: racing, conflict-check waves, tiering), memórias
`strategy:graph-engineering-refino-rodada2` (fan-out ≠ performance),
`audit2:graph-flow-portfolio-tudo-implementado`. Externas (lidas nos originais em
29/08): Anthropic *built-multi-agent-research-system* + *building-effective-agents*,
Cognition *dont-build-multi-agents*, LangChain *art-of-loop-engineering* + LangGraph
Send, arXiv:2505.22954 (DGM — paralelismo de exploração), claude.com/blog
*subagents-in-claude-code*, Harrison Chase *how-and-when-to-build-multi-agent-systems*
(a ponte read/write).

## Adendo 29/08 — veredito do cross-audit (pós-implementação)

Cross-audit completo sobre M0–M5 + fix ANN (`docs/audits/cross-audit-2026-08-29.md`,
commit `9e0f6d6`): **PASS com 2 correções feitas no próprio audit**. (1) A prova
do diamante M2 citada no fechamento não apontava artefato em disco — refeita ao
vivo: run `strategy-loop-1788014691`, `ground type=parallel`, ramos partindo no
mesmo ms, `parallel_joined`; ganho medido serial 33,5s → paralelo 25,3s (o ramo
menor sai do relógio, como a doutrina previa). (2) Os 2 KPIs novos ganharam
testes semânticos (`parallel_runs` distintos/STUB; `tiered_share` com tier:null
abaixando o share — a régua mede DECLARAÇÃO, não versão do runner). Provas vivas
re-executadas: aliases py+js (bordas incluídas), M3 no 10º arquivo exato
(counter 0→1), painel 3 críticos cegos `pass`, 50-dim 0.862–0.961 (≥Gold em
todos), P0 6/6 nos 8 arquivos.
