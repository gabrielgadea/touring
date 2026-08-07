---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-02-memory-subsystem-repair
tags: [loop, log]
timestamp: 2026-08-02T14:56:51.026038-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-02T16:28:54.037705-03:00 — P1 done

D4 (erro engolido) corrigido em crates/touring-server/src/daemon_client.rs: quando response.success e falso, daemon_failure_message() extrai o campo 'error' do handler e, na ausencia dele, produz um trecho de 400 chars seguro em fronteira de char (evita panic em UTF-8 multibyte). Antes o CLI descartava a mensagem do daemon e emitia um erro generico, o que mascarava D1/D2/D3. 4 testes unitarios cobrindo: campo error presente, ausente, payload nao-JSON e truncagem em fronteira multibyte.

## 2026-08-02T16:28:54.115233-03:00 — P2 done

D1 (list contra formato abandonado) corrigido em crates/touring-cli/src/cli/memory.rs: cli_memory_list reescrito para consultar a tabela memory_entries (o formato vivo) em vez do layout legado; parse_memory_row legado removido junto com seu codigo orfao (REGRA #0). Antes o comando retornava vazio mesmo com 6.9k entradas gravadas.

## 2026-08-02T16:28:54.187726-03:00 — P3 done

D3 (gotcha_stats reportando campos que nao sao o que medem) corrigido: campos de GotchaStats renomeados para total_hits / total_prevented em crates/touring-cli/src/cli/handlers/dispatch.rs e nos callsites. Decisao do Gabriel via AskUserQuestion: 'Renomear para o que medem' em vez de trocar a semantica.

## 2026-08-02T16:28:54.275029-03:00 — P4 done

D2 (reindex bloqueando o ator do daemon ate estourar o timeout) corrigido em crates/touring-cli/src/cli/memory.rs: DEFAULT_REINDEX_BUDGET=2000 e helper reindex_candidates() com LEFT JOIN, reindexando incrementalmente so o que falta no corpus ANN; flags --max-entries e --all expostas em crates/touring-server/src/cli/memory.rs. Gotcha registrado: rodar o reindex antigo travou o subsistema de memoria por minutos nesta sessao, recuperado com touring daemon-ctl restart (REGRA #19, nunca pkill).

## 2026-08-02T16:28:54.361178-03:00 — P5 done

F1 (separar outcome:* do corpus de recall) implementado em crates/touring-cli/src/cli/memory.rs: OUTCOME_PREFIX='outcome:' e filter_outcomes() aplicado as TRES fontes (SQL federado, ANN, TF-IDF) ANTES da fusao RRF -- filtrar depois da fusao deslocaria o ranking. Flag --include-outcomes para o comportamento antigo. Decisao do Gabriel: 'Excluir por padrao, flag para incluir'. Baseline medida: 72% das recuperacoes iam para outcome:*, um vies ruido:sinal de 2.4:1.

## 2026-08-02T16:28:54.450166-03:00 — P6 done

F2 (familia KPI touring.memory.*) implementada em crates/touring-cli/src/cli/kpi.rs: memory_corpus_coverage (JOIN-based, estruturalmente incapaz de exceder 1.0), memory_curated_recall_share, memory_never_recalled_ratio, com memory_db() abrindo o DB read-only; 3 commitments registrados em docs/kpi/commitments.yaml (27 no total). Inclui D5, achado durante a implementacao: o parametro --sort do list nunca funcionou porque o CLI enviava a chave 'sort_by' e o handler lia 'sort'.
