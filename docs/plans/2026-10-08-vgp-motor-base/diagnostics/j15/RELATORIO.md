---
type: Report
title: "J15 — 30.4.102 no held-out 20261101 e latência do daemon quente"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187::J15
okf_version: 1
timestamp: 2026-11-01
tags: ["#kind:report", "#domain:vgp", "#status:measured"]
---

# J15 — veredito pelo critério V3-QUENTE

## 1. Objeto e integridade

O held-out `corpus_holdout_disjunto_20261101` permaneceu intacto, com `sha_tree 6c228591f1f404f5`; o selo conferiu antes e depois dos runs (`selo_antes.out`, `selo_depois.out`). O híbrido contém 1.000 negativos selados e 64 positivos vivos de liveness, totalizando 1.064 chamadas do juiz.

| Binário | Papel | sha256 |
|---|---|---|
| `30.4.102` | Candidato | `404106ccdb32edb4329036291cb8cb55660adbbc9c7a8fc835237a9f148e3ef7` |
| `8fd0efb0` | Controle | `8fd0efb0e62e64a882380d109e922ea4635daca7ff24321b1232b9382d4c96d7` |

O reboot apagou o híbrido em `/tmp`, sem apagar os resultados já gravados. Para executar somente os passos ausentes, ele foi reconstruído do corpus selado e conferido por igualdade: `hibrido_sha_tree.txt` e `hibrido_sha_tree_recuperado.txt` contêm ambos `e29d070091150655`. Nenhum run frio concluído foi refeito.

## 2. Resultados frios

O p95 frio é informativo neste J15; o aceite de latência usa o daemon quente pré-registrado.

| Binário | Modo | Recall símbolo (arquivo/inline) | Falsos denies (arquivo/inline) | CP95 arquivo / inline | p95 frio | Timeouts L2 | Resultado do run |
|---|---|---|---|---|---:|---:|---|
| `30.4.102` | v2 | 11/13 · 21/27 | 0/600 · 0/400 | 0,50% · 0,75% | 75,123 ms | 0/1.064 | rc 0 — PRONTO |
| `30.4.102` | V3 | 12/13 · 23/27 | 1/600 · 0/400 | 0,79% · 0,75% | 1.049,523 ms | 152/1.064 (14,29%) | rc 1 — pendências |
| `8fd0efb0` | v2 | 11/13 · 21/27 | 3/600 · 1/400 | 1,29% · 1,18% | 75,483 ms | 0/1.064 | rc 1 — NÃO PRONTO |
| `8fd0efb0` | V3 | 12/13 · 23/27 | 3/600 · 1/400 | 1,29% · 1,18% | 1.119,7 ms | 522/1.064 (49,06%) | rc 3 — INCONCLUSIVO |

No candidato V3, os CP95 permanecem abaixo do teto de 1% e a taxa de timeouts permanece abaixo do patamar de 20%. A pendência fria de p95 não foi usada como gate. O controle V3 excedeu o patamar de timeouts e, por isso, não oferece uma comparação V3 conclusiva; seu resultado foi preservado sem repetição ou reinterpretação. A tentativa anterior, interrompida enquanto aguardava a temperatura baixar de 95 °C e sem JSON, está preservada em `8fd0efb0_v3.interrompido.out`.

## 3. Natureza do falso deny do candidato

O único falso deny de arquivo do `30.4.102` V3 é o id **`10607309d143`**. Em `30_4_102_v3.porid.json`, o registro tem `layer: 2`, `verdict: deny` e o finding `atributo calculate_irr`, com a mensagem de que esse não é um atributo conhecido de `kazuba_agents.rust_bridge`.

Portanto, é um **deny por finding de mérito do motor, não um timeout da camada 2**. Não há evidência que permita classificá-lo como ruído térmico ou removê-lo do comparativo. A consolidação mantém corretamente a pendência `falso deny (arquivo) piorou: 1 > base 0`.

## 4. Medição no daemon quente

A medição seguiu `PREREGISTRO-V3-QUENTE-2026-10-31.md`: duas chamadas consecutivas ao `touring-hook` por entrada, a primeira para aquecimento e a segunda medida; `elapsed_ms` da sombra VGP (L1+L2); p95 por posto mais próximo; execução exclusiva sob `hw-slot --class medida`.

| Sinal | Resultado |
|---|---:|
| Entradas esperadas | 1.064 |
| Medições com sombra | 1.064/1.064 |
| Medições com camada 2 | 1.064/1.064 |
| Rotas daemon | 2.128/2.128 |
| Entradas ignoradas | 0 |
| p50 do motor | 20,209 ms |
| **p95 do motor** | **337,332 ms** |
| p95 wall do shim | 621,247 ms |
| Timeouts L2 | 19/1.064 (1,79%) |
| `camada2_sem_veredito` | 106 |
| Instrumento | válido — OK |

**O critério de latência passa: 337,332 ms ≤ 1.000 ms.** Não houve anomalia de rota ou denominador a reparar.

Limitação obrigatória: o hook não aceita `--overlay` nem `--disk-file`. Entradas, paths, roots e conteúdos são os mesmos, mas overlays e disco histórico não são reproduzidos pelo hook. A medição vale para a latência do caminho real do daemon, não para equivalência semântica por id com o juiz.

## 5. Veredito

`aceite.json`, produzido por `consolidar_j15.py`, fecha sem motivos de inconclusividade e com uma única pendência:

> **Touring 30.4.102: NÃO PRONTO — o V3 tem 1 falso deny de arquivo contra 0 na base v2.**

O p95 quente aprovado não elimina essa regressão comparativa. O p95 frio acima de 1.000 ms permanece apenas informativo, conforme o pré-registro.

## 6. Validação e preservação

- `python3 -I -m pytest diagnostics/j15/test_consolidar_j15.py -q`: **9 passed**.
- Selo depois: rc 0, `sha_tree 6c228591f1f404f5` intacto.
- Consolidação final: rc 1, esperado para `NÃO PRONTO`; nenhum motivo de inconclusividade.
- `rcs.txt` preserva os rc individuais, a recuperação do híbrido, a interrupção anterior e o encerramento.
- Nenhum daemon foi reiniciado ou spawnado manualmente; nenhum build foi executado; nenhum conteúdo de corpus foi adicionado a este diretório.
