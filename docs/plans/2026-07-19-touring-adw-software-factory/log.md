---
type: Log
title: "Log — Touring ADW / Software Factory"
description: "Histórico cronológico do bundle"
timestamp: 2026-07-19
plan: /plan.md
---

# Log

## 2026-07-19T22:20-03:00 — Criação do bundle
- Transcript integral do vídeo VQy50fuxI34 obtido via yt-dlp (auto-subs EN, 329KB VTT → 37K chars
  deduplicados, 69 parágrafos com timestamps).
- 3 rodadas de exploração da estrutura Touring (dogfooding do loop-until-dry): (1) contexto/memória
  constitucional; (2) superfície CLI real — achado novo: `touring flow` YAML pipeline; (3)
  `flow list` (confirma: pipeline de dados, sem nós de agente) + `memory recall` ×2.
- Análise integral escrita (video-analysis.md) + matriz de gaps G1–G17 + plano F0–F6 (plan.md).
- Status: **AWAITING_APPROVAL** — human gate (loop-engineering passo 9) antes de implementar.

## 2026-07-19T23:05-03:00 — Rodada 4 (pedido do Gabriel: mais uma rodada + Context7)
- **Lente A (transcript)**: 2 conceitos subponderados incorporados — information orchestration
  [30:31] (results store inter-nós → F0.4/G18) e test-the-edges [31:00] (`adw test` → F0.5/G19);
  harness-agnosticismo [05:01] (driver plugável).
- **Lente B (repo, CLI verificada)**: `decompose templates` W1-W10 (workflows em template-prosa!),
  `workflow resume`, `jobs spawn/poll/drop`, `calibrate-confidence` conformal OPERACIONAL
  (ZTE rebaixado XL→M), `conflict-check`/`txn-acquire` (racing rebaixado XL→L), `activity`
  replay/projection (substrato de execução durável), `governor`, `cascade`, `predict-action`,
  `harness-metric`+`change-contract`. Arsenal Layer-3 confirmado por ls.
- **Lente C (best practices)**: Context7 MCP indisponível na sessão (ToolSearch confirmou);
  fallback WebSearch — Claude Code headless (session_id+--resume, allowed_tools fail-closed,
  max_turns sempre) + durable execution (Temporal/Vanlightly/LangGraph: determinismo no
  control-flow, idempotência nos nós, replay com skip).
- **plan.md reescrito como v2**: G18/G19 novos; F0 ganha execução durável + results store +
  adw test + drivers; F2 ganha plan-verified-depth; F3 ganha conflict-check; F4 ganha
  cascade/tasksfile/predict-action; F5 dividido (F5a ZTE [M], F5b racing [L], F5c/F5d [XL]);
  decisão aberta #4 (ZTE no MVP?). Dogfooding: 4ª rodada achou material novo de novo.

## 2026-07-19T23:40-03:00 — F1.5 CCE (pedido do Gabriel: estrutura que elimine a "rodada N+1" manual)
- Forense da própria sessão (causas-raiz FACT, verificáveis nos tool calls): (a) `head -100`
  truncou a superfície CLI sem contabilidade de cobertura; (b) lente best-practices nunca rodou
  nas rodadas 1-3; (c) `workflow` classificado por 1 linha de help (suposição gravada como fato);
  (d) meta-causa: o LLM explorador julgava a própria completude (satisficing).
- **F1.5 — Contrato de Convergência de Exploração**: matriz de cobertura lente×alvo enumerada por
  código (dispatch table/search-tools; truncamentos contados) + rotação obrigatória de lentes
  (dryness só vale após cobertura plena) + claims tipados por profundidade
  (surface/opened/verified; plano só cita `opened`+) + crítico fresh-eyes adversarial sem contexto
  do explorador (incentivos GAN-like via learning reward) + **inversão de autoridade** (só o
  runner encerra o loop) + convergência calibrada (calibrate-confidence conformal sobre o
  histórico de falsas-convergências; KPI `post_convergence_finds`; defer HITL como exceção
  estatística).
