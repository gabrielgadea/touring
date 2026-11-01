# Held-out 20261013 — estado no instante completo e adjudicação de rótulo — 10/10/2026

> Autor: instrumentos-p1 (Sonnet 5.5), a pedido da coordenadora touring-09. Sem commit, sem `kill`, sem `rm -rf`. **Nenhum motor nem juiz rodou no 20261013** (só construtor, hash e contagem). O CRITÉRIO (A bruta | B adjudicada) é do Gabriel: aqui estão os dois insumos, sem decisão.
> B = `docs/plans/2026-10-08-vgp-motor-base` · I = `B/diagnostics/instrumentos-2026-10-09`.

## 1. Resultado em seis linhas

1. `~/.claude/touring/vgp/corpus_holdout_disjunto_20261013/`, semente 20261013, parâmetros do 20261011. **arquivo 600 · inline 553** (c 200 · heredoc 200 · touring_run 153 de 200 · tmp 0). O inline fecha 47 abaixo do pedido: o teto de 18 por sessão barra o `touring_run` (640 recusas) e o estrato tmp está esgotado desde o 20261010.
2. **Estado no instante completo**: 1153 de 1153 entradas com manifesto completo, 0 incompletas dentro do corpus; 249 candidatos recusados por estado incompleto (222 arquivo, 27 inline), contados por motivo (§4).
3. Sobreposição **0 nas quatro chaves** contra vivo, temporal, 20261010 e 20261011, com controle positivo; mais 0 contra o vivo ANTERIOR à promoção de 09/10. Um achado: **1 entrada coincide com o backup obsoleto `corpus_v0d.bak-20261009-pre`** (§5).
4. Selo `8ce465ddeb03f500` (2294 arquivos, 16.471.738 bytes) conferido por `scripts/confere_selo.py` (selftest ok), e de novo ao fim. 20261011 e demais corpora intocados.
5. `adjudica_rotulo.py` congelado com sha **antes** de construir: `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`. Selftest 84 casos + lote; **49/49 mutantes mortos**.
6. TDD do estado no instante: RED 20 falhas/5 passam sobre o módulo original → GREEN 25 passam; **11/11 mutantes mortos**, inclusive «só o `__init__`». Prova na fixture sintética com o motor: manifesto antigo ⇒ `deny _publicar`; completo ⇒ `allow` (e935aaf1 e f55ec4b5).

## 2. Correção (i) — o estado no instante é completo

**A causa** (`6a25b00f2584`, P0M): `modulos_do_projeto` devolvia, por import, só o PRIMEIRO candidato. `from scripts.eleitoral import publicar_mapa as pm` resolvia para `scripts/eleitoral/__init__.py`, e `publicar_mapa.py` nunca entrava no manifesto: o motor o lia de HOJE. `overlays()` ainda pulava em silêncio o módulo que não reconstruía.

**A regra** (`scripts/estado_no_instante_v0d.py`): o manifesto sobrepõe TODO módulo do projeto que a entrada importa — `import a.b.c` (o módulo e o `__init__` de cada pacote do caminho), `from a import b` (`b` submódulo ou subpacote, além de `a`), `from . import x`, `from .m import y` (mais o `__init__` do pacote corrente). Tipos: `direto`, `ancestral`, `submodulo`. Nome que não é arquivo (`from a import funcao`), `*` e módulo fora da raiz do projeto não inventam overlay. `estado_completo(entrada, texto, indice)` devolve `(False, motivo)` se algum desses módulos não reconstrói NO INSTANTE (`corpus_v0d._conteudo_depois`): a entrada é **INCOMPLETA** e o construtor a deixa fora (`--exigir-estado-completo`). O manifesto passou a trazer `completo`, `faltantes` e `nao_cobertas_prof2`; `overlays()` manteve assinatura e comportamento (`overlays_e_faltas` é a nova). A regra antiga ficou como `modulos_do_projeto_legado`, só para medir quanto a nova completa e como mutante.

