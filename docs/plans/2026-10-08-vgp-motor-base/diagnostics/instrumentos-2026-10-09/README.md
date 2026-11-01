# Instrumentos de 09/10/2026 (coordenação analise-89 → transferidos ao touring)

Copiados do scratchpad efêmero da sessão analise-89 para que todo número do
[HANDOFF](../../HANDOFF-touring-2026-10-09.md) tenha o script que o reproduz. Rodam com
`PYTHONPATH=../../scripts` (importam `corpus_v0d`, `shell_inline`, `juiz_motor_vgp`…); os que leem
`~/.claude/projects/**` rodam NATIVOS (`TOURING_CODE_MODE=native`) — o sandbox do `touring run` nega esse caminho e
o glob devolve 0 em silêncio.

| script | mede / prova |
|---|---|
| `mutar_shell_inline.py` | os 7 mutantes da coordenadora contra o selftest do `shell_inline.py` (aceite: vivos 0/7) |
| `mutar_shell_inline_sub.py` | os 36 mutantes do subagente `shell-inline` |
| `medir_programa_inline.py` | a dívida do `programa_inline` antigo × o lexer novo (`--comparar`) |
| `tabela_d.py` · `medir_candidatos.py` | a tabela (d) completa e a taxa de compilação dos candidatos |
| `juiz_lado_a_lado.py` · `diff_vivo_staging.py` | corpus vivo × staging com o MESMO binário congelado |
| `populacao_recall.py` · `evidencia_nos_removidos.py` · `mutar_evidencia.py` | a escolha do candidato pela evidência do traceback (positivos) e seus 9 mutantes |
| `construir_holdout.py` · `descrever_holdout.py` · `montar_holdout_ctl.py` | o held-out TEMPORAL (ts > corte), sua descrição e o corpus híbrido que o juiz aceita |
| `construir_disjunto.py` · `exclusoes_disjunto.py` · `verificar_sobreposicao.py` | o held-out DISJUNTO (semente 20261010, 600+600, teto 3%) e a sobreposição em 4 chaves com controle positivo |
| `montar_ctl_generico.py` · `rodar_juiz_generico.sh` · `rodar_juiz_holdout.sh` · `rodar_juiz_holdout2.sh` | corpus híbrido de qualquer held-out; roda o juiz só com a V3 ausente e registra a coincidência |
| `classificar_holdout.py` · `limites.py` · `consolidar_holdouts.py` | construto de cada falso (via `classificar_falsos_vgp.py`), limite superior 95% Clopper-Pearson (`--selftest`) e a tabela 2×2×2. Leem os JSONs AO LADO do script: copie `../holdout/*.json` para cá ou rode do scratchpad |
| `causa_dos_7.py` · `hipoteses_dos_7.py` · `hipoteses_dos_7b.py` | a causa de cada um dos 7 falsos deny do e935 no disjunto |
| `sonda_atributos.py` · `sonda_numpy_nos_corpora.py` | prova de que o deny de `np.array`/`Image.LANCZOS`/`RUST_AVAILABLE` é geral (3 linhas por programa) e por que o vivo não o via |
| `p95_por_grupo.py` · `p95_touring.py` | a latência por projeto/balde e por arquivo (o p95 do temporal é do `corpus_semeado.py`) |
| `verificacoes_finais.py` · `integridade_final.sh` | vivo intocado (manifesto de 2.278 arquivos), bundle sem toque, limites |
| `licao_holdout.py` · `licao_holdout_disjunto.py` | gravadores das lições no grafo de memória (`PYTHONPATH=~/projects/analise`, `.venv/bin/python3`) |

Relatório completo dos dois held-outs (tabelas, ids dos falsos, construtos, limites): [`../holdout/RELATORIO.md`](../holdout/RELATORIO.md).
| `mutantes_g1_v2.py` · `mutante_d.py` | os mutantes M-a…M-e do G1 sem replay (`cli_suggester.rs` sha c79f12b0) |
| `sonda_decisoes.py` | a sonda de decisão 5/5 da prova do 30.4.95 |

Binários congelados que os juízes usaram: `~/.claude/touring/vgp/binarios/c8b503c4/touring` (pré-wave da Kimi) e
`~/.claude/touring/vgp/binarios/e935aaf1/touring` (entrega final da Kimi, sha256 `e935aaf17d9a09d0…`).
