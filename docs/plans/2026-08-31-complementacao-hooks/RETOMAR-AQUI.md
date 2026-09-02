---
type: Resume
title: "Retomar aqui — sinais dos hooks / signal layer (01/09/2026 ~18:30 BRT)"
description: "Estado completo para retomada: wave tier-ab 3/7, strategy sinais-ativos aguardando HUMAN GATE, sonda F0.3 pendente."
tags: [retomar, signal-layer, hooks, pre-write]
timestamp: 2026-09-01T18:30:00-03:00
plan: /plan.md
---

# Retomar aqui — sinais dos hooks (01/09/2026)

## Comando de reconstituição

```bash
touring memory recall "retomar:sinais-hooks-signal-layer:2026-09-01"
touring decompose ready task_1788294728027014117
```

## Estado

- **Wave signal-layer-tier-ab** (DAG `task_1788294728027014117`): **3/7 done** (F0 mirror
  canônico + KPI honesto · A1 `check_compile` handler · B5 resolvido por verificação — TDG
  já emitido pelo pre_edit W12). **4 pendentes**: A2 · A3 · B4 · F9 (detalhe: memórias
  `pendente:{A2-assists-cross-caller,A3-related-symbols,B4-call-graph-compacto,F9-regua-hooks-complement-kpi}:2026-09-01`).
- **Git**: branch `safety/2026-08-31-audit-closure`, commits `a87e65b 6ee1f6b 0561c18
  b74ac0a f3cd1b3`. Deploy LOCAL de F0/A1 feito (update-touring); toolchain **30.4.29**
  nos 3 projetos ainda SEM F0/A1 — propagação final única pendente ao fim da wave.
- **NOVO (esta sessão)**: exploração completa de sinais ativos pré-write (2 agentes
  read-only + lente Context7) → **strategy aguardando HUMAN GATE**:
  [strategy-2026-09-01-sinais-ativos-pre-write.md](strategy-2026-09-01-sinais-ativos-pre-write.md).
  Rota A recomendada (conf 0.85): **S0 fundação** (SignalContext v2 — `ctx.source` vazio
  nos 3 call sites; adicionar conteúdo proposto + old/new_string + tool_name) → TIER-1
  (S1 ast-grep source · S2 secrets Shannon P0 pré-disco · S3 missing-imports · S5
  antipatterns c/ linha · S7 py-syntax) → TIER-2 (S4 ApiCascadePreview ★ · S6 quality
  simetria · S8 CWE wired · S9 badge semântico · S10 TS/JS callgraph).
  **Gabriel NÃO aprovou ainda** — reapresentar o gate na retomada.
- **Reconciliação da DAG proposta**: B4 → re-escopado como S10 · A2/A3 → TIER-3 (ANN,
  pós-S0) · F9 → mantido/ampliado (LayerMetrics).
- Evidência bruta da exploração: [log.md](log.md) (entrada 2026-09-01T18:15).

## Pendências extra-wave

1. **Sonda F0.3 (sessão FRESCA, grátis)**: 1 comando `touring index find X` → verificar
   linha canônica em `~/.claude/touring/sdk_signal_mirror.jsonl` + trace "post_bash feeder"
   no log do daemon. (O evento PostToolUse(Bash) da sessão viva não alcançava o feeder;
   invocação manual do handler funciona.)
2. PR review → main da branch safety.
3. Emenda v0.2 do Briah da Frente 2 (débito documental do pivot SignalLayer).
4. Promoção F5 Warn→Block após 7d ≥0.80.

## Sequência sugerida na retomada

1. Sonda F0.3 (1 min).
2. Reapresentar o HUMAN GATE da strategy (rota A/B/C).
3. Com aprovação: re-escopar a DAG (B4→S10, A2/A3→fase posterior, +S0+TIER-1) e executar
   sob TDD (tdd-enforcer, INNER 12).

## Atualização 01/09 ~21:35 BRT (sessão 5ec1030b)

- Sonda F0.3 executada: **NEGATIVA** (0/47 vivas no mirror). Ler primeiro
  `touring memory recall "f0.3:post-bash-entrega-viva:2026-09-01"` — evita repetir ~90 min de forense.
- Strategy emendada (v1.1, §Emenda): rota recomendada passou a **A′** (Wave 0 = F0.3a–d antes de S0).
- OUTER desta sessão completo (diagnostic + ledger convergido + strategy). **HUMAN GATE segue aberto** —
  decisões pedidas a Gabriel: (1) rota A′/A/B; (2) autorizar a linha `TOURING_HOOK_TRACE_FILE` no
  `settings.json env` (F0.3a) — é a captura que fecha a causa-raiz em 1 chamada.
- Scripts da forense (reutilizáveis): scratchpad da sessão 5ec1030b — `stdin_matrix.py`, `replay_posttool.sh`,
  `hookmon.sh`/`hookmon2.sh`, `probe_props.py`, `probe_e1.py`.

## Atualização 01/09 ~22:20 BRT — Wave 0 (F0.3a–d) concluída, sem deploy

- Código pronto e verde (clippy + suítes + e2e); **não propagado** (decisão: compilar/testar sem restart).
- Para fechar a causa-raiz do vivo: (1) `scripts/propagate-release.sh <versão>` (ou toolchain install + `touring update`);
  (2) 1 comando Bash qualquer numa sessão CC; (3) `tail ~/.claude/touring/hook_trace.jsonl` → `route`/`stdin_state`/`exit_reason`
  do `post-bash` vivo dizem onde o sinal morre.