| evidência | resultado |
|---|---|
| RED: `test_estado_no_instante_v0d.py` sobre a cópia do módulo ORIGINAL (`tdd_red.log`) | 20 falhas, 5 passam (irmão do script, externo, o próprio arquivo, selftest do módulo e `test_modulo_fora_da_raiz`…) |
| GREEN: o mesmo teste sobre o módulo corrigido (`tdd_green.log`) | 25 passam |
| Mutantes sobre cópia (`mutantes_estado_no_instante.{log,json}`) | **11/11 mortos**, controle sem mutação passou, sha do original igual antes/depois. m1 = «só o `__init__`» (3 testes o matam, entre eles o caso real) · m2 sem ancestrais · m3 relativo sem o pacote corrente · m4 `estado_completo` ignora a falta · m5 esconde a falta · m6 sem a 2ª base · m7 aceita módulo fora da raiz · m8 volta à regra antiga · m9 grava o arquivo de HOJE · m10/m11 profundidade 2 |
| Construtor (`test_construir_disjunto_ho13.py`) | 7 passam: o filtro só age depois dos testes baratos, conta o motivo, e `--congelados` aborta se um sha mudou |
| **Prova com o motor, em fixture sintética** (`prova_motor_fixture.log`) | programa `from scripts.eleitoral import publicar_mapa as pm; pm._publicar()`; HOJE só tem `_publicar_e_conferir`. (a) sem overlay: `deny _publicar`; (b) manifesto ANTIGO (só `__init__`): `deny _publicar`; (c) manifesto COMPLETO (+ `publicar_mapa.py` do instante): `allow`. Igual em e935aaf1 e f55ec4b5 |

**Efeito medido no corpus novo**: das 161 entradas com ao menos um módulo do projeto importado, **130 (81%) tinham módulo que a regra antiga NÃO sobrepunha** (módulos: 213 diretos · 189 ancestrais · 134 submódulos). O defeito do P0M não era raro, era a regra.

## 3. Correção (ii) — `B/scripts/adjudica_rotulo.py`

`sha256 fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`, registrado em `congelados.json` e no `selo.json` antes de construir/selar. Um falso deny de **NOME** é `bug_latente` somente se: **(1)** o arquivo é autocontido — sem `import *`, `exec`/`eval`/`globals()`/`locals()`/`vars()`, `__builtins__`, `.__dict__`, `setattr` (salvo em `self`/`cls`), mutação de `sys.modules`, `__getattr__` de módulo; **(2)** o nome nunca é ligado em escopo nenhum (atribuição, `for`, `with`/`except` as, import, def/class, parâmetro, walrus, compreensão, `match`, `global`/`nonlocal`, `del`) e não é builtin nem implícito de módulo; **(3)** o uso (na linha do achado) executa incondicionalmente — nível de módulo ou corpo de classe, corpo de `try` sem `except` que pegue `NameError` ou `finally`, `with` sem `suppress`, teste de `if`/`while`, iterável do `for`, e o que o `def` avalia na definição (decorador, default, anotação sem `from __future__ import annotations`). **Não** são incondicionais: corpo de função/lambda (mesmo se chamada), ramo de `if`/`for`/`while`/`match`/`except`/`else`, curto-circuito, compreensão (salvo o 1º iterável) e tudo depois de `raise`/`return`/`sys.exit`/`while True`. Na dúvida NÃO adjudica; toda recusa nomeia o motivo.

- **Lote** (`--juiz-json J --corpus C`): um falso deny só é adjudicado se TODOS os seus achados deny são de nome adjudicados como `bug_latente`; `achados` com 3 itens (o juiz corta em 3) não é adjudicado por possível truncagem. Imprime por balde a bruta ao lado da adjudicada e o CP95 das duas.
- **Selftest**: 84 casos, 22 esperam `bug_latente` (o padrão `finally` com nome nunca ligado, uso direto, corpo de classe, decorador, default, anotação sem future, teste do `if`, iterável do `for`, `try` com `except` que não pega NameError…) e 62 esperam `nao_adjudica` (controles exec, eval, globals, locals, setattr, star, `__getattr__`, `__dict__`, `sys.modules`, função não chamada, função chamada, 16 formas de ligação, ramos condicionais, `except` que captura, curto-circuito, compreensão, código depois de saída, `suppress`, future annotations, linha errada, sintaxe inválida) + o lote.
- **Mutantes** (`mutantes_adjudica_rotulo.{log,json}`): **49/49 mortos**, controle passou. Cada um quebra uma regra: autocontido (a1-a8), nome ligado (b1-b11), execução incondicional (c1-c27), lote (d1-d3). Dois ramos ficaram sem mutante de propósito, por serem equivalentes (o desvio `construto_desconhecido` e o `Match` com campo ≠ `subject`; `mutantes_adjudica_rotulo.py` documenta).
- **Decisões de leitura literal** a conhecer: o `if __name__ == "__main__":` conta como ramo (não adjudica); função chamada no módulo não adjudica; a anotação de `def` é tratada como avaliada na definição (`ANOTACAO_AVALIADA_NO_DEF = True`, certo até Python 3.13, o do projeto é 3.12; no 3.14 ela é adiada e a constante deve virar False, o que só adjudica MENOS).
- **Não foi aplicado a nenhum dado real.** Não testei contra `b9d26a878f4c`: não li o conteúdo do 20261011 além do que o DIAGNOSTICO cita.

