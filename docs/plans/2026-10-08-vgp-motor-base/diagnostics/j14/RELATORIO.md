---
type: Report
title: "J14 — 30.4.101 no held-out 20261031b e camada 2 fria × quente"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187::J14
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:vgp", "#status:measured"]
---

# J14 — vereditos frios e medição no daemon quente

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## 1. Corpus e binários

O held-out `corpus_holdout_disjunto_20261031b` está selado com `sha_tree e8d8016f45bff2c5` (2.393 arquivos, 15.749.853 bytes). O selo conferiu antes do J14, depois do J14 e depois da medição quente. As sobreposições com vivo, temporal e todos os held-outs anteriores são **zero nas quatro chaves**, com controles positivos ativos. Detalhes: `../holdout-20261031/RELATORIO-HELDOUT-20261031B.md`.

| Binário | Papel | sha256 |
|---|---|---|
| `a6b9a550` (`~/.claude/touring/vgp/binarios/a6b9a550/touring`) | Touring 30.4.101 | `d064e1ebef33f4ace9a06e5edbdad4582b062adef0f2a1a01e814900390f4159` |
| `8fd0efb0` | Controle de discriminação | `8fd0efb0e62e64a882380d109e922ea4635daca7ff24321b1232b9382d4c96d7` |

O juiz foi `B/scripts/juiz_motor_vgp.py` do HEAD, com `--exigir-limite-superior`, critério A (contagem bruta), CP95 unilateral ≤ 1% por balde e p95 ≤ 1.000 ms. O corpus híbrido teve **1.064 chamadas**: 1.000 negativos do held-out + 64 positivos vivos usados apenas como controle de liveness.

## 2. Vereditos do juiz

| Binário | Modo | Recall símbolo (arquivo/inline) | Falsos denies | CP95 arquivo / inline | p95 | Timeouts L2 | Exit / veredito |
|---|---|---|---|---|---:|---:|---|
| `a6b9a550` | v2 | 11/13 · 21/27 | 0/600 · 1/400 | 0,50% · **1,18%** | 64,8 ms | 0/1064 | **1 — NÃO PRONTO** |
| `a6b9a550` | V3 | 12/13 · 23/27 | 1/600 · 1/400 | 0,79% · **1,18%** | **1.014,9 ms** | 74/1064 (6,95%) | **1 — NÃO PRONTO** |
| `8fd0efb0` | v2 | 11/13 · 21/27 | 2/600 · 2/400 | **1,05%** · **1,57%** | 68,5 ms | 0/1064 | **1 — NÃO PRONTO** |
| `8fd0efb0` | V3 | 12/13 · 23/27 | 10/600 · 2/400 | **2,81%** · **1,57%** | **1.009,7 ms** | 56/1064 (5,26%) | **1 — NÃO PRONTO** |

Nenhum run ficou INCONCLUSIVO: os timeouts ficaram abaixo do patamar 0,20 e o sinal da camada 2 esteve presente. O `camada2_sem_veredito` foi 165 no `a6b9a550` V3 e 147 na referência V3. A referência discrimina: o binário novo reduz falsos denies, mas o caso inline compartilhado `2106f0ebf279` (`exit`) permanece e, com n=400, um único falso deny já eleva CP95 a 1,18%.

## 3. Frio × quente

### 3.1 Método

- Frio: `touring vgp check` por processo, como no J13/J14; `elapsed_ms` do motor.
- Quente: `touring-hook` no daemon global 30.4.101, pelo mesmo corpus híbrido. Duas chamadas consecutivas por entrada: a primeira aquece, a segunda mede. `elapsed_ms` da sombra VGP (L1+L2), p95 por posto mais próximo. `pre-write` para arquivos e `pre-bash` para inline, com comandos inline reconstruídos dos snapshots integrais.
- Exclusividade: `scripts/hw-slot --class medida`. O trace comprova a rota; sinal ausente não conta como zero.
- Resultado válido: **1.064/1.064 medições com sombra e camada 2, 2.128/2.128 rotas daemon** (`hot_reparado.json`, `hot_reparado.trace.jsonl`).

### 3.2 Resultado

| Medida | Frio `a6b9a550` V3 | Quente consolidado |
|---|---:|---:|
| Entradas | 1.064 | 1.064 |
| p50 | não reportado aqui | 18,14 ms |
| **p95** | **1.014,9 ms** | **252,48 ms** |
| p95 wall do shim | não se aplica | 514,18 ms |
| Timeouts L2 | 74 (6,95%) | 15 (1,41%) |
| `camada2_sem_veredito` | 165 | 106 |

O p95 quente é aproximadamente **4,02× menor** (−762,4 ms, −75,1%) que o frio. Isso evidencia o custo do caminho CLI frio, mas **não muda o veredito do juiz**: CP95 e o critério pré-registrado continuam reprovando o binário.

### 3.3 Anomalias e reparo, sem apagar evidência

1. A primeira tentativa ignorou 40 positivos sem campo `extrator` (`hot_daemon_1024.*`). O fallback `<string> → c` e `<stdin> → heredoc` foi corrigido e testado; a saída parcial foi preservada, não usada no aceite.
2. Na repetição completa, um aquecimento caiu em standalone na posição 268 (`59d7d74321ed`, 4.228 ms); o run ficou INCONCLUSIVO e foi preservado em `hot_daemon.*`. A entrada 134 foi repetida isoladamente (`hot_reparo_134.*`), com duas rotas daemon e 48,85 ms. `consolidar_reparo_quente.py` substituiu o par inteiro, manteve a anomalia no relatório e produziu `hot_reparado.*`, com 2.128/2.128 rotas daemon.

### 3.4 Limites da comparação

O hook não aceita `--overlay` nem `--disk-file`: entradas, paths, roots e conteúdos são os mesmos, mas overlays e disco histórico não são replayed pelo hook. Portanto a comparação mede **latência do caminho real do daemon**, não equivalência semântica por id com o juiz. A fumaça sintética prévia cobriu arquivo, `c`, heredoc e `touring_run`: 4/4 entradas, 8/8 rotas daemon, zero timeouts.

## 4. Conclusão

**30.4.101 (`a6b9a550`) NÃO ESTÁ PRONTO no critério A.** Em v2, 1/400 falso deny inline já ultrapassa CP95; em V3, permanece esse limite e o p95 frio excede 1.000 ms. O caminho quente reduz substancialmente a latência, mas não altera os falsos denies nem o critério de aceite.

Instrumento e dados ficaram separados nos commits `443564a3`, `1256b7f0` e `b4ef21c8`. Este diretório contém apenas metadados, saídas de juiz, traces e números; nenhum conteúdo de corpus foi commitado.