- F2 ganha convergência estrutural: **plan-delta = 0 sob fresh-eyes** antes do human gate.
- Sequência atualizada: F1+F1.5 são uma unidade (F1 sem F1.5 reproduz a falha de 19/07).
- Aceitação do CCE: replay do cenário desta sessão DEVE recusar convergência na rodada 3.

## 2026-07-20T00:15-03:00 — Rodada 5 (lente institucional: memória Touring, KBs, RL)
- **Precedente taco-forge descoberto e analisado** (memórias 02/07 + 24/05): sistema de
  workflow-scripts que envolvia cada tool call individual (perfect-edit ≈ Edit com pipeline bash
  619L), desconectado por ineficiência. Vira a **Lei L1 (Granularidade)**: código orquestra entre
  sessões de agente, nunca entre o agente e seus tool calls. tf_v2 (lib/common.sh) migra p/ o
  executor de nós code (REGRA #0).
- **Leis de estado do runner** herdadas do endurecimento do loop-engineering 02/07: marker
  per-projeto, fail-OPEN sobre DAG morto, TTL, archive, gotcha `{task:null}` sem `error`.
- **Cruzamento I1-I10 (docs 26/06) × CLI atual**: I1→C5 Summarizer, I4→C9 Class-D, I6→budget-verify,
  I9→consistency — já shipped; viram componentes do runner (Leis L3 veredito≠narrativa e L4
  sumário-inline-first; patologia file-ref 93→55% evitada no results store).
- **TACO-wt** verificado (8 scripts) — seed de F0/F2; consolidação de duplicatas no F6.
- **plan.md v2.2**: +§0.5 (evidência institucional), +tabela de 4 Leis de projeto, F0 itens 6/7
  ampliados (budget-verify, estado per-projeto/fail-open/TTL, Class-D no veredito, tf_v2),
  F3.4 ganha gate GED (`consistency`), aceitação F0 ganha teste Class-D sintético.

## 2026-07-20T00:50-03:00 — CCE v2 + F1.7 Scout Perpétuo (2ª iteração da pergunta "estrutura p/ rodada N+1")
- Forense contra o próprio CCE v1 (por que não teria evitado a rodada 5): (a) profundidade de
  VISITA de lente não contabilizada — institucional rodou D0 (2 recalls ruidosos) na rodada 4 e
  contaria como VISITED; rodada 5 fez D1/D2 (arquivos + correlação); (b) **alvos endógenos** — a
  pergunta "já construímos um workflow system antes?" só existiu DEPOIS do plano propor um; a
  matriz cresce com o entendimento; (c) limite epistêmico — completude é indecidível em mundo
  aberto; "estruturalmente impossível" (pós-rodada 4) foi narrativa Class-D do próprio harness.
- **CCE v2**: +visit_depth D0/D1/D2 por célula (plano exige ≥D1; arquitetura exige D2) +
  ledger de perguntas com question-generation obrigatório por rodada (dryness = lentes secas E
  fila de perguntas vazia) + crítico fresh-eyes em 2º modo ("formule 1 pergunta que ninguém fez")
  + cláusula de honestidade epistêmica (veredito sempre datado e condicionado, nunca "completo").
- **F1.7 Scout Perpétuo [M]** — o ponto fixo da recursão: exploração vira processo permanente
  (ADW de fundo, cadência adaptativa por rendimento com backoff, nunca a zero); achados viram
  TICKETS da factory (cascade/tasksfile → router → plan-revision ADW); o gate muda de "está
  completo?" (indecidível) para "agir ou esperar?" (decisão sob incerteza declarada, com curva de
  rendimento + perguntas abertas + irreversibilidade); agir não para o scout — achados de
  meio-de-execução viram change-orders. A pressão que o Gabriel exerceu manualmente 2× fica
  internalizada no sistema. Sequência atualizada (F1.7 após F0; interino via jobs+cron OK).

