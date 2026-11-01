---
title: "Diagnóstico do gate OUTER — pedágio na saída, contrato móvel e laço fechado"
date: 2026-09-03
scope: /home/gabrielgadea/projects/touring
kind: diagnostic-strategy
status: aguardando decisão de Gabriel
author: TACO (sessão 00fc1e5d)
---

# Diagnóstico do gate OUTER

**Pergunta de Gabriel (03/09/2026)**: escrever um `.md` para satisfazer os gates do
OUTER está saindo caro (interrompe o raciocínio, come janela de contexto) e ineficaz
(o OUTER não acontece no momento previsto, logo não é gate). Hipótese dele: o gate
deveria ser como a skill `grilling`, em que a **interação é o próprio gate**.

**Veredito**: a leitura está correta. O defeito primário é de **posição no pipeline** —
o gate mora em `Stop`, o fim do turno. Um gate no fim só verifica o passado; o único
comportamento que consegue induzir é a produção retroativa de recibo. Somam-se a ele
dois defeitos mecânicos independentes (D5, D6) que respondem pela maior parte da
não-conformidade observada.

## Evidência

Fonte primária: `~/.claude/loop-engineering/compliance.jsonl` — 936 avaliações reais
do Stop hook, 77 sessões, 23/08 a 03/09/2026.

| Medida | Valor |
|---|---|
| Avaliações incompletas | **518 / 936 = 55,3%** |
| Sequências de bloqueio consecutivo | mediana 2 · média 3,1 · **máx 63** |
| Sessões que nunca completaram o OUTER | **18 / 77 = 23,4%** |
| Flows que nunca fecharam | **73 / 94 = 77,7%** (47 markers abertos só no cwd touring) |
| Artefatos faltantes | `explore-ledger` 395 · `diagnostic-okf` 266 · `strategy-doc` 249 |
| Bundles `*-work-outer` acumulados | 320 arquivos, 16,6 MB |

Momento do OUTER dentro do turno (n=38, transcripts, medida ordinal — instrumento
validado contra baseline; a primeira medição por relógio foi descartada porque o
formato do Claude Code registra `tool_result` como `type:"user"`):

| | |
|---|---|
| Turnos em que o OUTER foi a **primeira** ação | **4 / 38 = 11%** |
| Ações já executadas antes do 1º passo OUTER | mediana **5** · média **19,1** · p90 **50** |

→ Em **89% dos casos o OUTER interrompe o trabalho em vez de precedê-lo.**

Tendência da incompletude (piora, não aprende):
`08-23 36% · 08-26 66% · 08-31 68% · 09-01 67% · 09-03 69%`

Assimetria que aponta a causa: `cross-audit` (invocado como a tarefa) tem **3,1%** de
incompletude; `strategy-outer` (armado por cima de outra tarefa) tem **60,0%** e
`work-outer` **53,1%**. Quando o OUTER é o trabalho, converge. Quando é imposto sobre
o trabalho, vira recibo.

## Os seis defeitos

| # | Defeito | Natureza | Evidência |
|---|---|---|---|
| **D1** | Gate no evento errado (`Stop` = pedágio na saída) | Estrutural | 89% dos OUTERs interrompem em vez de preceder |
| **D2** | Juiz gravável pelo julgado, com fail-open | Estrutural | 7 `except Exception` marcados fail-open em `loop_stop_guard.py`; predicado é "artefato existe", não "pensamento ocorreu" |
| **D3** | Arm por heurística textual → falso positivo | Estrutural | armou neste turno, sobre uma pergunta conceitual *a respeito do gate* (verbo "escrever" + ≥5 palavras) |
| **D4** | Erosão, não aprendizado | Observacional | 36% → 69% em 11 dias; `continuations: 30` contra `max_continuations: 2` |
| **D5** | **O `next_action` prescrito não pode satisfazer o gate** | Mecânico | ver abaixo — 76% das faltas |
| **D6** | **Contrato móvel: o marker muda de flow durante o turno** | Mecânico | ver abaixo |

### D5 — laço fechado no `explore-ledger`

O manifesto do `work-outer` afirma: *"so ONE `touring adw run strategy-loop`
satisfies it"*. Medido neste turno — falso:

