---
type: Strategy
title: "Os Mundos da Criação — ritos de interatividade Gabriel × TACO"
description: "Proposta inicial (gate humano aberto): Atziluth/Briah/Yetzirah/Assiyah como camada de ritos SOBRE o loop-engineering existente, com roteador de intensidade, executor por mundo e espelho cognitivo do operador."
plan_id: 2026-08-30-mundos-da-criacao
tags: [strategy, interatividade, ritos, briah, yetzirah, assiyah, atziluth, espelho-cognitivo]
timestamp: 2026-08-30T20:15:00-03:00
okf_version: "0.1"
---

# Os Mundos da Criação — estratégia inicial (30/08/2026)

> Diretiva de Gabriel (verbatim na sessão): ritos, gates e fases com dinâmica de
> interatividade distinta por fase — startup proativo com classificação de
> planos/tarefas; Briah (protocolo de descoberta da criação, modo junguiano
> profundo OU coaching executivo, terminando no prompt perfeito); Yetzirah
> (formação: ≥5 elos se→então, exploração interna/externa, estratégia de CEO,
> decomposição, KPIs, gate humano; planos estratégico/tático/operacional
> sempre); Assiyah (execução com gates que alimentam memória/aprendizado do
> Touring); níveis de intensidade por fase. Pedido: opinião sincera + proposta.

## 1. Os três objetivos que a proposta empacota (separar antes de construir)

| # | Objetivo | Sobre quem | Cobertura atual |
|---|---|---|---|
| O1 | Potencializar o output da LLM | o harness | alta (OUTER/INNER, gates, ADW) |
| O2 | Treinar o pensamento de GABRIEL (análise, decisão, criação) | o humano | **ZERO** — todo o aparato aprende sobre código e harness; nada aprende sobre o operador |
| O3 | Ritos com dinâmica própria por fase | a interação | parcial (flows existem; a INTERATIVIDADE de cada fase não é desenhada) |

O2 é o inédito. O espelho cognitivo (§5) é a resposta a ele.

## 2. Mapa fiel: mundos → o que JÁ existe (não reconstruir)

| Mundo | Já existe | Falta (o delta real) |
|---|---|---|
| **Atziluth** (Emanação — o briefing de startup; Gabriel descreveu sem nomear, e é o 4º mundo que fecha a arquitetura) | `session_startup_intelligence.py` injeta saúde + próximas ações | classificação GUT × dimensionamento × etapa-no-plano dos itens VIVOS; abrir o 1º turno SEMPRE com o Painel |
| **Briah** (Criação — descobrir O QUE criar) | nada além do prompt cru | ~90%: os dois protocolos (executivo/profundo), o roteador de intensidade, o artefato `criacao.md` = prompt perfeito |
| **Yetzirah** (Formação) | ~80%: OUTER = recall→diagnose→explore-until-dry→estratégia→HUMAN GATE→taco-planning→DAG; lentes externas (context7/arxiv) como lentes do explore | cadeia causal ≥5 elos se→então como seção COBRADA do strategy doc; nomear e cobrar os 3 níveis (estratégico=strategy doc, tático=plan.md, operacional=DAG) no doc-link gate |
| **Assiyah** (Ação) | ~90%: ADW runner + gates falantes + `loop_phase_close` (memory store + learning reward + DAG finalize) + KPIs; hooks já são Rust in-daemon | facetas `#process:briah\|yetzirah\|assiyah` nos stores; transição de mundo no journal |

## 3. As três verdades duras (todas medidas ESTA semana)

1. **`touring.flow.compliance_ratio` = 0,4633 vs piso 0,9** — 46% das avaliações
   do Stop terminam sem a evidência do protocolo ATUAL. Rito sem executor não
   adere (`protocol-adherence-diagnosis`: nudges MUST conf 0.95 ignorados na
   própria sessão). **Cada mundo nasce como flow com manifesto de artefatos**
   (`flow_manifests.json` — a infra JÁ cobra isso) ou não nasce.
2. **Afordância que taxa o caso comum mata a adesão** (refusal do DeepSeek; a
   rajada do code mode foi calibrada para a isolada passar). Briah completo para
   "corrija um typo" é esse erro. **Os níveis de intensidade não são detalhe —
   são A peça estrutural**: roteador I0-I3 (análogo do CILA L0-L4) decide quanto
   rito cada criação merece; Gabriel sempre sobrescreve.
