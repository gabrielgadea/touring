---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-19-doctor-poliglota
tags: [loop, log]
timestamp: 2026-08-19T19:19:58.798837-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-19T19:37:20.523357-03:00 — P1 done

config polyglot_wiring + polyglot_wiring_for_root em touring-foundation (env > projeto > usuario > false); 5 testes, incl. dois projetos discordando e toml malformado nunca ligando

## 2026-08-19T19:37:20.591063-03:00 — P2 done

FileKnowledgeDB.polyglot resolvido 1x por abertura; 5 leitores migrados do OnceLock global (3 SQL de leitura + gate de escrita + gate Go em index.rs); polyglot_wiring_enabled REMOVIDA; 4 testes de comportamento provam gate por projeto e a defesa dos 258 FP intacta

## 2026-08-19T19:37:20.649418-03:00 — P3 done

doctor e health reportam polyglot=on|off ao lado de root=; o censo so e' legivel com o modo declarado

## 2026-08-19T20:17:56.144734-03:00 — P4 done

Opt-in ligado em analise/konverter/transferegov (touring fica off: ganho medido de so 45 arquivos, o resto do JS era rustdoc). O campo polyglot= do doctor pegou meu proprio erro na hora: escrevi a chave depois de [daemon], virou daemon.polyglot_wiring e foi ignorada silenciosamente. Ligar sozinho PIOROU o grafo (orfaos de analise 5.975 -> 142.689) porque o leitor discorda do escritor: filtro de leitura olha extensao, gate de escrita olha caminho. 72,4% dos produtores legiveis eram virtualenv. Corrigido em 30.4.8 (expurgo como invariante) e 30.4.9 (expurgo estava mais severo que o gate: julgava o consumidor pelo predicado completo e apagou 13.621 arestas de teste legitimas no touring). Provado: 6.077 consumidores em tests/ e 822 em benches/ sobrevivem; 0 produtores em tests/; 0 em target/.

## 2026-08-19T20:40:29.512582-03:00 — PreCompact snapshot

Loop active. Pending: [P5]. Resume: `touring decompose ready task_1787178729444366820`.

## 2026-08-19T21:36:54.353119-03:00 — P5 done

Exercitar o poliglota expos 3 defeitos que planejar sobre ele nao expunha. (1) Resolvedor Python e Java fabricava arquivos: 'import pathlib' virava produtor para pathlib.py — 88 fantasmas em analise, 76 com kind unknown; os bracos rust e ts/js provam existencia no disco ha meses, e o rust aprendeu isso DUAS vezes. Agora os 4 bracos provam. 2 testes que asseguravam o bug foram substituidos. (2) O predicado do doctor perguntava 'e Rust?' e errava nas duas direcoes: konverter (18.695 linhas) e transferegov (4.268) eram limpos e levavam warning so por serem Python; touring era absolvido carregando 102 linhas de benches/. Trocado por 3 contadores onde so non_wireable julga, chamando is_wireable_source — o vocabulario do ESCRITOR — nos 3 sitios (C08). (3) 242 truncamentos &s[..s.len().min(N)] matavam o project-actor em texto acentuado; o proprio hook pre_edit avisava sobre esse antipadrao numa string literal. Guard estrutural achou 11 sitios que minha regex perdeu. Provado por comportamento em 30.4.10: 4 predicoes registradas antes do deploy, 4 acertos — konverter e transferegov warning->ok sem reindex.

## 2026-08-19T23:21:49.692810-03:00 — PreCompact snapshot

Loop active. Pending: []. Resume: `touring decompose ready task_1787178729444366820`.