## 2026-07-20T02:40-03:00 — Auditoria decompose/tasks/activity/hooks (pedido do Gabriel)
- **Activity log: ÍNTEGRO** (11.168 eventos, verify hash OK, projection consistente) — única peça A.
- **Decompose: dados degradados** — 2.334 containers ("created" eterno: 2.330; 1 status vazio;
  5 active), 10 subtasks vivas AMBAS zumbis (trabalho já concluído); subtask_results 0 linhas;
  finalize DELETA a linha (1.245 finalized = 1.245 snapshots; sem trilha na tabela).
- **Task-sync CC→Touring: create-only + double-create** — E2E vivo: 1 TaskCreate → 2 containers
  (1 sem descrição, padrão histórico idêntico); TaskUpdate→completed NÃO propaga (status/updated_at
  intactos); hook_task_completions 0 linhas NA HISTÓRIA; 0 counters task_sync em gate-metrics.
- **Integridade da API**: `decompose validate` em task INEXISTENTE retorna `valid:true`;
  `get` responde shard-relativo sem indicação (task MVP do loop-eng invisível de /home; shard
  rust = clone divergente do migrate 27/06: 2.280 vs 2.330 created).
- **Flags inconsistentes**: `-j` rejeitado em decompose status/evolution insights; `--brief` em
  memory recall (recorrência da classe do bug 20/06).
- **Impacto no plano**: risco novo ALTA/ALTO + **F0-pre wave de higiene** como pré-requisito;
  escolha do activity log como substrato durável CONFIRMADA pela auditoria.

## 2026-07-20T03:20-03:00 — EXECUÇÃO INICIADA (/goal multi-sessão) — F1 done, F1.5/F2 em curso
- Human gate atravessado via /goal. FASE 0 PASS (cargo exit 0, doctor 6/6). DAG
  `task_1784514488954145764` (11 fases, validado) + marker loop ativo (`active-df4a8bd525f8`).
- **F1 DONE**: `explore_until_dry.py` (Layer-3, 5 lentes automatizadas + célula manual `external`,
  ledger persistente, truncation accounting, D2 por corroboração, fila de perguntas, veredito
  honesto; CC 22→12) + **12/12 pytest** + smoke real: curva `[16,13,8,6,0,10,0,1]` = 54 findings
  em `run_gateway`; dry-tail K=2 recusou convergência falsa após rodada seca única (a 6ª achou 10).
- **F1.5 em curso**: wiring `touring explore` em `touring-server/src/cli/{master.rs,command_table.rs}`
  (gotcha confirmado: dispatch vive em touring-server, NÃO touring-cli); gates verdes
  (check 0 / testes master 8/8 / clippy 0); `update-touring` rebuild em background.
- **F2 core DONE**: `plan_refine.py` (cobertura depth-weighted D2=3×, plan-delta estrutural com
  headings normalizados sem effort-tag — o evento XL→M da rodada 4 conta como reclassificação —
  claims VGP-lite, contrato de platô ε=0.02 + threshold 0.90, veredito honesto; CC 16→11) +
  **13/13 pytest** (25/25 no total com F1). Fix de design real pego por teste: heading
  reclassificado contava como add+remove.

## 2026-07-20T03:50-03:00 — F1.5+F2 fechados · ACHADO P0: DAG decompose é volátil
- Rebuild ok (6m24s, dual-target, doctor 5/5). **`touring explore` VIVO** no binário instalado
  (9º master command). F1.5 e F2 DONE.
- **ACHADO P0 (reproduzido ao vivo)**: o restart do daemon APAGOU o DAG de execução
  (task_1784514488954145764 — zero rastro em ambos os shards, NEM em decomposition_events).
  Estado runtime do decompose é in-memory; o DB é um segundo store que só alguns caminhos
  escrevem (split-brain). Explica a task MVP do loop-engineering sumida (auditoria 02:40).
  Premissa "DAG = progresso autoritativo" INVÁLIDA até o fix — bundle+memória são os
  autoritativos da retomada.
- DAG recriado: `task_1784515556131783713` (F1/F1.5/F2 done; ready = F0-pre); marker atualizado.
- **F0-pre repriorizado**: item #1 vira durabilidade write-through (create/add/update → DB
  imediato + reload no boot); depois idempotência do `bridge_task_created` por cc-task-id
  (double-create diagnosticado: `task-sync-create` E `task-created` ambos → `run_task_created` →
  `cli_decompose_create` gera id novo cada vez; payload do evento sem subject → container vazio;
  `let _ =` do R14-S1 engole falhas), propagação update/complete, validate honesto, counters.
