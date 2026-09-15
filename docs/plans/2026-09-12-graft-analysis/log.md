---
type: Log
title: "Log — 2026-09-12-graft-analysis"
description: "Chronological history of the phases closed in this bundle."
plan_id: 2026-09-12-graft-analysis
okf_version: 1.0
tags: ["#kind:log", "#artifact:log"]
timestamp: 2026-09-12T21:09:53.599668-03:00
---

# Log — 2026-09-12-graft-analysis

Cada entrada é um fecho de fase registrado por `loop_phase_close.py`.
O plano: [`plan.md`](/plan.md)

## 2026-09-12T21:09:53.599668-03:00 — P1 done

Aquisição: clone de trailhq/Graft (commit f9e6539, 478 commits), metadados via gh (7.297 estrelas, 665 forks, criado 03/07/2026, MIT, @nanonets/graft 0.18.0), git stats (433 commits sem merge, 2 autores nanonets = 75%, 10 tags), varredura estrutural (270 TS / 57.962 LOC, 122 test files).

## 2026-09-12T21:09:53.924266-03:00 — P2 done

Exploração estrutural: leitura integral de README/CHANGELOG/TELEMETRY/SECURITY/CREDITS/docs/github-app/CI/Dockerfile/SKILL.md; esqueleto de exports dos 145 arquivos de src+viewer; mapa de módulos com LOC e acoplamento; 25 comandos CLI, 6 tools MCP, 11 hosts, 16 .scm.

## 2026-09-12T21:09:54.265348-03:00 — P3 done

Análise profunda de código: schema v1 (NodeV1/EdgeV1/Confidence), invariantes, resolução com famílias de linguagem, ranking idf+PPR+fusão por escopo, freshness fingerprint+refresh sob lock, blast diff∩grafo, savings, hooks Claude Code, contrato de telemetria, registry de hosts. Build a partir do source; suíte 1217/1221; causa-raiz das 4 falhas = diff.mnemonicPrefix (reproduzida e revertida por override, 11/11).

## 2026-09-12T21:09:54.590069-03:00 — P5 done

Comparativo com o Touring: sonda em 3 crates Rust (2.857 nós, hubs falsos path 260←), mesma consulta run_gateway nos dois (8 vs 7 consumidores), tabela de 16 eixos, 8 itens a importar (I1-I8), 4 a não importar (N1-N4), riscos e decisões humanas.

## 2026-09-12T21:19:47.401801-03:00 — P4 done

Pesquisa externa (coletada diretamente após o agente web ficar mudo): issues 54/54 mediana 5,3 dias; PRs 110 merged/64 abertos; npm 30 versões, 11.632 downloads/semana; Show HN 39 pts/44 comentários com crítica de p=0,22 e amostras arbitrárias; site trailhq.com/graft afirma 'No telemetry' contradizendo TELEMETRY.md; categoria com CodeGraph 47k e GitNexus 42k estrelas.

## 2026-09-12T21:22:18.166571-03:00 — P6 done

Síntese: sequential-thinking em 6 passos; analysis.md v2 (578 L, 13 seções, OKF, provenance blake2b da v1) emitido por okf_emit; página publicada https://claude.ai/code/artifact/740de4b8-a816-4155-880b-cf37e371f73a (estática, sem JS, gerada por build_artifact.py); 3 memórias Touring + 2 memórias locais + 3 rewards; veredito: Graft é análogo direto na camada contexto-portátil; importar freshness pré-query, experimento de seleção de tools, SWE-bench cold-vs-touring, razão touring-vs-Read/Grep, cards grep-áveis, confidence por aresta; não importar savings-baseline nem breadth por nome.

## 2026-09-12T21:39:43.921742-03:00 — R1 done

Rodada 2 — viabilidade: rust-analyzer 1.98 disponível; nenhum runtime LLM local (sem ollama/llama-server, sem chaves no ambiente; GPU RAG :8200 só embeddings) → --deep segue impossível; DAG R1-R7 criado.

## 2026-09-12T21:39:44.149719-03:00 — R2 done

