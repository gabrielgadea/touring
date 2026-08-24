---
type: Strategy
title: Consolidação da família de skills TACO/Touring — medida, não suposta
description: Estratégia para incorporar princípios ADW/loop/gauntlet/graph às skills, com gates de co-evolução pinados às fontes
tags: [skills, adw, loop-engineering, gauntlet, graph-engineering, co-evolution, gates]
timestamp: 2026-08-19T23:59:00-03:00
plan_id: 2026-08-19-skills-consolidation
---

# Consolidação da família de skills — o que as medições disseram

## O instrumento antes da conclusão

Duas ferramentas novas, ambas provadas por mutação antes de qualquer uso:

- `TACO-skilling/scripts/skill_overlap.py` — sobreposição por **conteúdo**
  (Jaccard sobre k-shingles), por **ativação** (descriptions) e por **superfície
  de comandos**, mais o grafo de fontes. Headings foram rejeitados como
  instrumento: são metadados de apresentação, e decidir fusão a partir deles é
  decidir do instrumento errado (correção do Gabriel, 19/08).
- `TACO-skilling/scripts/skill_pin_gate.py` — pina cada skill às fontes sobre as
  quais ela faz afirmações (`pins.json`) e falha quando uma fonte muda sem a
  skill. exit 0 limpo · 1 drift · 2 erro de ambiente.

Dois defeitos do próprio instrumento foram encontrados e corrigidos **antes** das
conclusões: o extrator de comandos capturava um só nível (`ast` em vez de
`ast meta`), produzindo sobreposição 1.00 entre skills que só compartilhavam a
palavra `ast`; e `--master` inexistente devolvia lista vazia em silêncio, que se
lia exatamente como "nenhuma subsunção encontrada". O segundo é a mesma forma de
falha silenciosa que esta sessão passou o dia corrigindo em Rust.

## O que a medição refutou

| hipótese inicial | veredito | evidência |
|---|---|---|
| as 6 micro-skills `touring-*` são ponteiros redundantes | **FALSA** | ensinam comandos que o master não documenta: `ast callgraph/features/skeleton/todos`, `scip emit`, `wiring modules`, `query` |
| há duplicação textual a eliminar na família | **FALSA** | nenhum par dos 22 passou do limiar de conteúdo (0.25) nem de ativação (0.30) |
| `TACO-wt` é redundante (100% de cobertura de comandos) | **FALSA** | cobertura de comandos mede vocabulário, não propósito; TACO-wt ensina um método |

**Fundir era a parte de menor valor do pedido e a de maior risco de erro.**

## O que a medição confirmou

1. **Divergência constitucional em `TACO-subagent`** (o achado mais grave).
   A skill de 463L declara v6.0 enquanto o reference canônico está em v6.2/v6.3,
   e contém **zero** ocorrências de `symbol_verification` — o campo cuja ausência
   a REGRA #15 define como "checkpoint REJECT, composite 0.0". Quem seguir a
   skill produz output que a constituição rejeita.

   **Correção de uma afirmação anterior deste documento (20/08).** A versão
   inicial dizia que a skill *duplica* o reference. A medição por blocos
   normalizados refuta: 57 dos 63 blocos da skill (90%) não existem no
   reference, e 47 dos 54 blocos do reference (87%) não existem na skill —
   **6 blocos compartilhados**. Não é cópia; é divergência. Cada documento tem
   metade do protocolo (a skill: fluxo de fases desenhado, PHASE 0 perception,
   `classify_intent`; o reference: constraints do subagent, o binding `@path`,
   roteamento CILA por nível) e nenhum declara relação com o outro.
   A afirmação de duplicação foi inferida de "mesmo assunto + versões próximas"
   sem medir — o erro que este próprio documento diz ter vindo corrigir.
2. **O vetor de apodrecimento é a CLI, não a duplicação**: 138 pins de comando
   contra 16 de arquivo e 2 de símbolo, num binário que foi de 30.4.9 a 30.4.12
   num único dia.
3. **As skills não declaravam suas fontes.** Só 3 fontes tinham 2+ dependentes
   antes da derivação. Não há como pinar o que não está declarado — por isso a
   parte 3 do pedido começa pela declaração, não pelo detector.
4. `plan-protocol` tem 572L contra o limite de 500 da REGRA #13 — violação
   objetiva.

## Os quatro princípios como forma da intervenção

ADW (composição por `[[use]]`, não cópia) · loop-engineering (convergência
medida por exit code) · gauntlet (juízo cego com quorum contado por código) ·
graph engineering (representação plana inspecionável). O denominador comum:
**substituir declaração por verificação, e cópia por composição.** Não são texto
a colar nas skills — são a forma do trabalho.

## Fases propostas

| # | Fase | Entregável | Estado |
|---|---|---|---|
| F1 | Instrumentos | `skill_overlap.py` + `skill_pin_gate.py`, provados por mutação; 28 skills pinadas | **feito** |
| F2 | Drift constitucional | `TACO-subagent` passa a compor o reference em vez de duplicá-lo | proposto |
| F3 | Gates | pin gate no CI + `propagate-release.sh`; hook de aviso em Edit de rules/CLAUDE.md | proposto |
| F4 | Composição | skills que duplicam references passam a referenciá-los (princípio ADW) | proposto |
| F5 | Planejamento | medir colisão de ativação com prompts reais antes de propor fusão | proposto |

## Discordância declarada

O pedido diz "as skills sejam **automaticamente atualizadas**". Proponho
**detectar automaticamente, re-pinar manualmente**. Um gate que reescreve a skill
para se calar é um juiz gravável pelo julgado — o nó 114 da Darwin Gödel Machine
e a memória `juiz-gravavel-pelo-julgado`. Pior: atualizar o texto automaticamente
significa fabricar afirmações sobre uma fonte que ninguém leu. O hook detecta
(barato, imediato, não-bloqueante); o CI bloqueia (caro de contornar);
`--update` é um ato deliberado de quem revisou.

## Limites conhecidos deste diagnóstico

- `command_overlap` mede vocabulário compartilhado, **não propósito**. Nenhuma
  medição aqui responde "estas duas skills fazem a mesma coisa".
- Os limiares (0.30 / 0.25 / 0.60) são escolha minha, sem calibração contra um
  corpus rotulado.
- "Nenhuma colisão de ativação" vale para o limiar 0.30; descriptions curtas
  podem colidir abaixo dele. Evidência que mudaria a avaliação: rodar as
  descriptions contra prompts reais de planejamento (F5).

## Defeito encontrado no produto durante o OUTER

O gate de artefatos do `strategy-loop` aceitou **ledgers de outra corrida**:
`explore-ledger` foi satisfeito por `refino-da-estrategia-graph-engineering`,
`rodada-3` e `scout-do-scratchpad`, nenhum deste tópico. O `strategy-loop` rodou
3 rodadas com `new_findings: null` e `dry_signal: "absent"` e mesmo assim
`verdict: pass`. Pela Lei L2 do próprio protocolo, sinal ausente ≠ zero
(fail-closed). Registrado para correção.