- Próxima sessão/continuação: implementar F0-pre em
  `touring-dispatch/src/lifecycle/task_create.rs` + `hook_decompose_bridge` + camada de
  persistência decompose (write-through) + testes.

## 2026-07-20T04:15-03:00 — F0-pre: diagnóstico COMPLETO (3 probes empíricos)
- Probe 1 (fd sweep): daemon sem .db persistente aberto; cwd=~/.claude/rust.
- Probe 2 (marker XYZZY): task criada via CLI não aparece em NENHUM `*.db` do disco (find -mmin) —
  o handler `cli_decompose_create` (que TEM write-through SQL, decompose.rs:432-473) executa
  contra um **SQLite em memória**: o RuntimeMap do daemon resolve project_root do RPC para o
  fallback `:memory:` (mesma classe do bug F5 "project_root vazio", junho). Path B (hooks)
  persiste porque roda no processo `touring-hook` efêmero com cwd correto do CC.
- Agravante: `server/mod.rs:420` — `TaskDecomposer::new()` sem `checkpoint_manager.load()` no
  boot (MCP-tools checkpointam em tools_analysis.rs mas nunca são restaurados).
- **Plano de fix F0-pre (ordem)**: (1) roteamento project_root real nos RPCs `cli-decompose-*`
  (payload carrega cwd do cliente; validar path antes de qualquer fallback em memória);
  (2) boot-restore `load()`; (3) idempotência `bridge_task_created` por cc-task-id;
  (4) propagação update/complete no task-sync; (5) `validate` honesto em task ausente;
  (6) counters `task_sync_*`/`decompose_*`.
- Estado da sessão 1: **F1, F1.5, F2 DONE** (3/11 fases); DAG task_1784515556131783713
  (volátil — bundle+memória autoritativos); marker ativo; próximo = F0-pre implementação.

## 2026-07-20 (sessão 2) — F0-pre IMPLEMENTADO (diagnóstico corrigido + 7 fixes)
- **Diagnóstico CORRIGIDO** (refuta o "SQLite in-memory" da sessão 1): decompose sempre persistiu —
  em **shards por cwd cru** (DAG original em `rust/.claude/touring/`; recriado+XYZZY em
  `skills/Touring/scripts/.claude/touring/`). Meu probe `find|head -12` truncou a lista antes do
  shard certo — a lesão `head -100` DENTRO do próprio probe (CCE vindicado 2×).
- **Fixes implementados** (todos com gates verdes — check 0, clippy 0, testes 267+103+1310):
  1. `TouringConfig::normalize_project_root` (foundation/paths.rs) — walk-up por marcadores REAIS
     (`.touring/`, `Cargo.toml [workspace]`, `.git`; NUNCA `.claude/`), fallback `$HOME`=global;
     **8/8 testes** novos; aplicado no cliente (`daemon_client.rs`) E no dispatch (defesa dupla).
  2. `cli_decompose_create` honra `task_id` explícito do payload (mata o gerador do double-create
     e habilita ids determinísticos p/ o runner ADW).
  3. `cc_mirror_task_id` + idempotência SELECT-first no `bridge_task_created` (2ª chegada = dedup).
  4. `lifecycle/task_create.rs`: scaffold scout→implement→validate atacha ao id-espelho (antes ia
     p/ o id cru do CC — por isso espelhos nunca tinham subtasks).
  5. `lifecycle/task_update.rs`: update/complete endereçam o espelho (antes propagavam p/ NADA).
  6. `cli_decompose_validate` honesto: task inexistente → `valid:false` + error + hint de shard.
  7. `cli_decompose_get` honesto: task null ganha `error` + hint (fecha o gotcha Bug B de 02/07).
- Teste r125 corrigido (assertava o bug do validate); DAG de execução MIGRADO ao shard global
  (11 subtasks; merge por colunas nomeadas — `SELECT *` falhou por ordem divergente de colunas
  entre shards, engolido por OR IGNORE: mais um silencioso).
