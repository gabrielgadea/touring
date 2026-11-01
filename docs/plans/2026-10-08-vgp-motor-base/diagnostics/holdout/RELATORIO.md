# Relatório dos dois held-outs do motor VGP (temporal e disjunto) — 09/10/2026

> Autor: teammate `shell-inline` (sessão analise-89, coordenadora `team-lead`; destino passado à sessão touring-09).
> Binário medido: `touring_kimi_e935` (sha256 `e935aaf17d9a09d0…`, entrega final da Kimi). Controle: `c8b503c4` (pré-wave).
> Juiz: `scripts/juiz_motor_vgp.py` stock, `--v5-estrito`, sem edição. Nenhum arquivo do bundle foi alterado.
> Dados brutos deste relatório: esta pasta. Scripts que os produziram: `../instrumentos-2026-10-09/`.

## 0. Veredito em cinco linhas

1. O held-out **disjunto** (semente 20261010) tem **600 negativos de arquivo + 600 inline**, com **sobreposição 0** nas quatro chaves contra o vivo e contra o temporal.
2. O e935 deu **7 falsos deny em 1.200**: arquivo **5/600** (limite superior 95% **1,74%**), inline **2/600** (**1,05%**), total **7/1200** (**1,09%**). **"Falso deny ≤ 1%" NÃO está provado.** O juiz diz PRONTO porque compara a taxa pontual (0,83% e 0,33%) com 1%, não o limite superior.
3. **3 dos 7 são um defeito real e geral do coletor de atributos de módulo** (`np.array`/`np.where`, `Image.LANCZOS`, `rust_bridge.RUST_AVAILABLE`): uma sonda de 3 linhas por programa reproduz o deny. Em arquivo o numpy sai só como warn; em inline sai como deny. O vivo (0/300 inline) não tinha nenhum inline com `np.array`.
4. Os outros **4** (todos de arquivo) são **ruído de rótulo** em grau variável (um deles, `WS`, não confirmado).
5. O **temporal** (fora do tempo) deu 0/77 e 0/189, mas o n é pequeno: limites **3,82%** e **1,57%**.

## 1. Como foram construídos

Corte comum aos dois: `2026-10-09T18:14:31.894Z` (maior ts do corpus vivo + 1 min, lido do arquivo; o vivo termina em `…18:13:31.894Z`).

| | TEMPORAL (`corpus_holdout`) | DISJUNTO (`corpus_holdout_disjunto`) |
|---|---|---|
| Universo | edições/execuções com ts > corte e ≤ `2026-10-09T21:12:22.428Z` (congelado) | o MESMO universo do vivo, ts ≤ corte |
| Semente | 20261008 (arquivo), 20261009 (inline) | **20261010** |
| Seleção | mesmas funções do vivo (`varrer_transcripts`, `universo_negativos`, v5 `universo/monta/_sorteia/_grava`) | idem, com exclusão por id, tool_use_id, sha do conteúdo (snap) e comando |
| Exclui | — | tudo do vivo **e** do temporal |
| Estratificação | n/a (tudo o que houve na janela) | projeto (proporcional à mistura do vivo) × estrato inline |
| Teto de concentração | nenhum (por isso `corpus_semeado.py` = 24 de 77) | cada arquivo e cada sessão ≤ 3% do balde (18 de 600) |
| Positivos | 0 | 0 |
| Saída | `~/.claude/touring/vgp/corpus_holdout/` | `~/.claude/touring/vgp/corpus_holdout_disjunto/` |

Parâmetros do disjunto (`holdout_meta_disjunto.json`): `n_arquivo 600`, `n_inline_por_estrato 200`, `n_tmp 100`, `teto 0.03` (= 18 por arquivo e por sessão). Cotas de projeto proporcionais à mistura do vivo (touring 36 / analise 259 / simulador 2 / .claude 3): **touring 72, analise 518, .claude 6, simulador-urna-rs-2026 4**.

Universo do disjunto: arquivo — 13.391 candidatos limpos na história, **13.266 até o corte**, 3.427 podados por não reconstruírem, 600 sorteados. Inline — 11.986 execuções, pool após filtros de id 11.249 (c 2.787, heredoc 3.032, touring_run 1.196, tmp 1).

### 1.1 Composição final

