---
type: ExperimentReport
title: S-5.1 — P8 skill-picker: o gate de silêncio, medido
description: A cascata catálogo→top-3→Choice com gate de silêncio. 9/9 testes; e a medição que quase reprovou o design: com cobertura simples o gate dispara em 95,8% das tarefas reais, em qualquer τ. Com IDF e τ medido, cai para 27,5%.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [experiment, s-5.1, w5, p8, skill-picker, gate-de-silencio, idf]
subtask: S-5.1
owner: touring-02
timestamp: 2026-10-01T11:06:00-03:00
okf_version: "0.1"
---

# S-5.1 — P8 skill-picker: o gate de silêncio, medido

Subtask `task_1790819065619924538::S-5.1`, dona `touring-02`. Nada foi registrado
em hook: o picker é um **script medido**, e ativá-lo é decisão do relatório final
(o plano lista "skill-picker registration" entre as decisões do Gabriel).

- **Script**: `scripts/skill_picker.py` · **Testes**: `scripts/test_skill_picker.py` — **9/9 PASS**
- **Pool de medição**: `experiments/s5-skill-picker/queries.json` — 784 prompts humanos reais, **200 persistidos** (seed 20261001) para a medição não depender de rerodar a extração

## O que o plano pedia, e o que o gate virou

O plano pede a cascata do cookbook, o gate de silêncio e o override manual, com a
condição *"quando NÃO usar: se o gate de silêncio não puder ser medido"*. O que
segue é a medição — e ela quase reprovou o design.

## A forma real do catálogo (medido, 01/10)

| | n |
|---|---|
| entradas em `~/.claude/skills/` | 189 |
| com `SKILL.md` | **185** |
| sem `SKILL.md` (plugins, caches) | 4 |
| **conteúdos distintos** | **~83–89** (o número exato varia com a permissão de leitura, ver §4) |
| symlinks → `~/.agents/skills/` | 102 (**101 vivos, 1 quebrado** — `dev-de-sloppify` → `~/projects/analise/…`, que não existe mais) |

**[F] Duas conclusões que mudam a economia do P8:**

1. **Deduplicar por conteúdo, não por entrada.** O espelho em `~/.agents/skills/`
   expõe o mesmo `SKILL.md` por dois nomes. Um catálogo por entrada ofereceria
   ao modelo 102 cópias de `recipe-batch-invite`.
2. **A granularidade é o problema, não o tamanho.** As `description` vão de 1 a
   70+ caracteres de mediana, e o número de **termos** por skill vai de 1 a **95**
   (mediana 26): `analysis-loop` tem 95, `lexcore-execute` 59. Uma Choice de 185
   opções de description curta não é uma Choice — é um sorteio com 370 ms de
   latência.

## A medição que quase reprovou o design

**[F] Versão 1 — cobertura simples (`len(hits)/len(q)`):**

```
tau=0.20  dispara 115/120 = 95.8%   distintas=18
tau=0.80  dispara 115/120 = 95.8%   distintas=18
vencedora: lexcore-execute em 56 de 120
```