- Rebuild final em background; próximos: probes E2E (normalização + task-sync ciclo completo) →
  fechar F0-pre → F0.
- **Probe E2E rodada 2** (pós-rebuild): validate/get honestos **PASS** no binário instalado.
  Probe de normalização expôs caso não-antecipado: **`~/.claude/.git` existe** (dotfiles
  versionado) → walk-up promovia `~/.claude` a projeto → shard patológico
  `~/.claude/.claude/touring/` (onde os 2 probes caíram — daemon lia de volta, disco "vazio").
  Meta-lição CCE 3×: meu sweep usara `-newermt` no arquivo principal (escritas WAL não tocam
  mtime) — outra variante da lesão de truncamento. **Fix**: exceção harness-config na
  normalização (`dir == $HOME/.claude` nunca é root; projetos internos com marcador próprio
  continuam vencendo) + 2 testes novos (10/10) + clippy 0; rebuild #3 em background.

## 2026-07-20 (sessão 2, cont.) — Probes E2E rodada 3 + fixes rodada 2 do F0-pre
- **Probe A (normalização) PASS**: create de `skills/Touring/scripts` → GLOBAL ✓; shard patológico
  vazio ✓; daemon fresco.
- **Probe B parcial**: espelho `cc_task_1` com descrição ✓, MAS: (a) `cc_task_unknown` vazio —
  o PostToolUse lê `task_id` do tool_input, e no TaskCreate o id NASCE na resposta;
  (b) scaffold ausente no caminho do evento — vivia só no handler PostToolUse.
- **Probe C (reprodução direta do hook)**: `cc_task_probeC` + 3 subtasks ✓ — o núcleo funciona;
  MAS caiu no shard do rust: **touring-hook envia `project_root=""`** e o normalize resolvia
  marcadores RELATIVOS contra o cwd do daemon (`~/.claude/rust` tem `.touring`).
- **Fixes rodada 2** (gates verdes: check 0, clippy 0, foundation 11/11 + dispatch 602 + handlers 103):
  (1) guard `!cwd.is_absolute() → fallback(home)` no normalize + teste (path vazio/relativo);
  (2) **scaffold movido para `bridge_task_created`** — o ponto de convergência dos DOIS caminhos,
  idempotente por construção (dedup early-return); (3) guard anti-"unknown" no bridge (sem id real
  → sem espelho); (4) `task_id_from_response` — extrai `#<digits>` da resposta do TaskCreate.
- Lixo de probes limpo dos 3 shards; rebuild #4 em background; pendente: Probe B final
  (create→espelho único+scaffold; update→propagação) + fechar F0-pre.

## 2026-07-20 (sessão 2, cont. 2) — camelCase + counters + GC: F0-pre rumo ao fechamento
- **Probe B2 pós-rebuild#4**: espelho único `cc_task_2` + descrição + scaffold ✓, zero lixo novo ✓
  (o "1" do meu check era artefato SQL: created_at RFC3339 com 'T' compara string-maior que
  datetime() do SQLite — gotcha catalogado), MAS propagação ainda falhava.
- **6ª camada da cebola**: CC tool schemas usam **camelCase** (`taskId`); os 5 lifecycle handlers
  liam só snake_case → update roteava p/ espelho "unknown". Fix nos 5 (update/stop/delete/
  output/get); rebuild #5; **PROBE FINAL FULL PASS**: `cc_task_3` completed propagado + R158
  auto-advance (implement/validate → completed).
- **GC**: 1.057 containers vazios arquivados (`task_decompositions_archive`) e removidos;
  1.282 restantes com descrição preservados como histórico.
- **Counters**: `task_sync_{create,deduped,update_propagated}_count` — campo+init+record na
  foundation, snapshot 2 touchpoints (+`#[serde(default)]` p/ JSON legado — 2 testes de
  compat pegaram), 3 chamadores wired (bridge ×2, task_update ×1). Gates: foundation 418/418,
  clippy 0. Rebuild #6 em background — última verificação: counters vivos no gate-metrics →
  **F0-pre DONE** → F0.

