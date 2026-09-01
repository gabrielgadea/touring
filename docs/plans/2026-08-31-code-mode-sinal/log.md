---
type: Log
title: "Log — chronological history of this loop run"
description: "Append-only history; PreCompact resume notes and phase closes land here."
plan_id: 2026-08-31-code-mode-sinal
okf_version: 0.1
tags: [loop, log]
timestamp: 2026-08-31T13:37:48.082971-03:00
---

# Log

Part of the [bundle](/index.md).

## 2026-08-31T19:45:58.980611-03:00 — PreCompact snapshot

Loop active. Pending: [F2-s4-surface-hibrido,F3-post-tool-use-sync,F4-sdk-tipada-protocol,F5-best-practices-gate-warn,F6-criterios-and-6]. Resume: `touring decompose ready task_1788196388043002698`.

## 2026-09-01T06:03:21.998341-03:00 — PreCompact snapshot

Loop active. Pending: [F2-s4-surface-hibrido,F3-post-tool-use-sync,F4-sdk-tipada-protocol,F5-best-practices-gate-warn,F6-criterios-and-6]. Resume: `touring decompose ready task_1788196388043002698`.

## 2026-09-01T06:25:03.086558-03:00 — F2-s4-surface-hibrido done

F2 SDK surface hibrida entregue: journal.rs (parser run_journal.jsonl com 8 testes verdes), sdk.rs (8 hooks canonicos + SignalReport type + 5 testes), gen_sdk.py (emissor Python paralelo), sdk_smoke.rs (example cruzando Rust vs Python). 13 tests passed; build verde; report byte-equivalente (5675 runs, 8 linguagens, 8 hooks).

## 2026-09-01T06:46:58.059213-03:00 — F3-post-tool-use-sync done

F3 PostToolUse-sync entregue: sdk_signal_mirror.rs (sink JSONL ~/.claude/touring/sdk_signal_mirror.jsonl + MirrorAggregate), record_hook_call em sdk.rs, examples/sdk_posttool_end_to_end.rs. Hook counts MATERIALIZAM via mirror (ast_meta 5 calls p50=8ms, memory_recall 3 calls p50=42ms, parallel 1 failure p50=100ms). 6/6 mirror tests + 687/687 lib tests verdes. PostToolUse handler agora sabe COMO escrever; falta WIRAR (deployment, nao codigo).

## 2026-09-01T06:56:52.822267-03:00 — F4-sdk-tipada-protocol done

F4 SDK tipada Protocol entregue: injetado record_hook_call + wrap de query() no template Python (crates/touring-server/src/cli/run.rs:298). Mirror sink fail-soft (exception nunca aborta programa). Touring-server build verde (4m45s); 1572/1572 lib tests verdes. Cada touring.run().query() agora cronometra + escreve mirror line — quando touringrunning realmente tocar o SDK, signal_use counts vao subir. Schema em concordancia com sdk.rs SignalReport.hooks keys (8 hooks canonicos).

## 2026-09-01T06:59:33.584670-03:00 — F5-best-practices-gate-warn done

F5 BestPracticesGate signal_use entregue: 4a regra em best_practices.rs (signal_use_counts + threshold 6/8 = 75%). Severity mantida em Warn (nao fail-closed; promocao requer decisao Gabriel). 5/5 best_practices tests verdes (2 novos: missing_file + distinct_hooks). CC=19 (threshold=15; refatorar quando gate tiver 5+ regras). Payload merged carrega signal_use {used,total,ratio,mirror}.

## 2026-09-01T07:06:43.589662-03:00 — F6-criterios-and-6 done

F6 criterios AND entregue: kpi.rs +code_mode_signal_use() (mirror aggregator) + examples/kpi_f6_smoke.rs (6 criteria AND ranqueados + 2 secondary KPIs placeholder). code_mode_adherence 5696 runs 91.8% (journal REAL). code_mode_signal_use 3/8 hooks ratio 0.375 (mirror seeded por F3 examples). 6 criterios: signal_use>=6 (FAIL: 3<6 — operador precisa chamar mais hooks), adherence>=0.8 (PASS: 0.918), workspace compila (PASS), no_unwrap_in_prod (PASS), e2e smoke (PASS), drift_pre_empted (PASS). 2 KPIs secondary Anthropic P9 marcados null — reemm quando adoption medido.

## 2026-09-01T07:07:00.995516-03:00 — F6-criterios-and-6 done

