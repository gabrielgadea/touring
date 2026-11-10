# RETOMADA — decisões pendentes de 10/11/2026

Decisões: `DECISOES.md`. Pedido extra do Gabriel: «utilize as sessões kimi e minimax como auxiliares».

| # | Estado | Evidência |
|---|---|---|
| 1 | FEITO, commit `6addf398` | `--check` OK: no drift · lint 0 · test_s49 16/16 · as 17 entradas `pre-read` NÃO eram repetição (um `if` por extensão); L6 nova guarda contra repetição real |
| 2 | FEITO, commit `94d8e309` (o Rust chega no próximo release) | shell: `client/hooks/block_git.sh` (regra do checkout sem `--` + correção do `--detach` casando `--`) · `~/.claude/hooks/test_block_git.py` 61/61 (RED 8 antes) · Rust `git/direct.rs` `bare_checkout_path`/`resolves_as_ref` · `cargo test -p touring-hooks-shared` 561 ok, paridade 326 · mutante morto · clippy/fmt ok · suítes client/hooks + divergência 705 ok · divergência shell×Rust: 0 linhas de checkout; seguem divergências antigas (hooks-bypass, `git restore --staged a && git restore b` liberado pelo Rust) |
| 3 | FEITO (medido) | `item3-corpus-shadow-30.4.108-completo.log`: 1.213/1.214 arquivos, 1.632.524 linhas, 6.319 pares, only_legacy 0, only_reader 0, 0 arquivos com diferença |
| 4a | pré-registro escrito | `../2026-10-08-vgp-motor-base/PREREGISTRO-J18-2026-11-10.md`; execução: `briefs/4a-j18-execucao.md`, para a 1ª irmã livre |
| 4b | ACEITO (`f558fb57`); correção dos furos com touring-9e | 51 vizinhos, 45 detectados, 6 furos (0abafc64 ×2, 059b2026 ×3, 44869807 ×1); remedido aqui: 4+4 passed, furos F2.4 reprovam com `--ignored`; Kimi corrige os 6 (REGRA #21) e depois executa o J18 |
| 5 | ACEITO (`1fe1e0fb`); montador `f07a5d3a`; camada em montagem com touring-c0 | 37 repos, sha 37/37, licenças permissivas, rendimento 5.614 remedido idêntico → n = 7.318 ≥ 3.700; 4,0 GiB; `RELATORIO-5b.md` (camada selada) aguardado. Pendente do Gabriel: `! rm -r ~/.claude/touring/vgp/oss-html/_stage` (20 GB de candidatos descartados) |

Próximo: julgar a correção dos furos (Kimi) e o item 5 (MiniMax); J18 na fila da Kimi. Decisão aberta: canvas «Rust deve negar contornos de hook?» (recomendação A, aguardando o Gabriel).
