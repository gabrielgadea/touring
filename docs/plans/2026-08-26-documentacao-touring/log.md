---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-26-documentacao-touring
tags: [loop, log]
timestamp: 2026-08-26T08:02:07.292463-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-26T08:26:07.233939-03:00 — F1 done

Mapa de fusao verificado: 20 nomes fantasma, 6 confirmados por definicao de simbolo (ALTA), 7 por tamanho dominante (MEDIA), 7 sem destino achado (BAIXA, provavel nunca-implementado). Escopo dividido: 26 arqs/105 mencoes no NUCLEO (crates/ARCHITECTURE+CLAUDE+README, ARCHITECTURE.md raiz, RFCs, CONSTITUTION) vs 30 arqs/457 mencoes que sao planos/relatos antigos (vao para pasta indefinidos, F6b). 3 dos 5 primeiros palpites por nome de diretorio estavam ERRADOS (ast->touring-code nao analysis; learning->intelligence::rl 1.7MB nao analysis::learning 12KB; cognitive->intelligence::reasoning::CognitiveRuntime sem diretorio homonimo algum) — confirmado que find/replace teria produzido doc confiantemente errada.

## 2026-08-26T08:28:07.658229-03:00 — F6b done

14 arquivos movidos de docs/ para docs/indefinidos/ via git mv (historico preservado), com index.md de proveniencia (idade/tamanho/motivo por arquivo). 3 falsos positivos da varredura original (memory-hashtag-library.md, code-mode.md, code-mode-execution-layers.md) VERIFICADOS como doc viva (citados por CLAUDE.md ou interlinkados entre si) e mantidos em docs/. 4 links quebrados identificados, todos em docs/internal/sessions/ (relatos historicos, politica de nao editar) ou client/ (gerado, regra 6) — registrados no index, nao corrigidos.

## 2026-08-26T08:45:22.565064-03:00 — F2 done

26 arquivos do nucleo reconciliados (105 mencoes corrigidas). Correcao NAO foi find/replace cego: cada mencao verificada por evidencia executavel (existencia de path exato, definicao de simbolo, ou o proprio Cargo.toml declarando a dependencia real). Achado maior durante a execucao: meu proprio mapa F1 errou touring-core (confianca MEDIA, palpite por tamanho de diretorio homonimo em touring-generator) — a evidencia real (18 de 19 paths do touring-core-ARCHITECTURE.md batendo exatamente em touring-foundation) reverteu para touring-foundation, e essa correcao foi propagada retroativamente para ARCHITECTURE.md e RFC-004 que ja tinham sido editados com o palpite errado. Tambem corrigido via Cargo.toml direto: 3 features do touring-server (mcts-synthesis/cognitive-nexus/nlp-reranking) apontavam para touring-generator, nao touring-intelligence como o tamanho de diretorio sugeria. 2 ausencias reais documentadas (nao inventadas): touring-hooks/src/touring_ast_integration.rs e docs/schemas/event.schema.json nao existem em lugar nenhum do workspace. gen_fusion_map.py atualizado com a correcao de touring-core.

## 2026-08-26T08:48:32.119983-03:00 — F3 done

scripts/test_docs_no_phantom_crates.py — reprova doc do escopo sistema/repo/infra citando crate inexistente sem nota de provenienciacao. Ground truth via os.listdir('crates') (nunca lista hardcoded, que envelheceria). Tolera notas de provenienciacao (era/fundido/verificado/data) — nao proibe a HISTORIA da fusao, so a mentira presente. 3 rodadas de refinamento por falso positivo: (1) nomes de arquivo colados com extensao (.md/.rs/.sh), (2) path de diretorio (docs/plans/touring-X/), (3) hifen orfao antes de palavra com maiuscula quebrando o boundary do regex. Provado por mutacao (reintroduzir crate fantasma isolado -> exit 1; restaurar -> exit 0). Registrado no CI.

## 2026-08-26T08:57:28.711984-03:00 — F7 done

mkdocs.yml + docs-site/ (symlinks para README/CLAUDE/ARCHITECTURE/docs/) + requirements-docs.txt. mkdocs build --strict exit 0, reproduzido em VENV LIMPO a partir do requirements-docs.txt (nao so no venv onde instalei manualmente). Correcao factual a estrategia de 20/08: mkdocstrings NAO tem handler para Rust (confirmado via Context7 — so C/Crystal/Python/TypeScript/MATLAB/Shell/VBA); o padrao real para Rust e cargo doc, documentado no mkdocs.yml. docs_dir usa symlink de PASTA inteira para docs/ (nao arquivo por arquivo) — achatar quebrava links relativos internos em cascata (cada camada resolvida revelava outra referencia quebrada: 11 -> 8 -> 1 warnings ate a estrutura certa). 1 link genuinamente quebrado achado e corrigido: docs/landing/index.md apontava para docs/touring-license.md que nunca existiu; corrigido para link absoluto ao codigo-fonte real (crates/touring-license/src/lib.rs, que nao tem README). site/ e .venv-docs/ gitignorados; CI roda o build em venv limpo.

## 2026-08-31T10:52:43.255021-03:00 — P3-guard-ci done

scripts/drift_semantic_scan.py agora tem --check mode (CI guard). F2.1 Diamond (1.000). Exit 0 se nenhum stale, exit 1 se drift detectado (561 stale signals baseline). Modo --quiet suprime progresso para CI silencioso. Argparse via stdlib. v0.1 do guard W6/W10.

## 2026-08-31T10:53:40.166355-03:00 — P4-converge done

Convergence gate parcial: 6/8 clauses passam (judge_intact, quality_gold, no_p0_fail, measured_whole_scope, cargo_green, dag_done após mark). 2 unmet: (1) orphans_base 5409 vs 2357 baseline — 5 symbols novos (flaky_test_pattern_detector.rs::{Input,Output}, lib.rs::set_input, manifest.rs::{validate_wasm,wasm_default_fuel}) NÃO introduzidos por este loop (escopo: scripts/drift_semantic_scan.py, docs). Provável baseline stale ou pre-existing orphans em código não tocado. Surface para Gabriel como potencialização P5. (2) cross_audit skipped por falta de audit-plan-completion.sh.