## 2026-07-20 (sessão 2, cont. 3) — F0-pre DONE (probe final pós-rebuild#6)
- **Rebuild #6 OK** (UPDATE_EXIT=0; daemon novo PID 232818, exe válido; aviso de PID stale era
  processo efêmero já morto).
- **PROBE COUNTERS FULL PASS**: baseline 0/0/0 → `TaskCreate #4` → `task_sync_create_count=1` →
  `TaskUpdate completed` → `task_sync_update_propagated_count=1`; espelho `cc_task_4` persistido
  com descrição+status no shard global. Nota operacional: espelhos `cc_task_*` vivem no shard do
  projeto da SESSÃO CC (home→global), não do cwd do Bash tool — by design, não bug.
- **F0-pre → done no DAG** (`task_1784515556131783713`); reward 0.95 `decompose_bridge`;
  checkpoint `adw-f0pre-done-2026-07-20` (tier semantic). Ready agora: **F0** (destrava F1.7+F3).

## 2026-07-20 (sessão 2, cont. 4) — F0: substrato ADW implementado + 4 aceitações provadas
- **Superfícies verificadas antes de codar (CCE aplicado a mim)**: `activity append <action>
  --actor --payload` (13 ações tipadas, log per-projeto + replay hash); `workflow
  run/stats/slowest/compare/resume`; `governor status/limits/reset/report`; `touring run`
  sandbox; claude headless tem `--resume`/`--model`/`--allowedTools`/`--max-budget-usd` mas
  **NÃO tem `--max-turns`** (VP-Scout C5 — campo fica reservado no spec, budget via USD+timeout).
- **`adw.py` (Layer-3, ~700L)**: spec TOML nós code/agent/gate/loop/human + edges
  on_pass/on_fail/on_dry; journal fsync'd append-only + `--resume-run` (replay determinístico
  do grafo re-deriva exec_keys — visits NUNCA pré-carregados, bug pego por teste); results
  store L4 `{summary, omitted_bytes, full_ref}` + template vars `{{nodes.X.summary}}`; drivers
  claude-headless (fail-closed) e mock (recordings); `lint` (edges, órfãos, ciclos sem saída,
  budget-verify Σ nós ≤ run) + `test` (agentes mockados — testa ARESTAS) + `from-template`;
  Class-D (Lei L3): gate FAIL após agente que narrou sucesso → `class_d_divergence` no journal;
  loop node: runner conta `NEW_FINDINGS=<n>` e decide dry (Lei L2); human node → pausa durável
  + `--approve` no resume; flock com reclaim de lock morto; espelho best-effort no activity.
  CC refatorado 33/29/17→≤15 sob pressão do post-edit hook. **pytest 16/16**.
- **Wiring 10º master**: `touring adw` em script_for + MASTER_COMMANDS + `pub fn adw` +
  CommandDescriptor. Gates: check 0, clippy 0, **977/977** lib tests, harness_gate R6
  **10 scripts PASS** (adw.py entrou na superfície automaticamente, ≥0.80). SKILL.md
  sincronizado (masters + descrição explore/adw).
- **Aceitações provadas**: A1 hello-factory REAL no workspace (mock build → gate sentinel
  FAIL→feedback→PASS→report consumindo `{{nodes.clippy_gate.summary}}`; activity 41→53
  eventos); A2 **kill -9 VIVO** (journal sobreviveu com `slow` started; resume replayou
  `fast#0`, re-executou `slow#0`, completed); A3 `adw test` mockado (pytest); A4 Class-D
  sintético (pytest) + ocorrência viva no A1 (`class_d_divergence:true` com run completed
  via feedback loop — narrativa≠veredito registrada).
- Rebuild #7 em background — última verificação: dispatch `touring adw` no binário → F0 DONE.

## 2026-07-20 (sessão 2, cont. 5) — F0 DONE (dispatch provado via binário)
- **Rebuild #7 OK**; `touring adw list/lint/run/test` via binário instalado: hello-factory
  completed com feedback loop + `class_d_divergence:true` + `adw test` mocked PASS.
