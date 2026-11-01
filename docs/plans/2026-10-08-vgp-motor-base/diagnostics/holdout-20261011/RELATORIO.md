# Juiz com limite superior + held-out selado 20261011 — 09/10/2026

> Autor: teammate de instrumentos da touring-09 (Sonnet 5.5). Sem commit. Scripts: `../instrumentos-2026-10-09/`. Dados brutos: esta pasta.

## 1. Juiz (`scripts/juiz_motor_vgp.py`)

| | sha256 |
|---|---|
| antes | `b7f7fd9b4ccddf62f90f0417d84f4a8483a1188a0e1b8c371ff3e8b323c73f1f` (cópia: `juiz_motor_vgp.ORIG-b7f7fd9b.py`) |
| depois | `89a976164c291d9d7b3ae561f9f83004235ce1c6bfd6bbda80f0a0d6a9513a35` (diff: `juiz_ORIG_para_NOVO.diff`) |

- Limite superior Clopper-Pearson unilateral 95% (cópia de `limites.py`, autocontida) de **cada balde**, sempre: JSON `falso_deny_por_balde.<balde>.{falsos,n,taxa,limite_superior_95,falsos_permitidos_pelo_teto}` e linha da tabela no stdout.
- `--exigir-limite-superior`: balde com limite > 1% vira pendência (k, n, taxa, limite, máximo de falsos que o n admite; se nem 0 falsos basta, o n mínimo = 299). Sem a flag o veredito é o de antes.
- **Selftest: passou** (rc 0, 32 s): 9/9 cenários + 11 casos de `classe_erro` (os de antes) + 5 valores de referência (0/600 0,50 · 1/600 0,79 · 3/1200 0,64 · 5/600 1,74 · 2/600 1,05, ±0,01 pp; medido 0,498 · 0,788 · 0,645 · 1,744 · 1,046) + 8 cenários do limite (k/n exatos via motor em processo) + 2 da CLI (`main()` com e sem a flag).
- **Mutantes: 8/8 mortos** (cada um numa cópia no tmp; o sha do original igual antes e depois; a cópia sem mutação passou): m1 taxa pontual no lugar do limite · m2 bilateral (alfa/2) · m3 flag sem pendência · m4 `main` ignora a flag · m5 flag sempre ligada · m6 mensagem sem o n permitido · m7 bisseção curta · m8 teto frouxo (`mutantes_limite_superior.{log,json}`). Os 3 pedidos são m1-m3.
- **Não-regressão** (`naoregressao_comparacao.{out,json}`): e935 (`e935aaf17d9a09d0`), corpus vivo `corpus_v0d`, `--v5-estrito`, sem a flag, de `~/projects/analise` via `hw-slot`, juiz antigo × novo, `main()` inteiro. **762 ids comparados, 0 divergências** no veredito do motor (verdict + classe, nome, linha, severidade de cada achado). JSON do juiz igual fora dos 2 campos novos e do p95; pendências iguais; stdout igual fora da tabela nova e do número do p95; rc 0 = 0; PRONTO nos dois (falsos 0/400 e 0/300; limites 0,75% e 0,99%).
- **Controle positivo da flag** (híbrido do disjunto 20261010, remontado com `montar_ctl_generico.py`; `entradas.jsonl`, `entradas_inline_neg.jsonl` e o manifesto idênticos byte a byte aos do híbrido usado às 18:43): **com a flag, NÃO PRONTO, rc 1**, pendências `arquivo 1.74% (5/600)` e `inline 1.05% (2/600)`, "com n=600 o teto admite no máximo 1 falso(s)". Contraste isolado com `--p95-ms 1000` (a máquina estava a 90-93 °C): sem a flag PRONTO rc 0; com a flag NÃO PRONTO rc 1 e **exatamente** essas duas pendências. Os 7 ids de falso deny são os de `holdout/k_v2_disjunto.json`.

## 2. Held-out 20261011 (SELADO)

`~/.claude/touring/vgp/corpus_holdout_disjunto_20261011/` — semente 20261011, mesmos parâmetros do 20261010 (`--n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03`), nativo, `hw-slot`, rc 0, 366,8 s.

| balde | n | composição | concentração |
|---|---|---|---|
| arquivo | 600 | atribuído 600 · tmp 0 (estrato esgotado) · touring 72 / analise 518 / .claude 6 / simulador 4 | 542 arquivos distintos, maior 5 (0,83%) |
| inline | 600 | c 200 · heredoc 200 · touring_run 200 · tmp_escrito_e_executado 0 | 149 sessões, maior 18 (3,00%, no teto) |

