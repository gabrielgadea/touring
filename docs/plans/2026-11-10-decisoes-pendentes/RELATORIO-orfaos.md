# Relatório — 13 órfãos do loop de convergência

Data: 10–11/11/2026. Tarefa: `task_1794353972767264831::S3`. Escopo do juiz preservado: `/home/gabrielgadea/projects/touring`.

## Método

1. `touring index ingest` dos arquivos produtores e consumidores para eliminar índice velho.
2. Confirmação tipada por dono (`impl`), chamadas associadas, chamadas de método, imports e consumidores de produção/teste em todos os crates.
3. Decisão por consumidor previsto, visibilidade mínima ou remoção, sem `allow(dead_code)` e sem rebaixar `--scope`.

## Tabela símbolo → evidência → decisão

| Símbolo | Evidência | Decisão |
|---|---|---|
| `cdn.rs::urls_do_conteudo` | `index find` com zero referências; `preencher` só era chamado em testes e nenhum comando consumia a função | removida |
| `cdn/cache.rs::{len,is_empty}` | usados apenas em assertions dentro de `cdn/cache.rs` | `#[cfg(test)] pub(crate)`; sem dead code no build normal |
| `typescript/globais.rs::Mundo::novo` | consumidores de produção em `analise.rs` e testes do próprio crate | `pub(crate)` |
| `typescript/modulos.rs::Carregador::novo` | consumidores em `analise.rs`, `tipos.rs`, `inferencia.rs`, todos no mesmo crate | `pub(crate)` |
| `typescript/tipos.rs::Resolvedor::novo` | mesmos consumidores intra-crate | `pub(crate)` |
| `hook_wire.rs::SUPPRESS_OUTPUT` | consumido por `injection_policy::DECISION_FIELDS` | aresta confirmada pelo ingest; nenhuma edição |
| `hook_wire.rs::SYSTEM_MESSAGE` | plano E previa listas de campos com constantes; literal duplicado em `CONTEXT_FIELDS` | ligado em `injection_policy::CONTEXT_FIELDS` |
| `hook_wire.rs::Decision::said` | `hook_trace.rs:133` usa `wire.said()` em produção | aresta confirmada pelo ingest; nenhuma edição |
| `hook_wire.rs::stop_reason` | `injection_policy::DECISION_FIELDS` consome `crate::hook_wire::STOP_REASON` | aresta confirmada pelo ingest; nenhuma edição |
| `run_outcome::FailureKind::as_str` | `journal.rs` e `cli/kpi/code_mode.rs` usam o rótulo em produção | `Display` com os mesmos valores; consumidor migrado a `kind.to_string()`; `as_str` ficou `pub(crate)` |
| `run_outcome::RunOutcome::new` | `reader.rs` e `touring-server` construíam o tipo | consumidor externo migrado para os campos públicos `exit`/`failure`; construtor ficou `pub(crate)` |
| `command_policy::SHELL_GUARD_TOKENS` | testes usam a tabela e `git/mod.rs` duplicava `TOKEN` | `TOKEN` agora consome `SHELL_GUARD_TOKENS[0].assignment`; API pública preservada |

## Prova do `Display`

Antes da implementação, `label_table_round_trips_in_both_directions` falhou com `E0599: FailureKind doesn't implement std::fmt::Display` (rc 101). Após `impl fmt::Display`, o mesmo teste passou. Os 7 rótulos continuam idênticos ao journal: `exception`, `timeout`, `abort`, `proc-exit`, `invalid-output`, `output-limit`, `other`.

## Verificação

- `cargo check -p touring-code -p touring-foundation -p touring-hooks-shared -p touring-server -p touring-cli`: rc 0.
- `cargo test -p touring-code -p touring-foundation -p touring-hooks-shared -p touring-server -p touring-cli --quiet`: **5.613 passed, 0 failed, 85 ignored em 75 suites** (incluindo doctests).
- `cargo clippy -p touring-code -p touring-foundation -p touring-hooks-shared -p touring-cli --all-targets --all-features -- -D warnings`: rc 0.
- `cargo clippy -p touring-server --all-targets -- -D warnings`: rc 0. A forma `--all-features` do servidor é estruturalmente inválida por `prod-allocator` competir com `jemalloc` e `dhat-heap` (`src/main.rs:37-42`), incompatibilidade independente deste diff; as features padrão foram preservadas.
- `cargo fmt --all -- --check`: rc 0.
- `loop_converged.py --task task_1794353972767264831 --scope /home/gabrielgadea/projects/touring`: **CONVERGED, rc 0**, `orphans_base scoped=709 baseline=2481`.
- Commit: pendente.