- **F0 → done no DAG**; reward 0.95 `adw_runner`; checkpoint `adw-f0-done-2026-07-20`.
- **REGRA #21**: doctor acusou `wiring_diagnostic` polluted (non_rust=193194, abs_paths=792,
  kind_unknown=27754 no wiring_map do rust) — remediação do próprio check: `touring index
  rebuild` (EAGAIN no client, segue server-side; watcher aguardando ok). Ready agora: **F1.7 + F3**.

## 2026-07-20 (sessão 2, cont. 6) — F3 em curso: biblioteca + endurecimento por incidente real
- `wiring_diagnostic` → **ok** pós index rebuild (kind_unknown=0, non_rust=0, abs_paths=0);
  doctor **All checks passed**.
- **Biblioteca F3**: `adw-library/` central com 6 specs (explore-plan, bugfix, feature, chore,
  hotfix, audit) + `tiers.toml` (sota/workhorse/light → modelo, override central);
  `from-template` instancia da library; `adw test --var`; **27/27 pytest** (inclui lint
  paramétrico dos 6 specs reais + retry-limit + permission flags).
- **Incidente-lição (chore round 1)**: gate quebrado re-invocou o headless **17×** até o
  timeout — implementado **retry shutoff Lei L2** (`max_retries` default 3, evento
  `retry_limit_exceeded`); round 2 parou honesto em 4 invocações com exit 1. Class-D pegou o
  headless VIVO fabulando "edição concluída" com arquivo intacto (rounds 1-2).
- **RESTRIÇÃO ESTRUTURAL descoberta (prova A/B)**: headless `--permission-mode acceptEdits`
  escreve em /tmp mas é BLOQUEADO em qualquer path sob `~/.claude/` (harness protege a própria
  config; inclui o workspace rust). Consequência de design: nós agent com Edit/Write operam em
  projetos fora de `~/.claude`; chores na config são do orquestrador interativo. Memory:
  `adw-headless-claude-dir-protection-2026-07-20`.
- **conflict-check DENTRO do ADW provado**: `adw test feature` rodou o gate CEG real —
  "2 waves must be serialized" no artefato (aceitação F3 item paralelismo ✓). Cheatsheet
  `touring-cli-index.md` ganhou seção MASTERS (eu, interativo — a tarefa do chore original).
- Em execução: **bugfix ADW ponta-a-ponta** em projeto real fora de ~/.claude (parser-proj,
  bug real + pytest gate).

## 2026-07-20 (sessão 2, cont. 7) — bugfix ADW E2E FULL PASS
- **`touring adw run bugfix` no parser-proj: recall→diagnose→fix→verify TODOS pass na 1ª
  tentativa.** O agente workhorse aplicou o fix mínimo correto (trailing bare number →
  minutos) com comentário da convenção; gate pytest 2/2 verde, re-verificado
  independentemente. Memory-pack (nó recall) alimentou o agente com as lessons do shard.
  Aceitação F3 "bugfix E2E em tarefa real" ✓.
- Em execução: chore final com edição efetiva (README no parser-proj) p/ fechar o critério
  chore sem asterisco.

## 2026-07-22/23 (sessão 3) — F1.7+F4 DONE · F5a/F5b implementados · F6 em curso
- **Sessão retomada** (daemon auto-spawn ok; scratchpad/tmp limpo — parser-proj recriado).
- **F1.7 DONE**: scout_perpetuo.py + spec ADW; aceitação viva no bundle — 4 ciclos bg, curva
  **19→5→3→2** (prova empírica da tese rodada-N+1), 4 tickets reais, backoff 24→6h, pausa
  durável no gate act-vs-wait. 34/34 pytest.
- **F4 DONE**: factory.py (route determinístico-primeiro 6 famílias + CILA + prior + evidência;
  LLM só ambíguo; start recusa gate fake; RL reward por outcome + stats KPI) + master
  `touring factory` (12º). Aceitação: 3 tickets → 3 ADWs distintos com justificativa;
  bugfix start COMPLETO (pytest 2/2, reward 1.0), chore COMPLETO (README real, reward 1.0),
  feature INICIADO (journal) e cancelado pré-opus. 16/16 pytest.
