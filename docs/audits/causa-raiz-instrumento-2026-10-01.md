---
type: RootCause
title: Por que 9 erros de instrumento numa tarde, com a lição lida — e o que se construiu
description: A causa raiz não é esquecimento: as 17 lições existiam, eram entregues no contexto, e não tinham poder de execução. O projeto já tinha medido que nudge é persuasão e persuasão falha. Este documento regista a prova, a defesa em três camadas, o que ela cobre e — o que ela NÃO cobre.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [cross-audit, root-cause, instrument, affordance, evidence-guard]
author: touring-02
timestamp: 2026-10-01T18:20:00-03:00
okf_version: "0.1"
---

# Causa raiz: "9 erros de instrumento sem nenhum me parar"

## 1. A CAUSA RAIZ, medida

**Não é esquecimento. É que prosa não é executável.**

`~/.claude/projects/*/memory/licoes-instrumento-que-mente.md` tem **17 lições
nomeadas**, cada uma com a sua assinatura observável — e cobre, uma a uma, os
nove erros de 01/10:

| lição (escrita ≤ 25/09) | erro de 01/10 |
|---|---|
| Detector não conclui por ausência | 6 dimensões lidas como `?` |
| Verde de cache não prova medição | `Permission denied` → acusei o gate |
| Ausência de sinal tem duas causas | `^EXIT=` vazio → "o e2e não deixou exit" |
| Array elidido: confira o escalar | `doctor -j` é lista, chamei `.items()` |
| Denominador erra nos dois sentidos | `gotcha_hit_rate 2987015%` |
| Número plausível sobrevive à observação | substring casa dentro de outra palavra |

**A lição foi entregue, lida, e não impediu nada.** E o motivo pelo qual isso não
é umrscheinliche é que **o próprio projeto já o mediu**:

> `protocol-adherence-diagnosis` — nudges MUST com confiança **0,95** são
> ignorados *na sessão que os emitiu*. Por isso enforcement mora no **executor**,
> não no anúncio. (CLAUDE.md, regra `touring-4-pillars`, D8)

E a receita do remédio **já estava escrita há 15 dias** e nunca foi compilada:

> *"ponha um shim observador à frente no PATH — encaminha para o binário real e
> guarda duração, exit e stderr, sem tocar no julgador."*
> — `verde-de-cache-nao-prova-medicao`, 16/09/2026

O shim não existe. Dezessete lições em prosa, zero linha de código que as leia.

## 2. A LACUNA ARQUITETURAL

A posição de enforcement **existia e estava ligada desde sempre**:
`PostToolUse` → `touring-hook post-bash` está registado em `settings.json` e não
fazia nenhuma verificação de evidência. 17 lições, nenhuma lida por máquina.

## 3. O QUE FOI CONSTRUÍDO

`~/.claude/hooks/evidence_guard.py`, registado em `PostToolUse[matcher=Bash]`.

Não julga a conclusão — julga se a **medição que a sustenta é íntegra**. Distingue
três coisas que se parecem:

- "o mundo não tem" — 0 resultado legítimo
- "a minha pergunta errou" — glob errado, flag inexistente, tipo errado
- "o instrumento morreu" — deny, permissão, motor morto, cache

**O princípio vem da lição que ele executa** (`detector-nao-conclui-por-ausencia`):
*"pergunte 'o que ele responde quando não sabe?'. Se a resposta for o valor
permissivo, inverta."* Então são **três valores, nunca dois** —
`clean` | `suspect` | `degraded`, mais `unmeasured` quando o próprio detector
não sabe. **Não existe caminho que devolva `clean` por ausência de evidência.**

### Medido, não afirmado

```
$ python3 evidence_guard.py --self-test
  #  estado     lição citada                          gap
  1  degraded   sinal-de-progresso-que-mentem,verde-de…  ok
  3  degraded   detector-nao-conclui-por-ausencia       ok
  4  degraded   detector-nao-conclui-por-ausencia       ok
  5  degraded   verde-de-cache-nao-prova-medicao       ok
  6  suspect    ausencia-de-sinal-tem-duas-causas      ok
  9  degraded   verde-de-cache-nao-prova-medicao       ok
  detectados: 6/9   lição correta: 6/9
  auto-auditoria: scan_output(None) -> unmeasured

$ [8 saídas REAIS limpas: e2e, kpi, teste verde, ls, sha, git add, find]
  FALSOS POSITIVOS: 0/8
```

## 4. A DESCOBERTA QUE O SELF-TEST PRODUZIU

