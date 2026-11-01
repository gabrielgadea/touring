---
type: Report
title: "Retroativo 20261101 — deriva de dependências locais, mérito × mérito"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793544410603554012::retroativo
okf_version: 1
timestamp: 2026-11-01
tags: ["#kind:report", "#domain:vgp", "#status:measured"]
---

# Diagnóstico retroativo do held-out 20261101

## Escopo

Medição de sensibilidade da reconstrução, sem novo veredito de release, sem reclassificação da verdade dos rótulos e sem reescrever J14/J15. População: 600 entradas de arquivo + 400 inline.

O resolvedor `8d166db3` reconstruiu os módulos locais pelo Git no `ts`. A comparação final executou **os dois lados pelo caminho do produto**, com camada 1 do binário 30.4.102 e camada 2 com `--l2-budget-ms 30000`. O campo `l2_budget_ms` foi conferido em todas as 109 comparações. O binário debug foi compilado no `target/` normal de desenvolvimento, sem release e sem deploy.

## Dependências alteradas

| Medida | Resultado |
|---|---:|
| Entradas | 1.000 (600 arquivo + 400 inline) |
| Dependências locais examinadas | 21.684 |
| Dependências diferentes entre `ts` e hoje | 5.146 |
| **Entradas com ao menos uma dependência alterada** | **119/1.000 (11,9%)** |
| Entradas INCONCLUSIVO-RECONSTRUÇÃO | 53/1.000 |
| Entradas alteradas reconstruíveis e comparadas | 109/119 |

Dez entradas alteradas não tinham fecho reconstruível e não foram ao motor.

## Controle positivo obrigatório

O id `10607309d143` está entre os 119 alterados e os 109 reconstruídos. Na medição final pelo produto:

- estado atual: **deny por mérito**, camada 2 completa, `motivo = None`, 1.073,3 ms, `l2_budget_ms = 30000`;
- estado histórico: **não-deny por mérito**, camada 2 completa, `motivo = None`, 1.638,1 ms, `l2_budget_ms = 30000`;
- 318 overlays, 76 dependências alteradas, zero faltantes.

**Controle positivo: PASSA — `deny → não-deny` com mérito nos dois lados.** Não é o `allow` fail-open de 1 s observado na primeira tentativa.

## Contagens finais por classe

| Classe do par atual × histórico | n |
|---|---:|
| **Mérito × mérito** | **104/109** |
| — estáveis | 103 |
| — **`deny → não-deny`** | **1** |
| — `não-deny → deny` | 0 |
| INCONCLUSIVO-TIMEOUT | **0** |
| `unknown_outro → unknown_outro` | 5 |
| **Total** | **109** |

O caminho do produto eliminou os 6 timeouts históricos do instrumento externo. Os 5 pares `unknown_outro` pertencem ao projeto `touring`, sem `pyright-langserver` no venv; todos têm esse motivo estruturado em JSON e ficam fora do denominador de mérito.

## Interpretação

Entre os 104 pares com mérito nos dois lados, houve **1 mudança de veredito efetivo**, exatamente o caso que motivou a investigação. Os valores intermediários 0/39 e 1/98 foram substituídos pela medição completa pelo produto e não são mais usados.

A exposição estrutural é de 119 entradas e a mudança comprovada é de **1/104 pares de mérito**. Os 5 casos sem language server impedem extrapolar essa taxa para os 109 sem ressalva.

## Validação

- TDD do resolvedor: RED 3 → GREEN 4; 56 testes; 23/23 mutantes mortos.
- Orçamento: default/env verde; pool com 1 ms classifica timeout; CLI registra flag/default no veredito; juiz passa a flag ao motor.
- Testes Python: 11 passed (9 retroativo + 2 juiz), incluindo as 6 asserções de selftest.
- Agregação independente: 104 pares de mérito = 103 estáveis + 1 mudança; 0 timeouts; 5 `unknown_outro` = 109; todas as comparações carregam `l2_budget_ms = 30000`.

Pacotes de terceiros não foram congelados. Mudança de veredito mede sensibilidade da reconstrução, não verdade do rótulo. Nenhum conteúdo do corpus foi incluído neste diretório.