Escala: workspace Touring inteiro 2.211 arquivos → 47.055 nós/58.252 arestas em 15 s, RSS 1,74 GB, ask 0,8 s, sonda ≈90 ms, check 7,7 s. LSP: +607 calls/lsp_resolved, 1.010 inferred intactas, hub falso path 260← sobrevive; callers não expõe confidence (PR #336). Freshness: touch não reconstrói; edição real reconstrói 1 arquivo em ~170 ms; nota vai para stderr.

## 2026-09-12T21:39:44.378813-03:00 — R4 done

Retrieval determinístico: graft ask TS hit@1 9/12 hit@3 12/12; Rust breadth 8/10 e 10/10; touring tantivy 0/10 nas mesmas perguntas NL (search unified/bm25/fuzzy vazios; índice mistura memória/DAG/docs; três alfabetos de caminho) — lacuna do Touring registrada.

## 2026-09-12T21:41:32.205806-03:00 — R3 done

--deep continua sem execução (nenhum runtime LLM local, nenhuma chave no ambiente); substituído pela extração verbatim dos três prompts (summarize 3-8 frases; synthesize system/file/concept com 7 verbos fechados e 'NON-OBVIOUS information', lotes de 48k chars; crux ≤8 linhas, 0/0 para trivial, misses classificados) e do formato do nó em disco (frontmatter, marcadores generated, notas humanas preservadas, manifest, /graft/ ancorado, .ignore re-admit).

## 2026-09-12T21:41:32.452022-03:00 — R6 done

Código não lido + segurança: gate de injeção (STRONG_FLOOR 0,1 / HIGH_FLOOR 0,5, ≤3 ponteiros, 40 lembrados, 2 nudges, INJECT_MIN_COVERAGE 0,15 superado por caso real), bloco de hosts verbatim, settings-merge (5 hooks com timeouts, allowlist derivada da forma de invocação), shim com 5 candidatos incl. <projeto>/dist/claude como último recurso, Brain digest (commits, PRs, 4.000 símbolos com assinatura, CLAUDE.md/CI/lint íntegros ≤300k chars), tier generic (rust.scm sem receptor, exported:true sempre), LSP só outgoingCalls e aditivo; npm audit 1 high transitivo (js-yaml), node_modules 440 MB, .mcp.json via npx -y.

## 2026-09-12T21:46:04.015154-03:00 — R5 done

Arqueologia + comunidade: pivô de motor de contexto para PDFs/web UI (03-08/07, BUSINESS-CASE/PRODUCT-DESIGNS) para code-graph em 13/07; harness bench/ recuperado (3 braços, Sonnet 5 + juiz Opus 4.8 + piso de keywords, custo cache-aware, split por localidade, 162 = 18×3×3, 2 corpora privados); spec de 20/07 declarava 'tokens saved %' e $ como 'not measurable — excluded by design', depois exibidos; 74 PRs abertas como roadmap (4 corrigem achados da rodada 1); Trendshift #4 TS em 23/08; stargazers 404/vazio; npm audit 1 high transitivo; agente de arqueologia não entregou — leitura direta dos essenciais.

## 2026-09-12T21:46:42.414036-03:00 — R7 done

Síntese v3: analysis.md re-emitido com 20 seções (882 L, provenance blake2b da v2), §0/§11/§12 revisados, sete seções novas (13-19); página republicada em https://claude.ai/code/artifact/740de4b8-a816-4155-880b-cf37e371f73a; 2 memórias Touring + 1 local atualizadas; sequential-thinking 5 passos; lacuna aberta: --deep sem runtime local.

## 2026-09-12T22:27:40.901556-03:00 — R3-1 done

Delta upstream + OUTER: 0 commits após f9e6539, npm 0.18.0 última, 7.302 estrelas/666 forks/128 issues, 11.632 downloads/semana; issues #360-#368 de 11-12/09 confirmam §17 (#366 timeouts do post-edit) e #363 callers; sem cobertura externa nova; strategy-loop ADW pass (diagnostics/touring-20260912T220504.md), marcador liberado.

## 2026-09-12T22:27:41.145652-03:00 — R3-3 done

Segurança de conteúdo por execução: repo sintético (4 segredos, 3 canários de injeção, .env, symlink externo, unicode/espaço/latin-1, 3,9 MB, binário); 6/11 indexados, big.ts >1 MB e symlinks pulados em silêncio, nada lido fora do repo; cards só assinaturas, segredos/injeção só em .cache/extract.*.json; grep devolve literais verbatim; hook UserPromptSubmit injeta 519 B só de assinaturas (0/7 canários), SessionStart 3.151 B; com grafo --deep o hook injeta prosa do modelo; --deep envia arquivo inteiro (todos os 7 canários chegaram ao endpoint).

## 2026-09-12T22:27:41.383966-03:00 — R3-4 done

Robustez: 2 builds frios byte-idênticos (cards, wiring.json, ask-index); SIGKILL aos 4 s de 15 s → 0 arquivos, ask 105 ms, check NO GRAPH; 4 asks concorrentes após edição: 1 refresh, saídas idênticas, caches íntegros (LOCK_WAIT_MS 2000); sem .git e vazio degradam sem erro; ACHADO: cards markdown não reprojetados pelo refresh incremental (wiring/ask-index/skeleton sabem, card não; só build reescreve) — não reportado upstream; teto 1 MB silencioso com mensagens enganosas em grep/skeleton; 105,7 s observado 1× não reproduzido.

## 2026-09-12T22:27:41.623954-03:00 — R3-5 done

Multi-host + captura ao vivo: init --all-agents dry-run (22 escritas no repo + 8 na máquina) e real (19 arquivos: SKILL.md 9.448 B ×3, seções cercadas 2,3 KB ×4, MCP npx -y em 6 arquivos, hooks só Claude/Cursor); telemetria capturada em servidor local: first_run/query{command,surface,hit,repo_id}/build_completed{files_bucket,langs,mode,duration_bucket,incremental} + agent_host=claude-code, distinct_id UUID, sem conteúdo; brain push capturado: 1.031.941 B com 433 commits (corpo verbatim, 53 noreply@), 70 threads de PR, 907 assinaturas, 6 workflows CI íntegros — 'No file contents leave this machine' falso para config.

## 2026-09-12T22:27:41.830995-03:00 — R3-6 done

--deep sob servidor OpenAI-compatível simulado: summarize (texto, arquivo inteiro ≤24k) + record_symbols (tool forçada, arquivo numerado ≤18k) por arquivo + record_graph (≤60k) por build; rust-probe 125 arquivos → 251 requisições, 3,57 MB (~0,9 M tokens), máx 4 em voo, 0 cache_control; build repetido 0 chamadas; 1 edição = 2 chamadas + síntese inteira (41,7 KB); HTTP 500 → 5 tentativas, exit 1, --allow-partial exit 0, 'nothing computed was lost'; escreve nós conceituais YAML+prosa, cards com [[link]] e resumos, manifest.json; ask devolve conceito, check reporta meaning tier, viz --export 57 KB; extrapolação Touring ≈ 16 M tokens.

## 2026-09-12T22:28:04.292049-03:00 — R3-1 done

Delta upstream + OUTER (re-fechamento para a DAG): 0 commits após f9e6539, npm 0.18.0, 7.302 estrelas, issues #360-#368 confirmam §17; strategy-loop pass.

## 2026-09-12T22:32:49.386505-03:00 — R3-2 done

Arqueologia parte 2 (leitura direta; subagent ocioso pela 3ª vez, parado): PRODUCT-DESIGNS 06/07 recomendava a Ideia 1 (memória de agentes de código, write-back, sync git) e 'ship Idea 5's engine packaged as Idea 1'; BUSINESS-CASE do mesmo dia escolheu a Ideia 2 (team brain para docs) porque as ideias dev foram 'excluded by constraint', com 'not npm-first' e 'never leave your machine'; 13/07 lançou a ideia dev sem o motor de reforço; specs 15-23/07: 6 verbos enforced por enum, shim com fallback dist/claude, sidecar ask-index (<3 s em 32k nós), MCP 'exactly npx -y', RRF K=60 com gates byte-idênticos; harness: token-ab.ts define 'saved' = 1 − total GRAFT/total COLD sobre a execução inteira com juízo humano de equivalência — o rodapé mede outra grandeza com o mesmo nome.

## 2026-09-12T22:34:35.053307-03:00 — R3-7 done

Síntese v4: analysis.md re-emitido com 27 seções (1.192 L, provenance blake2b da v3), §0/§12 revisados, sete seções novas (20-26); página republicada em https://claude.ai/code/artifact/740de4b8-a816-4155-880b-cf37e371f73a (Version 3); resolver de fase do loop_phase_close aceita ':' (TDD, 7/7, espelho client/ sincronizado); servidor de captura parado; memórias atualizadas.

## 2026-09-12T23:05:13.167041-03:00 — E1 done

Três issues abertas em trailhq/Graft: #369 blast/diff.mnemonicPrefix (whole-file seeds), #370 teto de 1 MB silencioso, #371 cards markdown obsoletos após o refresh; nenhuma duplicada; corpo em inglês com reprodução em shell e correção sugerida.

## 2026-09-12T23:05:13.403800-03:00 — E2 done

Busca por prosa do Touring (I9): instrumento provado (bench com cwd no scratchpad caía no índice GLOBAL; tantivy search real = 7/10 hit@1); causa-raiz cli_search_symbols/docs com LIKE '%frase inteira%' e unified sem tantivy; fix: LIKE por token ranqueado + BM25 tantivy com fallback + normalize_path na fusão RRF; testes 6/6 + 32/32, clippy limpo, update-touring deploy (PID 376309); search unified 0/10 -> 8/10 hit@1, bm25 0/10 -> 6/10; doctor 7/7.

## 2026-09-12T23:05:13.598326-03:00 — E3 done

--deep com MiniMax-M3 (wire Anthropic) em 125 arquivos: 979 s, cobertura 84% (2.400/2.857), 10 arquivos empty-parsed/truncated, 13x HTTP 429 do plano, passe de conceitos parou após 12 falhas; 33 nós conceituais, 2.732 resumos, 0% eco de fonte, mediana 16 palavras; prosa precisa (constantes, invariantes, 'why'); retrieval hit@1 6/10 (só-wiring 8/10), hit@3 10/10; canários tratados como canários, valores de segredo não copiados para a prosa.

## 2026-09-12T23:05:13.831885-03:00 — E4 done

Canvas de decisão I12/I13/I16 preenchido (9 seções): recomendação B — I13 já (oversized_skipped existe em handlers/index.rs:583, falta superfície), I16 como medição de 1 h antes de fase (store.rs usa BEGIN/BEGIN IMMEDIATE, nunca provado sob kill -9 nem diff de dois rebuilds), I12 como regra de design até existir projeção; confiança 0,8.

## 2026-09-13T00:03:52.120874-03:00 — B2 done

I13 entregue: RPC cli-index-why + 'touring index why <path>' — tabelas do walker hoisted para o módulo (uma cópia), classify_index_candidate puro na ordem do walker, 10 vereditos; 4 testes novos (incl. e2e in-process sobre o rebuild real com huge.rs de 8 MiB+1), parse test CLI, tripwires 239->240/243->244/245->246 atualizados nos 4 sítios; clippy limpo em 4 crates; deploy PID 505359; rebuild vivo 4.613 arquivos/208 s, doctor 7/7; 9 caminhos reais → 9 vereditos (CI .github não indexada; baseline JSON 29 MB oversized; analysis.md eligible_not_indexed).

## 2026-09-13T00:03:52.372833-03:00 — B3 done

I12 como regra de design em crates/touring-cli/.claude/CLAUDE.md (invariantes): projeção legível PERSISTIDA do índice carrega o fingerprint do índice que a gerou e é marcada obsoleta quando o índice muda; hoje não há projeção persistida (map/overview sob demanda) → regra, não fase; vira fase quando uma for escrita. Cheatsheet touring-cli-index.md ganhou 'touring index why'.

## 2026-09-13T00:11:13.895624-03:00 — B1 done

I16 medido em cópia per-project isolada (7.392 arquivos, caminho curto por SUN_LEN): rebuild 183 s, 284.757 símbolos/84.695 wiring; kill -9 aos 3 s deixa wiring parcial (−85 linhas) com SQLite íntegro e sem sinal na consulta → atomicidade REPROVADA; rebuilds a quente byte-idênticos (0 linhas diferentes), rebuild frio difere pelo backfill de kinds (571 vs 54) → determinismo aprovado a quente, reprovado a frio. Veredito: I16 vira fase (rebuild por geração + convergência em um passe), aceite = os dois scripts.

## 2026-09-13T09:07:49.063691-03:00 — Z0 done

Passo zero do I16: politica de admissao hoisted para touring_hooks_shared::index_policy (uma copia: walker, varredura, escritores de hook, why); varredura do rebuild purga pelo predicado do walker (skipped_dir/outside_root/unsupported/oversized/missing/duplicate_spelling) e reporta policy_purged_by_reason; reindex_file_with_old recusa o que o walker recusa (140 arquivos absolutos e 306 sob .claude/ nao voltam a entrar); index why reporta residue; --dir . deixa de duplicar o store (4.483 arquivos/331.997 linhas dobradas desde 04/09). Testes: index_policy 4/4, e2e in-process why+reindex+purga, 557 touring-cli x3, clippy limpo.

## 2026-09-13T09:07:49.309301-03:00 — Z3 done

2A: removido o backend hybrid do search unified (InMemoryVectorStore vazio por chamada, resultado descartado a menos que os dois backends reais viessem vazios); unified funde symbols+docs por RRF. Os tres irmaos (find_code.rs, search_tools.rs, main.rs) ja estavam corrigidos por outra sessao (sem store, backend keyword do portfolio) — o canvas superestimou o escopo; corrigido no relatorio. Testes search_unified 37/37, touring-server 1600/1600.

## 2026-09-13T09:30:33.631315-03:00 — Z2 done

I16-b determinismo: caracterizado na copia (symbols 0 diffs; wiring 24 linhas, todas kinds de consumidor escolhidos por homonimo arbitrario) e corrigido por resolve_consumer_kinds_from_producers, passe unico pos-caminhada (mesmo modulo -> homonimo ordenado por (module_file,kind) -> extern so em caminhada completa), idempotente. Aceite com o binario novo (i16_accept.sh, cold vs warm): symbols 0 / wiring 0 linhas diferentes; 284.757 simbolos e 84.695 arestas nos dois.

## 2026-09-13T09:30:33.819140-03:00 — Z4 done

3A: cli-search-docs ganha mode=bm25|fuzzy; search fuzzy roteia a search_rrf (BM25+edit-distance-2+trigram por RRF) e search bm25 fica no BM25 puro — os dois deixaram de enviar os mesmos bytes (teste do payload). Bench nas 10 perguntas de prosa: bm25 hit@1 6/10 vs fuzzy 4/10 (hit@3 7/7) -> UNIFIED_DOCS_MODE = bm25, decidido por numero; fuzzy fica para typo (HokRuntime -> HookRuntime).

## 2026-09-13T10:01:47.672843-03:00 — Z1 done

I16-a atomicidade: selo index_generation (building/complete/aborted, tabela sob demanda; partial quando o dono morreu) exposto em index status/find/search/why e no doctor (server+cli); transacao por arquivo no wiring; arestas inferidas limpas e regravadas so na transacao pos-caminhada (a 1a aceitacao perdeu 91 por limpa-las na caminhada). Aceite i16_accept.sh (deploy 2): kill -9 aos 3 s -> 84.698 vs 84.695 arestas (0 perdidas, +3 fantasmas), partial explicito em 5 leitores, recuperacao 0/0 vs quente, geracao complete. Evento sem causa: daemon da copia morreu aos 61 s na 1a rodada, nao reproduzido em 4 rebuilds; o selo o tornou partial explicito.

## 2026-09-13T10:04:49.394723-03:00 — Z5 done

Deploy: tres update-touring (PIDs 1139447, 1200200, 1241253), doctor 8/8 com index_generation. Provas vivas: r6_live_proof.sh (rebuild 238 s, 5.867 arquivos purgados por motivo, store 697.101 -> 334.410 linhas, geracao 1 complete, residue false), tantivy reindex 64.478 -> 123.420 docs em 9 s e bm25 top-3 volta ao codigo vivo, search fuzzy acha HookRuntime, unified funde dois backends reais, bench depois (tantivy 7/8, unified 7/8, bm25 7/8, fuzzy 4/6). Secao 29 emitida (analysis.md v7, 30 secoes), index.md reemitido, artefato Version 7, memorias (auto-memory + 3 licoes Touring + analise:trailhq-graft:rodada6), espelho client CLEAN.

## 2026-09-13T14:51:19.635549-03:00 — Y0 done

Regra .claude/: o diretório entra no índice; touring/, session_summaries/ e cipher_queue/ recusados por nome (CLAUDE_STATE_DIRS) com o detalhe 'holds Touring runtime state, not documents'; dir_skip_reason(components) decide para walker, sweep e why; testes unitários + e2e com fixtures .claude/CLAUDE.md, skills/SKILL.md, touring/state.json, checkpoints/c.json.

## 2026-09-13T14:51:19.909725-03:00 — Y2 done

Bloco 2: Shape::would_overflow passa a ser fits dobrado sobre as linhas (teste would_overflow_is_fits_folded_over_lines, 12/12); ViolationsDiff::counts registrado como decisão de produto pendente para Gabriel (ligar ao histórico de qualidade ou apagar); NetworkScope::port e DocSymbolSignalLayer::new intocados por decisão.

## 2026-09-13T14:51:33.973758-03:00 — Y1 done

Raízes-companheiras: TouringConfig::companion_roots_for lê [index] companion_defaults e [index.companion_roots] (~ e {slug} expandem; defaults rules/commands/agents/skills/memory/memory-home; raiz sem .touring/ não tem companheiras). IndexPolicy {key_for, path_for_key, verdict_for_key, companions}; chave @companion/<nome>/<rel>. Walker caminha cada companheira contida à própria raiz; why responde por chave e por caminho absoluto; ingest reporta status refused em vez de ok-sem-escrever e grava sob a chave; sweep purga companion_not_configured; payload companion_roots {nome: arquivos}. admission_verdict removido (órfão) — a struct é a API. Testes: 7 em paths.rs, 6 em index_policy, e2e companion_roots_are_indexed_under_stable_keys_and_swept_by_the_same_policy; clippy -D warnings verde em foundation/hooks-shared/hook-runtime/touring-cli; 413+558 testes.

## 2026-09-13T19:40:53.086212-03:00 — Y1b done

Conteúdo buscável: markdown vira texto indexado (documento com descrição + corpo até 20k, blocos de 600), doc comments de código, rebuild/hooks/reindex alimentam o tantivy pelo mesmo construtor (tantivy_docs::docs_for_file), analisador com acentos e stopwords pt/en, frase por analisador, cobertura constante (5 + 1,25 por subconjunto), agregação por arquivo λ 0,25, compactação após escrita. Medido por examples/search_eval.rs sobre 75 perguntas: conteúdo 0/11 -> 11/11, code-train 9/10 estrito (10/10 corrigido), total 64/75 ao vivo = offline. Defeitos corrigidos: cache de consulta nunca invalidado, frase contra stemmer, top dobrado, ordem dependente do limite, compactação silenciosa, reindex apagando doc comments, cache de consulta sem escopo entre projetos.

## 2026-09-13T19:40:53.344178-03:00 — Y3 done

Deploy e prova viva no touring: update-touring com doctor 5/5, reindex completo (171.398 upserts, 6 segmentos fundidos, compact_error nulo), avaliação ao vivo 64/75 idêntica ao offline nas rotas tantivy search e search unified; index status provado por projeto contra o daemon (global 59, touring 5.223, analise 40.962, touring 5.223); companheiras agents 11, commands 25, memory 99, memory-home 152, rules 17, skills 699 no rebuild selado geração 9 complete.

## 2026-09-13T23:45:16.813108-03:00 — Y4 done

Propagação 30.4.40→30.4.43 para analise e konverter (prova comportamental 40/40, versões verificadas). Rebuilds provados: konverter geração 2 complete, 4.354 arquivos, 588 purgados (581 skipped_dir, 4 outside_root, 3 unsupported_extension); analise geração 4 complete, 50.573 arquivos, 17.558 purgados reconstruídos por diff de snapshot + index why (13.227 skipped_dir, 2.876 outside_root, 1.412 missing, 25 unsupported_extension, 11 skipped_subproject, 7 oversized). Cinco defeitos achados só nos consumidores, todos corrigidos com teste: panic UTF-8 no scanner de chamadas qualificadas (abortava o daemon), título de 841.812 chars multiplicado por bloco (1,2 GB de texto), guarda de memória contando páginas de arquivo (agora anônima, teto TOURING_REBUILD_MEMORY_HARD_MB), prova do release herdando ledger entre execuções (nonce por execução), índice ausente em wiring_map.consumer_file (migração v11). Work (pin 30.4.13, fora do registro) fora do escopo aprovado.

## 2026-09-13T23:46:36.869908-03:00 — Y5 done

Docs e fechamento da rodada 7: analysis.md v8 com §30 (31 seções; proveniência blake2b16 da v7 completa 862f564b94fbd542ab73cfc80fe3f91c), index.md com Y0-Y5 e bench/, CLAUDE.md do touring-cli (texto no índice, cobertura, limites de título e nome, guarda de memória anônima e TOURING_REBUILD_MEMORY_HARD_MB, TOURING_REBUILD_SEARCH_DOCS), cheatsheet das rules, memórias novas (cache-sem-escopo, corte-de-byte-em-str, titulo-gigante, indice-parcial) e atualizadas (busca-so-via-nomes, comentario-afirma-simetria, analise-trailhq-graft). Espelho client/ CLEAN 343 arquivos.

## 2026-09-14T01:19:17.670124-03:00 — V1 done

ViolationsDiff::counts explicado: diff_violations(previous, current) classifica MetricViolation em resolved/introduced/persisting por (rule_name, location); counts devolve os três tamanhos e total a soma. Nasceu no Sentrux Wave 2 P4 (09/05/2026), chegou pelo move a38ae33. Nenhum consumidor de produção: ViolationsDiff e diff_violations só aparecem no re-export de rules/mod.rs e nos 7 testes de diff.rs; tools_quality_signal avalia regras uma vez por chamada e a rota web /quality/diff compara notas, não violações. Decisão continua de Gabriel: ligar como catraca (bloquear só introduced deny) ou apagar.

## 2026-09-14T01:19:17.928864-03:00 — S1 done

Crash log do daemon reparado: install_hook abre o log; handler SA_SIGINFO|SA_ONSTACK sem alocação (clock_gettime, prctl, gettid); restaura a disposição anterior (falha repete, sinal enviado é re-levantado), preservando o diagnóstico de estouro de pilha do Rust; CrashContext nomeia o arquivo em indexação (rebuild e reindex_file_with_old). crash_path_tests: 3 filhos que morrem de verdade (falha, abort, estouro), RED antes (0 registros, sem mensagem de overflow), GREEN depois, 3 mutações cada uma reprovando teste; clippy e rustdoc -D warnings limpos. _panic_log_init sem consumidor removido.

## 2026-09-14T01:19:31.962653-03:00 — S2 done

Reprodução do SIGSEGV tentada e NEGATIVA: 3 rebuilds em processo (probe debug) de uma cópia por hardlink da árvore do touring com LD_PRELOAD=libc_malloc_debug.so e GLIBC_TUNABLES=glibc.malloc.check=3:perturb=165, todos rc=0 (250/216/215 s). Core 4181797 (30.4.43, build-id igual ao binário atual) analisado sem símbolos por varredura de pilha + strings: ts_parser_parse ← parser.rs thread_local ← method_calls.rs (query Rust) ← graph ← rebuild; PC=0x127 por ret com pilha desalinhada em 0x10 logo após ts_subtree_new_leaf, sem outra thread em tree-sitter. Core 1140844 (09-13, build sem snapshot) falhou dentro do malloc da glibc chamado de C do tree-sitter. Descartados por evidência: estouro de pilha (rsp perto do topo), mistura de alocadores (binding usa free da libc; jemalloc prefixado), SharedPipeline (parse sob write lock), wasm (sem WasmStore).

## 2026-09-14T09:37:56.999069-03:00 — R8-fechamento done

Rodada 8: SIGSEGV do daemon provado sob ASan (dois defeitos no scanner md), cessão cooperativa provada ao vivo, produtores de wiring, selo de geração, 75 de hook pesado, nota Silver por vendorizado; releases 30.4.44 e 30.4.45 propagadas com prova 40/40; juiz Platinum

## 2026-09-14T13:18:07.391115-03:00 — R9-decisoes completed

Rodada 9: 1-A companheiras fora do wiring, 2-A daemon em scope systemd próprio, 3-A index status fora do ator; 4.988 testes, 6 mutantes mortos, release 30.4.46 com prova 40/40, scope provado nos 3 daemons, status 75/75 em até 42 ms durante o rebuild