| balde | n | extrator/estrato | concentração |
|---|---|---|---|
| disjunto · arquivo | 600 | atribuído 600 · **tmp 0** | 516 arquivos distintos, maior arquivo 5 (0,83%) |
| disjunto · inline | 600 | c 200 · heredoc 200 · touring_run 200 · **tmp_escrito_e_executado 0** | 141 sessões, maior sessão 18 (3,00%, no teto) |
| temporal · arquivo | 77 | atribuído 76 · tmp 1 | 33 arquivos distintos, maior 24 (`corpus_semeado.py`, 31,17%) |
| temporal · inline | 189 | c 21 · heredoc 155 · touring_run 13 | 6 sessões, maior 82 (43,39%) |

O estrato **tmp esgotou**: o pool anterior ao corte tem 1 candidato e o vivo já o usou. No juiz, o balde "arquivo" é atribuído + tmp; no disjunto é só atribuído.

## 2. Sobreposição (`sobreposicao_disjunto.json`, `verificar_sobreposicao.py`)

| par | ids | tool_use_ids | shas (snap) | comandos |
|---|---|---|---|---|
| disjunto × vivo | **0** | **0** | **0** | **0** |
| disjunto × temporal | **0** | **0** | **0** | **0** |
| temporal × vivo (controle da régua) | 0 | 0 | 0 | 0 |
| **controle positivo** vivo × vivo | 791 | 491 | 809 | 522 |

Tamanhos: disjunto 1.200 ids / 600 tool_use_ids / 1.200 shas / 600 comandos (arquivos não têm comando: 600 vazios); vivo 791/491/809/522; temporal 266/190/261/190. Duplicatas internas de conteúdo no disjunto: 0. O controle positivo prova que a régua enxerga sobreposição quando ela existe.

Descritivo (não é critério de exclusão): dos 516 arquivos distintos do disjunto, **52** também foram editados no vivo em OUTROS instantes (78 das 600 entradas). A disjunção é por id, conteúdo e comando, não por arquivo.

## 3. Resultado por balde e binário

Limite superior = **Clopper-Pearson exato, unilateral, 95%** (`limites.py`, com `--selftest`; bisseção sobre a binomial, sem scipy). A regra de três (3/n) só aproxima o caso k = 0 e **não** se aplica com k > 0. Independência assumida (ver §9).

| held-out | binário | balde | n | falsos deny | taxa | limite superior 95% |
|---|---|---|---|---|---|---|
| DISJUNTO | **e935** | arquivo | 600 | **5** | 0,83% | **1,74%** |
| DISJUNTO | **e935** | inline | 600 | **2** | 0,33% | **1,05%** |
| DISJUNTO | **e935** | total | 1.200 | **7** | 0,58% | **1,09%** |
| TEMPORAL | e935 | arquivo | 77 | 0 | 0,00% | 3,82% |
| TEMPORAL | e935 | inline | 189 | 0 | 0,00% | 1,57% |
| DISJUNTO | c8b503c4 (controle) | arquivo | 600 | 70 | 11,67% | 14,05% |
| DISJUNTO | c8b503c4 (controle) | inline | 600 | 86 | 14,33% | 16,90% |
| TEMPORAL | c8b503c4 (controle) | arquivo | 77 | 11 | 14,29% | 22,54% |
| TEMPORAL | c8b503c4 (controle) | inline | 189 | 4 | 2,12% | 4,78% |

Referências do mesmo método (`limites.py`): 0/600 → 0,50% · 1/600 → 0,79% · 3/1200 → 0,64%.

O binário velho sobre o disjunto dá 11,67% e 14,33%, contra 10,75% e 14,0% no vivo: **mesma distribuição, instrumento com sensibilidade**. O zero do e935 no temporal vale porque o velho, no mesmo corpus, mede > 0.

### 3.1 Veredito do juiz (rc e pendências)

| execução | rc | pronto | pendências |
|---|---|---|---|
| e935 · disjunto (híbrido) | 0 | **sim** | nenhuma |
| e935 · temporal (híbrido) | 1 | não | `p95 678,2 ms > 150,0 ms` (único; 0 falsos) |
| c8b503c4 · disjunto | 1 | não | recall de símbolo 0,615 (8/13) e 0,444 (12/27) < 0,7; falso deny 11,667% e 14,333% > 1% |
| c8b503c4 · temporal | 1 | não | (ver `c8b5_v2_holdout.out`) |

