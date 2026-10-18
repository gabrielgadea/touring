---
type: Diagnostic
title: P0M — binário do P0 (f55ec4b5) no held-out disjunto 20261011 selado
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T04:40:00Z
autor: touring-09 (coordenadora)
tags: ["#kind:diagnostic", "#domain:harness", "#status:active"]
---

# P0M — veredito, classificação dos falsos e o que vem depois

## 1. Veredito (critério PRÉ-REGISTRADO: CP95 unilateral ≤ 1% por balde, `--exigir-limite-superior`)

**NÃO PRONTO.** Selo `c495bcab66718230` conferido antes e depois por `scripts/confere_selo.py`, com o
mesmo método que o selou e um selftest em que 1 byte trocado reprova.

| binário · camadas | falso deny arquivo | falso deny inline | recall (controle de liveness) | avisos em negativos | p95 |
|---|---|---|---|---|---|
| f55ec4b5 (P0) · v2 | 2/600 · CP95 1,05% | 2/600 · 1,05% | 11/13 · 21/27 | 73 | 132,8 ms |
| e935aaf1 (base) · v2 | 2/600 · 1,05% | 3/600 · 1,29% | 11/13 · 21/27 | 81 | 132,5 ms |
| f55ec4b5 · V3 | 4/600 · 1,52% | 5/600 · 1,74% | 12/13 · 23/27 | 76 | 1.013,8 ms |
| e935aaf1 · V3 | 4/600 · 1,52% | 6/600 · 1,96% | 12/13 · 23/27 | 84 | 1.016,9 ms |

Corpus híbrido (procedimento de `diagnostics/holdout/RELATORIO.md` §3.1): 600 negativos de arquivo + 600 inline do
held-out, mais 64 positivos do vivo usados só como controle de liveness. 1.264 chamadas em cada juiz. A tentativa 1,
sobre o held-out puro, deu 0 chamadas («corpus degenerado») e está guardada em `tentativa1-corpus-puro/`.
Script: `rodar_p0m2.sh`. Saídas: `{f55ec4b5,e935aaf1}_{v2,v3}.{out,json}`, `classif_*.json`.

**O held-out discrimina**: a base também reprova. O P0 tira 1 falso inline por camada e 8 avisos em negativos.
Os falsos que sobram são de outras naturezas.

## 2. Cada falso do f55ec4b5, por natureza (adjudicação DIAGNÓSTICA — não muda o veredito acima)

| id | balde · camada | achado | natureza | evidência |
|---|---|---|---|---|
| b9d26a878f4c | arquivo · 1 | nome `srv` (L90) | **rótulo**: bug latente real | `srv.shutdown()` dentro de `finally:`; `srv` tem 1 ocorrência no arquivo e nunca é ligado. O `finally` sempre roda, então dá NameError. O rótulo «negativo» do corpus significa «nenhum erro observado na sessão», não «correto em runtime» (`corpus_v0d.py`, origem `atribuido`/tmp) |
| 9a8eed8a7502 | arquivo · 1 | nome `Path` (L12/23/30) | **política**: fragmento, não programa | `scratchpad/exec/trecho_g3.py` é trecho para colar num teste («De scripts/tests/…»). `Path`, `_TEXTO_FERROVIARIO_ANTIGO` e `invoke_or_subprocess` vêm do arquivo de destino. Se rodasse, quebraria; não rodou. A P4 presume que `.py` em `/tmp` «roda agora» |
| 636f9a7adec1 | inline · 1 e 2 | atributo `folium.__file__` | **motor (camada 1)** | todo módulo carregado tem `__file__`, `__name__`, `__doc__`, `__spec__`, `__loader__`, `__package__` (+ `__path__` em pacote); o motor só vê o que o `__init__.py` liga |
| 6a25b00f2584 | inline · 1 e 2 | atributo `pm._publicar` | **corpus**: estado no instante incompleto | `from scripts.eleitoral import publicar_mapa as pm`; o manifesto sobrepõe só `scripts/eleitoral/__init__.py`, e não o submódulo `publicar_mapa.py` do instante. Hoje a função se chama `_publicar_e_conferir` (analise `4eeb869da6`) |
| eb2d7b7ba5b9 | arquivo · 2 | membro `no.lineno`, `no.end_lineno` | **motor (camada 2)** | `ast.AST` não declara `lineno` no typeshed; as subclasses têm. Erro de TIPO do pyright, não inexistência |
| b4aeefe5820e | arquivo · 2 | atributo `ssl._ssl._test_decode_cert` | **motor (camada 2)** | membro privado ausente do stub; existe em runtime |
| d85ca025881a | inline · 2 | membro `n.value.args` | **motor (camada 2)** | `Assign.value: expr`, sem `.args` no tipo; em runtime é `Call` |
| f01b4181af57 | inline · 2 | atributo `jsonschema.__version__` | **motor (camada 2)** | servido por `__getattr__` de módulo; o stub não declara |
| eb898e2aa211 | inline · 2 | nome `cr` | **motor (camada 2)** | ligado por `exec(open('contraste.py').read()…)`. A camada 1 ABSTEVE (dinâmico) e a camada 2 negou por cima |

Resumo: motor 7 (1 na camada 1, 6 na camada 2 contando o `folium.__file__` nas duas), corpus 1, rótulo 1, política 1.

## 3. Consequências

1. **P0B (motor)**: (a) atributos implícitos de módulo na camada 1, pela referência do modelo de dados do Python;
   (b) camada 2 só nega inexistência DEFINITIVA:
   - `reportAttributeAccessIssue` sobre objeto que não é módulo vira aviso, porque tipo estático ≠ existência;
   - em módulo de terceiros com membro privado, dunder ou `__getattr__`, abstém salvo confirmação da camada 1 no fonte real;
   - um sinal dinâmico da camada 1 (`exec`/`eval`/`globals()`/`*`) faz a camada 2 abster.

   Tudo com TDD e mutantes. Sítio: a política da camada 2 depois da integração da VL0 e da L2 (INT97).
2. **Held-out novo 20261013** (este foi GASTO no P0): disjunto do vivo, do 20261010 e do 20261011, com duas
   correções de CONSTRUÇÃO PRÉ-REGISTRADAS antes de qualquer motor rodar:
   - (i) o estado no instante sobrepõe TODO módulo do projeto que a entrada importa, inclusive `from pkg import mod`, e
     uma entrada sem esse estado é INCOMPLETA e fica fora, com a contagem do motivo;
   - (ii) regra de adjudicação congelada (script com sha) para falsos de RÓTULO: o nome nunca ligado no arquivo
     autocontido (sem import `*`, sem `exec`/`eval`/`globals()`, sem `__getattr__`), num ponto que executa
     incondicionalmente, é bug latente, reportado à parte. **Qual contagem decide é decisão do Gabriel**, tomada ANTES de
     abrir o 20261013: (A) a bruta, como hoje; ou (B) a bruta menos os falsos de rótulo adjudicados pelo script
     congelado, com a bruta sempre impressa ao lado. Sem decisão, vale (A).
3. **Política de fragmento** (`.py` em `/tmp` que não é programa): decisão do Gabriel. A P4 de hoje presume execução.
4. O p95 do V3 (1.013 a 1.017 ms nos dois binários) é ambiental: 96 °C e fila do hw-slot. Remedir com a máquina fria.
