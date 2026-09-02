---
type: Strategy
title: "Sinais ativos pré-write — SignalContext v2 + wiring dos analisadores existentes"
description: "Estratégia da exploração 2026-09-01: o Touring tem os analisadores; eles disparam no evento errado ou nunca. Fundação S0 (conteúdo proposto no SignalContext) + 10 sinais S1-S10 em 2 tiers."
tags: [signal-layer, pre-write, pre-edit, hooks, seguranca, qualidade, multi-linguagem]
timestamp: 2026-09-01T18:20:00-03:00
plan: /plan.md
---

# Estratégia — Sinais ativos pré-write/pré-edit para o signal layer

## Diagnóstico (evidência dos 2 exploradores read-only, paths verificados)

O Touring **já possui** os analisadores para todos os 7 eixos pedidos (precisão, qualidade,
dimensionamento, segurança, funcionalidade, excelência, impacto). O que falta é **wiring no
momento certo**. Três padrões de falha:

1. **Fome estrutural** — `ctx.source == ""` nos 3 call sites (`pre_write.rs:182`,
   `pre_edit.rs:464`, `pre_read.rs:970`); `SignalContext` (`signal_layer.rs:14-25`) não tem
   campos para a mutação proposta (`new_string`/`old_string`/`tool_name`). Nenhum layer
   consegue analisar o código que SERÁ escrito.
2. **Evento errado** — `AstGrepRiskSignalLayer` só em `pre_read` (risco dispara quando o
   Claude LÊ, nunca quando ESCREVE); `plan_api_cascade` só em `post_edit.rs:327` (avisa
   DEPOIS da quebra); `detect_antipatterns_with_lines` (variante acionável) só pós-evento.
