# RETOMADA — decisões pendentes de 10/11/2026

Decisões: `DECISOES.md`. Pedido extra do Gabriel: «utilize as sessões kimi e minimax como auxiliares».

| # | Estado | Evidência |
|---|---|---|
| 1 | FEITO, commit `6addf398` | `--check` OK: no drift · lint 0 · test_s49 16/16 · as 17 entradas `pre-read` NÃO eram repetição (um `if` por extensão); L6 nova guarda contra repetição real |
| 2 | FEITO, commit `94d8e309` (o Rust chega no próximo release) | shell: `client/hooks/block_git.sh` (regra do checkout sem `--` + correção do `--detach` casando `--`) · `~/.claude/hooks/test_block_git.py` 61/61 (RED 8 antes) · Rust `git/direct.rs` `bare_checkout_path`/`resolves_as_ref` · `cargo test -p touring-hooks-shared` 561 ok, paridade 326 · mutante morto · clippy/fmt ok · suítes client/hooks + divergência 705 ok · divergência shell×Rust: 0 linhas de checkout; seguem divergências antigas (hooks-bypass, `git restore --staged a && git restore b` liberado pelo Rust) |
| 3 | FEITO (medido) | `item3-corpus-shadow-30.4.108-completo.log`: 1.213/1.214 arquivos, 1.632.524 linhas, 6.319 pares, only_legacy 0, only_reader 0, 0 arquivos com diferença |
| 4a | J18 INCONCLUSIVO POR CORPUS (`a28615da`), aceito | Adendo 1 funcionou (construção rc 0, só as 3 linhas puladas), mas `validacao_pre_selo.json` = 503/600 arquivos + 229 inline → §3 proíbe julgar corpus menor; nenhum binário medido. Sinal a investigar: `ts_ausente_no_universo_arquivo` = 2.940 candidatos sem timestamp. Próximo held-out exige decisão do Gabriel |
| 4b | FEITO: revisão `f558fb57` + correção `c9729981`, ambas aceitas | 6 furos fechados; remedido aqui (`4b-correcoes-suites.log`): touring-analysis 1.197 passed/0 failed, touring-quality 551/0, controles positivos 8/8 e 6/6 sem ignored. Resíduo declarado: senha só alfanumérica dentro de tag Go (`json:"Tr0ub4dor3xK9"`) segue lida como nome de chave — pelo charset não se separa chave de senha |
| 5 | FEITO: lista `1fe1e0fb`, montador `f07a5d3a`/`fcf4db48`, camada selada (RELATORIO-5b) | `camada_html_oss_20261110`: 6.381 entradas = 5.944 negativos + 437 positivos; teto por chave 191, máx. 91 (arquivo) e 122 (commit), 0 podadas; 828 descartadas por id contra 40 corpora; 3 não-UTF-8; conferido aqui: `confere_selo.py` → selo INTACTO a665624d48b91494 (rc 0), 0 snaps ausentes. «provada» = a camada cumpre o critério de população, NÃO o julgamento do adaptador html (próximo passo). Pendente do Gabriel: `! rm -r ~/.claude/touring/vgp/oss-html/_stage` (20 GB) e `~/.claude/touring/vgp/camada_html_oss_20261110.parcial-exclusao` (8,7 MB) |

Extras (achados da touring-c0, corrigidos aqui): oráculo não-UTF-8 `fcf4db48` (descarta e conta; teste RED→GREEN); tdd-enforcer `-qq` sem linha de resumo → o rc do pytest decide (`~/.claude/skills/tdd-enforcer/scripts/enforce.py` + `test_enforce.py` 9/9; a skill não é versionada).

Ferramentas da touring-c0 (fora do repo, em `~/.claude/touring/vgp/oss-html/_tools/`): `preflight_fast.py` (lê os blobs como bytes — reusar no pré-registro do julgamento html), `licenca.py`, `conta_html_efetivo.py`, `test_oss_html_tools.py` (41 testes). Versionar: decisão pendente.

Próximo: julgar a correção dos furos (Kimi) e o item 5 (MiniMax); J18 na fila da Kimi. Decisão aberta: canvas «Rust deve negar contornos de hook?» (recomendação A, aguardando o Gabriel).