O corpus puro é recusado pelo juiz stock ("corpus degenerado: 0 positivos" e "sem positivo de SÍMBOLO de arquivo", 0 chamadas). Por isso medi com **corpora híbridos** no scratchpad (`montar_holdout_ctl.py` / `montar_ctl_generico.py`): negativos do held-out + os 65 positivos do vivo **só como controle de liveness**. Os held-outs permanecem puros.

### 3.2 Controles e custo (híbrido)

| held-out | binário | recall controle arquivo | recall controle inline | avisos em negativos | p95 | chamadas | erros | defeito de corpus |
|---|---|---|---|---|---|---|---|---|
| disjunto | e935 | 11/13 | 20/27 | 86 | 138,9 ms | 1.262 | 0 | 3 ids |
| disjunto | c8b503c4 | 8/13 | 12/27 | 383 | 134,5 ms | 1.262 | 0 | 3 ids |
| temporal | e935 | 11/13 | 20/27 | 1 | 678,2 ms | 328 | 0 | 3 ids |
| temporal | c8b503c4 | 8/13 | 12/27 | 42 | 677,5 ms | 328 | 0 | 3 ids |

O recall dos controles é de **conjunto de ajuste** (positivos do vivo), não é generalização. Os 3 ids de defeito de corpus (`6dce9f68d2da`, `c72427be59bd`, `9c269d2c1bf5`) são do lado dos positivos do vivo (snap não compila), idênticos nos quatro juízos.

## 4. Os 7 falsos deny do e935 no disjunto

Construto via `classificar_falsos_vgp.py` (`classificar_disjunto_e935.json`). A coluna "causa" é a minha leitura do código-fonte e da história do git (`causa_dos_7.py`, `hipoteses_dos_7.py`, `hipoteses_dos_7b.py`).

| id | balde | origem | arquivo ou cwd | classe · nome · linha | sev. | construto | causa |
|---|---|---|---|---|---|---|---|
| `bd4cfc7dd1e1` | arquivo | atribuído | `…83f700cf…/scratchpad/clk_prova_caminho.py` | atributo · `RUST_AVAILABLE` · L42 | deny | `-` | **DEFEITO DO MOTOR.** `scripts.aco.rust_bridge.RUST_AVAILABLE` é atribuído dentro de try/except; o coletor ignora ligação em bloco composto |
| `922195736c58` | inline (extrator `c`) | inline | cwd `/tmp/juri3_proposito` | atributo · `array` · L7 e `where` · L11 | deny | `-` | **DEFEITO DO MOTOR.** `numpy/__init__.py` liga `array`/`where` dentro de bloco composto (L123) |
| `e33a1e2e4030` | inline (extrator `c`) | inline | cwd `/tmp/miolo` | atributo · `LANCZOS` · L8 | deny | `-` | **DEFEITO DO MOTOR.** Pillow 11.3.0 liga por `setattr(module, item.name, item.value)` dinâmico; o `LANCZOS = 1` visível é o corpo de `class Resampling` |
| `7a4ad8f765aa` | arquivo | atribuído | `…9105f79c…/scratchpad/tendencia/baixar_2010_2012.py` | import · `TSEClient` · L30 | deny | `-` | **Ruído de rótulo, deny correto.** `git log -S TSEClient -- scripts` não tem nenhum commit; `scripts/external_apis/tse/` é pacote sem a classe |
| `ddbb2cfb4afc` | arquivo | atribuído | `…83f700cf…/scratchpad/exec/trecho_pctx.py` | nome · `Path` L4 e L12, `ComputeProcessContext` L7 | deny | `nao-ligado-no-arquivo` | **Ruído de rótulo.** Fragmento de teste sem os imports; é a última edição do arquivo (0 edições depois) |
| `c35d78e3eb25` | arquivo | atribuído | `…fde12f4c…/scratchpad/s24_route_patch.py` | nome · `WIRING` · L244 | deny | `nao-ligado-no-arquivo` | **Estado intermediário.** A edição seguinte define `WIRING` 7 s depois, sem execução entre as duas (mesma regra do `python_pre_gate` F821) |
| `74f04a24de12` | arquivo | atribuído | `…touring/3db9ea13…/scratchpad/retrieval_bench.py` | nome · `WS` · L131 | deny | `nao-ligado-no-arquivo` | **Não confirmado.** `WS` indefinido no snap e, pela minha regex, também no estado seguinte; candidato a ruído |

