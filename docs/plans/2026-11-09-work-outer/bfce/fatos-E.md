# E — leitor tipado da resposta do hook: fatos (09/11/2026)

Levantamento só-leitura (subagente sonnet), sem execução. DAG `task_1794217886022321956`.

## Escritor
`HookResponse` (`touring-hooks-core/src/hook_response.rs:102`) serializa por `to_json` à mão (:280):
Allow `{}`; Context/ContextWithUpdatedInput → hso `additionalContext` (+`updatedInput`);
Deny → hso `permissionDecision:"deny"` + `permissionDecisionReason`; Block → topo
`decision:"block"` + `reason`; Halt → `continue:false` + `stopReason`. Sem `ask`/`defer`/allow-rewrite
(escrito à mão em `cli_suggester.rs:3307`, `hook_rewrite.rs:149`). PermissionRequest:
`hso.decision.behavior` (`permission_request.rs:116`).

## Leitores
| leitor | lê | por que JSON |
|---|---|---|
| `touring-hook-runtime/src/hook_trace.rs:129` `classify_answer`/`verdict_of` | permissionDecision (deny/allow/ask/defer), `decision=="block"`, `continue==false`, updatedInput, additionalContext | stdout do daemon (fronteira de processo) |
| `touring-foundation/src/injection_policy.rs:503` `apply_at` | só remove additionalContext/systemMessage; `DECISION_FIELDS` guarda | L0, também hooks Python |
| `touring-hook-runtime/src/verdict.rs:50` `merge_pretool_json_at` | permissionDecision=="deny" + razão | único chamador `cli_suggester.rs:3293`, String de um `Verdict` tipado |
| `touring-hooks-shared/src/tool_trio.rs:508` `deny_gate_of` | substring pré-filtro; deny no hso ou block no topo | L2 não alcança hooks-core |
| `touring-hooks-shared/src/injection_dedup.rs:285` | additionalContext/systemMessage | — (não lê decisão) |
| `hook_response.rs:694` `exit_code_block_reason`; `cli/context_budget.rs:273` | texto | — |

Literais de fio em produção: 24 linhas em 8 arquivos (10 escritas, 14 leituras/guardas).
Camadas: foundation (L0) → hooks-shared (L2) → hooks-core (L3) → runtime/dispatch/cli/handlers.
Parser alcançável por todos: `touring_foundation::hook_wire` (só serde_json).

## Testes
paridade `verdict.rs:182` (+`variantes()` :113, :371); `hook_trace.rs:525,552`;
`tool_trio_tests.rs:407`; `injection_policy.rs:787,1250`; `hooks/main.rs:1185`;
`injection_dedup.rs:592`; `hook_response.rs:1210-1252`; `hook_runtime_tests.rs:1122,1191`.

## Discordâncias
Block/Halt com avisos (tipado → razão; merge JSON → additionalContext/bruto; latente);
allow+updatedInput → "allow" no trace, "rewrite" no tipado; ask/defer só no trace;
PermissionRequest `hso.decision.behavior:"deny"` → "silent"; tag de gate `[G<n>` × qualquer `[tag]`
(intencional); malformado sempre falha aberto (todos); pré-filtro de substring rejeita `"deny"`.

## Preserva comportamento
`parse(&str) -> Option<HookWireView{decision, reason, additional_context, system_message,
has_updated_input}>` para verdict_of, deny_gate_of (com os dois gate_tag), exit_code_block_reason.

## Muda comportamento (decisão explícita)
Block/Halt no merge; block de topo = deny; allow+updatedInput = "rewrite"; PermissionRequest no
trace; `{…` inválido no filter_core; remover pré-filtro de substring.
