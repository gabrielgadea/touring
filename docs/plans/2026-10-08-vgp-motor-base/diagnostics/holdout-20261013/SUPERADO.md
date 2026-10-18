---
type: Diagnostic
title: Held-out 20261013 — SUPERADO pelo 20261013f
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T05:30:00Z
autor: instrumentos-p1
tags: ["#kind:diagnostic", "#domain:harness", "#status:superseded"]
---

# 20261013 foi substituído pelo 20261013f

**Estado**: construído e selado em 10/10/2026 (selo `8ce465ddeb03f500`, `selo.json` ao lado). **Nunca aberto**: nenhum motor
nem juiz rodou nele, e o selo segue conferível por `scripts/confere_selo.py`. O corpus em
`~/.claude/touring/vgp/corpus_holdout_disjunto_20261013` **não foi alterado nem apagado**. O substituto é
`~/.claude/touring/vgp/corpus_holdout_disjunto_20261013f` (relatório em `../holdout-20261013f/RELATORIO.md`), construído com a
mesma semente (20261013) e os mesmos parâmetros. **A remoção deste corpus é decisão do Gabriel.**

## Por que foi substituído (dois defeitos de construção, achados antes de qualquer juiz abri-lo)

1. **Estado no instante só até a profundidade 1.** O manifesto sobrepunha os módulos que a ENTRADA importa, mas o motor segue
   os imports DENTRO dos módulos sobrepostos: um `pkg/__init__.py` que reexporta `from .nucleo import f` faz o motor ler o
   `nucleo.py` DE HOJE. A própria medição do 20261013 já mostrava a lacuna, só que como dado e não como regra: 91 das 1.153
   entradas e 717 módulos de profundidade 2 ficaram fora do manifesto (`holdout_meta.json` → `estado_no_instante`,
   `entradas_com_importacao_de_profundidade_2_nao_coberta` e `modulos_de_profundidade_2_nao_cobertos`). É a mesma classe do
   falso `6a25b00f2584` do P0M (módulo julgado contra o arquivo de hoje). O 20261013f exige o FECHO transitivo: todo módulo
   do projeto alcançável pelos imports, em qualquer profundidade, no conteúdo do instante.
2. **Duas exclusões faltavam.** `corpus_v0d.bak-20261009-pre` e `corpus_v0d_staging_evidencia` não estavam em
   `--excluir-corpus`. Medido sobre as quatro chaves (ids, tool_use_ids, sha do conteúdo+contexto, comandos): **1 entrada do
   20261013 já estava no `bak-20261009-pre`** (`fe5620ecee0a`, negativo de arquivo `norma_corpus.py`, mesmo id e mesmo sha do
   conteúdo); o `staging_evidencia` tem 0 em comum. Uma entrada que um corpus anterior já mostrou ao motor não serve de
   held-out.

## O que NÃO muda

O adjudicador de rótulo (`scripts/adjudica_rotulo.py`) continua congelado com o mesmo sha
(`fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`); a escolha do critério (A bruta | B adjudicada) segue sendo do Gabriel.
As versões dos instrumentos com que este corpus foi construído estão em `../holdout-20261013f/instrumentos_do_20261013/`
(cada arquivo conferido contra o sha de `congelados.json`).