Por construto (o juiz): arquivo — `import` 1, `nome/nao-ligado-no-arquivo` 5 achados em 3 entradas, `atributo` 1; inline — `atributo` 3 achados em 2 entradas.

### 4.1 Prova de generalidade do defeito de atributos (`sonda_atributos.py`)

Binário e935 congelado, `vgp check` em programas de 3 linhas, `--project-root ~/projects/analise`:

| programa | veredito | achados |
|---|---|---|
| `import numpy as np; np.array([1,0,2]); np.where(x)` | **deny** | `array`, `where` |
| `from PIL import Image; im.thumbnail((2,2), Image.LANCZOS)` | **deny** | `LANCZOS` |
| `import scripts.aco.rust_bridge as rb; rb.RUST_AVAILABLE` | **deny** | `RUST_AVAILABLE` |
| controle negativo: `os.path.join` | allow | nenhum |
| controle positivo: `np.arrai` (typo) | deny | `arrai` |
| observação: `os.pathh` (typo em stdlib) | allow | nenhum (não investigado; pode ser desenho) |

### 4.2 Por que o vivo não viu (`sonda_numpy_nos_corpora.py`, mesma `rodar_motor` do juiz)

Todo snap com `np.array`/`np.where`, e935: **vivo 9 snaps, todos de arquivo, todos `warn`** (0 inline); **disjunto 4 de arquivo `warn` + 1 inline `deny`** (`922195736c58`). O mesmo achado falso sai como warn em arquivo e como deny em inline. O vivo tinha 300 inline e nenhum usava `np.array`: 0/300 foi falta de amostra, não ausência de defeito.

### 4.3 Reclassificação pós-hoc (exploratória, **não vale como prova**)

Feita depois de ver as falhas. Só os 3 defeitos reais: **3/1200 = 0,25%, limite 0,64%**; arquivo 1/600 (0,79%); **inline continua 2/600 (1,05%)**, porque as duas falhas inline são reais. Mesmo descontando o ruído de rótulo, o inline não fecha ≤ 1%.

## 5. Falsos deny do binário de controle (c8b503c4)

**Temporal** — arquivo 11: `188acd723905 6e59fc8b807e 49aa1113dfdc 38c95e43c4cf 32b440d25f64 7518c2a2d672 a9d7cc295edd c790fcd133ec 8ca83fef80e7 e555722dcb68 40a40dfc08e7` (construtos: import 8, with-as 5, except-as 5, atribuição+aug 3, atribuição+walrus 2, atribuição+with-as 1, except-as+for-alvo 1). Inline 4: `e0ed7935c1f7 d306402602ef e2ede553b663 ddd010c3b179` (except-as 3, with-as 6).

**Disjunto** — arquivo 70, inline 86; ids completos em `classificar_disjunto_c8b5.json` e `c8b5_v2_disjunto.json`. Construtos (achados): arquivo — `-` 48, import 31, with-as 23, except-as 9, parâmetro 8, atribuição+aug 5, `nao-ligado-no-arquivo` 5, walrus 2, for-alvo+walrus 2, compreensão-alvo+except-as 1; inline — `-` 68, except-as 54, with-as 26, compreensão-alvo+with-as 2. É a família de construtos que o e935 fechou (ligações por `with`/`except`/import/walrus); o que sobrou no e935 é a classe `atributo`.

## 6. Validade da medição

- **V3 ausente em todas as medições.** Cada execução esperou 2 checagens vazias de `pgrep "[j]uiz_motor_vgp.py.*camadas"` e amostrou durante a execução (`rodar_juiz_generico.sh`): **amostras de V3 durante a medição: 0 de 1 / 0 de 11 / 0 de 11 / 0 de 23 / 0 de 23** (logs `rodar_juiz_*.log` nesta pasta). As medições foram de 18:25 a 18:45 BRT de 09/10/2026; a V3 da coordenadora, que depois terminou, não coincidiu com nenhuma.
- **Sem repetição.** Depois do aviso de que a V3 terminou NÃO repeti o juiz: o resultado é o mesmo determinístico (veredito por id, não por tempo) e o load da máquina estava em 7,9 (outras sessões), o que só distorceria o p95.
- Só flags existentes: `--touring`, `--v5-estrito`, `--corpus`, `--json`. Nenhuma edição de `juiz_*.py`, `replay.py`, `crates/**`, `shell_inline.py` ou do vivo.

