# RETOMADA — decisões pendentes de 10/11/2026

Decisões: `DECISOES.md`. Pedido extra do Gabriel: «utilize as sessões kimi e minimax como auxiliares».

| # | Estado | Evidência |
|---|---|---|
| 1 | FEITO, commit `6addf398` | `--check` OK: no drift · lint 0 · test_s49 16/16 · as 17 entradas `pre-read` NÃO eram repetição (um `if` por extensão); L6 nova guarda contra repetição real |
| 2 | FEITO, commit `94d8e309` (o Rust chega no próximo release) | shell: `client/hooks/block_git.sh` (regra do checkout sem `--` + correção do `--detach` casando `--`) · `~/.claude/hooks/test_block_git.py` 61/61 (RED 8 antes) · Rust `git/direct.rs` `bare_checkout_path`/`resolves_as_ref` · `cargo test -p touring-hooks-shared` 561 ok, paridade 326 · mutante morto · clippy/fmt ok · suítes client/hooks + divergência 705 ok · divergência shell×Rust: 0 linhas de checkout; seguem divergências antigas (hooks-bypass, `git restore --staged a && git restore b` liberado pelo Rust) |
| 3 | FEITO (medido) | `item3-corpus-shadow-30.4.108-completo.log`: 1.213/1.214 arquivos, 1.632.524 linhas, 6.319 pares, only_legacy 0, only_reader 0, 0 arquivos com diferença |
| 4a | pré-registro escrito | `../2026-10-08-vgp-motor-base/PREREGISTRO-J18-2026-11-10.md`; execução: `briefs/4a-j18-execucao.md`, para a 1ª irmã livre |
| 4b | com touring-9e (Kimi) | `briefs/4b-kimi-detectores.md`; aguardando RELATORIO-4b.md |
| 5 | com touring-c0 (MiniMax) | `briefs/5-minimax-html.md`; aguardando LISTA-HTML-OSS.md + RELATORIO-5.md |

Próximo: julgar 4b/5 quando chegarem → despachar 4a.
