---
type: Report
title: Held-out 20261013f — fecho transitivo do estado no instante, exclusões a mais, viés medido
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T12:55:00Z
autor: instrumentos-p1
tags: ["#kind:report", "#domain:harness", "#status:sealed"]
---

# Held-out 20261013f

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`. Este diretório: `B/diagnostics/holdout-20261013f/`.
**O motor e o juiz NÃO rodaram no 20261013f** (nem no 20261013). Escolher o critério (A bruta | B adjudicada) não é decisão minha.

## 1. Identidade

| Item | Valor |
|---|---|
| Corpus selado | `~/.claude/touring/vgp/corpus_holdout_disjunto_20261013f` |
| Selo | `sha_tree 2f9282aa59a0566f` · 2.525 arquivos · 17.755.226 bytes · `selo.json` (formato do 20261011/20261013) |
| Conferência do selo | `scripts/confere_selo.py`: selftest ok (intacto passa, 1 byte trocado reprova) e `selo INTACTO` (rc 0) — `confere_selo_antes.log`, `confere_selo_depois.log` |
| Semente e parâmetros | 20261013 · `--n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03` (os do 20261013) |
| Corte | `2026-10-09T18:14:31.894Z` (o do vivo e do temporal, assertado pelo construtor) |
| Adjudicador congelado | `scripts/adjudica_rotulo.py` sha256 `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`: **o mesmo** do 20261013 (conferido no `congelados.json`, no `selo.json` e na integridade) |
| Substitui | `corpus_holdout_disjunto_20261013` (sha_tree `8ce465ddeb03f500`, intacto, **não apagado nem alterado**; `../holdout-20261013/SUPERADO.md` criado) |

## 2. O que foi feito, com evidência

### 2.1 Fecho transitivo do estado no instante (TDD)

Regra pré-registrada antes de reconstruir: sobrepor **todo módulo do projeto alcançável pelos imports, em qualquer profundidade**, a
partir da entrada; cada módulo é lido NO INSTANTE da entrada (base git em `ts` + dobra da sessão) e os imports DESSE conteúdo dão o
nível seguinte, nunca o arquivo de hoje. Entrada com qualquer módulo do fecho irreconstrutível fica INCOMPLETA e é contada por motivo.
Implementação em `scripts/estado_no_instante_v0d.py`: `fecho_transitivo` (busca em largura; a menor profundidade vale; ciclo termina;
a própria entrada nunca entra; irreconstrutível fica no fecho com `conteudo=None` e não se expande), `avaliar(fecho, profundidade_maxima)`
(`1` reproduz a regra de antes, o que mede o viés), `estado_completo` e `overlays_e_faltas` sobre o fecho, e `construir` gravando
`fecho{tamanho, profundidade_max, por_profundidade}` por entrada no manifesto. `importacoes_nao_cobertas` (a medida de profundidade 2 do
20261013) virou o **verificador independente**: relê os overlays gravados e reaplica a resolução, sem repetir a busca em largura.

| Prova | Resultado | Arquivo |
|---|---|---|
| RED sobre a regra de profundidade 1 (cópia congelada `e90a8514…`) | 14 reprovados, 26 passaram. 7 reprovam por **asserção de comportamento** (ex.: `estado_completo` devolve `(True,'ok')` com o módulo de profundidade 2 irreconstrutível; `overlays_e_faltas` devolve 3 overlays onde o fecho tem 6) e 7 por API ausente | `tdd_red.log` |
| GREEN | 42 passaram no módulo; 52 com os 10 testes do construtor | `tdd_green.log`, `teste_final.log` |
| Fixture sintética de reexporte em 2 níveis | `pkg/__init__.py` → `from .nucleo import f` → `nucleo.py` → `from .base import g as f` → `base.py` (profundidades 1, 2, 3); casos de ciclo, de a entrada importada de volta, de conteúdo do instante ≠ hoje, de menor profundidade entre dois caminhos | `scripts/test_estado_no_instante_v0d.py` |
| Mutantes | **23/23 mortos**, controle sem mutação passa (42 passed), sha do original igual antes/depois (`cc4516d0c008…`). **m12 «para na profundidade 1» morto** por 7 testes; m13 «para na profundidade 2» morto; m14 (segue os imports de hoje), m15 (sem guarda de ciclo, morre por TIMEOUT), m16 (a entrada entra no fecho), m17-m19 (falta funda ignorada / profundidade sempre 1 / motivo raso), m20-m23 | `mutantes_estado_no_instante.{log,json}` |

Os mutantes m4, m5 e m9 do 20261013 foram **re-ancorados** no texto novo (mesma intenção); a versão antiga está em `instrumentos_do_20261013/`.
O runner agora trata TIMEOUT como morto (pytest de 60 s por mutante).

### 2.2 Exclusões a mais

`--excluir-corpus`: 20261010 (`corpus_holdout_disjunto`), 20261011, **`corpus_v0d.bak-20261009-pre`**, **`corpus_v0d_staging_evidencia`** e, por conta
própria e declarada, **`corpus_v0d.bak-2026-10-09`** (o vivo antigo, que o motor julgou; ver §6). Vivo e temporal são implícitos. Tamanho das exclusões no meta:
3.486 ids · 1.881 tool_use_ids · 3.522 shas · 1.912 comandos.

**`fe5620ecee0a`**: estava no 20261013 (negativo de arquivo `analise/.../norma_corpus.py`) e **era a única entrada em comum dele com o
`bak-20261009-pre`** (mesmo id e mesmo sha de conteúdo; `staging_evidencia` e `bak-2026-10-09` deram 0). No 20261013f **não está**
(`metricas_vies.json` → `fe5620ecee0a_no_20261013f: false`; cruzamento com o `bak-20261009-pre` = 0 nas quatro chaves).

### 2.3 Sobreposição nas quatro chaves (ids, tool_use_ids, sha do conteúdo+contexto, comandos)

`verificar_sobreposicao.py`, que recomputa dos arquivos gravados (`sobreposicao_20261013f.{out,json}`):

| Corpus | ids | tool_use_ids | shas | comandos | Controle positivo (corpus × ele mesmo) |
|---|---|---|---|---|---|
| vivo `corpus_v0d` | 0 | 0 | 0 | 0 | 763 · 463 · 782 · 493 |
| temporal `corpus_holdout` | 0 | 0 | 0 | 0 | (temporal × vivo = 0, como previsto) |
| 20261010 `corpus_holdout_disjunto` | 0 | 0 | 0 | 0 | 1200 · 600 · 1200 · 600 |
| 20261011 | 0 | 0 | 0 | 0 | 1200 · 600 · 1200 · 600 |
| `bak-20261009-pre` | 0 | 0 | 0 | 0 | 81 · 51 · 80 · 51 |
| `staging_evidencia` | 0 | 0 | 0 | 0 | 763 · 463 · 782 · 493 |
| `bak-2026-10-09` | 0 | 0 | 0 | 0 | 791 · 491 · 809 · 522 |

`OK_zero_em_todos_os_cruzamentos: True` · `controle_positivo_enxerga: True` · duplicatas internas de conteúdo: 0.
**20261013 (informativo, `sobreposicao_20261013f_x_20261013.out`)**: 1.086 ids, 543 tool_use_ids, 1.086 shas e 543 comandos em comum. Esperado: mesma semente, mesmo universo, e o 20261013
nunca foi aberto (§6). É o único cruzamento diferente de zero.

### 2.4 Viés do fecho (medido no próprio construtor, `holdout_meta.json` → `vies_do_fecho`, e `metricas_vies.json`)

**Baldes (regra de parada < 300: nenhum caiu, não parei)**: arquivo **600** · inline **553** (c 200, heredoc 200, touring_run 153, tmp 0). Idênticos ao 20261013.
O touring_run (pool 795, 47 a menos que a cota) e o tmp (pool de 1 registro) já faltavam antes do fecho; o fecho não os tirou.

**Recusas por estado incompleto, por motivo, antes e depois do fecho.** A mesma corrente de candidatos que chegou ao teste é avaliada pelas duas regras
(`avaliar(fecho,1)` = regra do 20261013; `avaliar(fecho)` = fecho). Não é a seleção que a regra antiga teria feito: é a taxa em que cada regra reprova a mesma corrente.

| Balde (candidatos avaliados) | ok antes | recusados antes (prof. 1) | recusados depois (fecho) | só pelo fecho | taxa antes → depois |
|---|---|---|---|---|---|
| arquivo (911) | 661 | 250 | 311 | **+61** | 27,4% → 34,1% |
| inline c (223) | 205 | 18 | 23 | +5 | 8,1% → 10,3% |
| inline heredoc (209) | 202 | 7 | 9 | +2 | 3,3% → 4,3% |
| inline touring_run (155) | 153 | 2 | 2 | 0 | 1,3% → 1,3% |

Motivos do balde arquivo: antes — submódulo 164 · módulo 84 · `__init__` de pacote ancestral 2. Depois — os mesmos 250 **mais** módulo do fecho (profundidade ≥ 2) 37 e
submódulo do fecho 24. Inline (somados): submódulo 22 · módulo 5 (iguais antes e depois) **mais** módulo do fecho 2 e submódulo do fecho 5. O 20261013, sem fecho, recusou
no arquivo 145 submódulo · 75 módulo · 2 ancestral e no inline 22 · 5 (a corrente dele tinha outro tamanho: números não somam com os acima).

**Tamanho do fecho** (nº de módulos sobrepostos por entrada):

| Conjunto | n | com fecho não vazio | média | p90 | p99 | máx |
|---|---|---|---|---|---|---|
| candidatos avaliados, arquivo | 911 | (mediana 0) | 7,03 | 11 | 69 | **415** |
| **aceitas**, arquivo | 600 | 94 (15,7%) | 1,19 | 2 | 42 | 51 |
| **aceitas**, inline | 553 | 11 (2,0%) | 0,09 | 0 | 2 | 28 |
| vivo, amostra de passo fixo 150/763 (sonda, só leitura) | 150 | 43 (28,7%) | 6,92 | 6 | 91 | 415 |

Aceitas: 1.153 entradas e 767 overlays; profundidade máxima 0 → 1.048 entradas · 1 → 75 · 2 → 16 · 3 → 4 · 4 → 2 · 7 → 1 · 8 → 3 · 9 → 4. 30 entradas (29 de arquivo)
têm módulo de profundidade ≥ 2 no fecho — exatamente o que o 20261013 deixava fora da regra. Verificador independente: **0 entradas com lacuna, 0 incompletas** no corpus.

**Mix de projetos** (entradas de arquivo, por `project_root`): 20261013f = analise 519 (86,5%) · touring 71 (11,8%) · .claude 6 (1,0%) · simulador-urna 4 (0,7%); vivo (negativos de arquivo)
= 259 (86,3%) · 36 (12,0%) · 3 (1,0%) · 2 (0,7%). **Igual ao 20261013** (519/71/6/4) e dentro de 0,2 ponto do vivo. Inline: 553 analise (vivo: 374 analise). A
cota por projeto preservou o mix; o fecho não o deslocou.

**Parentesco com o 20261013**: 1.086 de 1.153 ids em comum (94,2%); 67 saem e 67 entram. Coerente com os 70 candidatos recusados só pelo fecho (61 + 9 inline) mais a saída do
`fe5620ecee0a`; a atribuição entrada a entrada não foi feita (§8).

### 2.5 Outras provas e artefatos

- Construtor: `construir_disjunto_20261013f.log`, rc=0, 227,1 s de parede, vivo e temporal intocados; `entradas_incompletas` 0; `rodar_construtor_20261013f.sh` reproduz o comando exato.
- `construir_disjunto.py` ganhou `avaliar_candidato`/`resumo_do_vies` (medição na mesma passada) e `memoizar_git` (memo das leituras `rev-list`/`show`, 13.429 acertos e 5.573 faltas): o fecho
  lê dezenas de módulos por candidato e sem memo custaria ~20×. 10 testes do construtor passam (`teste_construtor_ho13f.log`).
- Integridade final: `integridade_final.{log,json}` → `tudo_intacto: true` (selo do 20261013f, do 20261013 e do 20261011 conferem; os 10 arquivos congelados do `f` batem; as 10 cópias dos instrumentos do
  20261013 em `instrumentos_do_20261013/` batem com o registro antigo; os 9 corpora lidos têm o mesmo sha_tree antes e depois).

## 3. Estado por item do pedido

| # | Pedido | Estado |
|---|---|---|
| 1 | Fecho transitivo, entrada incompleta contada por motivo, TDD (fixture de reexporte em 2 níveis, RED→GREEN, mutante «para na profundidade 1» reprovado) | feito (§2.1): RED 14/40, GREEN 42/42, 23/23 mutantes |
| 2 | `bak-20261009-pre` e `staging_evidencia` em `--excluir-corpus`; `fe5620ecee0a` fora | feito (§2.2); +`bak-2026-10-09` por conta própria, declarado |
| 3 | Destino `…20261013f` e `B/diagnostics/holdout-20261013f/`, mesma semente; 20261013 intocado; `SUPERADO.md` | feito |
| 4 | Viés: recusas por motivo antes/depois, tamanho do fecho, mix contra o vivo; parar se balde < 300 | feito (§2.4); nenhum balde < 300 |
| — | Sobreposição 4 chaves com controle positivo | feito (§2.3) |
| — | Selo no formato do 20261011, conferido por `confere_selo.py` | feito (§1) |
| — | Adjudicador continua `fabc36c9…` | confirmado |

## 4. Os processos

Tudo pesado passou pelo `scripts/hw-slot` (construtor, mutantes, sonda), `nice -n 19` no construtor, `TOURING_CODE_MODE=native` para os scripts que escrevem. Nenhum `kill`/`pkill`,
nenhum `rm -rf`, nenhum `git stash`/`reset`, nenhum commit ou push. O construtor esperou o `hw-slot` (92–93 °C, limite 88) e rodou 227,1 s. A queda de rede de 08:09Z a 12:4xZ não afetou o
disco: o build tinha terminado às 05:16Z (hora do log: 02:16 BRT). Nenhum motor, juiz ou subprocesso de `touring vgp` foi lançado contra o corpus.

## 5. Dados com sha

| Arquivo | sha256 (início) |
|---|---|
| `selo.json` | `247720f835966d3e6543…` (ver arquivo) · sha_tree do corpus `2f9282aa59a0566f` |
| `holdout_meta.json` do corpus | `6bb7c47ad084ede0365e96172ac78275ee6414f03746b838845a5a7c927ea7f4` |
| `entradas.jsonl` | `708a9aa41d6307abb954e04d27b501c195c75e582fe2cd83f131b2072fe3f6e7` |
| `entradas_inline_neg.jsonl` | `76b56b47396b7883be61a007f77e8db31e4b29d7cd605fad5b4c2ec244561397` |
| `congelados.json` | `68a785256cc497aa4725b88732b4333eb427a58ffd24cb691abc2ec6bc8ddc4f` |
| `scripts/adjudica_rotulo.py` | `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c` (inalterado) |
| `scripts/estado_no_instante_v0d.py` | `cc4516d0c008d3fad41659ed3d8a506417c19d0aa24f85454e3aad25e7d87891` (era `e90a8514627d…`) |
| `scripts/test_estado_no_instante_v0d.py` | `bb8f19b498398e3663fc…` (era `bb1f108fb8a0…`) |
| `construir_disjunto.py` | `e6713e59b798571280ff…` (era `f04f8cdf9909…`) · registrado no selo como `construtor_sha256` |
| `mutantes_estado_no_instante.py` | `f7301d221de1a40f40fc…` (era `044b82eb646f…`) |
| `test_construir_disjunto_ho13.py` | `eb3fcb90689b24619a53…` (era `88c7964212d7…`) |
| `integridade_ho13f.py` · `metricas_vies_ho13f.py` (novos) | `25f59b083806…` · `1c72cc5dfb39…` |
| Demais congelados (iguais ao 20261013) | `exclusoes_disjunto.py` `e2c76d00c6b3…` · `verificar_sobreposicao.py` `972cf5ee67f3…` · `mutantes_adjudica_rotulo.py` `8eed8f856976…` · `confere_selo.py` `fed3c847630f…` |

Os `instrumentos_do_20261013/` guardam cada arquivo congelado do 20261013 no estado exato dele (sha-verificado, `ARQUIVOS.json`), porque os instrumentos compartilhados foram editados no lugar.

## 6. Decisões e riscos

1. **`corpus_v0d.bak-2026-10-09` excluído a mais** (o pedido listava os outros). É o vivo de antes da promoção; o motor julgou esse corpus. Excluí-lo só protege a independência e mudou a corrente em, no máximo, os 28 ids que saíram do vivo. Declaro porque foge da lista literal; basta o Gabriel/coordenação dizer se prefere sem ele.
2. **20261013 não foi excluído do `f`.** O pedido não o lista; ele nunca foi aberto e está superado, então o parentesco (94,2% dos ids) não vaza informação de juiz. Se o 20261013 vier a ser aberto por algum motivo, o `f` deixa de ser independente dele. A remoção é do Gabriel.
3. **Viés de seleção que o fecho introduz e que nenhuma cota corrige.** Entre os candidatos de arquivo avaliados o fecho tem média 7,03 e máximo 415; entre os aceitos, média 1,19 e máximo 51, e só 15,7% têm fecho não vazio (vivo: 28,7% na amostra). O fecho grande é mais provável de ter algum módulo irreconstrutível, então o held-out sub-representa arquivos com grafo de import grande. O falso deny por deriva de módulo, que o fecho existe para evitar, é exatamente o que mais aparece nesses arquivos: **a taxa medida no `f` não vale para eles**. Se o critério final precisar dessa cobertura, é preciso ou reportar o balde «fecho ≥ 1» à parte (94 de arquivo + 11 inline) ou aceitar o limite.
4. **Over-aproximação do fecho.** `importacoes()` usa `ast.walk` e conta import dentro de função, `try/except ImportError` e `if TYPE_CHECKING`; o fecho recusa também por módulo que o programa talvez nunca carregue. Só pode recusar a mais, nunca a menos; está embutido nas taxas acima.
5. **Custo no motor.** Entradas aceitas chegam a 51 overlays (28 no inline); o 20261013 tinha poucos. Latência e argumentos `--overlay` do motor e do pyright serão medidos no P0B; não medi.
6. **`memoizar_git` muda o custo e estabiliza a leitura** (o mesmo par raiz+argumentos responde igual durante a construção); o 20261013 foi construído sem memo. Só difere se o HEAD de algum repo andou para trás de `ts`, o que não ocorre com commits novos.
7. **Instrumentos compartilhados editados no lugar** (estado, teste, construtor, mutantes, teste do construtor). O 20261013 continua verificável pelas cópias sha-verificadas; o `integridade_ho13.py` antigo passaria a acusar a diferença nos shas, por isso a verificação do `f` é `integridade_ho13f.py`.

## 7. Git

Sem commit nem push. `git status --short` (restrito ao que toquei) em B:

```
?? diagnostics/estado_no_instante_v0d.txt   (anterior a esta tarefa)
?? diagnostics/holdout-20261013/            (diretório inteiro não rastreado; ganhou SUPERADO.md)
?? diagnostics/holdout-20261013f/           (novo: este relatório e todas as provas)
?? diagnostics/instrumentos-2026-10-09/     (diretório inteiro não rastreado; instrumentos editados no lugar)
?? scripts/estado_no_instante_v0d.py        (não rastreado; editado)
?? scripts/test_estado_no_instante_v0d.py   (não rastreado; editado)
```

Conferido por `git ls-files --error-unmatch` (nenhum desses caminhos é rastreado) e `git check-ignore` (nenhum é ignorado): nada deles está no histórico, então o desfazer é só por
cópia, e é por isso que `instrumentos_do_20261013/` guarda as versões antigas. Corpora em `~/.claude/touring/vgp/` não são versionados; os sha_tree de todos os anteriores estão em
`integridade_final.json` (iguais antes e depois do build). O resto do `status` da árvore principal é de outras sessões e não foi tocado.

## 8. O que NÃO foi verificado

- **Nenhum motor ou juiz rodou**, de propósito: não sei como o motor se comporta com fechos de 28 a 51 overlays, nem se o fecho que calculei é o que ele realmente carrega.
- **A resolução de imports do `eni` é a do construtor, não a do motor.** Fora do fecho ficam: import dinâmico (`importlib`, `__import__`, `exec`), manipulação de `sys.path`, namespace packages sem `__init__.py`, `.pyi`, `conftest`/`sitecustomize` e plugins.
- **Módulo que existia no instante e foi apagado do disco depois** não resolve (a resolução olha o disco de hoje) e fica fora do fecho em silêncio; não medi quantos são. Para o motor o import sem arquivo é externo, então não deveria virar falso deny, mas não provei.
- **Determinismo da reconstrução**: não rodei o construtor uma segunda vez para comparar o sha_tree (no HO13 comparei a reprodução; aqui não).
- **Atribuição dos 67 ids trocados** em relação ao 20261013 (fecho × exclusão × cadeia de cotas): estimei pelas contagens, não por entrada.
- **`mutantes_adjudica_rotulo.py` não foi rerodado**: o adjudicador tem o mesmo sha do 20261013, cuja evidência está em `../holdout-20261013/`.
- A sonda do vivo (`sonda_fecho_vivo_amostra150.json`) é uma amostra de passo fixo de 150 entradas (positivos e negativos), usada só como referência de forma, não como população.