3. **Órfãos (REGRA #0)** — `CweScanLayer` (detector DUPLICADO em `cli/scan.rs:54`),
   `TemporalDriftLayer`, `detect_missing_imports`, `detect_broken_chains`,
   `validate_python_file`, `file_digest_signal`, `scan_path_cached`: zero consumidores.

Assimetrias comprovadas: `analyze_quality` (14 langs) roda em `pre_write` e **não** em
`pre_edit` (criar arquivo é auditado; modificar não); TS/JS têm call graph na lib
(`call_graph.rs:118`) e são **inalcançáveis** pelos hooks (filtro `.rs`/`.py` em
`pre_edit.rs:1416-1418`); 3 enums de linguagem divergentes; quality gates "agnósticos" na
prática só enforçam Rust.

## Lente externa (Context7, marcada no ledger CCE)

- **Semgrep**: severity de segurança = `likelihood × impact × confidence` — metadata
  obrigatória (CWE/OWASP), nunca booleano.
- **ast-grep**: severity `error/warning/info/hint` + **note pedagógica** (a mensagem ensina
  a correção — princípio A5 já constitucional) + composição `all/any/inside/has`.

Tradução: convenção de emissão nos layers novos — score proporcional à severidade
(segredo 1.0 · risco 0.8 · antipattern 0.6 · badge 0.3) para o cutoff SNR ordenar sob
budget; mensagem sempre nomeia a correção; segurança cita CWE quando o detector sabe.

## Proposta — fundação + 2 tiers

### S0 — Fundação (bloqueante de tudo)
`SignalContext` v2: campos `tool_name` + conteúdo proposto (Write: `content`; Edit:
`old_string`/`new_string`), preenchidos nos 3 call sites. Sem S0, qualquer layer novo é
decorativo (o CweScanLayer registrado hoje devolveria 0 achados sempre).

### TIER-1 — wiring quase puro (pure-parse, <10ms, motor existe)
| # | Layer | Motor existente | Eixo |
|---|---|---|---|
| S1 | `AstGrepRiskSourceLayer` (pre_write+pre_edit) | `scan_source_cached` (`ast_grep_signal.rs:163`, já usado pelo CEG) | segurança/excelência, 22 langs |
| S2 | `SecretsEntropyLayer` — P0 BLOCK antes do disco | `F2_4_Secrets` (Shannon; extrair fn por-string da assinatura `measure(&Path)`) | segurança |
| S3 | `MissingImportsLayer` — prevê E0433/NameError | `detect_missing_imports` (`import_resolver.rs:84`, órfão) | precisão |
| S5 | `antipatterns_with_lines` no pré (troca de variante) | mesma varredura SIMD já paga | qualidade acionável |
| S7 | `PySyntaxLayer` — parse de todo Write/Edit `.py` | `validate_python_syntax` (`qa_syntax.rs:30`) | precisão |

### TIER-2 — wiring + refactor pequeno
| # | Layer | Movimento | Eixo |
|---|---|---|---|
| S4 | `ApiCascadePreviewLayer` — "estes N callers quebram" ANTES do edit | `diff_api_surfaces`+`plan_api_cascade` de post→pre (syn duplo; guard >100KB + CILA ≥2) | impacto ★ a jóia |
| S6 | `analyze_quality` no `pre_edit` (simetria com pre_write) | chamada existente | qualidade, 14 langs |
| S8 | `CweScanLayer` wired + dedup do detector duplicado | agora que `ctx.source` vive | segurança |
| S9 | Badge `semantic_complexity`/`public_api_surface` (.rs) | `rust_semantic.rs:164,211` | excelência |
| S10 | Destravar TS/JS no callgraph dos hooks | remover filtro `.rs`/`.py` | dimensionamento |

### TIER-3 — estrutural (criar, não wire; fases posteriores)
Semântica não-Rust (Python/TS) · roteador language-aware nos quality gates · unificação
dos 3 enums de linguagem · refactor do concatenador de 350L do `pre_edit` em layers
reais (destrava `LayerMetrics` por sinal — alimenta F9) · A2/A3 (rota ANN de sítios
análogos/related-symbols — pós-S0, pois também querem ver o conteúdo proposto).

## Reconciliação com a DAG pendente (task_1788294728027014117)
- **B4** (call-graph compacto) → RE-ESCOPADO como S10 (callgraph já roda nos 2 hooks; o gap real é TS/JS + compactação).
- **A2/A3** → TIER-3 (design ANN, pós-fundação).
- **F9** (régua KPI) → mantido e ampliado: `LayerMetrics` já mede por-layer de graça.

## Alternativas ranqueadas (para o HUMAN GATE)
- **(A) recomendada** — Wave 1 = S0 + TIER-1 (6 subtasks, L3); Wave 2 = TIER-2. Confiança 0.85.
- **(B)** — S0 + TIER-1 + TIER-2 numa wave única. Confiança 0.65 (wave longa, mais risco de contexto).
- **(C)** — wiring sem S0 (layers com rota de disco). NÃO recomendada: `ctx.source` morto torna os layers decorativos. Confiança 0.2.

## Emenda v1.1 — retomada 01/09 (noite): sonda F0.3 NEGATIVA muda a fundação

**Fato medido em sessão fresca (5ec1030b)**: o PostToolUse(Bash) vivo do Claude Code **não alimenta o
mirror** (`~/.claude/touring/sdk_signal_mirror.jsonl`): 0 linhas para 47 chamadas Bash; o handler
`post_bash::run_returning` foi atendido pelo daemon em apenas **5/47** (janela 20:24–20:33), nunca fora dela.
Memória canônica com provas, hipóteses falsificadas e scripts: `f0.3:post-bash-entrega-viva:2026-09-01`.

Provado (evidência em disco):
1. O CC emite o evento sempre (`touring-telemetry.ndjson`: 31+ PostToolUse/Bash da sessão, incl. o epoch da sonda E1).
2. O processo `touring-hook post-bash` **é spawnado** (monitor `/proc`: binário da toolchain 30.4.29, ppid=claude,
   stdin=socketpair) e morre em milissegundos **sem stdout** — e o daemon, quando atende, imprime sempre `{}`
   (`HookResponse::Allow.to_json()`), logo os 42 silenciosos **não foram atendidos com payload**.
3. O fallback standalone de `post-bash` na toolchain implantada está **morto** (`unknown subcommand: post-bash`):
   qualquer falha do fast-path é perda total e silenciosa (REGRA #0 + #21).
4. `HookRuntime::read_stdin` (`hook_runtime_ext.rs:437`) degrada em silêncio para `{}` em `EAGAIN`
   (stdin não-bloqueante sem `shutdown` imediato — matriz T5/T6) e no timeout de 2 s (T7). Buraco de robustez
   real; mecanismo candidato para o vivo, **não provado** como gatilho.
5. Falsificado com 34+ sondas manuais (todas escrevem): classificador/schema, `HOME`, circuit breaker (estado limpo,
   sessão real passa), corrida `post-tool-rl`‖`post-bash` (18/18 + replay integral 8/8), stdin atrasado até 1,5 s,
   payload 600 KB, env exato do processo `claude`, ausência de `TOURING_DAEMON_SOCKET`, panic no ator (0), fila
   do ator (p99 0,66 s < 3 s).

**Consequência para a estratégia**: a régua F6/F9 e todo KPI que lê o mirror medem hoje um canal que só
recebe tráfego manual/sandbox — F0 (marcado `done`) reabre como **F0.3**. Entra como pré-fundação:

| # | Item | Movimento | Custo |
|---|---|---|---|
| F0.3a | Instrumento de rota no thin client | `TOURING_HOOK_TRACE_FILE` → 1 linha JSON por invocação `{hook, rota daemon\|standalone, stdin_bytes, stdin_err, elapsed_ms, exit_reason}`; armar via `settings.json env` (humano) e ler após 1 Bash vivo — fecha a causa-raiz em 1 chamada na próxima sessão | S |
| F0.3b | `read_stdin` LOUD e robusto | `fcntl` fd0 → blocking antes de ler; `Err`/timeout viram `warn` + contador `hook_stdin_degraded_count` em gate-metrics (nunca `{}` mudo) | S |
| F0.3c | Standalone `post-bash` restaurado | paridade `ALL_DAEMON_HOOK_NAMES` × `match` standalone garantida por teste (guard D8 cruzado) | S |
| F0.3d | Contador daemon-side por `hook_name` | `hook_dispatch_by_name` — hoje nenhum contador distingue post-bash de post-tool-rl | S |

Alternativas atualizadas: **(A′) recomendada** — Wave 0 = F0.3a–d (TDD, ~2 h, deploy local) → Wave 1 = S0 + TIER-1 →
Wave 2 = TIER-2. Confiança 0.85 (F0.3a exige uma linha no `settings.json`, decisão humana). **(A)** original segue
válida se Gabriel preferir tratar F0.3 em paralelo por outra sessão. (B)/(C) inalteradas.