## 4. Construção

`construir_disjunto.py --seed 20261013 --n-arquivo 600 --n-inline-por-estrato 200 --n-tmp 100 --teto 0.03 --exigir-estado-completo --excluir-corpus {20261010, 20261011} --congelados congelados.json`, nativo, via `hw-slot`; rc 0 em 197,3 s (`construir_disjunto_20261013.log`, `holdout_meta_20261013.json`). Antes dele, um dry-run descartável (semente 1, n pequeno, saída no scratchpad; `dryrun_construtor.log`) validou o caminho inteiro. Corte `2026-10-09T18:14:31.894Z`. Exclusões: 3429 ids · 1853 tool_use_ids · 3443 shas · 1883 comandos.

| balde | n | composição | concentração |
|---|---|---|---|
| arquivo | 600 | atribuído 600 · analise 518 (seleção) · touring 72 · .claude 6 · simulador 4 | 508 arquivos distintos, maior 6 (1,00%) |
| inline | 553 | c 200 · heredoc 200 · touring_run **153 (faltaram 47)** · tmp_escrito_e_executado **0 (faltaram 100)** | 152 sessões, maior 18 (3,25%) |

Recusas por estado incompleto (candidatos tentados e recusados, por motivo): arquivo — submódulo 145 · módulo 75 · `__init__` ancestral 2; inline — submódulo 22 · módulo 5. Outras recusas do arquivo: 540 «conteúdo não reconstrói», 467 «id já em outro corpus», 26 «conteúdo já visto», 2 «não compila». Manifesto: **1153/1153 `completo`**, 161 entradas com overlay (536 overlays; 1 entrada com 15), 600 com delta (`disk_file`), 0 snaps que não compilam.

## 5. Sobreposição (`sobreposicao_20261013.{out,json}`, `sobreposicao_20261013_extras.{out,json}`)

As chaves de cada linha são, nesta ordem, ids · tool_use_ids · shas · comandos. O rótulo `disjunto_x_*` do script refere-se ao 20261013.

| contra | resultado |
|---|---|
| vivo atual (`corpus_v0d`, 763 ids) | 0 · 0 · 0 · 0 |
| temporal | 0 · 0 · 0 · 0 |
| 20261010 | 0 · 0 · 0 · 0 |
| 20261011 | 0 · 0 · 0 · 0 |
| vivo ANTERIOR (`corpus_v0d.bak-2026-10-09`, 15:02) | 0 · 0 · 0 · 0 |
| `corpus_v0d.bak-20261009-pre` (backup de 08/10, 81 linhas) | **1 · 0 · 1 · 0** |
| controle positivo vivo×vivo · 20261010×20261010 · 20261011×20261011 | 763/463/782/493 · 1200/600/1200/600 · 1200/600/1200/600 (todos > 0) |

O 1 achado: o negativo de arquivo `fe5620ecee0a` (`analise/scripts/process_analysis/gates/norma_corpus.py` @ `2026-09-24T11:14:10.452Z`) também está no backup mais antigo do vivo. Esse backup **não** está na lista de exclusão pré-registrada (vivo, 20261010, 20261011, temporal) e o corpus já está selado; deixo a decisão (ignorar, ou retirar esse id na avaliação) com a coordenadora. Descritivo: 61 das 600 entradas de arquivo (45 dos 508 arquivos) também foram editadas no vivo em outros instantes.

## 6. Selo e integridade

- `selo.json` no formato do 20261011 (sha_tree, arquivos, bytes, sha256 dos 3 índices) com o **mesmo** `sha_tree` de `confere_selo.py` (`selar_holdout.py` o importa), mais proveniência: `adjudicador_sha256`, `estado_no_instante_sha256`, `construtor_sha256`, `congelados_json_sha256`. `sha_tree 8ce465ddeb03f500`.
- `confere_selo.py --selftest`: ok (intacto passa, 1 byte trocado reprova). `--corpus … --selo …`: **INTACTO** antes e depois (`confere_selo_antes.log`, `integridade_final.json`).
- Congelados (10 arquivos, `congelados.json`): conferidos pelo próprio construtor antes de construir e de novo ao fim: todos iguais.
- 20261011 (`c495bcab66718230`), 20261010 (`c448c37dfd7ef053`), temporal (`6f5fc46410884fd2`): intocados. **O vivo mudou desde 09/10**: `sha_tree` `b06b89113c869097` → `60ebf06bd598fd95` (791 → 763 ids), coerente com a promoção do staging registrada em `P2-promocao-2026-10-09.log` (19:43; inferência minha pela data e pela contagem, não li o log); a exclusão usa o vivo ATUAL, e o vivo anterior foi checado à parte (§5).
- Não foi tocado: `corpus_v0d.py`, `corpus_v5_inline_neg.py` (sha iguais ao do congelamento), `confere_selo.py`, os corpora existentes.