## 7. O p95 do temporal é do conteúdo, não do binário

p95 de 678,2 ms no temporal contra 138,9 ms no disjunto, com o mesmo binário. `p95_touring.out`: os 24 negativos de `corpus_semeado.py` (snap ~33,7 kB) levam em média **691 ms** (máx. 837), os 7 de `shell_inline.py` 291 ms; os demais, 9 a 24 ms. `p95_por_grupo.out`: touring · negativo · arquivo no temporal mediana 672 ms e p95 814 ms; analise · negativo · arquivo mediana 13 ms e p95 36 ms. No vivo, p95 113 ms. A latência do motor cresce com o conteúdo de um arquivo grande, e o temporal tinha o `corpus_semeado.py` editado 24 vezes na janela. O teto de 3% do disjunto evita isso por construção.

## 8. Integridade

- Vivo `~/.claude/touring/vgp/corpus_v0d`: **2.278 arquivos, manifesto idêntico** ao de antes (`cmp` byte a byte); `sha_tree b06b89113c869097` antes e depois (o próprio builder verifica nas duas pontas).
- Temporal: `sha_tree 6f5fc46410884fd2` antes e depois da construção do disjunto.
- Bundle (shas e mtimes inalterados): `corpus_v0d.py 98fb0941e8fb14e9` · `corpus_v5_inline_neg.py f676b1f439ebd87c` · `test_corpus_v0d.py 15addc6203a40869` · `shell_inline.py b6acd415c6b23d90`.
- Escrita: apenas o scratchpad da sessão, os dois diretórios de held-out (o do disjunto encomendado) e, agora, `diagnostics/holdout/` e `diagnostics/instrumentos-2026-10-09/`. Sem commit, sem cargo, sem kill, sem `rm -rf`.

## 9. Limites do que foi medido

1. **Independência.** O limite de Clopper-Pearson supõe amostras independentes; a amostra efetiva é menor que n (141 sessões inline, 516 arquivos). O limite verdadeiro é um pouco mais largo.
2. **Mesmo autor, mesmo repositório.** Disjunto por id, conteúdo e comando — 52 de 516 arquivos e 78 de 600 entradas de arquivo têm irmãos editados no vivo. Não é independência de população.
3. **Rótulo "limpo" por ausência de sintoma.** Negativo de arquivo = nenhum erro nomeou o arquivo antes da edição seguinte. Não afirma que o código estava correto (§4: 4 de 5 falsos de arquivo).
4. **tmp não está representado** no disjunto (estrato esgotado).
5. **Recall** vem dos 65 positivos do vivo (ajuste); só o falso deny é medida fora da amostra.
6. **Temporal pequeno e concentrado** (33 arquivos, 6 sessões inline, maior 43%): serve como teste "fora do tempo", não como prova de taxa.
7. **Só Python.** Os held-outs não cobrem Rust/TS/TSX/JS.

## 10. Reprodução

Tudo em `../instrumentos-2026-10-09/` (ver o `README.md` de lá). Os scripts de consolidação leem os JSONs **ao lado do próprio arquivo** (`SP = pasta do script`): copie `*.json` desta pasta para lá, ou rode do scratchpad.

```bash
S=~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/instrumentos-2026-10-09
cd /tmp && TOURING_CODE_MODE=native python3 -I $S/construir_disjunto.py --seed 20261010 \
    --n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03     # recusa diretório de saída não vazio
python3 -I $S/verificar_sobreposicao.py                                      # sobreposicao_disjunto.json
python3 -I $S/montar_ctl_generico.py ~/.claude/touring/vgp/corpus_holdout_disjunto <dir-híbrido>
bash $S/rodar_juiz_generico.sh <binário> <dir-híbrido> <saída.json> <log>    # espera a V3, roda o juiz, registra a coincidência
python3 -I $S/classificar_holdout.py <held-out> <juiz.json> <saida.json>     # construto de cada falso
python3 -I $S/limites.py 5 600 2 600                                         # Clopper-Pearson unilateral 95%
python3 -I $S/consolidar_holdouts.py                                         # tabela_holdouts.md + consolidado_holdouts.json
```

## 11. Decisões que cabem à coordenação (touring-09)

