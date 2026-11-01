---
type: PreRegistration
title: "V3-QUENTE — latência da camada 2 pelo daemon que atende o hook"
description: "Pré-registro do critério de latência V3 pelo caminho real do daemon quente, válido a partir do próximo held-out após 20261031b. Não reinterpreta o J14."
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187
okf_version: 1
timestamp: 2026-10-31
---

# PRÉ-REGISTRO — V3-QUENTE (31/10/2026)

> Escrito antes de qualquer nova medição. Decisão de Gabriel, transmitida pela touring-00:
> a latência V3 passa a ser julgada pelo **caminho do daemon quente usado pelo hook**, com o
> limite de **1.000 ms** mantido. O caminho CLI frio continua sendo reportado como informação.

## 1. Alcance temporal

Este critério vale **a partir do próximo held-out depois do 20261031b**. O J14 permanece
historiado pelo critério A anterior: frio como juiz e quente como investigação complementar.
Nenhum veredito anterior é reescrito por este documento.

## 2. Método congelado

Replica o método validado em `diagnostics/j14/hot_reparado.*`:

1. O corpus híbrido fica no scratchpad: negativos do held-out selado e positivos vivos apenas
   como controle de liveness. O denominador é o número de chamadas do juiz no mesmo híbrido.
2. Para cada entrada, o instrumento faz **duas chamadas consecutivas** ao `touring-hook`:
   a primeira aquece a sessão do language server e a segunda mede. `pre-write` para arquivo e
   `pre-bash` para inline. O comando inline é reconstruído do snapshot integral.
3. A medição usa `elapsed_ms` da sombra VGP (L1+L2), não o tempo externo do shim. O wall time
   do shim é reportado separadamente. p50/p95 usam **posto mais próximo**, sem interpolar:
   `ordenados[ceil(q*N)-1]`.
4. Todo o trabalho pesado passa por `scripts/hw-slot --class medida`, com espera explícita.
   O trace (`TOURING_HOOK_TRACE_FILE`) comprova a rota. Ausência de sombra, de camada 2 ou de
   rota daemon torna o resultado **INCONCLUSIVO**, nunca zero nem PRONTO.
5. Uma anomalia só pode ser reparada pela repetição da entrada afetada, preservando a saída
   original, o motivo, a posição no trace e o par substituído. A consolidação precisa fechar o
   mesmo denominador integral.

## 3. Denominador e sinais obrigatórios

O resultado válido exige, para o mesmo corpus híbrido do juiz:

- `medidas_com_sombra == chamadas_do_juiz`;
- `medidas_com_camada2 == chamadas_do_juiz`;
- `rotas_daemon == 2 * chamadas_do_juiz` (aquecimento + medição);
- nenhuma entrada ignorada sem motivo e denominador declarado;
- timeouts e `camada2_sem_veredito` reportados com seus numeradores e denominadores.

## 4. Critério

**p95 quente ≤ 1.000 ms.** O p95 frio é informativo e não pode reprovar sozinho. Os demais
critérios de recall e falso deny não mudam. O critério de timeouts da camada 2 do juiz também
permanece: taxa > 0,20 ou sinal ausente ⇒ INCONCLUSIVO.

## 5. Limitação declarada

O hook não aceita `--overlay` nem `--disk-file`. Entradas, paths, roots e conteúdos são os
mesmos, mas overlays e disco histórico não são replayed pelo hook. Portanto a medição vale para
**latência do caminho real do daemon**, não para equivalência semântica por id com o juiz.
Essa limitação deve aparecer em todo relatório que use este critério.
