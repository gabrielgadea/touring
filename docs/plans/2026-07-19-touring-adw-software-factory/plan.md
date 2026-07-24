---
type: Plan
title: "Touring ADW — Software Factory: plano completo de aperfeiçoamento (v2, pós-rodada 4)"
description: "Comparação vídeo VQy50fuxI34 × estrutura Touring + plano F0–F6 refinado: runner ADW código-orquestra-agentes com execução durável, loops de exploração/planejamento até convergência, ZTE conformal, evolução dos master CLI commands"
tags: [adw, software-factory, master-cli, exploration-loop, plan-refinement, durable-execution, touring]
timestamp: 2026-07-19
version: 2
status: AWAITING_APPROVAL
analysis: /video-analysis.md
---

# Plano — Touring ADW / Software Factory (v2)

> **Objetivo**: transformar o Touring de "sistema nervoso consultado pelo agente" em **Software
> Factory** — workflows declarados (código + agentes) executados por runner determinístico com
> **execução durável**, **loops de exploração e refino de plano até convergência medida** como
> primitivas de estágio, e master CLI commands como os nós de código dos workflows.

> **v2 (rodada 4)**: releitura do transcript + exploração profunda do repo + best practices
> externas. A rodada 4 **rebaixou o custo do plano**: várias peças que v1 classificava como
> "horizonte XL" já existem no Touring (ZTE conformal, write-set waves, execução com resume,
> templates de workflow) — o trabalho é ainda mais *montagem* e menos *construção* do que v1 media.
> Dogfooding: a rodada extra achou material relevante de novo, validando a tese do loop-until-dry.

## 0. Evidência da rodada 4 (o que mudou o plano)

**Lente A — transcript (releitura)**, 2 conceitos subponderados na v1:
- **Information orchestration** [28:31, 30:31]: *"separate the context out so that your context can
  move between individual agents and code… you're going to need a place for all the results in
  between each step."* → o ADW precisa de um **store tipado de resultados inter-nós**. FACT [1.0]
  (transcript).
- **Test the edges** [31:00]: *"you're going to need to test plan into build and to update the
  status and to testing and to fail"* → o spec ADW precisa de harness de teste próprio com mock/
  replay dos nós agente. FACT [1.0] (transcript).
