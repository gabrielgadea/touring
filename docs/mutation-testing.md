# Mutation Testing — playbook do `touring mutation-test`

> **Criado**: 2026-08-28 (estreia real do KPI `touring.mutation.kill_rate` — antes
> desta data este arquivo era uma referência fantasma: a tabela TIER 4 do client
> apontava para um playbook que nunca existiu).
> **Executores**: CLI `crates/touring-server/src/cli/mutation_test.rs` → daemon
> handler `cli-mutation-test` (`crates/touring-cli/src/cli/handlers/mutation_test.rs`)
> → core `crates/touring-hooks-core/src/mutation_test.rs` (cargo-mutants).

## O que é

Wrapper estruturado sobre [cargo-mutants](https://mutants.rs): o daemon roda a
corrida, calcula `kill_rate`, aplica o gate de threshold e devolve um envelope
JSON estável que CI e KPI consomem. Pré-requisito: `cargo install cargo-mutants`
— ausente, o daemon devolve envelope `binary_not_found` (degradação estruturada,
nunca crash).

```text
kill_rate = (killed + timeout) / (killed + timeout + survived) × 100
```

Mutantes `unviable` (não compilam) ficam fora do denominador — não medem teste
nenhum.

## Uso canônico

```bash
touring mutation-test --package touring-identity   # 1 crate — a corrida normal
touring mutation-test --cache-only                 # só lê o cache (nunca dispara corrida)
touring mutation-test --force                      # workspace INTEIRO (horas) — force obrigatório
touring mutation-test --threshold 70 --timeout 120 # calibra gate e timeout por mutante
touring mutation-test --jobs 8                     # paralelismo do cargo-mutants
```

## O force-gate de workspace (2026-08-28)

Payload **sem `package` e sem `force`** é recusado ANTES de qualquer execução
com envelope falante:

```json
{ "ok": false, "kind": "workspace_requires_force",
  "hint": "pass --package <crate> for the normal run, or --force to accept the full-workspace cost" }
```

Origem: um teste unitário sem args (`no_args_routes_to_daemon_without_panicking`)
disparava uma corrida de workspace REAL no daemon a cada `cargo test` — jobs=24,
load 26, background tasks mortas no floor de timeout. A recusa é o executor da
regra; o teste hoje usa `--cache-only` (a variante inerte). Memória:
`teste-unitario-dispara-corrida-de-producao`.

## Orçamentos e classificação heavy

| Camada | Mecanismo | Valor |
|---|---|---|
| Server (client) | `raise_timeout_floor(HEAVY_OP_BUDGET_SECS)` no branch não-cache | floor 1800 s (`--timeout` explícito maior vence) |
| Daemon | `is_heavy_hook` inclui `cli-mutation-test` | budget heavy (vs 15 s light) |
| Guard | `mutation_test_is_classified_heavy` (daemon_tests.rs) | lê a janela da fn — declaração ≠ executor reprova |

Sem essas duas pontas a corrida real (~19 min no touring-identity) morria no
budget light de 15 s do daemon **ou** no floor de 120 s do client — o defeito
"client com floor menor que o budget do server" da classe declaração≠executor.

## Probe do binário (CARGO_HOME)

`cargo_mutants_available()` consulta o `PATH` **e** `$CARGO_HOME/bin` (fallback
`~/.cargo/bin`) — a mesma resolução do cargo real. O daemon roda com PATH
estreito (restart via systemd/daemon-ctl não herda o shell), então o probe só
por PATH dava `binary_not_found` com o binário instalado. O restart canônico
pós-deploy carrega o PATH: `PATH="$HOME/.cargo/bin:$PATH" touring daemon-ctl restart`.

## Cache

- Local: `<workspace>/.touring-cache/mutation-test/<package>.json` (workspace →
  `_workspace.json`).
- TTL: 7 dias; `--force` ignora, `--cache-only` nunca dispara corrida.
- O KPI lê pelo cache (ver abaixo) — corrida cara roda quando alguém decide,
  não a cada `touring kpi`.

## KPI — `touring.mutation.kill_rate`

Declarado em `docs/kpi/commitments.yaml`:

```yaml
source: "daemon:cli-mutation-test@touring-identity:/kill_rate"
threshold: 50.0   # gte
```

A gramática `@<scope>` (2026-08-28, `split_handler_scope` em kpi.rs) injeta o
package no payload do handler com `cache_only: true` — o KPI **nunca** dispara
corrida; STUB honesto quando o cache não existe. Estreia medida em 2026-08-28:
**kill_rate 98,15%** no touring-identity (106 killed + 0 timeout + 2 survived;
2 corridas reproduzindo o número). Guia do sistema de KPI: `docs/kpi/README.md`.

## Gotchas conhecidos (todos pagos por execução)

1. **Corrida workspace órfã**: se um processo cargo-mutants ficou órfão, matar
   pelo PID do cargo-mutants (verificado em `/proc/<pid>/cmdline`) — JAMAIS
   `pkill -f touring` (REGRA #19; um dos PIDs vizinhos era o daemon de outra
   sessão CC viva).
2. **ICE do compilador durante baseline**: um internal compiler error (GCC 16 em
   `sqlite3.c` via cc-rs) mata o baseline e o envelope vira `no_viable_mutants`
   — transiente; retry passa. O envelope está correto: baseline quebrado não é
   kill_rate 0.
3. **Rodar sob build pesado**: mutation em paralelo com `cargo build --release`
   do workspace disputa o mesmo target dir lock e satura I/O — sequencie.

## Referências

- Cross-audit da estreia: `docs/audits/cross-audit-2026-08-28-kpi-mutation-lifecycle.md`
- Sistema de commitments: `docs/kpi/README.md`
- cargo-mutants: https://mutants.rs
