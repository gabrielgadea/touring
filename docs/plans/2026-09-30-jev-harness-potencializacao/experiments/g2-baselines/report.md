---
type: ExperimentReport
title: G2 — baselines de decisão no split de teste congelado (gate G2)
description: keyword × Jev × professor Claude × Laya sobre as 3 tarefas do corpus G1 (prompt_intent n=120, memory_kind n=73, recall_relevance n=45). Jev empata com o professor em memory_kind a 1/55 da latência e acerta 86,7% em recall — mas o Noul é grotescamente descalibrado (ECE 0,71) e o professor vence em intent. Laya skipped por EDQUOT (qgroup btrfs).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [g2, baselines, jev, intent, memory-kind, recall, ece]
timestamp: 2026-10-01T02:20:00-03:00
okf_version: "0.1"
---

# G2 — baselines (S-2.1/S-2.2)

Parte do [bundle](/index.md). Dados: [results.json](/experiments/g2-baselines/results.json).
Split congelado: `~/.claude/touring/g1-corpus/split.json` (seed 20260922, sha256 no
[relatório da touring-02](/experiments/split/report.md)). Split hash medido pelo harness:
`74c035469d9d1455732e6f537810e1557faf75dab16c9dc905410ae4a6c7409d` (blake2b do JSON do split).

Convenção: **[F]** número do results.json · **[I]** inferência · custo Jev calculado a
US$ 0,042/MTok de entrada (saída grátis).

## 1. A tabela

| Tarefa | current (hoje) | **Jev** | professor (Claude sonnet) | Laya |
|---|---|---|---|---|
| prompt_intent (n=120) | **0,536** (produção, n=97 humanos; 23 payloads de máquina corretamente recusados pelo G0) | **0,533** (n=120, inclui os 23) · ECE 0,116 · p50 370 ms | **0,696** (n=23) · p50 26.910 ms | skipped² |
| memory_kind (n=73) | 0,575 (weak_label) | **0,699** · ECE 0,113 · p50 379 ms | **0,700** (n=20) · p50 22.096 ms | skipped² |
| recall_relevance (n=45) | unmeasured³ | **0,867** · ECE **0,709** ⚠️ · p50 369 ms | 0 células⁴ | não treinado |

**Linha do aluno (W4.2, adicionada em 01/10 13:30):** prompt_intent — **0,608** ·
ECE 0,174 · p99 25,6 ms · **local, zero egresso**. Vence keyword-produção e Jev
no ponto dele; memory_kind/recall ficam com o Jev hospedado (cada backend onde
mediu melhor). Detalhe: [w42-student/report](/experiments/w42-student/report.md).

¹ O baseline de intent exigiu **três correções de instrumento** na mesma métrica:
(a) o parser lia `action=` — campo de outra enum (7,5%); (b) `touring
classify-intent` é um **terceiro** classificador (RegexSet 48 padrões), não o de
produção, e sem o header MODE — o parser caiu no fallback "general" nos 120
itens: 37,5% = **exatamente a classe majoritária** (45/120); (c) o classificador
de produção do UserPromptSubmit é `prompt_enhance::classify`, medido pelo shim
`touring-hook prompt-enhance`: **0,536** (n=97). O universo fecha: 97 humanos +
23 payloads `cross-session` que o G0 recusa corretamente = 120. Regra
operacional: número que bate redondo com a proporção de uma classe é constante,
não grandeza. **[F]**
² `pip install laya` morreu em **EDQUOT** (qgroup btrfs; `/home` tem 344 G
livres — não é espaço, é quota). **Resolvido em 01/10**: safe-clean incremental
(REGRA #12, aprovada por Gabriel) liberou 21,3 GB → Laya 0.3.22 instalado e o
aluno treinado localmente (a linha do aluno acima). **[F]**
³ Não existe baseline por item no G1 — declarado unmeasured, nunca inventado. **[F]**
⁴ O professor não devolveu JSON parseável para a tarefa Noul em nenhuma das 45
chamadas — célula unmeasured, não zero. **[F]**

## 2. O que os números decidem (S-2.2 — decisão de trilha)

1. **memory_kind → Jev APROVADO para lote (W5.2).** Empate estatístico com o
   professor (0,699 × 0,700) a **1/55 da latência** e ~US$ 0,0002/memória; vence o
   weak_label de produção em +12,3 pp (0,699 × 0,575). ECE 0,113 — utilizável com
   recalibração leve. **[F]** A W1 (amostra livre, vocabulário de produção) dava
   8% — a divergência confirma o diagnóstico da touring-02: **o problema era o
   vocabulário, não o modelo.** Depende de S-5.7 (vocabulário, ticket Gabriel). **[I, 0,9]**
2. **recall → Jev aprovado como RANKING/corte binário, NUNCA como τ calibrado.**
   86,7% de acerto no corte 0,5, mas **ECE 0,709**: as probabilidades do Noul não
   medem confiança utilizável (o estudo OOD de 22/09 — "Noul subconfiante" —
   confirmado nos nossos dados). Operação permitida: ordenar/filtrar por P(sim)
   como sinal relativo. Operação proibida: qualquer gate `P ≥ τ` sem recalibração
   medida (Platt/isotônica) em dados nossos. **[F + I, 0,9]**
3. **intent → o professor vence; keyword e Jev empatam estatisticamente**
   (0,696 × 0,536 keyword-produção × 0,533 Jev), e nenhum dos dois serve o
   caminho vivo a 191 prompts/dia.**A trilha W4 é professor → aluno** (Claude já
   rotulou 100% do corpus — S-4.1 colapsada para verificação), com o Jev como
   professor reserva para dados não-sensíveis. W4.4 (intent→CILA) espera W4.2
   (aluno), que espera o ambiente de treino (EDQUOT). **[F + I, 0,85]**
4. **Custo medido do experimento inteiro**: 238 chamadas Jev ≈ **US$ 0,008**
   (journal: 183 api + 6 cache na janela; input médio ~700 tok). A pergunta
   "vale medir com Jev?" está respondida: medir é essencialmente grátis. **[F]**

## 3. Gates da v2, veredito

- "Jev ≥ haiku com ≥ 90% acima do threshold de automação" — **não se aplica mais
  como escrito**: o baseline de referência virou o professor (sonnet), e o Jev
  empata nele em 1 de 3 tarefas, vence em 1 (sem professor parseável), perde em 1.
  O gate correto por tarefa está na seção 2. **[F]**
- **Laya**: sem medição local (EDQUOT) — zero-shot fica como lacuna declarada; o
  plano B (Colab) cobre. **[F]**

## 4. Defeitos de instrumento desta rodada (a lição de método, de novo)

1. Meu parser do keyword lia `action=` (perception enum) — o intent mora no
   header `MODE`. Dois campos, um nome parecido, resultado absurdo (7,5%) que só
   foi pego porque a tabela inteira não fechava. **[F]**
2. O emissor de journal intercalava linhas sob concorrência (`writeln!` por
   `write_fmt` em pedaços; 4/84 concatenadas — achado da touring-02). Fix: um
   `write_all` sob O_APPEND + primitiva compartilhada `hook_trace::append_json_line`
   + teste de corrida (400 linhas × 8 threads, todas parseáveis). **[F]**
3. Testes do crate escreviam no journal de PRODUÇÃO (~80 linhas) — isolamento
   `cfg(test)` no caminho default; journal real limpo e reiniciado. **[F]**