**O gate não distinguia nada.** O corte não mudava o resultado em uma faixa de 0,6
de τ, e **47% das tarefas reais iam parar na mesma skill**. A causa está no número
que a medição expõe: `lexcore-execute` tem **59 termos** porque sua `description`
é um parágrafo inteiro ("Auto-execute next PLN wave from checkpoint. Resumes from
latest lexcore-checkpoint, identifies next pending subtask via touring decompose,
spawns touring-engineer agent…"). Qualquer tarefa de código cita `executar`,
`cargo`, `clippy` ou `checkpoint` — e **cobertura de um termo não separa**.

**[F] Versão 2 — cobertura ponderada por IDF** (`idf_table` + `score_skill`):

```
score: min=0.217  p25=0.468  mediana=0.468  p75=0.607  max=1.000

distribuição:
  [0.2,0.3)    5  #####
  [0.3,0.4)    7  #######
  [0.4,0.5)   56  ########################################################
  [0.5,0.6)   18  ##################
  [0.6,0.7)    5  #####
  [0.8,0.9)    1  #
  [0.9,1.0]   23  #######################
```

| corte | passa | taxa |
|---|---|---|
| 0,25 | 112/120 | 93,3% |
| 0,35 | 108/120 | 90,0% |
| 0,45 | 98/120 | 81,7% |
| **0,55** | **33/120** | **27,5%** |
| 0,65 | 25/120 | 20,8% |

**[I] (0,85) A distribuição é bimodal, e é isso que faz o gate funcionar.** O
massa de 56 cai em [0,4–0,5) — consultas que casam com meia-dozeira de skills
genéricas; e há 23 em [0,9–1,0] — consultas cujo vocabulário nomeia uma skill
específica. **O vale está em ~0,55**, e é aí que o corte separa as duas modalidades.

**[I] (0,80) τ = 0,55 é o valor medido, não o padrão.** A escolha é defendível pelo
número (o corte que minimiza a distância entre as duas massas), e não pela
simpatia de "um terço". O `tau` default no script está **0,34 — errado**, herdado
da primeira versão: **[F] o default ainda não foi corrigido** e quem rodar sem
`--tau` vai obter o comportamento de 90% de disparo. É o primeiro item da lista
abaixo.

**[F] As três consultas de score 1.0** mostram o que o gate deve deixar passar:

- `briah` ← *"vá preparando os artefatos, documentos, páginas e demais arquivos…"*
- `search-connectors` ← *"rode os 8 testes e pesquise qual melhor lugar para inserir registry…"*
- `analysis-loop` ← *"refaça toda extração, verificação, linkagem, camada semântica…"*

Três tasks diferentes, três skills diferentes, três empates perfeitos. **É o
comportamento que a cascata promete.**

## Os testes (9/9)

| Teste | O que fixa |
|---|---|
| `test_silent_when_below_threshold` | **[F] o gate nomeado do plano** — abaixo do corte, silêncio **e zero chamadas** (o spy levanta se o Jev for chamado) |
| `test_manual_override_bypasses_gate` | **[F] o segundo teste nomeado do plano** — `!skill` ignora o gate |
| `test_gate_silent_without_a_jev_call` | o caso mais comum (não é skill) cala ANTES de gastar os 370 ms |
| `test_shortlist_never_exceeds_three` | a cascata é 3; a Choice não cresce com o catálogo |
| `test_every_option_carries_a_contrastive_criterion` | cada opção diz o que **não** é — é o que separa skills irmãs |
| `test_catalog_deduplicates_by_content` | duas entradas, mesmo conteúdo → **uma** opção + o alias viaja junto |
| `the_alias_travels_with_the_option` | o nome sobrevivente pode ser qualquer um dos dois; o que não pode é perder o outro |
| `the_real_catalog_has_no_duplicated_content` | nenhum nome é simultaneamente canônico e alias |
| `test_term_set_drops_stopwords` | sem stopword, toda consulta genérica casa com toda skill |

**[F] Um teste corrigiu a asserção, não o código** — o mesmo padrão da minha
retratação da S-1.1: `the_alias_travels_with_the_option` exigia que
`skill-original` sobrevivesse, mas quem sobrevive é o **primeiro em ordem
alfabética** (`skill-espelho`). O código estava certo; a expectativa é que não
podia assumir um "canônico" que a implementação não promete. **[I] (0,85)** O que
importa é que o alias viaje, para que a resposta possa ser apresentada com o nome
que o **usuário** digitou.

## O que a S-5.1 **não** entrega

1. **[F] τ default errado no script** (0,34 em vez do 0,55 medido). Correção de
   uma linha, deixada de fora desta entrega porque muda o comportamento padrão
   de quem já chama o script — e o número é uma **decisão de Gabriel**, que o
   plano lista como "skill-picker registration".
2. **Não há hook.** O picker é um script; ligar em `session_startup` ou
   `UserPromptSubmit` é o mesmo tipo de decisão.
3. **A prova externa do plano (erro 17% → 7,3%) é de terceiro** e não é
   reproduzida aqui. O que este relatório mede é a **taxa de disparo**, que é a
   pré-condição que o próprio plano escreve. **[I] (0,90)** Sem um gold de
   "qual skill devia carregar" para as 120 consultas, a taxa de disparo é o
   máximo mensurável hoje; medir **acerto** exige rótulo, e rótulo de skill é o
   que o professor ainda não fez.
4. **O ECE do `none` não foi medido.** O gate tem dois caminhos de silêncio
   (`below_tau`, determinístico, e `jev_said_none` / `low_confidence`, do modelo),
   e só o primeiro está calibrado por construção.

## Cross-references

- `scripts/skill_picker.py` · `scripts/test_skill_picker.py` · `experiments/s5-skill-picker/queries.json`
- `plan.md` S-5.1 (gate) · S-5.2 (herda τ daqui)
- `harness-scan-t02.md` §8.6 — o censo de 187 entradas que motivou a deduplicação
- `experiments/w3-baseline/report.md` — S-3.3, a baseline das réguas
- Receitas de progressive disclosure em `research/` do bundle (janela de linhas acima de 255 opções; catálogo → top-3)