3. **`pillar_induction_ratio` STUB com nudge armado desde 29/08** — sem contador,
   um rito não é evidência de nada. **Cada mundo carrega seu KPI de adesão desde
   o dia 1** (medir → calibrar → endurecer; nunca deny primeiro).

## 4. A proposta — fases pequenas, cada uma com executor e medidor

| Fase | Entrega | Tam. | Executor | Medidor |
|---|---|---|---|---|
| **F0 Atziluth** | `painel_emanacao.py`: classifica itens vivos (DAGs pendentes, RETOMAR-AQUI, scout tickets, KPIs FAIL/STUB, memórias `#status:pending`) por GUT (gravidade/urgência/tendência) × T-shirt × etapa→próxima; injetado pelo SessionStart existente; o 1º turno SEMPRE abre com o Painel + opção "ir direto" | S | hook SessionStart (existente) | painel presente no 1º turno (compliance.jsonl) |
| **F1 Briah** | skill `briah`: protocolo EXECUTIVO default (GROW, first principles, pré-mortem, 5-whys, definition-of-done) e PROFUNDO opt-in (amplificação, sombra da criação, projeções — enquadrado como exploração criativa, nunca análise); mecânica `AskUserQuestion` uma camada por vez; converge para `criacao.md` = descrição perfeita = prompt executável (práticas 2026: contexto, contrato de saída, exemplos, critérios de aceitação, anti-goals) | M | flow `briah` no flow_manifests (artefato: criacao.md) | `mundos.briah_adherence` (criações com criacao.md / criações totais) |
| **F2 Δ-Yetzirah** | fragment `causal-chain` no strategy-loop (≥5 elos se→então, seção cobrada); doc-link gate passa a cobrar os 3 níveis nomeados | S | loop_doc_link_gate (existente) | cláusula no gate |
| **F3 Δ-Assiyah** | facetas de mundo nos stores de gate + transição de mundo no journal do ADW | S | loop_phase_close (existente) | `memory query "#process:briah"` etc. |
| **F4 Espelho cognitivo v0** | cada gate humano registra COMO Gabriel decidiu (pergunta que destravou, alternativa descartada, rodadas até clareza) em `#domain:cognicao-gabriel`; Atziluth devolve o padrão acumulado ("nas últimas 5 criações, a pergunta que destravou foi de fronteira de escopo") | S | loop_phase_close + painel | nº de registros + recall no painel |

**Ordem**: F0 + F4 primeiro (valor imediato, zero fricção) → F1 → F2/F3.
**Intensidades** (roteador, default por heurística + override de Gabriel):
I0 pergunta/trivial = zero rito · I1 executivo curto (3 perguntas) · I2 executivo
completo · I3 profundo (opt-in explícito, sempre).

## 5. Opinião sincera (registrada para o gate)

- **A arquitetura é boa porque ela NOMEIA o que já existe** e concentra o novo
  onde há lacuna real: Briah (intake hoje é um prompt cru) e o espelho cognitivo
  (O2, coberto por nada). Reconstruir Yetzirah/Assiyah seria vaidade — é rename
  + delta.
- **O startup proativo tem um limite técnico honesto**: o harness não fala sem
  prompt. Solução prática: o Painel viaja no SessionStart e o 1º turno abre com
  ele SEMPRE (qualquer prompt, até "oi"); atrito zero via alias de shell que abre
  a sessão com "briefing" como 1º prompt.
- **O modo junguiano**: valor prático real como EXTERNALIZAÇÃO (extrai o que não
  foi verbalizado — e o prompt perfeito é exatamente a externalização completa
  da intenção). Limites: exploração criativa, não análise; opt-in, nunca
  default; eu conduzo perguntas, não interpreto a psique.
- **O maior risco não é técnico, é teatro de ritual**: cumprir a forma sem o
  conteúdo (compliance 46% é a prova de que já acontece com menos ritos). As
  mitigações são as três verdades do §3 aplicadas por construção.

## 5b. Refinamento de Gabriel (30/08, pós-apresentação) — o rito É o aparelho de treino