- **(a)** Devolver o defeito de atributos à Kimi: regressão pelas 3 sondas de §4.1; saída = abstenção ou warn em inline (como já é em arquivo) para atributo de módulo de terceiros, e/ou reconhecer `setattr` dinâmico e ligação em bloco composto no coletor.
- **(b)** Trocar o critério do juiz de "taxa pontual ≤ 1%" para "limite superior 95% ≤ 1%", ou declarar o n mínimo por balde. Com o critério novo o e935 reprova aqui (1,74% e 1,05%).
- **(c)** Repetir o disjunto com outra semente depois da correção. Com k = 0 em 600, o limite é 0,50% e prova ≤ 1%.
- **(d)** Os 4 falsos de ruído de rótulo (`7a4ad8f765aa`, `ddbb2cfb4afc`, `c35d78e3eb25`, `74f04a24de12`) são candidatos a um critério de limpeza do corpus: negativo só se o arquivo foi **executado** depois da edição, não apenas ausente de erro.

## 12. Arquivos desta pasta

| arquivo | conteúdo |
|---|---|
| `RELATORIO.md` | este relatório |
| `tabela_holdouts.md` · `consolidado_holdouts.json` | a tabela 2×2×2 e o consolidado por balde (n, falsos, limites, clusters, construtos, ids) |
| `k_v2_disjunto.json/.out` · `k_v2_holdout.json/.out` | juiz v2 do e935 sobre cada held-out (híbrido) |
| `c8b5_v2_disjunto.json/.out` · `c8b5_v2_holdout.json/.out` | o mesmo com o binário de controle |
| `classificar_disjunto_e935.json/.out` · `classificar_holdout_e935.json` | construto dos falsos do e935 (o do temporal é vazio: 0 falsos) |
| `classificar_disjunto_c8b5.json/.out` · `classificar_holdout_c8b5.json` | construto dos falsos do controle |
| `sobreposicao_disjunto.json` | a sobreposição em quatro chaves e o controle positivo |
| `holdout_meta_disjunto.json` · `holdout_meta_temporal.json` | parâmetros, universo, rejeições e sha_tree dos dois held-outs |
| `holdout_descricao.json` | descrição do temporal |
| `p95_por_grupo.out` · `p95_touring.out` | latência por grupo e por arquivo |
| `rodar_juiz_*.log` | espera pela V3 e coincidência durante cada medição |
| `scripts/` | **não é meu**: 13 cópias dos scripts (mtimes preservados) que apareceram aqui às 18:54:26, no mesmo segundo em que criei a pasta. As cópias que valem estão em `../instrumentos-2026-10-09/`, com 24 scripts e README. Deixei `scripts/` como encontrei |

## 13. Impacto e decisões — acrescentado pela analise-89 (coordenadora anterior)

**Origem do que o §12 registra**: o diretório `scripts/` (13 cópias, 18:54) e um `RELATORIO.md` anterior foram criados
pela analise-89, que copiou os artefatos do scratchpad efêmero antes de o subagente gravar aqui. O relatório do
subagente (§1–§12) é mais completo e substituiu aquele; o único conteúdo do anterior que não estava aqui é esta seção.

- **Não bloqueia o release 30.4.96**: os três hooks só negam com `[vgp] modo = "ativo"` — o retorno antecipado
  `if mode != VgpMode::Ativo` em `crates/touring-hook-handlers/src/hooks/pre_write.rs:1375`,
  `pre_edit_prevention.rs:562` e `pre_bash.rs:1001`; o padrão é sombra (`vgp/sombra.rs`, teste
  `sombra_check_grava_a_linha_e_nao_toca_a_resposta`) e `~/projects/analise/.touring/touring.toml` não declara `[vgp]`.
  **Bloqueia a V4** (ligar o deny).
- Recomendações (a decisão é da touring-09; enviadas a ela por mensagem às ~21:55Z):
  (a) devolver o defeito de atributos à touring-35 como P0 antes da V4 — módulo EXTERNO com sinal de ligação dinâmica
  (`setattr`, `globals()[…]`, `__getattr__` de módulo, nome ligado em bloco composto ou em `try/except`) → abstenção
  (warn/unknown), nunca deny; mesma severidade em arquivo e inline para o mesmo alvo externo incerto; as 3 sondas viram
  regressão. (b) o juiz exige **limite superior 95% ≤ 1% no held-out disjunto**, além da taxa pontual no ajuste.
  (c) depois do conserto, um disjunto NOVO com outra semente (ex. `20261011`) — este já informou o conserto.
