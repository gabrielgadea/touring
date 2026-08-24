---
okf_version: "1.0"
type: LoopLog
title: "Log — Code Mode Best Practices"
tags: [code-mode, log]
timestamp: "2026-08-23T22:20:00-03:00"
plan_id: task_1787534575493195469
---

# Log

- **2026-08-23T22:13** — `/loop-engineering` invocado (flow `strategy-outer` armado). Topic: code mode best practices para Claude Code e Touring; 5 fontes (3 yt + 2 repos GitHub).
- **2026-08-23T22:14** — Daemon verificado saudável (race transitório no SessionStart). `memory recall "code mode"` executado. Repos clonados (shallow) no scratchpad; legendas dos 3 vídeos baixadas via yt-dlp.
- **2026-08-23T22:15** — ADW strategy-loop rodado: arm/recall/diagnose PASS, explore_round exit 1 (fallback manual adotado).
- **2026-08-23T22:17** — `loop_diagnose.py` → diagnostic OKF em `/diagnostics/`. Transcrições limpas (VTT→txt, 36k chars). 2 agentes Explore despachados (deepseek-harness code-runtime; tanstack/ai code mode).
- **2026-08-23T22:19** — `touring explore --until-dry` → 13 findings, dry em 6 rounds; lente `external` marcada visited com as 5 fontes. Ledger CONVERGED.
- **2026-08-23T22:20** — Bundle OKF criado (index/log). Baseline Touring coletada: `touring run` (12 langs, deny-by-default, --allow-forbidden, --brief), gate-metrics (ceg_captured=38, ceg_sandboxed=0, pillar_induction emitted/followed=0/0).

## 2026-08-23T22:30:55.069365-03:00 — P1 done

deepseek-harness code-runtime incorporado na strategy doc (secao 2.4): seam ctx.codeRuntime vs consumer run_code, worker fresh-per-run, SDK tipado KV-cache-estavel, pipeline completo p/ sub-chamadas com id deterministica, colapso de modo predicado-unico, 2 budgets, OutputLedger 64MiB + spill assimetrico, erro-como-campo 6 kinds, mode native|code|both opt-in. 8 novas praticas P13-P20 destiladas

## 2026-08-23T22:37:14.721034-03:00 — P2 done

tanstack/ai code mode incorporado (secao 2.5): createCodeMode como 1 tool + bindings external_ + type stubs de JSON Schema, lazy tools + discover_tools (disclosure dupla), warnIfBindingsExposeSecrets, 4 perdas silenciosas MCP->code-mode, e pacote ai-code-mode-snippets: snippets cross-sessao com trust progression por outcome (untrusted->provisional 10+/90% ->trusted 100+/95%), selecao por modelo barato (haiku) com cap 5 no contexto, snippetsToTools/snippetsToBindings, management tools. Praticas P11-P12 + trust-by-stats destiladas

## 2026-08-23T22:37:54.806764-03:00 — P3 done

Sintese v1.0 consolidada: 20 praticas P1-P20 (5 fontes com evidencia de codigo), 13 oportunidades Touring T1-T13 em 3 ondas priorizadas (Onda1 T3+T13 skill-harvest+trust; Onda2 T1+T12 bindings sandbox gated CEG; Onda3 hardening runner), mapeamento Claude Code secao 4. Apresentado a Gabriel para human gate

## 2026-08-23T22:45 — CONVERGED