- Reforço: **harness-agnosticismo** [05:01] (*"insert your favorite agent… it's about the
  workflow"*) → driver plugável no nó agente. FACT [1.0].

**Lente B — repo (verificação CLI 19/07)**, descobertas que rebaixam custo — todas FACT [1.0]
(output de `--help`/execução real):
- `touring decompose templates` — **10 templates de workflow W1-W10 (TR-5)** com steps
  (`tool_or_cmd`), padrões P1-P10 e pitfalls, consumidos por touring-web `/plans`. O conhecimento
  dos workflows JÁ existe em forma de template-prosa; falta o executor.
- `touring workflow run|stats|slowest|compare|resume` — tracking de execução **com resume**, não só
  visualização.
- `touring jobs spawn|poll|drop` — workers async reais (execve, sem shell).
- `touring calibrate-confidence` — **conformal prediction (KnowNo) operacional**: 2000 outcomes →
  814 exemplos de calibração, threshold 0.800 p/ coverage 90%, `defer_hitl: true`. **O ZTE do
  vídeo já existe com garantia estatística** — v1 classificava como horizonte XL; é wiring [M].
- `touring conflict-check` / `txn-acquire` — write-sets → ondas paralelas + TxnLockManager: a
  primitiva de serialização para corridas de agentes e engineers paralelos.
- `touring activity` — event log **append-only com replay/verify/projection**: o substrato de
  event-sourcing para execução durável.
- `touring governor` (ResourceGovernor), `cascade` (queue/drain), `predict-action` +
  `world-model-status` (prior de outcomes), `harness-metric` + `change-contract` (gate de
  não-regressão para automutação do harness).
- Arsenal Layer-3 confirmado em `~/.claude/skills/Touring/scripts/`: discover_symbol,
  discover_workspace, investigate, analyze_blast, read_file, pre_edit_gate, harness_gate +
  diagnósticos (systemic_diag_v2, crate_50dim_matrix, workspace_arch_diag, clone_blocks,
  vgp_batch) + `lib_touring.py` compartilhado.

**Lente C — best practices externas** (Context7 MCP indisponível nesta sessão — FACT [1.0] via
ToolSearch; fallback WebSearch com fontes nomeadas):
- **Claude Code headless / Agent SDK** ([docs oficiais](https://code.claude.com/docs/en/headless),
  [sessions](https://platform.claude.com/docs/en/agent-sdk/sessions),
  [guia CI/CD](https://hidekazu-konishi.com/entry/claude_code_cicd_and_headless_automation.html)):
  capturar `session_id` do JSON e `--resume <id>` para o feedback pass/fail na MESMA sessão;
  permissões **fail-closed** (`allowed_tools` exatos por nó — prompt inesperado trava automação);
  **`max_turns` + budget SEMPRE** (bounded work); `-p --output-format json`; sessão persiste a
  conversa, NÃO o filesystem (checkpoint de arquivos é preocupação separada → worktree/CEG).
- **Durable execution** ([Temporal best practices](https://raphaelbeamonte.com/posts/good-practices-for-writing-temporal-workflows-and-activities/),
  [Vanlightly — determinism](https://jack-vanlightly.com/blog/2025/11/24/demystifying-determinism-in-durable-execution),
  [LangGraph durable execution](https://docs.langchain.com/oss/python/langgraph/durable-execution)):
  workflow = **control-flow determinístico**; nós = side effects **idempotentes** executados
  exactly-once; recuperação por **replay do histórico** com nós completos pulados ("remembers their
  results"). Mapeia 1:1 no Touring: `activity` (event-sourcing) + `decompose` (status) +
  `workflow resume`.

## 0.5 Evidência da rodada 5 — lente institucional (memória, KBs, RL)

**O precedente taco-forge (o achado mais consequente)** — FACT [1.0] via
`memory/project_taco_forge_disconnect_loop_hardening_2026_07_02.md` +
`project_perfect_workflows_v3_elite_2026_05_24.md`: o Touring JÁ TEVE um sistema de
workflow-scripts (`perfect-create/edit/create-crate/create-script.sh`, 539-619L cada, 10-13 stages
internos com VGP/idempotência/flock/wiring) — desconectado por ordem do Gabriel em 02/07/2026
(*"não tem se mostrado eficiente"*). Análise da falha, INFERENCE [0.9]: taco-forge colocou a camada
de código na **granularidade errada** — envolvia cada tool call individual (1 edit = pipeline bash
de 619L invocado PELO LLM no lugar do Edit nativo), duplicando o que os hooks pre/post-edit já
faziam, com atrito a cada edição (nudges `perfect-*` constantes), modos de falha opacos (gotcha:
rewrite via ast-grep manglava arquivo com exit 0) e **zero orquestração de nível workflow** (o LLM
seguia decidindo tudo entre as chamadas). O ADW inverte na granularidade OPOSTA — e é exatamente a
do vídeo [27:30]: código entre **sessões de agente**, nunca entre o agente e as próprias mãos.
**REGRA #0**: o `lib/common.sh` v2 do taco-forge (tf_v2: timeout+retry+circuit-breaker em chamadas
touring, idempotency signatures com TTL, flock, stage timing, JSON mode) migra como biblioteca do
executor de nós `code` do runner — a engenharia sobrevive; o erro de granularidade não.

**Endurecimento do loop-engineering (02/07)** — FACT [1.0]: o incidente `missao-chile`×
`verifiers-dedup` produziu as leis de estado de runner que o F0 herda: marker **per-projeto** (nunca
singleton), **fail-OPEN sobre DAG morto** (só bloquear com confirmação positiva de subtasks
pendentes), **TTL 24h**, **archive na convergência**, e o gotcha `decompose get <missing>` retorna
`{task:null}` SEM chave `error` (checar ambos).

**Pesquisa de coupling 26/06 (I1-I10) × CLI atual** — FACT [1.0] cruzando
`2026-06-26-harness-architecture-insights.md` com a superfície verificada na rodada 4: os insights
P0/P1 de junho **já viraram comandos** — I1 Summarizer→C5 (CEG), I4 Class-D silent-failure→C9 (X9),
I6 budget conservation→`budget-verify` (Σ nós ≤ root), I9 consistency mesh→`consistency`
(GED+cosine). Viram componentes do runner ADW: sumário de output entre nós, **veredito de nó agente
nunca é a narrativa do agente** (Class D: narrativa-vs-exit), orçamento conservado por sub-DAG,
merge de engineers paralelos gatado por GED. `capability-map.md` §3 adverte a **patologia file-ref**
(*"file-based piorou 93→55% quando a LLM não relê"* — Is Grep All You Need?): o results store
inter-nós DEVE ser sumário-inline-first com `full_ref` opcional, nunca só ponteiro de arquivo.

**TACO-wt verificado** — FACT [1.0] via ls: toolkit determinístico de execução de waves já existe
(`scaffold_wave/forensic_runner/cross_audit/dimension_scorer/gap_detector/plan_validator/
evidence_collector/toon_checkpoint`) — seed direto de F0 (executor) e F2 (validação); duplicação
com taco-planning a consolidar no F6. **RL/evolution**: gotcha DB com 146 entradas e 182K hits
(profundidade institucional real); sem sinal plan-changing adicional.

## 1. Diagnóstico — vídeo × Touring (matriz de gaps, v2)

| # | Conceito do vídeo | Touring hoje (verificado) | Veredito v2 |
|---|---|---|---|
| G1 | ADW como artefato declarado | TACO = prosa executada pelo LLM; `flow` = pipeline de DADOS; **`decompose templates` W1-W10 = workflows em template-prosa** (novo, rodada 4) | **GAP CENTRAL**, mas com seed forte: templates existem, falta executor |
| G2 | Código orquestra agentes ("same session ID feedback") | `run --orchestrate` (R4) SDK read-only; headless `claude -p --resume` disponível no harness | **INVERSÃO AUSENTE** — driver headless é o elo que falta |
| G3 | Nós determinísticos com roteamento pass/fail | Gates prontos (audit, harness_gate, loop_converged, cargo); roteamento é model-driven | Nós prontos; falta o roteador-código |
| G4 | Scout→Plan split | TACO F1/F2 | OK, **single-pass** |
| G5 | **Exploração iterativa até secar** | Inexistente (1 rodada por protocolo) | **GAP #1 do pedido** |
| G6 | Refino de plano até platô | gap_detector/dimension_scorer/plan_validator single-shot; `plan-gated`/`plan-verified-depth` (MCTS CEG-gated) disponíveis | **GAP #2 do pedido** — o loop falta; verificação de profundidade já existe |
| G7 | Worktrees por agente | Harness CC (worktree isolation) | OK |
| G8 | Agent sandboxes | CEG X0-X9 sandboxa código (landlock+rlimit+caps) | Parcial — horizonte |
| G9 | Kanban intake → factory start | `decompose` + `tasksfile` YAML + **`cascade` queue/drain** (novo) | Parcial — peças de fila existem |
| G10 | Factory router (price/perf/speed) | `classify-intent` + `route` + `plan-chain` + LinUCB + **`predict-action`/`world-model`** (novo) | Peças fortes, desmontadas; falta catálogo ADW |
| G11 | Tiering de modelos | `model` no Agent tool; sem política | Codificar no spec |
| G12 | Biblioteca de ADWs especializados | Templates W1-W10 (prosa) + TACO L0-L4 genérico | Construir sobre os templates |
| G13 | Agent experts (expertise + memória curada) | agents/*.md + `memory recall` + `context` compiler | Formalizar memory-pack |
| G14 | Corrida de sandboxes (first-to-pass) | **`conflict-check` waves + `txn-acquire` + `jobs`** (novo) | Rebaixado XL→L: primitivas existem |
| G15 | ZTE — dropar review por confiança | **`calibrate-confidence` conformal + HITL defer OPERACIONAL** (novo) | **Rebaixado XL→M: é wiring**, não construção |
| G16 | "Separe código das skills" | Scripts chamados pelo agente | Migrar orquestração p/ runner |
| G17 | Masters como fusões de descoberta | 8 masters + arsenal + lição "adoption must be induced" | Evoluir p/ **nós de ADW** (adoção estrutural) |
| G18 | **Results store inter-nós** [30:31] (novo) | `activity` log + decompose artifacts + memory — sem contrato tipado | Formalizar no F0 |
| G19 | **Testar as arestas do workflow** [31:00] (novo) | Nada específico | `adw test` com mock/replay (F0) |

**Leitura essencial (v2)**: o vídeo valida a arquitetura Touring; o defeito estrutural é o motor do
workflow ser o LLM. A rodada 4 provou que a cura é *ainda mais montagem* do que v1 media: execução
durável (`activity`+`workflow resume`), ZTE (`calibrate-confidence`), corrida (`conflict-check`),
fila (`cascade`) e catálogo-seed (`decompose templates`) **já existem**. INFERENCE [0.9]: o custo
real do F0-F4 é dominado por integração + spec, não por engines novas.

## 2. O plano — 7 fases (F0–F6)

Princípios transversais (do vídeo + best practices):
- **"Do it by hand first"** [29:02]: cada ADW é primeiro o protocolo manual de hoje, desenhado em
  mermaid (`viz adw`), depois spec.
- **KISS** [27:00]: runner mínimo; sem DSL turing-completa.
- **Durable-by-design** (Temporal/LangGraph): control-flow determinístico, nós idempotentes,
  replay com skip de nós completos.
- **Fail-closed nos nós agente** (headless docs): `allowed_tools` exatos, `max_turns`, budget.

**Leis de projeto (destiladas nas rodadas 4-5 — invioláveis no design)**:

| Lei | Enunciado | Origem |
|---|---|---|
| **L1 Granularidade** | Código orquestra na fronteira **entre sessões de agente** (nós), NUNCA entre o agente e seus próprios tool calls. Wrapper de Edit/Write individual = taco-forge = proibido. | precedente taco-forge (§0.5) + vídeo [27:30] |
| **L2 Autoridade** | Término de loop e veredito de convergência pertencem ao **runner** (código sobre ledger); o LLM só alimenta. | forense 19/07 (F1.5 CCE) |
| **L3 Veredito ≠ narrativa** | O sucesso de um nó agente é decidido por **gates + detector Class-D** (narrativa-vs-exit, C9/X9), jamais pelo autorrelato do agente. | I4/C9 (§0.5) + REGRA #21 |
| **L4 Sumário-inline-first** | Contexto inter-nós = `{summary, omitted_bytes, full_ref}` — sumário denso inline; file-ref é complemento, nunca substituto (patologia Codex: 93→55%). | I1/C5 + capability-map §3 |

---

### F0 — Substrato ADW: spec + runner durável `touring adw` — **[L]**

1. **Spec declarativo** (`.touring/adw/<name>.toml`): nós tipados
   `code` (comando + args + timeout + CEG profile) · `agent` (prompt template + `tier` +
   `driver` + `allowed_tools` + `max_turns` + `budget` + session policy + worktree) ·
   `gate` (exit code roteia) · `loop` (corpo + saída + `max_iters` + `dry_rounds`) ·
   `human` (aprovação; bypass condicionado ao F5a) — edges `on_pass`/`on_fail`/`on_dry`.
   **Importador de templates**: `touring adw from-template W1..W10` gera spec-esqueleto do
   template-prosa correspondente (G1-seed).
2. **Runner** `touring adw run <name>`: nós `code`/`gate` via CEG X0-X9; nós `agent` via **driver
   plugável** — driver 1 = Claude Code headless (`claude -p --output-format json`, captura
   `session_id`, feedback pass/fail com `--resume <id>` — o conselho [27:30] com a mecânica das
   docs oficiais). Drivers futuros: codex/pi (harness-agnóstico [05:01]).
3. **Execução durável**: cada transição de nó → evento no `activity` log (append-only);
   status/ledger no DAG `decompose`; **retomada** (`adw run --resume-run <id>`) por replay do
   histórico com skip dos nós completos (semântica Temporal); telemetria via `workflow
   stats/slowest/compare`. Nós `code` DEVEM ser idempotentes (lint do spec avisa).
4. **Results store inter-nós (G18, forma da Lei L4)**: cada nó grava artefato tipado
   `{summary, omitted_bytes, full_ref}` (`.touring/adw-runs/<run_id>/<node>.json` + digest no
   activity log) — sumário denso inline via Summarizer CEG (C5); nós seguintes referenciam por
   template vars (`{{nodes.scout.summary}}`) — a "place for all the results in between each
   step" [30:31], sem a patologia file-ref.
5. **Qualidade do próprio harness (G19)**: `adw lint` (órfãos, ciclos sem saída, gates sem
   on_fail, code-node não-idempotente) + **`adw test`** — executa o workflow com nós agente
   mockados por replay de artefatos gravados (record/replay), testando as ARESTAS [31:00];
   `change-contract` gata automutações do harness (no-regression).
6. **Orçamento**: `governor` integra budget por run/nó + **conservação verificada** com
   `budget-verify` (Σ orçamentos dos nós ≤ orçamento do run — I6/C11).
7. **Estado do runner (leis herdadas do endurecimento 02/07)**: run state **per-projeto** (nunca
   singleton), **fail-OPEN sobre estado morto** (só bloquear/reter com confirmação positiva de
   pendências vivas; gotcha `decompose get <missing>` → `{task:null}` sem `error` — checar ambos),
   **TTL** e **archive** na conclusão. Veredito de nó agente segue a **Lei L3**: gates + detector
   Class-D (C9/X9), nunca o autorrelato. Executor de nós `code` absorve a biblioteca tf_v2 do
   taco-forge (timeout+retry+circuit-breaker, idempotency-signature TTL, flock) — REGRA #0 sobre o
   precedente, sem repetir sua granularidade (Lei L1).

*Aceitação*: ADW "hello-factory" (build agent → clippy gate → fail volta com `--resume` → pass →
relatório) roda ponta-a-ponta sem LLM orquestrador; kill -9 no meio → `adw run --resume-run`
retoma do nó exato; `adw test` passa com agente mockado; veredito diverge do autorrelato num nó
Class-D sintético (agente narra sucesso com exit≠0 → runner marca FAIL).

---

### F1 — `touring explore --until-dry`: loop de exploração multi-lente — **[M]** · GAP #1

O 9º master command, nó de descoberta dos ADWs — resposta direta à dor "uma rodada nunca basta":

1. **Rodadas multi-lente**: léxica (`index find`/`tantivy`) · estrutural (`wiring impact/chains`) ·
   institucional (`memory recall`+`gotcha`) · anti-staleness (grep polyglot Cadeia 4b) · qualidade
   (`ast meta/tdg`) · topológica (`graph communities`). **Compõe o arsenal existente**
   (discover_symbol/discover_workspace/investigate/lib_touring — verificados), não duplica.
2. **Ledger de exploração** (`exploration-ledger.json`): achado = `{id determinístico, lente,
   evidência CLI, rodada}`; dedupe de cada rodada contra o ledger INTEIRO (dedupe vs `seen`, não
   vs aceitos — senão nunca converge).
3. **Convergência**: para após K rodadas consecutivas sem achado novo (default K=2) ou
   `--max-rounds`. Relatório expõe a curva rodada×achados-novos — prova mensurável da incompletude
   da rodada 1 (KPI `explore_rounds_to_dry`).
4. **Lentes-agente opcionais** (em ADW): nó scout com tier SOTA [20:31] alimenta o mesmo ledger —
   LLM descobre, código conta/dedupa/decide parada.

*Aceitação*: em alvo real, ≥1 achado novo na rodada 2+ (validação empírica) + convergência provada.
*(Meta-evidência: nesta própria sessão, rodada 2 achou `flow`, rodada 4 achou templates/conformal/
waves — 4 rodadas, 3 com material novo.)*

---

### F1.5 — Contrato de Convergência de Exploração (CCE) — **[M]** · resposta à "rodada N+1"

> **Origem**: forense da sessão 19/07 — o plano v1 foi declarado pronto após 3 rodadas; a rodada 4
> (forçada pelo Gabriel) reclassificou 3 itens XL→M/L. Causas-raiz FACT [1.0], verificáveis nos
> tool calls da sessão: (a) `touring --help | head -100` truncou a superfície CLI e as descobertas
> maiores estavam no tail não-lido — **nenhum ledger contava cobertura**; (b) a lente
> best-practices externas **nunca rodou** nas rodadas 1-3; (c) `workflow` foi classificado como
> "só visualização" a partir de 1 linha de help — **suposição gravada como fato**, sem abrir
> subcomandos; (d) meta-causa: **o mesmo LLM que explora julga a completude** (satisficing — a
> sensação de cobertura cresce com tokens gastos, não com cobertura real).

Quatro mecanismos, um por causa-raiz. O princípio unificador é o do próprio vídeo aplicado à
convergência: **separar os atores** — explorar é trabalho de agente; *medir* completude é trabalho
de código; *contestar* completude é trabalho de um segundo agente adversarial.

1. **Matriz de cobertura lente×alvo (código; mata a causa a)** — antes de explorar, o runner
   **enumera deterministicamente os alvos**: comandos da dispatch table do daemon (o daemon SABE
   seus ~297 comandos — `command_table.rs`; `search-tools` já os indexa), árvore de subcomandos,
   arquivos do escopo, parágrafos do transcript-fonte. Cada célula lente×alvo termina
   `VISITED {digest de evidência}` ou `WAIVED {justificativa}` — **nunca implícita**. Outputs
   truncados são contabilizados pelo runner (`bytes_lidos/bytes_totais` por alvo): `head -100`
   numa superfície de 160 linhas vira célula 62% → INCOMPLETE, visível no gate. A exploração não
   pode ser declarada completa com célula UNVISITED.
2. **Rotação obrigatória de lentes (mata a causa b)** — "rodada seca" só é válida se o catálogo
   INTEIRO de lentes (léxica, estrutural, institucional, **externa/best-practices**, qualidade,
   topológica, adversarial) já rodou ≥1× contra cada alvo aplicável. Rodada N+1 com as mesmas
   lentes de N não conta para dryness — dry-rounds K=2 só valem **após cobertura plena**.
3. **Claims tipados com profundidade mínima (mata a causa c)** — todo achado entra no ledger com
   `depth ∈ {surface, opened, verified}`. Classificar um comando/símbolo a partir de 1 linha de
   descrição = `surface`; o gate exige `opened` (subcomandos/assinatura inspecionados) para
   qualquer item **citado no plano** — VGP aplicado à exploração, não só à geração.
4. **Crítico fresh-eyes adversarial (mata a causa d)** — antes de aceitar convergência, o runner
   spawna um agente **sem o contexto do explorador** (fresh eyes — o contexto do explorador o
   enviesa a ver completude) com um único job: dado escopo + matriz + ledger, **encontrar 1 item
   fora do ledger**. Achou → loop continua e o RL registra qual lente falhou
   (`learning reward` negativo no braço da lente); não achou (com tentativas evidenciadas) →
   convergência aceita. Incentivos GAN-like: crítico é recompensado por ACHAR, explorador por o
   crítico NÃO achar. É o papel que o Gabriel exerceu manualmente em 19/07, institucionalizado.

**Inversão de autoridade (a regra que amarra tudo)**: a decisão de terminar o loop pertence ao
**runner** (nó `loop` do ADW avalia o predicado sobre o ledger JSON), NUNCA ao LLM explorador — o
explorador só alimenta achados; não existe caminho em que ele declare "done". Mesmo padrão do
Stop-hook do loop-engineering (converge-or-continue), agora com a cláusula
`exploration_coverage` no gate.

**Convergência calibrada (potencialização de peça existente)**: o claim "exploração completa" vira
consumidor do **`calibrate-confidence`** (conformal/KnowNo, já operacional): o histórico de
falsas-convergências (KPI `post_convergence_finds` — achados APÓS convergência declarada, por
fresh-eyes ou humano) calibra um threshold com garantia de cobertura; claim abaixo do threshold →
força rodada extra ou **defer HITL** (o "pergunte ao Gabriel" vira exceção estatística, não rotina).
Economia: curva achados-por-token monitorada, mas **parada econômica nunca sobrepõe cobertura
incompleta** — budget esgotado com células abertas ESCALA para humano (circuit breaker), não para
silenciosamente.

*Aceitação*: replay do cenário 19/07 — dado o mesmo escopo, o gate DEVE recusar convergência na
rodada 3 (célula CLI-surface incompleta + lente externa não-rodada + claims `surface` citados no
plano) e só aceitar após o equivalente da rodada 4. KPIs: `post_convergence_finds` → 0,
`explore_rounds_to_dry`, `waived_cells` auditáveis.

**CCE v2 (pós-rodada 5 — a forense contra o próprio CCE v1)**. A rodada 5 encontrou material
decisivo MESMO com o CCE v1 desenhado, expondo 2 defeitos + 1 limite epistêmico:

5. **Profundidade de VISITA de lente (não só de claim)** — cada célula lente×alvo registra
   `visit_depth ∈ {D0 query-listagem, D1 abertura-de-fonte, D2 correlação-cruzada}`. A rodada 4
   marcou "institucional" com 2 recalls ruidosos (D0); a rodada 5 fez D1 (arquivos de memória) e
   D2 (KB × superfície CLI). Alvos que tocam o plano exigem ≥D1; alvos que fundamentam decisões
   de arquitetura exigem D2. Sem isso, célula rasa conta como coberta — a falha exata da rodada 4.
6. **Ledger de PERGUNTAS (alvos endógenos)** — após cada rodada, um passo obrigatório de
   question-generation (código enumera + agente propõe): *"quais perguntas estes achados tornam
   formuláveis?"* (ex.: plano propõe workflow system → "já construímos um antes? o que houve?" —
   a pergunta que só a rodada 5 fez, e que rendeu o taco-forge). Perguntas viram alvos novos na
   matriz — **a matriz é endógena, cresce com o entendimento**. Dryness passa a exigir: varredura
   de lentes seca E fila de perguntas derivadas vazia. O crítico fresh-eyes ganha segundo modo:
   além de "ache 1 item fora do ledger", **"formule 1 pergunta que ninguém fez"**.
7. **Cláusula de honestidade epistêmica** — o veredito do gate NUNCA é "exploração completa"
   (indecidível em mundo aberto); é sempre *"sob as perguntas correntes, profundidade ≥ D e
   rendimento marginal < ε, não há mais achados"* — datado e revisável. Declarar "completo"
   absoluto = a mesma narrativa Class-D que a Lei L3 proíbe nos agentes, aplicada ao harness.

---

### F1.7 — Scout Perpétuo: de convergência episódica a equilíbrio contínuo — **[M]** · o ponto fixo

> A resposta à recursão "preveja uma estrutura…" feita 2× (pós-rodada 4 e pós-rodada 5): um gate
> melhor sempre admite uma rodada N+1 que acha algo — porque os alvos são endógenos (CCE v2.6).
> O regresso só termina mudando o TIPO da resposta: **exploração deixa de ser fase com saída e
> vira processo permanente** — a Software Factory aplicada ao próprio conhecimento. É o L3 Event
> loop do loop-engineering (adiado no MVP), agora desenhado.

1. **Processo de fundo com cadência adaptativa**: um ADW `scout-perpetuo` re-executa
   explore (F1, com CCE v2) + question-generation contra o bundle vivo em background —
   `jobs`/cron/scheduled-agent — com cadência guiada por rendimento: achados novos → encurta o
   intervalo; rodadas secas → backoff exponencial (diário → semanal → mensal), **nunca a zero**.
   KPI `scout_yield_per_round` + curva de rendimento no bundle.
2. **Achados viram tickets (fecha o ciclo com F4)**: um achado com potencial de plan-delta não
   interrompe ninguém — vira **ticket na fila da factory** (`cascade`/`tasksfile`), roteado pelo
   factory router para um ADW `plan-revision` (ou notificação ao Gabriel quando toca decisão
   humana). O sistema de conhecimento e o sistema de trabalho unificam — exatamente a Kanban
   queue do vídeo [12:02], com a exploração como mais um produtor de tickets.
3. **O gate muda de pergunta: de "está completo?" para "agir ou esperar?"** — o human gate deixa
   de aprovar completude (indecidível) e passa a decidir **ação sob incerteza declarada**: o
   runner apresenta a curva de rendimento, as perguntas abertas ranqueadas por impacto no plano e
   a irreversibilidade da próxima fase. Agir NÃO para o scout — ele segue rodando durante a
   execução, e achados de meio-de-execução entram como change-orders (tickets). Com histórico, o
   próprio "agir vs esperar" é calibrável (`calibrate-confidence`) — defer HITL como exceção.
4. **Pressão internalizada**: as 2 intervenções do Gabriel ("faça mais uma rodada") não
   adicionaram informação — adicionaram **pressão**. O scout perpétuo + question-generation + o
   crítico em modo pergunta são o gerador de pressão institucionalizado. O papel humano sobe de
   nível: de bombear rodadas para arbitrar ação sob incerteza — as 2 restrições do vídeo [08:01]
   na forma final.

*Aceitação*: com o bundle deste plano como alvo, o scout-perpetuo roda ≥2 ciclos em background,
produz ≥1 ticket de achado (ou 1 ciclo seco com backoff correto), e o gate agir-vs-esperar
apresenta curva de rendimento + perguntas abertas. Limite honesto declarado: nenhuma estrutura
garante completude; esta garante que **o sistema, não o Gabriel, carrega a pressão** — INFERENCE
[0.85]; o que a refutaria: um achado crítico que nenhuma cadência razoável do scout alcançaria a
tempo (mitigação: fase irreversível sempre re-roda explore dirigido antes de executar).

---

### F2 — `touring plan refine`: loop de refino até platô — **[M]** · GAP #2

1. **Score de cobertura**: plano × ledger → % achados endereçados, símbolos VGP-verificados,
   claims sem evidência (reusa `gap_detector`/`dimension_scorer` 9-dim/`plan_validator` da
   taco-planning — hoje single-shot).
2. **Loop de crítica**: gaps → re-exploração dirigida (F1 nas lentes deficitárias) → re-plano →
   re-score; para em platô (Δscore < ε por 2 iters) E cobertura ≥ 0.9, ou budget.
3. **Profundidade verificada**: opcionalmente valida a cadeia de ações do plano com
   `plan-verified-depth` (MCTS até a profundidade CEG-verificada — EAGLE draft-tree) — claims do
   plano bounded por credibilidade executável, não por otimismo do LLM.
4. **Autocrítica estruturada** por iteração (nó agente completeness-critic): "o que este plano NÃO
   cobre? qual lente não rodou? qual claim está sem evidência?" — o "explore mais e refine" do
   Gabriel institucionalizado.
5. **Integração**: taco-planning `--refine-until-plateau`; loop-engineering OUTER vira loop
   explorar↔planejar convergido antes do human gate.

6. **Convergência estrutural (CCE aplicado ao plano)**: além do score, mede-se o **plan-delta**
   por iteração — fases adicionadas/removidas, reclassificações de esforço (ex.: XL→M da rodada 4),
   gaps novos na matriz. Plano só fica APPROVED-ready quando uma rodada fresh-eyes (F1.5.4, com o
   plano como alvo) produz **plan-delta = 0** — o crítico tenta achar 1 fato que MUDARIA o plano;
   falhou com tentativas evidenciadas → pronto para o human gate.

*Aceitação*: plano real com N≥2 iterações, score subindo até platô + plan-delta→0 sob fresh-eyes,
diff v1→final evidenciando o que a iteração encontrou. KPI `plan_refine_iters`.

---

### F3 — Biblioteca de ADWs + agent experts — **[L]**

1. **6 ADWs**: `explore-plan` (F1+F2) · `bugfix` · `feature` · `chore` (agente único, tier light,
   nunca o pipeline pesado [20:31]) · `hotfix` (surgical, velocidade>elegância [15:27]) · `audit`
   (masters audit+blast+investigate + cross-audit 50-dim). Derivados dos templates W1-W10 onde
   couber (`adw from-template`).
2. **Agent experts**: agente (.md) + **memory-pack** (`touring context` compila recall de
   lessons/gotchas do domínio no spawn — o "specialized set of mental memory" [16:00]) +
   prioridades explícitas + `allowed_tools` fail-closed por expert.
3. **Tiering**: `tier = sota|workhorse|light` por nó, mapeado em `tiers.toml` central.
4. **Engineers paralelos**: write-sets declarados → `conflict-check` computa ondas
   paralelizáveis → `txn-acquire` serializa conflitos (substitui a coordenação ad-hoc atual) →
   merge gatado por `consistency` (GED+cosine, I9/C14) — divergência semântica entre engineers
   bloqueia o merge antes do dano.

*Aceitação*: 6 specs lintados; `bugfix` e `chore` executados ponta-a-ponta em tarefa real;
1 execução paralela serializada por conflict-check.

---

### F4 — Factory router: `touring factory route|start` — **[M]**

1. `factory route "<ticket>"` → `classify-intent` (CILA C01-C12) + `route` (L0-L4+topologia) +
   `investigate --brief` (evidência do codebase) + **prior de `predict-action`/world-model** →
   ADW recomendado + tier + custo estimado (JSON). Determinístico primeiro; LLM router só para
   ambíguos [19:31].
2. `factory start "<ticket>"` → route + DAG decompose + `adw run` (sync ou `jobs`); fila via
   `cascade queue/drain`; intake externo via `tasksfile` YAML (bridge Kanban). L0-L1 permanece
   SOLO (chore não ganha factory pesada).
3. **Feedback RL**: outcome do ADW → `learning reward` no braço do router (LinUCB) — o router
   aprende qual ADW rende em qual categoria. KPI `router_accuracy`.

*Aceitação*: 3 tickets sintéticos (bug/chore/feature) roteados para ADWs distintos com
justificativa e execução iniciada.

---

### F5 — Factory avançada — **v2: parcialmente rebaixada de XL**

- **F5a ZTE conformal [M — era XL]**: nó `human` ganha bypass gatado por `calibrate-confidence`
  (conformal KnowNo já operacional: coverage 90%, defer HITL). Review humano é dropado APENAS com
  garantia estatística + auditoria a posteriori [21:00]. Direção: por-ADW, warm-up mínimo de M
  execuções aprovadas.
- **F5b Racing [L — era XL]**: N `adw run` paralelos (jobs + worktrees), write-sets via
  `conflict-check`, first-to-pass-gate vence, perdedores cancelados [16:30].
- **F5c Agent sandboxes [XL]**: elevar CEG de sandbox-de-código a sandbox-de-agente
  (container/namespace por agente headless).
- **F5d Horizonte org/produto [XL]**: ADWs embutidos em produtos com clientes como nós [26:30];
  intake de tickets org-wide (relay/Slack → tasksfile → cascade).

---

### F6 — Co-evolução: skill, rules, docs, memória — **[S]**

1. **loop-engineering v2**: reframe — loop = disciplina de convergência DENTRO dos estágios de um
   ADW; OUTER vira explorar↔planejar convergido (F2); INNER delega execução ao `adw run` quando o
   ADW existir.
2. `touring-cli-index.md` + skill Touring: `explore`/`plan refine`/`adw` no TIER 1; seção ADW.
3. `touring-4-pillars.md`: corolário "masters são os nós de código dos ADWs" — **adoção
   estrutural** (o runner os chama; fecha "adoption must be actively induced" por afordância —
   tese ①: affordance muda U(a), persuasão não).
4. `memory store` de decisões + KPIs novos (`explore_rounds_to_dry`, `plan_refine_iters`,
   `adw_runs`, `router_accuracy`, `zte_bypass_rate`) no `touring kpi`.

## 3. Sequência e dependências

```
F1 (explore) ──► F2 (plan refine) ─────────────┐
F0 (substrato ADW durável) ──► F3 (biblioteca) ─┼─► F4 (router) ──► F5a/F5b ──► F5c/F5d
F6 co-evolução contínua ────────────────────────┘
```

**Ordem recomendada**: F1+F1.5 (uma unidade — o loop E seu contrato de convergência v2; dor
declarada, valor imediato, zero deps) → F2 → F0 → F1.7 (Scout Perpétuo — precisa do runner F0
para rodar como ADW de fundo; versão interina via `jobs`+cron é aceitável antes) → F3 → F4 →
F5a (ZTE, barato agora) → F5b → F6 contínuo → F5c/F5d horizonte. F1 sem F1.5 reproduziria a
falha de 19/07 (término sob controle do LLM); F1.5 sem F1.7 reproduziria a recursão das rodadas
4-5 (gate episódico sobre mundo aberto — o Gabriel como gerador de pressão).

## 4. Riscos e mitigações (v2)

| Risco | P/I | Mitigação |
|---|---|---|
| Runner vira motor genérico (scope creep) | MED/ALTO | KISS: grafo + loops; sem DSL completa; `flow` cobre plumbing de dados |
| Nós agente headless travam/estouram custo | MED/MED | Best practices nomeadas: `allowed_tools` fail-closed, `max_turns` SEMPRE, budget via `governor`, captura de `session_id`, `-p --output-format json` (nunca dialog interativo) |
| Exploração não termina | BAIXA/MED | `--max-rounds` + escopo obrigatório + dedupe determinístico |
| Masters sub-adotados | MED/MED | Adoção estrutural via runner (F6.3) |
| Duplicar Workflow tool do CC | BAIXA/MED | Complementares: Workflow tool = ad-hoc de sessão; ADW = artefato persistente/versionado/roteável; um pode invocar o outro |
| Replay não-determinístico (durabilidade quebra) | MED/ALTO | Semântica Temporal: control-flow puro no runner; side effects só em nós; `adw lint` avisa código não-idempotente; timestamps/random proibidos no spec |
| ZTE dropa review cedo demais | BAIXA/ALTO | Conformal coverage 90% + warm-up M execuções + auditoria a posteriori + REGRA: irreversíveis sempre human |
| REGRA #11/#19 | BAIXA/ALTO | CEG em todo nó code; git segue proibido; daemon-ctl only |
| **Decompose como ledger F0 não é confiável HOJE** (auditoria 20/07: 2.334 containers zumbis, double-create no task-sync, sync create-only, `validate` retorna `valid:true` em task inexistente, shards fragmentados/divergentes, 0 counters de observabilidade) | ALTA/ALTO | **Wave de higiene decompose é pré-requisito do F0** (F0-pre): fix double-create + propagação update/complete + validate honesto em task ausente + unificação/roteamento de shards + counters task_sync_* + GC de zumbis. Activity log auditado ÍNTEGRO (11.168 eventos, hash OK) — a escolha dele como substrato durável se confirma |

## 5. Gate de aprovação (HUMAN GATE)

Decisões abertas para o Gabriel:
1. Ordem proposta (F1→F2 primeiro) ou começar pelo substrato F0?
2. `touring-adw` como crate novo ou módulo em `touring-cli`?
3. F1/F2 nascem como scripts Layer-3 (padrão masters, mais rápido) ou nativos em Rust?
4. **(novo, v2)** F5a ZTE conformal entra no MVP (é wiring de peça existente) ou fica pós-F4?

## 6. Fontes externas (rodada 4)

[Claude Code headless](https://code.claude.com/docs/en/headless) · [Agent SDK sessions](https://platform.claude.com/docs/en/agent-sdk/sessions) · [CI/CD headless guide](https://hidekazu-konishi.com/entry/claude_code_cicd_and_headless_automation.html) · [Temporal best practices](https://raphaelbeamonte.com/posts/good-practices-for-writing-temporal-workflows-and-activities/) · [Vanlightly — determinism in durable execution](https://jack-vanlightly.com/blog/2025/11/24/demystifying-determinism-in-durable-execution) · [LangGraph durable execution](https://docs.langchain.com/oss/python/langgraph/durable-execution)