Tese dele (verbatim em essência): a prática habitual do rito em cada
plano/tarefa força o pensamento por todas as etapas; a repetição treina a mente
a internalizar os padrões até o Sistema 1 (Kahneman) operá-los automaticamente
em outras áreas da vida. Consequências no desenho:

**Esclarecimento 2 de Gabriel (30/08)**: o alvo do treino NÃO é calibração de
domínio (estimar esforço de software, prever bugs) — é a OPERAÇÃO FORMAL,
invariante de domínio: pensar em fases; encadeamento lógico; traduzir uma
ideia/emanação em imagem→plano→concepção bem definida; estruturar estratégias
para o objetivo. Automatizar o COMO pensar e o COMO especificar — a forma, não
o conteúdo. Consequências:

1. **O feedback muda de objeto: mede a FORMA, não o palpite de domínio.** O
   ciclo prever→confrontar→ajustar (Tetlock) permanece, mas as previsões de
   custo/bugs eram o proxy errado. As métricas formais (invariantes de
   domínio): `forma.completude_concepcao` (rubrica fechada sobre criacao.md —
   as perguntas invariantes respondidas com substância), `forma.integridade_cadeia`
   (elos se→então sem salto de inferência: cada elo nomeia premissa e
   consequência — lint lógico), `forma.sobrevivencia_fases` (fases desenhadas
   em Yetzirah que atravessaram Assiyah sem redesenho estrutural / total — o
   previsto×realizado DO PROCESSO: fase redesenhada é falha do pensar-em-fases,
   não do domínio). Kahneman & Klein (feedback rápido e inequívoco) e Hogarth
   (feedback inválido treina intuição errada) continuam valendo — aplicados à
   forma.
2. **Os mundos SÃO as operações a treinar** — Atziluth→Briah = "dar forma"
   (emanação→concepção); Yetzirah = "estruturar" (cadeia+estratégia+fases);
   Assiyah = o teste do esquema contra a realidade. O prompt perfeito é o
   instrumento de medida da operação "dar forma": sua completude formal é o
   score.
3. **Schema induction exige superfícies variadas com esquema constante**
   (Gick & Holyoak): o esquema só se descola do domínio quando praticado em
   conteúdos diferentes — código, texto, negócio, vida — e o sistema APONTA o
   isomorfismo ("esta decisão tem a mesma forma da decisão X da semana
   passada"). O apontador de isomorfismo entra no Painel de Atziluth.
4. **Scaffolding com fading (espelho invertido)** — o rito desaparece
   progressivamente: primeiro o sistema pergunta; depois Gabriel enuncia as
   operações primeiro e o sistema só completa as faltantes.
5. **A internalização vira NÚMERO** — `cognicao.antecipacao_ratio`: fração do
   protocolo formal que o prompt cru de Gabriel já cobre espontaneamente antes
   do rito perguntar. Curva subindo = Sistema 1 operando o esquema.
6. **Far transfer deixa de ser ressalva e vira o alvo direto** — como o objeto
   do treino já É o esquema abstrato (não a habilidade de domínio), a
   transferência não é subproduto esperado: é o que se treina; a condição do
   item 3 é o que a garante.

## 6. Gate humano — FECHADO (Gabriel, 30/08/2026)

| Decisão | Veredito |
|---|---|
| (a) Rumo geral | **APROVADO** — as 3 leis de construção (§5b/§3) valem |
| (b) Ordem | **F0+F4 primeiro** — a régua nasce antes do rito |
| (c) Nomes | **HÍBRIDO** — cabalístico canônico (facetas `#process:briah…`, flows, KPIs, vocabulário do rito) + glosa funcional na 1ª menção de cada doc ("Briah — a descoberta da criação") |
| (d) Modo profundo | **OPT-IN COM OFERTA** — o roteador pode notar e perguntar ("esta criação tem cara de I3 — quer o modo profundo?"), jamais entrar sem o sim explícito |

Execução iniciada na mesma sessão: DAG registrada, F0 (Painel de Atziluth) e
F4 (ciclo de medição formal v0) implementados conforme plan.md.

## Ligações

- Diagnóstico + ledger CCE: `diagnostics/` (strategy-loop run desta data)
- Skill hospedeira: `~/.claude/skills/loop-engineering/SKILL.md`
- Evidência de adesão: `~/.claude/loop-engineering/compliance.jsonl` · `touring kpi -j`
