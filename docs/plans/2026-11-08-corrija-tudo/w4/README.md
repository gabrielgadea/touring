# W4 — release 30.4.105 e prova no binário instalado (09/11/2026)

`scripts/propagate-release.sh 30.4.105` rc=0 na 3ª tentativa (log:
`docs/plans/2026-11-04-correcoes-aprofundamento/propagate-30.4.105.log`): gates, build, freeze em
`~/.touring/toolchains/30.4.105`, default, propagação (analise e konverter → 30.4.105), prova
comportamental 43/43. A 2ª tentativa reprovou no gate 1/6 (`ceg_e2e::bash_go_forbidden_find_delete_is_denied`
saiu Allow sob carga) e expôs o orçamento de relógio do `command_policy::structural`, removido em `6c3e9197`.

Prova contra `~/.touring/toolchains/30.4.105/bin/` — `provar_w4.py` → `prova.jsonl` (7/7):

| Checagem | Resultado |
|---|---|
| J17 `b81d9fa3c3e5` (falso deny do 30.4.104) | warn |
| `json.loadz` (camada 2, raiz analise) | deny |
| `finalize` de task inexistente | error, archived:false |
| `finalize` com subtask aberta | recusado, blocking=1 |
| `finalize` depois de fechar | archived |
| pre-bash `find . -name '*.go' -delete` | deny |
| pre-bash com 1M args de enchimento | deny |

`teammate-idle-gate` (manual, mesma toolchain): transcript JSONL com `SendMessage` → rc 0 (libera);
transcript sem evidência → rc 2 com a rota (`ToolSearch … TaskList`).