- Próxima wave (com ok): S0 SignalContext v2 → TIER-1. `touring decompose ready task_1788294728027014117`.

## Atualização 01/09 ~23:50 BRT — S0 (SignalContext v2) concluído; F9 em curso

- **S0 done** (TDD, clippy limpo): `ProposedChange` + `tool_name`/`proposed` + `analysable_text()` em
  `touring-hooks-shared/src/signal_layer.rs`; call sites via `context_for_{write,edit,read}` em
  `touring-hook-handlers/src/shared/signal_pipeline.rs`. Layers TIER-1 devem ler `ctx.proposed`/`ctx.analysable_text()`.
- **F9 em curso**: KPI `hooks_complement` em `touring kpi -j` (kpi.rs): `hook_dispatch_by_name` (daemon, desde
  `hook_dispatch_since_epoch`) × entregas ao mirror na mesma janela → `post_bash_delivery_ratio`. Testes RED escritos
  (`hooks_complement_from`), implementação pendente se a sessão cair aqui.
- Depois: S1, S2, S3, S5, S7 (TIER-1) — `touring decompose ready task_1788294728027014117`.

## Atualização 02/09 ~01:10 BRT — Wave 1 quase completa

- **Done (TDD, clippy limpo)**: F0.3a–d · S0 · F9 (`hooks_complement` em `touring kpi -j`) · S1 (ast-grep sobre conteúdo
  proposto, pre_write+pre_edit) · S7 (`PySyntaxSignalLayer`, Write .py) · S5 (antipatterns com `L{n}:`).
- **S2 em verificação** (job background): `scan_text`/`SecretScan`/`ALLOW_SECRETS_PRAGMA` em
  `touring-quality/src/verifications/f2_4_secrets.rs` + `SecretsSignalLayer` em
  `touring-hook-handlers/src/shared/secrets_signal.rs` (dep `touring-quality` adicionada; já estava no grafo via ceg),
  registrado em pre_write e pre_edit; testes: 3 quality + 3 layer + 1 handler.
- **Pendente**: S3 (`MissingImportsLayer` — precisa de `known_types` do índice: `ImportResolver::detect_missing_imports(source, known_types)`;
  decidir fonte dos tipos: símbolos do índice do runtime ou tabela curada) e as TIER-2/3 (S4, S6, S8, S9, S10=B4, A2, A3).
- **Gotcha aprendido**: `cargo test -p touring-hook-handlers --lib` SEM `--features pre-hooks,post-hooks` não compila os handlers
  (crate sem default features) — os 106 testes eram só `shared/`. Sempre passar as features (ou testar pela fachada).
- Nada commitado ainda; nenhum deploy/restart (binários vivos = toolchain 30.4.29).

## Atualização 02/09 ~02:10 BRT — Wave 2: S3 · S10(B4) · A2 · A3 (ordem de Gabriel "prossiga")

- **S3 `MissingImportsLayer` done** (TDD completo, revert-proof no teste de integração): imports do CONTEÚDO PROPOSTO
  (`extract_imports_resolved`; `expand_use_arg` achata `use a::{B, c::{D as E}, *}` — antes cada import agrupado lia como
  faltante; alias conta pelo alias; último segmento em vez de `ends_with`); **fonte dos tipos conhecidos =
  `FileKnowledgeDB::find_pub_symbols_by_name`** (IN indexado por nome, same-crate first, mesma predicação de
  `all_pub_symbols`) — `pre_edit.detect_unresolved_types` migrado para a mesma fonte (antes: scan da wiring_map inteira
  por edit); `suggest_imports_for` gera `use` crate-aware (`touring_code::ast::x` / `crate::x`, `lib.rs`/`mod.rs`
  colapsados); `is_builtin_type_name` é a lista única dos 3 detectores. Módulo: `touring-hook-handlers/src/shared/missing_imports.rs`.
- **S10 (ex-B4) done**: hooks gateiam o callgraph por `touring_code::ast::call_graph::supports_call_graph` (TS/JS
  destravados; `debug_assert` de concordância com o dispatch); `enrich_with_callgraph` devolve callers/callees DISTINTOS
  (6 chamadas de `main` liam como HOTSPOT de 6 callers).
- **A3 `RelatedSymbolsLayer` done** (pre_write): nomes que o arquivo PROPOSTO declara e o `SymbolStore` já define em
  outro arquivo → `[related] \`X\` already defined in f:l — homonym (VP-Scout chain 4)`; polyglot (rs/py/ts/js);
  nomes genéricos/curtos nunca chegam ao índice. Motor determinístico (`find_symbol`); rota ANN fica como v2.
- **A2 `CrossCallerLayer` done** (pre_edit): chamada que o Edit MUDA (`total(a,b)`→`total(a,b,c)`, whitespace
  ignorado; definições e macros não são chamadas) × `find_references` → `[C08] \`total\` changes here and has N other
  call sites in M files: f:l …`. O pipeline do pre_edit agora roda também quando só este sinal existe.
- Acesso ao índice nos hooks: `runtime.symbol_store()` (método, `Option<&SymbolStore>`), não campo.
- DAG `task_1788294728027014117`: S3 e B4 fechados via `loop_phase_close.py`; A2/A3 fechados na sequência (ver log.md).
- **Ainda pendente**: TIER-2 S4 (ApiCascadePreview ★) · S6 · S8 · S9; propagação da toolchain + leitura de
  `~/.claude/touring/hook_trace.jsonl` (causa-raiz F0.3); commit da branch safety.