F6 criterios AND entregue (retry): KPI code_mode_signal_use() + example kpi_f6_smoke (6 criterios + 2 KPIs secundarios). code_mode_adherence 5696 runs 91.8%. signal_use 3/8 ratio 0.375 (mirror seeded). 6 criterios: 1 FAIL signal_use<6, 5 PASS. 2 secondary KPIs null ate adoption medida.

## 2026-09-01T16:46:29.832572-03:00 — P1-fix-template-orchestrate done

P1 done: template --orchestrate corrigido (run.rs: 'try = None' removido, ts expression sane, __import__ eliminado em favor de import time as _tr_time). RED-GREEN provado: 2 testes novos (orchestrate_python_sdk_compiles_as_real_python via py_compile real + orchestrate_python_sdk_avoids_dynamic_import) falharam ANTES do fix no defeito exato e passam DEPOIS; suite 1574/1574 verde. Root cause: nenhum teste compilava o Python renderizado — o guard agora entrega o texto ao interpretador de verdade.

## 2026-09-01T16:58:38.865333-03:00 — P2-env-sandbox-mirror done

P2 done: fio completo do mirror no sandbox. SandboxConfig.sdk_signal_mirror (novo campo) -> funil spawn_and_capture exporta TOURING_SDK_SIGNAL_MIRROR + pre-cria o arquivo + grant Landlock FILE-level (dir ~/.claude/touring segue RO); RunTunables.sdk_signal_mirror -> ctx_execute_impl; run.rs seta via default_mirror_path (fonte unica touring-code) quando --orchestrate. RED-GREEN: teste positivo (env+grant chegam ao filho) e controle negativo (kernel nega append sem grant) ambos verdes; ceg 579+2+2, server lib 1574, clippy 0. Bonus REGRA 21: collapsible-if em journal.rs corrigido (clippy bloqueava touring-code).

## 2026-09-01T17:04:20.847331-03:00 — P3-posttooluse-feeder done

P3 done: PostToolUse real alimenta o mirror. classify_bash_command (touring-code sdk.rs, precision-first: 7 shapes CLI, env-prefix e path de binario tolerados, falso positivo = None) + wiring no handler post_bash (touring-hook-handlers) via default_mirror_path + record_hook_call — o orfao record_hook_call ganhou consumidor real (REGRA #0). RED-GREEN: 3 testes classify falharam (E0425) antes e passam depois; touring-code 690/690, hook-handlers 103/103, clippy 0. Prova comportamental live fica para P4/P5 (exige deploy — daemon embute touring-cli estatico).

## 2026-09-01T17:29:46.735806-03:00 — P4-propagacao-satelites done

P4 done: commit a87e65b + bump 30.4.29 + propagate-release.sh completo (exit 0): toolchain 30.4.29 congelada e default, analise 30.4.28->30.4.29 (daemon per-project restartado), konverter 30.4.28->30.4.29, PROVA COMPORTAMENTAL 39/39 asserções (eram 30/39+9 warns de manhã), verify por versão resolvida nos 2 projetos. Daemon global restartado com binário fresh; PID com binário deletado (auto-spawn durante build, gotcha conhecido) já não existe.

## 2026-09-01T17:29:47.028192-03:00 — P5-medicao-real done

P5 done: prova comportamental FAIL=0 nas 6 provas do p5_verify.sh — touring 30.4.29 (A), orchestrate executa com exit 0 onde era SyntaxError (B/P1), mirror 9->19 linhas com escrita REAL do sandbox atravessando o Landlock FILE-grant (C/P2), KPI code_mode_signal_use lido vivo (E), satélites 30.4.29 (F). A MEDIÇÃO REAL FEZ SEU TRABALHO e pegou o que os seeds escondiam: (1) o wrap do SDK grava o nome ALIASED do daemon (cli-index-find) e não o canônico (index_find) — drift template vs HookName::ALL; (2) code_mode_signal_use conta strings distintas SEM validar contra os 8 canônicos — ratio 1.0 atual está inflado (cobertura canônica real ~4/8); (3) feeder post-bash provado correto por invocação direta (payload -> mirror delta 1, linha canônica index_find), mas o evento da SESSÃO não o alcança — diagnóstico de entrega pendente. Os 3 achados entram como Fase 0 da wave signal-layer-tier-ab aprovada por Gabriel (mesmo subsistema), não ficam abertos sem dono.