Positivos 0; 0 snaps que não compilam. Corte `2026-10-09T18:14:31.894Z` (igual ao do vivo/temporal).

**Sobreposição** (`sobreposicao_20261011.{out,json}`): contra vivo, temporal e disjunto 20261010, **0 nas quatro chaves** (ids, tool_use_ids, shas de conteúdo e contexto, comandos) nos três. Controle positivo: vivo×vivo 791/491/809/522 e 20261010×20261010 1200/600/1200/600 (todos > 0). Exclusões usadas pelo construtor: 2257 ids · 1281 tool_use_ids · 2270 shas · 1312 comandos. Descritivo: 63 dos 542 arquivos (89 das 600 entradas) também foram editados no vivo, em outros instantes (no 20261010 eram 52/516 e 78/600).

**Selo** (`selo.json`): `sha_tree c495bcab66718230` (2077 arquivos, 13.017.450 bytes), conferido de novo ao fim: inalterado. Vivo `b06b89113c869097`, temporal `6f5fc46410884fd2` e disjunto 20261010 `c448c37dfd7ef053` também inalterados (`integridade_final.json`). O motor e o juiz **não** rodaram aqui; nenhum conteúdo foi copiado além do diretório de saída (o cache do universo de candidatos, só `caminho@ts`, ficou no scratchpad). Meta: `holdout_meta_20261011.json`.

**Flags novas, sem regressão do comportamento antigo** (`construir_disjunto.py`: `--excluir-corpus DIR` repetível e `--cache`; `--saida` já existia; `exclusoes_disjunto.exclusoes(extras=())`; `verificar_sobreposicao.py ... --contra DIR` repetível): `exclusoes()` sem extras igual à original; `verificar_sobreposicao.py` sem `--contra` dá o mesmo JSON mais a chave `OK_zero_em_todos_os_cruzamentos`; e a **reconstrução do 20261010 sem nenhuma flag** (semente e parâmetros de antes) bate com o corpus gravado: `entradas*.jsonl` byte a byte, 1200 snaps, seleção de arquivo e inline (`repro_20261010_comparacao.out`). A única diferença no meta é o universo inline, que cresceu 54 eventos, todos posteriores ao corte.

## 3. O que NÃO foi verificado

- Nenhuma taxa de falso deny do held-out novo (selado por desenho); nem se ele contém o defeito de atributos ou ruído de rótulo.
- Independência: Clopper-Pearson supõe amostras independentes; 149 sessões inline e 542 arquivos; 63 arquivos têm irmãos no vivo. Mesma população, mesmo autor. Só Python. Estrato tmp ausente.
- A flag foi exercida só com `--v5-estrito`; não com `--base` (V3) nem `--arg-motor`, nem em corpora Rust/TS.
- O p95 não entra na comparação (carga térmica). Sob 90-93 °C o p95 do híbrido deu 159,9 / 171,2 / 161,7 ms (> 150) — pendência de p95 independente da flag. Antes do P0, rodar com a máquina fria.
- A primeira tentativa de mutantes e da rodada original saiu com rc 75 (`hw-slot: sem vaga após 600s: temp 94C > 88C`; logs `*.tentativa1-hwslot-sem-vaga.*`); reexecutadas com `--wait 3300`, sem tocar no limite de temperatura.

## 4. Achados para a coordenadora

1. **Margem da flag no P0:** com n = 600 por balde o teto admite no máximo **1** falso (0,79%); 2 já dão 1,05% e reprovam. No vivo (n = 400 e 300) o limite com 0 falsos é 0,75% e 0,99% — o inline passa por 0,01 ponto, e `falsos_permitidos(300) = 0`.
2. **Não-determinismo do motor (e935):** para o mesmo binário e o mesmo id, `findings[].reais` sai em ordem diferente a cada chamada (10 chamadas, 10 ordens, em 7 ids) e, em dois deles, o texto de `sugestao` ("Você quis dizer `x`") oscila, até entre "com" e "sem" sugestão (`motor_ordem_de_reais.out`). O veredito (verdict, classe, nome, linha, severidade) não variou nos 762 ids. Conjunto não ordenado vazando para a saída; um teste de golden exato sobre `sugestao` seria instável.
