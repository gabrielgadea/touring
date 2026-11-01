# Artefatos do P0-atributo (09/10/2026)

Relatório: [`../../RELATORIO-P0-atributo.md`](../../RELATORIO-P0-atributo.md). Tudo roda de `~/projects/analise` (venv com numpy 2.4.1 / Pillow 11.3.0).

## scripts/
| script | faz |
|---|---|
| `sonda_p0.py <bin> --json` | as 3 sondas + controles + `os.pathh`/`math.sqr`, em 3 baldes (proj/tmp/inline) |
| `ext_sonda.py <bin…>` · `ext_varre.py <bin>` | `from lxml import etree` & cia · 159 imports de extensão `.so` do venv |
| `varre_runtime.py <bin> <saida>` | todo nome público de `dir(pacote)` (358 pacotes) vira `m.<nome>`; nenhum pode ser negado |
| `censo.py` · `censo2.py` | namespace ESTÁTICO (teste `#[ignore]` `namespace::tests::censo…` imprime `CENSO\t…`) × runtime — pacotes e submódulos |
| `comparar_porid.py` | veredito do motor POR ID, dois binários (verdict + classe/nome/linha/severidade) |
| `juizes.sh` · `juizes_f55.sh` | juiz v2 + V3 com `rodar_juiz_instrumentado.py`, corpus promovido ou `--corpus …bak-2026-10-09` |
| `juiz_tempos.py` · `tempos.sh` | elapsed_ms por id (3 repetições alternadas) — p95 pareado |
| `bench_ab.py` | A/B alternado de `elapsed_ms` em 6 programas |
| `mutantes.py` · `mut_pos_teste.sh` | 23 mutantes (substituição de texto + restauração por `cp`); `mut_pos_teste.sh` aborta se os testes limpos falham |
| `gates1.sh` · `gates_finais.sh` · `gates_server.sh` · `gates_hooks.sh` · `rebuild_verify.sh` | testes+censo+clippy+release · fmt/clippy/suítes · E2E do touring-server com os binários debug por symlink · suítes dos hooks · rebuild e `cmp` do binário congelado |
| `dump.py` | a gramática tree-sitter-python (py-tree-sitter 0.25) das formas usadas |
| `det_varre.py <bin> <corpus> <saida> [K] [--ids …]` | determinismo: cada id roda K vezes no MESMO binário pelo executor do juiz; saída sem `elapsed_ms` byte a byte |
| `mutantes_det.py` · `mut_det.sh` | os 10 mutantes D1–D10 do determinismo (70e0051a) |
| `mutantes_dc.py` · `mut_dc.sh` | 26 mutantes (D1–D7, D10 + N1–N18 da semântica do dataclass), com o marcador `.TROCA-EM-CURSO` durante a janela |
| `dc_red.sh` | RED dos testes `dc_*`: troca `checks.rs`/`modules.rs` pelos do 70e0051a (marcador), roda, restaura e confere por `cmp` |
| `det_a.sh` · `dc_chain.sh` · `dc_final.sh` | suíte `vgp::` limpa + build release + congelamento (`binarios/<sha8>/touring` + `touring.sha`) e o encadeamento |
| `dc_medidas.sh` · `det_juizes.sh` · `juizes.sh` | varredura de determinismo + juiz v2/V3 do binário novo no `corpus_v0d` e no `.bak` |
| `det_gates.sh` | gates finais numa reserva: fmt · clippy · suítes de touring-code/server/hook-handlers/hooks · rebuild e `cmp` do binário |
| `tempos_det.sh` · `agrega_tempos.py` · `juiz_tempos.py` | p95 PAREADO por id (3 repetições alternadas entre binários, mínimo por id), L1 e V3, com a contagem de timeouts da camada 2 |
| `comparar_k.py` · `compara_f55.py` | juiz do binário × `k_*_vivo/ev.json` chave a chave e por id · veredito por id f55ec4b5 × novo, explicando diferenças de V3 por timeout da camada 2 |

## resultados/
`red_e935.json` / `green_f55ec4b5.json` (sondas) · `mutantes_final.json` (23/23) · `varre_base.json` / `varre_f55.json` · `censo_resultado3.json` / `censo2b_resultado.json` · `ext_varre_f55.out`.
`juizes_bak/` e `juizes_promovido/`: base `e935aaf1` e a 1ª versão do novo (`4b7578d2`, superada) nos dois corpora; `juizes_f55_bak/` e `juizes_f55_promovido/`: o binário final `f55ec4b5`
(+ `cmp_v2.json`/`cmp_v3.json` com cada divergência por id). `*.porid.json` = veredito do motor por id.

### Rodadas de 10/10 (determinismo e semântica de dataclass)
`det_red_*` / `det_green_*` (varreduras de determinismo antes/depois) · `determinismo_7ids_antes_depois.json` (os 7 ids da coordenadora) · `mutantes_dc.json` (26 mutantes) ·
`dc_red.log` / `dc_red.rc` (RED dos 6 testes `dc_*`) · `juizes_det_ev/` e `juizes_det_bak/` (binário 70e0051a; `base_*` = e935aaf1 reexecutado por id; `cmp_*` = divergências por id) ·
`juizes_dc_ev/` e `juizes_dc_bak/` (binário final) · `tempos_det/` (p95 pareado, `agregado.json`) · `dg.rc` e `dg_*` (gates do 70e0051a).