## 7. O que medi a mais, e os vieses que a construção correta traz

- **Profundidade 2 (medida, não exigida)**: das 161 entradas com overlay, **91 (57%)** têm módulo sobreposto que importa outro módulo do projeto que o manifesto NÃO cobre (717 módulos). Um `pkg/__init__.py` que reexporta de `pkg/sub.py` é o caso típico. A regra pré-registrada cobre o que a ENTRADA importa; este número diz o que ela deixa de fora, e a mesma classe de erro do P0M pode reaparecer por aí (`nao_cobertas_prof2` no manifesto lista os módulos).
- **Viés de seleção**: 27% dos candidatos que chegaram ao teste (222 de 822 no arquivo) caíram por estado incompleto. O corpus tende a código cujas dependências o histórico reconstrói; programas que importam módulo cujo estado de então é incognoscível ficaram de fora.
- **Tamanho do inline** 553: CP95 com 0/1/2 falsos = 0,54% / 0,85% / 1,13%. Arquivo 600: 0,50% / 0,79% / 1,05%. Ou seja, o teto de 1% admite no máximo **1 falso por balde** nos dois.
- O universo de candidatos de arquivo até o corte encolheu: 13.266 (09/10) → 12.944 (hoje), com o mesmo `corpus_v0d.py`; não investiguei a causa (hipótese: transcripts novos sujaram janelas «limpo» abertas).
- Inconsistência pequena: a seleção contou touring 72 / analise 518; pelo `project_root` das entradas são 71 / 519 (1 arquivo). Não investigada.

## 8. O que NÃO foi verificado, e o que é do Gabriel

- **Decisão do Gabriel**: A (bruta) ou B (bruta menos adjudicados, com a bruta ao lado). Sem decisão vale A. O adjudicador só toca o resultado depois do P0, com `--juiz-json` e `--corpus`.
- Nenhuma taxa de falso deny, recall nem p95 do 20261013 — selado. O adjudicador nunca viu dado real; sua taxa de acerto sobre rótulos humanos é desconhecida (só provei que cada regra é exercida e que errar uma a derruba).
- Que o motor com o manifesto completo se comporte no corpus como na fixture (a fixture prova o mecanismo para `from pkg import mod`; os 130 casos reais não foram julgados).
- A profundidade 2 (§7) e a política de fragmento (`.py` em `/tmp` que não é programa) ficam abertas.
- Só Python; estrato tmp ausente; a independência que o CP95 supõe não vale com 152 sessões e 508 arquivos (93% das entradas são do `analise`: 519 de arquivo e todo o inline, que roda em `analise`).
- Os manifestos dos corpora antigos (vivo, 20261010, 20261011) NÃO foram regravados com a regra nova; o módulo de estado agora produz overlays a mais se alguém os reconstruir.
- A primeira checagem de sobreposição com `--contra` extra foi barrada pelo sandbox (`redirection`) e refeita com `TOURING_CODE_MODE=native`; nenhum bypass `TOURING_GATE_OK` foi usado.

## Arquivos desta pasta

`congelados.json` · `selo.json` · `holdout_meta_20261013.json` · `construir_disjunto_20261013.log` · `dryrun_construtor.log` · `tdd_red.log` · `tdd_green.log` · `teste_construtor_ho13.log` · `mutantes_estado_no_instante.{log,json}` · `mutantes_adjudica_rotulo.{log,json}` · `prova_motor_fixture.log` · `sobreposicao_20261013{,_extras}.{out,json}` · `confere_selo_antes.log` · `integridade_final.json`.
Scripts: `B/scripts/{adjudica_rotulo,estado_no_instante_v0d,test_estado_no_instante_v0d}.py` · `I/{construir_disjunto,exclusoes_disjunto,verificar_sobreposicao,selar_holdout,integridade_ho13,mutantes_estado_no_instante,mutantes_adjudica_rotulo,test_construir_disjunto_ho13}.py`.