As três lacunas (#2, #7, #8) **não são falhas do detector — são um limite
estrutural**, e ele é o achado mais útil do trabalho:

| | casos | por quê |
|---|---|---|
| Apanhavel pela saída | #1 #3 #4 #5 #6 #9 | há assinatura no dado |
| **Invisível a qualquer detector de saída** | #2 #7 #8 | o dado está **CORRETO**; o defeito está no *padrão de busca* (#2, #7) ou no *significado do número* (#8) |

**Nenhum detector de saída apanha um glob errado, quando o glob devolveu
exatamente o que foi pedido.** Por isso a defesa é em três camadas:

| camada | apanha | mecanismo | estado |
|---|---|---|---|
| **L1 · predicado de saída** | 6/9 | `evidence_guard.py` | **construído e medido** |
| **L2 · contrato de medição** | #2 #7 | a medição declara `population` vs `sampled`; subconjunto não pode ler como universo | a aplicar no próximo medidor |
| **L3 · revisor independente** | #8, e o resto | `touring adw run cross-audit` (critic-panel, quórum por código) | **provou funcionar hoje**: achou A-8, um BLOCKER que o autor não via |

L3 é o que salvou o audit de hoje. L1 é o que impede que a mesma tarde repita.

## 5. A ARMADILHA QUE O PRÓPRIO DETECTOR ME TROUXE

Ao testar o modo hook com um payload real, **não injetou nada** — e a causa era
`_extract_text` descer até `resp["stdout"]`, descartando as flags irmãs do
envelope. A degradação é declarada ao lado da saída; quem lê uma sem a outra lê
meia medição.

O self-test continuava **verde**, porque o corpus guardava o envelope como
string e não no formato de fio que o hook recebe.

> **Um corpus que não tem o formato de produção prova a imaginação do autor,
> não o detector.**

Isto é o erro nº 11 da mesma classe, e o mais instrutivo: **o teste validou um
caminho que a produção nunca toma.** Corrigido na raiz — o envelope inteiro é
serializado — e verificado ponta a ponta depois.

## 6. O QUE ESTE TRABALHO NÃO PROVA

**[F] A eficácia do guard é `UNVERIFIED`.** Ele está armado desde 18:17 e ainda
não foi posto à prova numa sessão real. Dizer "resolveu" seria o erro nº 12.

A promessa feita aqui é a mesma que `protocol-adherence-diagnosis` mediu e
desmentiu para a persuasão. **Um guard não se prova com um teste verde do
guard; prova-se com a contagem dele sobre turnos reais.**

**Plano de medição (o que decide se isto fica ou é desligado):**

| métrica | onde | limiar |
|---|---|---|
| `evidence_guard_fired` por lição | `touring gate-metrics -j` | contagem por turno |
| Catch-rate observado em turno real | firing ÷ assinaturas reais | ≥ 0,60 |
| Falso positivo | injeções em saída limpa | ≤ 0,05 |
| **Efeito real** | a próxima ocorrência da classe conta como 0 erros? | **é esta que decide** |

Se `catch-rate < 0,60` ou falso-positivo > 0,05 em 2 semanas, o guard é
**desligado** e a camada sobe para L2/L3. Um guard ruim é pior que nenhum,
porque treina o agente a ignorar a camada seguinte.

## 7. CONTEXT7

O plugin MCP `context7` **não está autenticado** nesta sessão — só estão
expostas `authenticate`/`complete_authentication`, sem as ferramentas de
consulta — e `touring context7` **não existe** como subcomando (verificado).
Nenhuma consulta Context7 foi feita, e nenhuma está a ser fingida.

A complementaridade de boas práticas veio do **acervo medido deste próprio
repositório**, que é mais forte que uma fonte genérica porque cada regra já
paga: affordance > persuasão, executor > anúncio (D8), guard D8 cruzado,
`unmeasured` como estado de primeira classe, e o próprio
`detector-nao-conclui-por-ausencia` como princípio de desenho do detector.

## 8. A REGRA QUE FICA

> **Toda lição escrita sem detector é uma intenção, não uma proteção.**
> A pergunta a fazer ao arquivar uma lição não é "isto é verdade?" — é
> **"qual é a assinatura, e quem vai lê-la em código?"**
> Sem a segunda resposta, a lição entra na categoria de `planned-not-built`:
> dívida com linguagem de conhecimento.

---

_Instrumento desta análise: cada número acima saiu de um comando executado
nesta sessão, e **onze** erros de instrumento meus estão registados — cada um
com a razão de ter sido detectado. O detector apanhou dois deles no seu
próprio código (complexidade 15, e o descarte do envelope)._
