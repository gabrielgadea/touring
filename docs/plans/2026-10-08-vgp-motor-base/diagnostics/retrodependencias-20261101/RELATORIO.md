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

O resolvedor corrigido (`ab256a5b`) reconstruiu os módulos locais pelo Git no `ts`. A comparação final executou **os dois lados** com camada 1 do binário 30.4.102 e camada 2 com orçamento declarado de **30.000 ms**. A biblioteca `touring-pyright` está idêntica ao release; o instrumento externo usa `CARGO_TARGET_DIR` próprio e `PyrightSession::with_budget(30s)`, sem alterar produção. Fonte versionada em `orcamento/src/main.rs`, sha256 `f6e90a5541d4a5b29853125ca024006c2e9e90b9d74936c602e119b45fa30f02`.

A flag `--timeout-ms` foi testada e **não** altera o orçamento da camada 2; por isso o instrumento direto foi necessário.

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

O id `10607309d143` está entre os 119 alterados e os 109 reconstruídos. Na medição final:

- estado atual: **deny por mérito**, camada 2 completa, `motivo = None`, 1.256 ms;
- estado histórico: **não-deny por mérito**, camada 2 completa, `motivo = None`, 3.104 ms;
- 318 overlays, 76 dependências alteradas, zero faltantes.

**Controle positivo: PASSA — `deny → não-deny` com mérito nos dois lados.** Não é o `allow` fail-open de 1 s observado na primeira tentativa.

## Contagens por classe

| Classe do par atual × histórico | n |
|---|---:|
| **Mérito × mérito** | **98/109** |
| — estáveis | 97 |
| — **`deny → não-deny`** | **1** |
| — `não-deny → deny` | 0 |
| `não-deny → INCONCLUSIVO-TIMEOUT` | 6 |
| `unknown_outro → unknown_outro` | 5 |
| **Total** | **109** |

Os 6 timeouts históricos persistiram mesmo com 30 s e ficam fora do denominador de mérito. Os 5 pares `unknown_outro` pertencem ao projeto `touring`, sem `pyright-langserver` no venv. O instrumento original fazia `expect` nessa ausência e produzia `JSONDecodeError`; após a correção e a reexecução dos 5, todos vieram como `unknown` JSON estruturado, com o motivo explícito de ausência do servidor. Nenhum foi promovido a mérito: o denominador final permanece **98**. Nenhum desses 11 pares é contado como estabilidade ou mudança de mérito.

## Interpretação

Entre os 98 pares com mérito nos dois lados, houve **1 mudança de veredito efetivo**, exatamente o caso que motivou a investigação. O valor anterior **0/39** foi substituído porque misturava estados com camada 2 incompleta; ele não é mais usado.

A exposição estrutural é de 119 entradas e a mudança comprovada é de 1/98 pares de mérito. Os 11 casos não conclusivos impedem extrapolar essa taxa para os 109 sem ressalva.

## Validação

- TDD do resolvedor: RED 3 → GREEN 4; 56 testes; 23/23 mutantes mortos.
- Instrumento retroativo: 9 testes pytest + 6 asserções de selftest; Diamond 0,961, 0 blockers; clippy externo `-D warnings` verde.
- Reconstrutor: Diamond 0,964, 0 blockers.
- Agregação independente: 98 pares de mérito = 97 estáveis + 1 mudança; 6 timeouts + 5 `unknown_outro` = 109.

Pacotes de terceiros não foram congelados. Mudança de veredito mede sensibilidade da reconstrução, não verdade do rótulo. Nenhum conteúdo do corpus foi incluído neste diretório.
