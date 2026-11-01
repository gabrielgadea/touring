---
type: Report
title: "Held-out 20261031b — identidade, selo, disjunção e viés de timestamps"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187::HOLD
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:harness", "#status:sealed"]
---

# Held-out 20261031b — selo e disjunção

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`. Artefatos em `B/diagnostics/holdout-20261031/`.

## 1. Identidade

| Item | Valor |
|---|---|
| Corpus | `~/.claude/touring/vgp/corpus_holdout_disjunto_20261031b` |
| Selo | `sha_tree e8d8016f45bff2c5` · 2.393 arquivos · 15.749.853 bytes |
| Conferência | `confere_selo.py`: selftest ok e `selo INTACTO` (rc 0), antes do J14 |
| Semente / corte | `20261031` · `2026-10-09T18:14:31.894Z` |
| Adjudicador | `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c` |
| Construtor | `a206a6d08f91970671346809daaad2decae3ff0265f0d275ac93d02a14cb4967` |
| Estado no instante | `cc4516d0c008d3fad41659ed3d8a506417c19d0aa24f85454e3aad25e7d87891` |
| Instrumento | commits `443564a3` e `1256b7f0` |
| Índices | `holdout_meta.json` `f3ff82d85da1…` · `entradas.jsonl` `7491e9a14060…` · `entradas_inline_neg.jsonl` `ea62a78847b4…` |

O parcial `corpus_holdout_disjunto_20261031` permanece abortado, sem selo e sem motor/juiz. O primeiro `b`, cujo meta perdia a contagem acumulada, foi preservado como `corpus_holdout_disjunto_20261031b-meta-pre-1256b7f0`. Na reconstrução, os 2.393 caminhos permaneceram e **somente `holdout_meta.json` mudou de conteúdo**.

## 2. Baldes e estado no instante

| Balde | Selecionado | Regra dos 300 |
|---|---:|---|
| Arquivo | **600** | passa |
| Inline agregado | **400** = c 200 + heredoc 200 + touring_run 0 + tmp 0 | passa |

Validação antes do selo (`validacao_pre_selo.json`): **0 entradas com `ts=""` e 0 posteriores ao corte**. Estado no instante: 1.000 entradas · 788 overlays · **0 incompletas** · 0 lacunas no verificador independente · fecho máximo 49 módulos. Vivo e temporal permaneceram intactos.

## 3. Viés do defeito e da correção

No parcial abortado, 317/1.134 entradas tinham `ts=""`. Por balde/extrator:

| Balde/extrator | Sem `ts` no parcial | Fração | Sem `ts` no 20261031b |
|---|---:|---:|---:|
| Arquivo | 53/600 | 8,83% | **0/600** |
| Inline `c` | 63/200 | 31,50% | **0/200** |
| Inline heredoc | 67/200 | 33,50% | **0/200** |
| Inline `touring_run` | 130/130 | **100%** | **0/0** |
| tmp escrito e executado | 4/4 | **100%** | **0/0** |

O universo de arquivo registra **1.966 exclusões `ts_ausente`**; o inline não deixou resíduo sem instante após a escolha da duplicata datada. Pools antes → depois da correção: `c` 2.083 → 1.653 · heredoc 2.350 → 1.987 · `touring_run` 616 → 461 · tmp 4 → 0. A redução total da seleção inline (534 → 400) é exatamente a perda de `touring_run` (−130) e tmp (−4); `c` e heredoc mantiveram as cotas.

A causa provada nos transcripts foi a duplicação `assistant`/`user` datados + `api-request-blob` sem data. A seleção antiga aceitava a cópia vazia e descartava a datada. A correção escolhe a menor data não vazia por tool id e conta o resíduo como `ts_ausente`.

## 4. Sobreposição nas quatro chaves

`sobreposicao_20261031b.{json,out}`, recomputado dos arquivos: ids, tool_use_ids, shas de conteúdo+contexto e comandos.

| Corpus comparado | ids | tool_use_ids | shas | comandos |
|---|---:|---:|---:|---:|
| Vivo `corpus_v0d` | 0 | 0 | 0 | 0 |
| Temporal `corpus_holdout` | 0 | 0 | 0 | 0 |
| 20261010 `corpus_holdout_disjunto` | 0 | 0 | 0 | 0 |
| 20261011 | 0 | 0 | 0 | 0 |
| `bak-20261009-pre` | 0 | 0 | 0 | 0 |
| `staging_evidencia` | 0 | 0 | 0 | 0 |
| `bak-2026-10-09` | 0 | 0 | 0 | 0 |
| 20261013 | 0 | 0 | 0 | 0 |
| 20261013f | 0 | 0 | 0 | 0 |

`OK_zero_em_todos_os_cruzamentos: true` · `controle_positivo_enxerga: true` · duplicatas internas de conteúdo: **0**. Cada corpus anterior também teve controle positivo contra si mesmo com as quatro chaves > 0.

## 5. Reprodutibilidade e privacidade

- Construção: `rodar_construtor_20261031b.sh`, rc 0, 358,2 s, sob `hw-slot`.
- Validação/selo/cruzamento: `selar_e_cruzar_20261031b.sh`, rc 0.
- O comando original abortado e o diagnóstico ficam em `rodar_construtor_20261031.sh` e nos logs deste diretório.
- Nenhum conteúdo de corpus foi copiado para o repositório: apenas metadados, selo, scripts e números. O J14 usa uma cópia híbrida no scratchpad; os positivos vivos entram só como controle de liveness.