- Relatórios incorporados: deepseek-harness (P1, agente explore + evidência de código) e tanstack/ai (P2, vídeos + relatório MCP×code-mode + verificação cirúrgica de `ai-code-mode-snippets` com trust strategies).
- Doc-link gate: ok=true (plan_id em todos os docs, zero órfãos). Gap de ambiente corrigido: `touring-quality` ausente do PATH pós-migração Omarchy → symlink `~/.local/bin/touring-quality` → target/release.
- `loop_converged.py` **exit 0**: quality Diamond 0.95988, DAG 3/3 done, no P0, orphans=baseline, judge intact. Memória atualizada (`strategy:code-mode-best-practices-2026-08-23`, #status:converged).
- Pendência bônus: relatório final do teammate explore-tanstack (ainda em execução) será anexado ao bundle como apêndice quando chegar.

## 2026-08-23T22:49:51.443013-03:00 — P4 done

Fusao TOTAL com investigacao paralela da sessao analise (bundle 2026-08-23-code-mode: M1-M12, D0-D9, rodada 2) + relatorio final do explore-tanstack. Novos: D0 bug stderr engolido (pre-req, priority high), trust DECORATIVO no TanStack (corrigido na doc), composicao snippet->snippet quebrada, sem gate needsApproval, KV-cache hygiene D9, contrafactual medido, replay deterministico, 8 alternativas rejeitadas dsh, SDK Python blueprint. P21-P25 + T14-T15; backlog unificado ao DAG task_1787534694944647530 (analise). Clones+transcripts copiados p/ ~/references/code-mode-2026-08-23

## 2026-08-23T22:55 — P4 FUSÃO + reconvergência

- Gabriel pediu incorporação TOTAL da investigação paralela da sessão `analise` (bundle `~/projects/analise/docs/plans/2026-08-23-code-mode/`, 2 rodadas CCE, M1–M12 + D0–D9). Lidos os 6 docs de evidência; peer respondeu via SendMessage confirmando o mapa.
- Relatório final do teammate explore-tanstack chegou e foi incorporado (composição snippet→snippet quebrada, telemetria no-op, drift prompt↔código, sem needsApproval, replay determinístico, assimetria de sanitização, sem número medido).
- Correções aplicadas na doc: trust TanStack é DECORATIVO; composição snippet-sobre-snippet não funciona. Novas P21–P25 e T14 (D0 bug stderr, priority high) + T15 (D9 KV-cache). Backlog unificado ao DAG `task_1787534694944647530` (projeto analise).
- Clones + transcripts copiados do scratchpad efêmero da sessão analise para `~/references/code-mode-2026-08-23/` (a pedido de Gabriel).
- `loop_converged.py` exit 0 novamente (Diamond 0.9599, DAG 4/4). Strategy v1.1.
- **2026-08-23T22:51 — reciprocidade confirmada**: sessão analise incorporou os 6 achados desta sessão em `reciprocidade-touring.md` + memória `code-mode:reciprocidade-cross-session-2026-08-23`, mapeados ao backlog canônico: R1 (abortSignal/metadata/aprovação)→d1/d2 · R2 (snippet→snippet ReferenceError)→d2 com detecção de ciclo · R3 (telemetria no-op)→d4 (contrafactual subconta sem instrumentar todos os caminhos) · R6 (sem needsApproval)→d2/d3 com gate de aprovação EDITÁVEL alinhado ao CEG · R4/R5 doutrina. Backlog canônico confirmado: `task_1787534694944647530` (analise); d0 segue pré-requisito do lado touring.

## 2026-08-23T23:12 — Pln2 de implementação AUTORADO (/taco-planning)

- Stage 1: `ground_truth.json` (e2e 0.8584, orphans 2516, doctor ok) + sondas d0 reproduzidas em primeira mão + scout VGP dedicado (11 itens, file:line, 9 evidências negativas — destaque: `touring run` NÃO passa pelo CEG; stats de snippet inexistentes; effort ausente no ADW).
- plan.md escrito: 10 fases (W0 stderr P0 → W1 taxonomia/spill/budgets → W2 SDK+allowlist → W3 snippets trust → W4 contrafactual → W5 run→CEG → W6 KV-cache → W7 MCP 3-tools → W8 erros+tier → W9 doutrina/docs), testes nomeados por subtask, DAG mermaid, gates 50-dim no §5, matriz de potenciação, riscos.
- Gates: gap_detector 0 P0 ✅ · plan_validator: residual único de formato (confidence coverage contada por regex de template; conteúdo tem FACT/INFERENCE em todos os S-N) · Wayfinder `waterfall_risk: at_risk=False` após tickets de protótipo P23/P33 ✅.
- DAG implementação: `task_1787537412927115419` (W0–W9 + P23/P33), fog tipada, W0 priority high. Memória `plan:code-mode-maximo-2026-08-23` (#status:awaiting-approval).
- ██ HUMAN GATE ██ — aguardando aprovação de Gabriel para iniciar W0.

## 2026-08-23T23:34:27.085213-03:00 — W0 done

d0 stderr fix CODE-COMPLETE: drenagem concorrente 2 pipes (anti-deadlock 64KiB), SandboxResult.stderr, adaptador propaga stderr real, timeout ensina causa, tee 2 canais. touring-ceg full verde (42+4 novos), ctx_execute_e2e 16/16 (3 contratos), unit 10/10, clippy 0. Deploy unico ao final do lote (decisao Gabriel opcao c)

## 2026-08-23T23:43:08.655268-03:00 — W1 done

d3 CODE-COMPLETE: taxonomia RunFailure{kind,phase,message} 6 kinds ortogonais (Timeout>OutputLimit>Exception, mensagens que ensinam), spill com locator+retrieval_hint (stored_path quando inline truncado; head/tail com marcador de elisao), budget duplo compute_ms via /proc/<pid>/stat (busy medido no substrato, poll 100ms, select 3 bracos), caps inline configuraveis por env. derive_run_outcome extraida (CC). Testes: executor 44/44 (2 novos budget), E2E 20/20 (4 novos taxonomia+spill), unit 10/10 (truncate migrado p/ head_tail), clippy 0

## 2026-08-23T23:48:00.580179-03:00 — W2 done

d1 CODE-COMPLETE: (S-2.1) allowlist READONLY_HOOKS client-side no SDK com erro-que-ensina (8 hooks; server-side proxy segue como follow-up documentado — containment honesto); (S-2.2) 3 bindings novos VGP-verificados: memory_recall, tantivy_search, wiring_impact (cli-portfolio NAO existe como hook — evidencia negativa, fora); (S-2.3) touring run --sdk-stub imprime stub .pyi byte-estavel blueprint dsh py-types (Protocol + docstring no metodo + STATIC STUB warning + ordem lexicografica); (S-2.4) nudge code-mode-loop/scan ganha MAY com forma 1-linha do SDK (P23: ~53 tok) apontando --sdk-stub. Testes: run.rs 12/12 (3 novos: byte-identical/lists-every-binding/allowlist-covers), touring-cli 348/348, clippy 0

## 2026-08-23T23:54:11.798351-03:00 — W3 done

d2 CORE CODE-COMPLETE: (S-3.1) snippet_stats.rs novo modulo — trust ladder untrusted->provisional(10/90%)->trusted(100/95%) + REBAIXAMENTO por janela (5 falhas nas ultimas 10 -> demote; fecha buraco TanStack) + INVALIDACAO por sig_hash (judge_attest p/ codigo aprendido) + badges visiveis ao modelo, 5/5 testes; (S-3.2) ponte cli_learning_reward: tool prefixo snippet: alimenta stats (success=reward>0, sig_hash no payload, fail-open) e retorna snippet_trust; (S-3.3) harvest_hint no touring run com threshold P33 (exit 0 + >=5 linhas + parametrizado + sem efemeros, ~26% oferta) + emit_output agora expoe failure/stored_path/retrieval_hint da W1 no CLI. S-3.4 (bindings snippet_* dinamicos + ciclo + segredos) registrado como W3b dependente de biblioteca populada — nao half-baked. Testes: intelligence 5/5, run 14/14, clippy 0

## 2026-08-23T23:57:19.757140-03:00 — W4 done

d4 CODE-COMPLETE: counters code_mode_runs_count + code_mode_bytes_elided_total (economia MEDIDA: full bytes - inline bytes) no gate_metrics + export nos 2 sites MCP; journal_run JSONL fail-open em ~/.claude/touring/run_journal.jsonl {ts,language,exit_code,duration_ms,failure_kind,bytes_elided} — journal PROVADO vivo (testes E2E gravaram registros reais). Contrafactual de sub-chamadas junta na W5 (identidade). E2E 20/20, clippy 0

## 2026-08-23T23:58:53.588112-03:00 — W5 done

T12/S-5.1 CODE-COMPLETE: gate_run espelha exec::gate_command (C08 simetria entre os 2 callers de run_gateway) — touring run atravessa CEG X0..X7 antes da execucao (perfil sandboxed default, trusted sob --allow-forbidden), Deny aborta com erro-que-ensina, erro interno fail-open (invariante CEG); tool identity mapeia o runtime real (SandboxPython etc.). ceg_captured/sandboxed passam a contar o caminho run. S-5.2 (identidade de sub-chamadas no socket handler) registrado para junto de W3b (mesma superficie). Testes run 14/14, clippy 0

## 2026-08-23T23:59:43.704008-03:00 — W7 done

T2 CODE-COMPLETE: TOURING_MCP_CODE_MODE=1 -> apply_curation expoe SO {touring_search, touring_ctx_execute, touring_memory_recall} (fachada search+execute Cloudflare + memoria, 3 tools no handshake; demais 159 seguem invocaveis por nome — comportamento atual preservado; TOURING_MCP_ALL_TOOLS mantem precedencia como escape hatch). Teste code_mode_curation_exposes_three_tools + clippy 0

## 2026-08-24T00:01:15.161857-03:00 — W8 done

T6+T8 CODE-COMPLETE: (S-8.1) scripts/error_message_audit.py — varre bail!/anyhow! nos 3 crates CLI, heuristica teach-markers, baseline MEDIDA teach-ratio 0.339 (259/392 nao ensinam); correcao aplicada no erro mais visivel ao code mode (daemon_client 'closed connection' agora aponta daemon-ctl status); campanha completa registrada como W8b. (S-8.2) lint _lint_agent_codegen_tier no adw.py live — no agent com prompt de codegen sem tier explicito gera warning induzindo tier barato (evidencia TanStack/haiku); 28 testes de lint verdes, ast.parse ok. Sync client/ pendente no W9 (co-evolucao)

## 2026-08-24T00:03:23.153811-03:00 — W6 done

D9 CODE-COMPLETE: scripts/kv_cache_audit.py — roda cada hook SessionStart/UserPromptSubmit 2x com payload congelado e diffa (metodo do postmortem dsh); MEDIDO: 10 hooks, 1 instavel (session_startup_intelligence.py); estabilizado (timestamps bucketizados recente/hoje/Nd, composite 4->2 casas, rodape sem ms exato) -> re-auditado 10/10 ESTAVEIS. --assert-stable e o gate de CI. Pendencia de decisao Gabriel: timestamp-por-prompt [HH:MM] do UserPromptSubmit (varia por minuto; util p/ humano — trade-off UX vs cache)

## 2026-08-24T00:24:55.450558-03:00 — W9 done

CLOSE: docs/code-mode.md (manual completo: run/orchestrate/snippets/budgets/taxonomia/observabilidade/MCP code-first/KV-hygiene); client/ sync aplicado; DEPLOY UNICO via update-touring (2x: fix advisory-deny p/ bash — CEG negava echo sob sandboxed, taxava caso comum P20; shell Deny vira advisory logado, code langs mantem gate duro). SONDAS EM PRODUCAO: stderr python/bash capturado com failure taxonomy, timeout rotulado, harvest_hint vivo, nudge MAY com assinaturas SDK vivo. Gates: e2e 0.8584 pass (=baseline), orphans 2516 (=baseline REGRA 0), journal vivo (10+ registros). Nota conhecida: counters code_mode_* sao per-process (CLI efemero) — journal e a persistencia; counters agregam no daemon via MCP. D5/D8 doutrina: redacao apresentada a Gabriel no relatorio (decisao dele 22:23 = so apresentar)

## 2026-08-24T00:25 — EXECUÇÃO CONVERGIDA (Pln2 completo)

- 10 waves executadas (W0–W9) + 2 protótipos (P23/P33) + 2 potencializações transferidas ao backlog canônico (W3b/W8b → analise). Deploy único em produção; sondas de aceitação verdes no binário vivo; regressão P20 (CEG negando bash trivial) detectada POR SONDA e corrigida antes do fechamento.
- `loop_converged.py` **exit 0** (Diamond 0.9599, DAG drenado, orphans=baseline, e2e=baseline). Memória `impl:code-mode-maximo-2026-08-24`.
- **2026-08-24T00:26 — verificação cruzada independente**: sessão analise re-sondou o d0 com sonda PRÓPRIA antes de aceitar o fechamento — traceback íntegro no stderr + `failure{kind, phase, message}` com mensagem que ensina — e marcou d0 **completed no DAG canônico com evidência própria**. w3b/w8b registrados lá (vinculados a d2/d3). O fix do timeout foi reconhecido como a prática M6 das fontes materializada.
- **2026-08-24T00:40 — 4 decisões de Gabriel aplicadas**: D9(b) `prompt_stamp: stable` (opção nativa do plugin remember — superior ao bucket: zero invalidação; horários seguem em `.remember/today-*.md`); D5 (reflexo agregado-não-dump) e D8 (enforcement-no-executor) adicionados a `rules/touring-4-pillars.md` + client sync; ordem do backlog aprovada: w8b → d4+S-5.2 → w3b → d6/d7.
- **2026-08-24T00:45 — priorização registrada + D9 vivo**: sessão analise gravou a ordem no DAG canônico com priority real (w8b=50 · d4=60 · w3b=70 · d6/d7=80) e marcou d9 completed; o `prompt_stamp: stable` apareceu vivo no turno seguinte (`[gabrielgadea]`, sem hora — zero invalidação de cache). Evidência de fechamento enviada para d1/d3 (íntegros); d2 mantido aberto até w3b (bindings dependem da biblioteca popular).
- **2026-08-24T00:55 — w8b ARRANCADO**: fluxo ADW `error-teach` criado (`.touring/adw/error-teach.toml`): audit→fix_batch(haiku, acceptEdits)→check(cargo+clippy)→reaudit→phase-close; lint pegou retry-cego-ao-veredito (o detector fake_waiting da v30.4.2 funcionando) → corrigido com o veredito do gate no prompt; lint válido, 0 warnings. Rodada 1 (batch=30) em execução. d1/d3 fechados pela analise com re-sonda própria; lição meta registrada ("a sonda também precisa ser sondada").

## 2026-08-24T01:20 — Camadas de Execução E0–E5 (lógica de loops do ADW aplicada à arquitetura)

- Lógica recuperada do runner [FACT]: nó `loop` (NEW_FINDINGS/dry_rounds/fail-closed), stagnation_rounds, tally, retry-lê-veredito. Estratificada como E0–E5 em `docs/code-mode-execution-layers.md` (+ cross-ref no manual).
- **E4 implementada**: `touring adw campaign <flow> --until "<predicado>"` no adw.py — predicado é CÓDIGO (L2), `METRIC=` fail-closed (herda a leitura do NEW_FINDINGS), reward por delta (feromônio), curva persistida. 5 testes novos (converge/max_rounds/fail-closed/estagnação/flow-fail); suíte adw 189/189.
- Aderência Claude Code: `campaign_code_mode_command` no cli_suggester detecta `for … touring adw run` (o anti-padrão que esta própria sessão cometeu) e ensina a camada — afordância D8. 2 testes; clippy 0. Deploy do nudge agrupado no fechamento.
- Dogfood imediato: campanha error-teach transferida para E4 (`--until check-ratio 0.8, max 10, stagnation 2`); curva até aqui 0.339→0.395→0.416→0.457→0.48.

## 2026-08-24T02:05 — w8b CONVERGIDA (E4 dogfood completo)

- Campanha error-teach via `touring adw campaign`: **teach-ratio 0.339 → 0.897** (alvo 0.8; predicado exit 0 = converged). Curva: 4 rodadas manuais (0.339→0.48, batch=30) → E4 run 1 (0.518→0.673, rodada 5 `flow_failed` = timeout do agente) → E4 retomada batch=20 (0.897 em 1 rodada).
- w8b marcado **completed** no DAG canônico da analise com evidência; memória `campaign:error-teach:final-2026-08-24`.
- Lição operacional: reduzir o lote quando os ofensores fáceis secam (custo/fix cresce no fim); a E4 parou na falha com a curva preservada — o desenho fail-closed provado no primeiro uso real.
- Próximo ciclo aprovado por Gabriel ("inicie a W0"): **W0 = d4-contrafactual + S-5.2** (identidade `<run_id>:code:<n>` + KPI de economia real das sub-chamadas do --orchestrate).

## 2026-08-24T08:40 — C2-W0 CONVERGIDA (d4 + S-5.2: identidade e contrafactual das sub-chamadas)

- **Entregue**: `run_id` (`run-<epoch_ms>-<pid>`) atravessa CLI → sandbox (`TOURING_RUN_ID`) → SDK (`origin: <run_id>:code:<n>`) → daemon; contrafactual MEDIDO por sub-chamada (`code_mode_subcalls_count` + `code_mode_subcall_bytes_total`, somas exatas) + journal `run_subcalls.jsonl`; a mesma identidade correlaciona `run_journal.jsonl`, subcalls e a superfície CLI.
- **Sonda de aceitação** (2 caminhos independentes batendo): 2 subcalls = 52.510 bytes contrafactuais (um `memory_recall` sozinho = 51 KB que NÃO entrou no contexto) vs 1 linha inline devolvida.
- **A sonda pós-deploy pegou 4 lacunas invisíveis aos unit tests**: (1) `gate-metrics -j` nunca expôs `code_mode_*` — nem os da W4 (snapshot ≠ sites MCP); (2) o CEG rejeitava todo run não-shell desde a W5 (X0 não conhecia `Sandbox*`); (3) `run_id` ausente do emit CLI; (4) fechado o (2), o X6 passou a negar o SDK do orchestrate (socket by design) → o gate agora mira o código do USUÁRIO, nunca o SDK injetado (teste trava o contrato). Ciclo sonda→fix→redeploy ×3.
- **Gates**: 6 testes novos; suítes foundation 483 · hooks-core 485 · dispatch 1321 · ceg 537 — 0 falhas; clippy `-D warnings` 0 nos 7 crates; 3 warnings pré-existentes corrigidos (REGRA #21). d4 **completed** no DAG canônico da analise; memória `impl:c2-w0-subcall-identity-2026-08-24`.
- **Próximo do backlog aprovado**: w3b (bindings `snippet_*` dinâmicos no orchestrate, fecha d2) → d6/d7 (lado analise).
