---
type: PhaseReport
title: "Ciclo 2 — W0: d4-contrafactual + S-5.2 (identidade de sub-chamadas do orchestrate)"
description: "Sub-chamadas E1 ganham identidade <run_id>:code:<n>; daemon mede o custo contrafactual (bytes que tool calling teria injetado no contexto)"
tags: [code-mode, orchestrate, counterfactual, subcall-identity]
timestamp: 2026-08-24T02:20:00-03:00
plan_id: 2026-08-23-code-mode-best-practices
status: done
---

# C2-W0 — d4 + S-5.2: identidade e contrafactual das sub-chamadas

> Backlog canônico (analise, `task_1787534694944647530::d4`, claimed owner=999a45f8).
> D4: "contrafactual medido — logar sub-chamadas do --orchestrate, reconstruir
> tool-parts que o tool calling teria custado → KPI economia REAL; alimenta
> pillar_induction com evidência". S-5.2 (registro W5): identidade
> `<run_id>:code:<n>` no socket handler.

## Desenho (7 arquivos, 5 crates)

| # | Arquivo | Mudança |
|---|---|---|
| 1 | `touring-ceg/src/gateway/sandbox_executor.rs` | `SandboxConfig.run_id: Option<String>`; child recebe `TOURING_RUN_ID` via `cmd.env` (após `apply_credential_whitelist` — env explícito sobrevive ao `env_clear`) |
| 2 | `touring-server/src/tools/ctx_execute_tools.rs` | `ctx_execute_impl` gera `run-<epoch_ms>-<pid>`, injeta no config, no journal (`run_journal.jsonl` ganha `run_id`) e no output (`CtxExecuteOutput.run_id`) |
| 3 | `touring-server/src/cli/run.rs` (SDK const) | `_TouringClient` lê `TOURING_RUN_ID`, contador `_n` por query; request carrega `"origin": "<run_id>:code:<n>"` |
| 4 | `touring-hooks-core/src/ipc.rs` | `DaemonRequest.origin: Option<String>` `#[serde(default)]` — wire-compatible nas 2 direções (serde ignora campo desconhecido; default preenche ausente) |
| 5 | `touring-dispatch/src/daemon.rs` | no caminho JSON legacy (o único que o SDK fala): origin presente → `record_code_mode_subcall(payload_bytes, output_bytes)` + journal `run_subcalls.jsonl` `{ts, origin, hook, payload_bytes, output_bytes}` fail-open; rkyv path intocado |
| 6 | `touring-foundation/src/gate_metrics.rs` | counters `code_mode_subcalls_count` + `code_mode_subcall_bytes_total` + `record_code_mode_subcall` |
| 7 | `touring-cli/src/cli/handlers/mcp.rs` | export dos 2 counters nos 2 sites de gate-metrics |

Literais `DaemonRequest {` a completar com `origin: None`: daemon.rs:1222, 1470,
1820; touring-hooks/src/main.rs:132, 620.

## O contrafactual (a métrica, não estimativa)

Cada sub-chamada do SDK é exatamente 1 tool call MCP que NÃO aconteceu. O
tool-part que teria entrado no contexto ≈ `len(payload)` (args) +
`len(output)` (resultado). O daemon soma isso em
`code_mode_subcall_bytes_total`; a economia REAL do orchestrate =
subcall_bytes (mortos no sandbox) − o agregado inline que o run devolveu
(já medido por `code_mode_bytes_elided_total`). Journal durável
(`run_subcalls.jsonl`) permite reconstruir a curva por run — evidência para
`pillar_induction`.

Decisão de escopo: logar TAMANHOS + hook, não o conteúdo integral (volume +
risco de segredo no journal; o KPI e a reconstrução de custo precisam dos
bytes, não dos dados).

## Aderência

- **Fail-closed onde importa**: origin ausente = request comum (zero custo no
  caminho quente — `payload.to_string()` só roda quando origin presente).
- **KV-cache**: nada muda no texto do stub (origin é transporte, não API).
- **Correlação de 3 pontas**: `CtxExecuteOutput.run_id` ↔ `run_journal.jsonl`
  ↔ `run_subcalls.jsonl` — o mesmo id atravessa CLI, sandbox e daemon.

## Testes entregues (todos verdes)

1. `run_id_env_reaches_the_child` (ceg) — child vê `TOURING_RUN_ID`; sem
   run_id a var é AUSENTE (nunca string vazia).
2. `ctx_execute_output_carries_run_id` (server) — prefixo `run-`.
3. `sdk_carries_subcall_origin` (run.rs) — SDK lê `TOURING_RUN_ID`, numera
   `:code:<n>`, e só anexa origin quando a identidade existe.
4. `daemon_request_origin_roundtrip_and_backcompat` (hooks-core) — wire
   compatível nas 2 direções.
5. `record_code_mode_subcall_sums_counterfactual_bytes` (foundation) — somas
   exatas.
6. `detect_sandbox_runtimes_as_ctx_execute` (ceg) — fix da sonda, abaixo.

Suítes completas: foundation 483 · hooks-core 485 · dispatch 1321 · ceg 537 —
0 falhas; clippy `-D warnings` 0 nos 7 crates.

## O que a SONDA em produção pegou (que os testes unitários não viam)

A primeira sonda viva (orchestrate real com 2 sub-chamadas) provou o núcleo —
`run_subcalls.jsonl` com origin `run-…:code:1/2` correlacionado ao
`run_journal.jsonl` — e o ciclo sonda→fix→redeploy (×3) expôs 4 lacunas, todas
corrigidas:

1. **`touring gate-metrics -j` nunca carregou NENHUM counter `code_mode_*`**
   (nem os da W4): o comando serializa `GateMetricsSnapshot`, e os exports da
   W4 estavam só nos sites MCP — a doc apontava para um lugar onde os campos
   não existiam. Fix: 4 campos no snapshot (`#[serde(default)]`).
2. **O CEG rejeitava todo `touring run` não-shell desde a W5**: `gate_run`
   nomeia o runtime real (`SandboxPython`…), mas o X0 `ExecSurface::detect`
   só conhecia `Bash`/`ctx_execute`/`inferlet` → NonExec → WARN fail-open a
   cada run. O "ceg gateia o caminho run" da W5 era verdade só para bash.
   Fix: `Sandbox*` → `CtxExecute` + teste.
3. **`run_id` não chegava ao caller CLI**: `emit_output` monta o JSON à mão e
   não incluía o campo — a chave de correlação existia em toda parte menos na
   superfície. Fix: `run_id` no payload do emit.
4. **Fechar a lacuna 2 quebrou o `--orchestrate`**: com `Sandbox*` admitido ao
   gateway, o X6 passou a negar a capability `socket` — do SDK INJETADO, que
   abre o socket do daemon por desenho (a contenção dele é a allowlist
   read-only). Fix: o gate mira o código do USUÁRIO, nunca o SDK
   (`gate_targets_user_code_because_the_sdk_itself_would_deny` trava o
   contrato; código do usuário que abra socket continua negado).

A sonda de aceitação final bateu por 2 caminhos independentes: counters do
daemon (`2 subcalls, 52.510 bytes`) = soma exata do journal; um único
`memory_recall` representou 51 KB de contrafactual que morreu no sandbox
enquanto 1 linha voltou inline.

Lição (repete a de d0/P20): o contrato pode estar 100% testado por unidade e
ainda assim NUNCA ter sido verdade fim-a-fim — a sonda pós-deploy é parte do
definition-of-done, não cortesia.