| Comando | Tempo | Resultado |
|---|---|---|
| `loop_diagnose.py` (next_action) | 26,4 s | ✅ 1 de 2 artefatos |
| `touring adw run strategy-loop` (preferred_next) | 3,3 s | `exit=0 verdict:pass` — **não satisfez** |
| `touring explore --until-dry --max-rounds 8` (next_action) | 3,2 s | `exit=1 converged:false` |
| `touring explore --max-rounds 40` | 12,4 s | `exit=1 converged:false` (32 rodadas secas) |
| `touring explore --mark-lens external:waived` | <1 s | ✅ `converged:true` |

Veredito do ledger não-convergido:

```
clauses: { dry_tail: true, open_questions: 0, degraded_rounds: 0 }   ← tudo OK
unmet:   [ "lens 'external' pending — mark visited/waived (--mark-lens)" ]
```

A lente `external` é **manual por design**; o comando prescrito nunca a marca. Nenhuma
quantidade de rodadas resolve. `explore-ledger` é justamente o artefato mais faltante:
**395 das 518 incompletudes (76%)**. É o anti-padrão D8 da própria constituição — o
texto promete o que o executor não aplica — instalado no coração do juiz.

**Correção mínima**: acrescentar `--mark-lens external:waived` ao `next_action` do
artefato, ou remover `require_json.verdict.converged` do manifesto.

### D6 — contrato móvel

O marker desta sessão (`active-43224dc4d9af-547537c7.json`) foi **criado como
`work-outer`** (contrato de 2 artefatos, lido no início do turno) e, durante o mesmo
turno, passou a **`strategy-outer` com `continuations: 3`** (contrato de 5, incluindo
`strategy-doc` — que o `work-outer` explicitamente dispensa). O Stop hook bloqueou
primeiro como `[work-outer] 1/2` e depois como `[strategy-outer] 3/5`.

Efeito: satisfazer o contrato armado não libera, porque o contrato cobrado passa a ser
outro. Isso explica mecanicamente as sequências de até 63 bloqueios e a inoperância do
`max_continuations` (o contador é por marker; o marker muda).

Atribuição causal da mutação: confiança 0,75 de que foi disparada pela avaliação do
guard com payload sem `transcript_path` (o avaliador muta o estado que avalia).
**A verificar antes de corrigir.** O fato objetivo (confiança 1,0) é a mudança de flow
dentro do turno.

## Por que o desenho do `grilling` é o correto

O gate do grilling funciona porque satisfaz três propriedades que o work-outer viola:
acontece **no ponto de decisão** (não na saída); **bloqueia o avanço**, não o
encerramento; e o artefato produzido (a resposta humana) é **input do trabalho**, não
recibo dele. Custo: uma pergunta (dezenas de tokens) contra um documento (milhares).

É o mesmo desenho dos gates que comprovadamente funcionam nesta casa: os `PreToolUse`
do code mode. Neste próprio turno o G10 negou um par write→run *antes* da ação e
devolveu a rota exata — custo de uma linha, mudança de comportamento imediata. A tese ①
da constituição já diz isso: afordância muda `U(a)`; persuasão não. Um gate no `Stop`
não é nem uma coisa nem outra.

## Decisão pendente (Gabriel)

**O que se quer do OUTER: o *efeito* ou o *artefato*?**

| Resposta | Redesenho |
|---|---|
| **Efeito** (que o modelo recale/diagnostique antes de agir) | gate migra para `PreToolUse`, bloqueando a 1ª ação mutante; o hook injeta o recall pronto; o `.md` deixa de existir |
| **Artefato** (registro para sessões futuras / Atziluth) | o hook **executa** `loop_diagnose` + `explore` sozinho (~30 s medidos) sem passar pela janela do modelo |
| **Ambos** | dois mecanismos separados; definir qual bloqueia e qual apenas registra |

Correções independentes da decisão, aplicáveis já: **D5** (uma linha no
`flow_manifests.json`) e **D6** (resolver marker por `session_id`, não por `cwd`;
higienizar os 47 markers órfãos).

## Kill switch

`TOURING_WORK_OUTER_DISABLED=1` — decisão humana, por ordem da própria regra.