- **F5a DONE**: ZTE conformal no runner — human node `zte=true` + warm-up (M runs completed)
  + `run_confidence` determinística (class_d = desqualificante) + `calibrate-confidence`
  REAL (IN/OUT); fail-closed em toda dúvida; journal `zte_bypass` p/ auditoria a posteriori.
  **Prova viva**: 3 seeds aprovados → 4º run bypassa com conformal IN (conf 1.0, hist 3).
- **F5b DONE**: `adw race` — N lanes por CÓPIA de diretório (sem git, REGRA #11), spawn
  python direto (sem wrapper touring no caminho do cancel), first-to-pass vence, perdedores
  cancelados, merge winner-only + conflict-check no write-set. Teste real de corrida passa.
  **Bug latente achado e corrigido**: code/agent nodes herdavam cwd do processo em vez de
  `--root` (root-anchoring em 6 pontos). **57/57 pytest** (adw+factory+scout).
- **F6 em curso**: KPIs `touring.adw.*` (5) via `derived:` no kpi.rs + commitments.yaml
  (23 total; advisory por precedente 2026-07-01); TIER 1 do cli-index ganhou explore/adw/
  factory; loop-engineering v2 reframe (loop = disciplina DENTRO de ADW; INNER delega a
  `adw run`); corolário 4-pillars "masters são os nós de código dos ADWs" (adoção estrutural
  por afordância); seção ADW no SKILL.md. Gates: touring-cli check/clippy 0, 27/27 kpi.

## 2026-07-23 (sessão 3, FINAL) — PLANO COMPLETO: DAG 11/11 DONE
- **F6 DONE** após prova viva pós-rebuild#10: KPIs `touring.adw.*` **per-project** de verdade
  (bug pego por probe: resolvers rodavam no cwd do DAEMON — fix `rt.project_root`; parser-proj
  agora reporta runs=7, router_accuracy=1.0, zte_bypass_rate=0.143 ≠ rust runs=8/STUB/0.0).
- **harness_gate corrigido** (REGRA #21): SCRIPT_TARGETS nunca incluíra os scripts novos —
  agora 13 scripts + 2 rust = **15 targets, PASS, 0 below floor** (adw/factory/scout_perpetuo/
  explore/plan_refine todos ≥ 0.80 no 50-dim).
- **Gates finais**: pytest **82/82** (5 módulos) · kpi Rust 27/27 · clippy 0 (server+cli) ·
  doctor **All checks passed** · e2e **Overall 0.87 PASS** (4 issues advisory pré-existentes:
  orphan-meter cwd-sensitive + candle_bge CC=17 + fastembed antipatterns — **roteados pela
  própria factory** para o ADW `audit` e enfileirados em `task_1784818024931392697`).
- **DAG `task_1784515556131783713`: 11/11 done** (F1, F1.5, F2, F0-pre, F0, F1.7, F3, F4,
  F5a, F5b, F6). F5c/F5d permanecem horizonte XL declarado fora do DAG. 10 rebuilds
  update-touring ao longo da execução; memória: `project_adw_software_factory_2026_07_23.md`
  (arquivo) + checkpoints `adw-f*-done-*` (touring). Follow-ups documentados: `adw stop`
  nativo; tickets audit da fila; scout-perpetuo segue com cadência própria (6h).

## 2026-07-20 (sessão 2, cont. 8) — F3 DONE (4/4 aceitações provadas)
- **Chore final FULL PASS**: README criado de verdade no parser-proj, gate grep verde, 1ª
  tentativa. Com isso: (1) 6 specs lint 0 errors ✓; (2) bugfix E2E real ✓; (3) chore E2E
  real ✓; (4) conflict-check serializando dentro de ADW ✓. **F3 → done no DAG**; reward
  0.95 `adw_library`; checkpoint `adw-f3-done-2026-07-20`. Ready agora: **F1.7** (F0+F2 done)
  e **F4** (F3 done).
