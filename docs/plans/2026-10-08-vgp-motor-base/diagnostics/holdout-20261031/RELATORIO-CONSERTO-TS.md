---
type: Report
title: "Conserto do instrumento — proveniência de timestamps no held-out 20261031b"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:harness", "#status:verified"]
---

# Conserto do instrumento — timestamps e `api-request-blob`

## 1. Causa provada na fonte

Foram abertos o caso de arquivo `ce80fdd90113` e casos dos extratores `touring_run`, `heredoc` e `c`. Em todos, **o instante existe no transcript**, portanto a hipótese “fonte sem instante” foi descartada para esses casos.

| Caso | Forma na fonte | Instante encontrado |
|---|---|---|
| `ce80fdd90113`, arquivo | Mesmo `tool_use_id` em `assistant` e `api-request-blob`; o blob não tem timestamp | `2026-10-27T14:22:17.466Z` |
| `6035fa7e3c43`, `touring_run` | Registros `assistant` e `user` datados, mais blobs/attachments | `2026-10-24T08:08:25.624Z` |
| `d6971a22f1db`, `heredoc` | Subagente com `assistant` e `user` datados | `2026-10-23T22:53:29.103Z` |
| `25373e6c6ac6`, `c` | Subagente com `assistant` e `user` datados | `2026-10-24T04:13:25.304Z` |

Essas datas são posteriores ao corte `2026-10-09T18:14:31.894Z`. O `ts=""` não apenas derrubava `estado_no_instante_v0d.py`: fazia entradas posteriores ao corte atravessarem o filtro temporal. A desduplicação ordenava a cópia vazia primeiro e descartava a datada como duplicata.

## 2. Correção mínima

- `diagnostics/instrumentos-2026-10-09/construir_disjunto.py`:
  - agrupa o universo inline por `tool_use_id` e escolhe a **menor data não vazia**; assim nenhum blob vazio vence uma cópia datada, e uma duplicata posterior ao corte não esconde uma elegível;
  - registro realmente sem instante vira exclusão contada como `ts_ausente`;
  - o cache de arquivo repete a barreira: `ts=""` nunca entra em `candidatos_ate_o_corte`, e a exclusão fica no meta.
- `scripts/corpus_v0d.py`: `universo_negativos` preserva o evento datado e exclui apenas o candidato vazio, contando `ts_ausente`.

`estado_no_instante_v0d.py`, juiz, `pyright.rs` e adjudicador não foram alterados. O adjudicador continua `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`.

## 3. TDD e mutantes

| Prova | Resultado | Evidência |
|---|---|---|
| RED antes do conserto | **4 failed** — preferência pela cópia vazia, inline sem ts aceito, cache sem barreira, arquivo com blob aceito | `tdd_red.log` |
| GREEN | **4 passed** | `tdd_green.log` |
| Mutantes em cópias temporárias | **4/4 mortos**, todos `rc=1` por asserção | `mutantes_ts_ausente.json`, `mutante_*.log` |
| Regressão construtor + estado | **52 passed** | `regressao_construtor_estado.log` |
| Selftest `corpus_v0d.py` | **11 ok** | `regressao_corpus_v0d.log` |

Mutantes: remover a preferência pela data, aceitar inline sem ts, remover a barreira do cache e aceitar arquivo sem ts. Na primeira execução do runner, os quatro deram `rc=4` por `ModuleNotFoundError`; esse resultado foi **descartado**, não contado como mutante morto. O runner passou a copiar as dependências e a exigir `rc=1`.

## 4. Destino seguinte

O parcial `corpus_holdout_disjunto_20261031` permanece abortado, sem selo e sem motor/juiz. A reconstrução usa destino e cache novos: `corpus_holdout_disjunto_20261031b` e `cache_universo_20261031b.json`, com a mesma semente `20261031`, o mesmo corte e o mesmo adjudicador. O congelamento `congelados_20261031b.json` inclui os shas dos dois arquivos corrigidos e adiciona `corpus_v0d.py` às dependências obrigatórias.
