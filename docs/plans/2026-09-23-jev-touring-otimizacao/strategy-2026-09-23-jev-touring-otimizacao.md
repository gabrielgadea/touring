---
type: Strategy
title: Otimizar loops, fluxos ADW, recuperação de contexto, memória e snippets do Touring com os padrões Jev (System One)
description: Pesquisa profunda no notebook NotebookLM "JEV" (7 fontes de vídeo), cruzada com a estratégia v2 de 22/09 e os KPIs vivos do daemon — o que os vídeos adicionam, o que confirmam, e o plano de otimização por subsistema com gates medidos.
plan_id: 2026-09-23-jev-touring-otimizacao
tags: [research, jev, system-one, loops, adw, context-recovery, memory, snippets, strategy]
timestamp: 2026-09-23T19:30:00-03:00
version: "1.0"
okf_version: "0.1"
---

# Otimização Jev × Touring — loops, ADW, contexto, memória, snippets

Parte do [bundle](/index.md). Continua (não substitui) a estratégia v2 de 22/09 em
`docs/plans/2026-09-22-jev-system-one/strategy-2026-09-22-jev-system-one.md` — aquela
cobriu a documentação oficial e o experimento Laya; esta cobre as **7 fontes de vídeo**
do notebook "JEV: A Revolução da IA no Sistema Um de Decisão" (NotebookLM, consultado
em 23/09 com 5 perguntas e citações verificadas) e os **KPIs vivos** do daemon.

Convenção de confiança (herdada da v2): **[F]** fato lido na fonte citada ·
**[F-3P]** fato reportado por terceiro nos vídeos, sem verificação nossa ·
**[I]** inferência (0,7–0,9) · **[E]** estimativa derivada · **[M]** medido nesta
máquina em 23/09 (KPI do daemon, fonte citada em cada linha).

---

## 1. Veredito em uma tela

- **A direção da v2 está certa e os vídeos a fortalecem.** Professor → aluno (G1–G3),
  filtro determinístico antes de modelo (G0), volume antes de priorizar. Nenhuma fonte
  de vídeo muda a arquitetura; duas mudam o **detalhe de onde aplicar** (§3.2 e §3.5). [I]
- **O achado novo mais acionável é sobre os fluxos ADW**: nos benchmarks do JEV
  Gateway com Claude Code, o Jev ajuda fluxos de **correção/revisão** (Sonnet 5 bugfix:
  −41% tokens de saída, −48% de entrada, 25% mais rápido) e **prejudica** fluxos de
  **criação** (Opus 5 feature nova: **+61% tokens de entrada, 83% mais lento**). Ou seja:
  screening System-1 entra por **classe de fluxo**, nunca transversal. [F-3P]
- **Quatro lacunas medidas hoje não dependem de modelo nenhum** — são as otimizações
  mais baratas do relatório: `memory.corpus_coverage` **0,597** (meta ≥ 0,95; remédio de
  1 comando, disparado nesta sessão) · `memory.graph_contract_share` **0,0** (meta 0,5) ·
  `code_mode_reuse` **FAIL 0,146** (piso 0,20; 390 snippets na escada, **10 re-usados**) ·
  `flow.compliance_ratio` **0,499** (meta 0,9). [M — `touring kpi -j` 23/09]
- **A chave continua ausente** (`TYPESAFE_API_KEY` UNSET, verificado 23/09). Todo gate
  até G2 segue sem exigir chave; o desenho professor-via-Claude cobre o dado sensível. [M]

---

## 2. O que as 7 fontes de vídeo adicionam à pesquisa

Fontes: *Como usar JEV no Codex e Claude Code* · *Jev + Hermes: 300 tasks por $0,001* ·
*JEV é o novo REI da IA (193× mais rápido, 444× mais barato)* · *É aqui que eu vejo
dinheiro de verdade* · *Jev + Claude Code = the cheapest agentic coding loop yet* ·
*Por que o lançamento do Jev é uma notícia melhor do que parece* · *We need to talk
about Jev...* (Matthew Berman, contraponto).

### 2.1 Confirmações da v2 (sem mudança de rota)

| Tese da v2 | Evidência dos vídeos |
|---|---|
| 3 primitivas (Choice ≤ 255 opções; Score ≤ 11 níveis; Noul) | idêntico, com demo no playground [F-3P] |
| Calibração RLCD é de grupo, não de indivíduo | "de 1.000 decisões a 90%, ~900 certas"; thresholds por risco da ação [F-3P] |
| "0% alucinação" = formato, não julgamento | "pode escolher a categoria errada com confiança alta — garante o formato, não o acerto" [F-3P] |
| Acurácia bruta abaixo de frontier | 67,8% nos evals da própria TypeSafe, < Opus 5 (73,1%) e Sol (74,1%) [F-3P] |
| Fallback obrigatório | cascata confiança alta → age · média → confirma · baixa → LLM/humano [F-3P] |

### 2.2 Material novo (não estava na v2)

1. **JEV Gateway como padrão de integração com Claude Code/Codex.** Proxy que
   intercepta a requisição, decide tool/skill com o Jev (~100 ms) e **injeta a decisão
   como instrução adicional na requisição corrente — sem reescrever o histórico** —
   preservando o prompt cache do LLM principal. Reescrever histórico é o anti-padrão
   nomeado: quebra o prefixo de cache e anula a economia. [F-3P]
   → **O Touring já é cache-alinhado por construção**: toda injeção nossa é
   `additionalContext` aditiva no turno (hooks), nunca reescrita de transcript. [I]
2. **Números por classe de tarefa no Claude Code** (benchmark do gateway): bugfix
   Sonnet 5: −41% output / −48% input / −26% requests / 25% mais rápido; bugfix Astra:
   −39% tempo / −57% output; **feature nova Fable: −24%/−27%/26% mais rápido**;
   **feature nova Opus 5: +61% input / 83% mais lento**. O sinal inverte por classe e
   por modelo — medir por (fluxo × modelo), nunca transversal. [F-3P]
3. **Roteamento de skills em escala.** Hermes: 182 skills; descrições custam ~10k
   tokens/req; skill errada caiu de **17% → 7,3%** (Haiku 4.5); cargas desnecessárias
   de 9,8% → 4,0%. O Touring tem ~180 skills globais e a memória registra 75% dos
   gatilhos fora da `description` (memory `triggers-inertes-no-frontmatter`). [F-3P + M]
4. **Funil de screening → shortlist → System 2.** 150 comentários analisados em 9,3 s
   por ~US$ 0,01; code review com **10× menos contexto lido** pelo modelo principal;
   "reflexos System-1 baratos especializados na nossa codebase". [F-3P]
5. **Gating calibrado aplicado a memória** (padrão do vídeo 6): > 0,85 aplica direto ·
   0,50–0,85 carrega condicional / valida · < 0,50 fallback para LLM/humano. É o
   desenho exato do candidato P3 (filtro de relevância no recall) da v2. [F-3P]
6. **Cegueira contextual documentada**: notícia sobre a emissora "MS Now" classificada
   como banco Morgan Stanley — o classificador não faz inferência sutil; estado
   ambíguo pertence ao Sistema 2. [F-3P]

### 2.3 O que os vídeos NÃO cobrem (e o Touring já tem)

Convergência medida por exit code (juiz), DAG durável, memória facetada com RRF/GPU,
CEG/Landlock, RL com replay, juiz de record com atestação. A lacuna é deles, não nossa:
nenhuma fonte mostra nada comparável ao `loop_converged.py`. [I]

---

## 3. Análise por subsistema — estado medido × padrão Jev → otimização

### 3.1 Loops (loop-engineering)

**Estado medido [M]**: `flow.compliance_ratio` **0,499** (meta 0,9) ·
`adw.explore_rounds_to_dry` **8,09** (teto 8) · `adw.plan_refine_iters` **1,0**
(meta ≥ 2) · OUTER strategy-loop desta sessão: 16 findings na rodada 1, seco nas
rodadas 2–3, exit 0 (funcionou como desenhado).

**Diagnóstico**:
- Compliance ~0,5: metade das avaliações do Stop-gate termina sem manifesto completo.
  Causa provável [I]: flows `work-outer` armados por prompt substantivo e abandonados
  no meio do turno (TTL/TTL de sessão), não flows explícitos. A régua mede intenção,
  não abandono — investigar `~/.claude/loop-engineering/compliance.jsonl` por flow
  antes de mexer no gate.
- Explore 8,09 rodadas até secar: as lentes rendem devagar demais ou o alvo é aberto
  demais. A régua já nomeia a hipótese ("lenses yielding too slowly or targets
  unbounded").
- plan_refine 1,0: planos aceitos no primeiro rascunho — o gate de refino (F2,
  `plan_refine.py`) não está sendo alcançado ou não está armado.

**Otimizações (sem modelo)**:
- **L1** — Atribuir compliance por flow: agregar `compliance.jsonl` por
  `flow` × `outcome` (1 script; o JSONL já existe). Se `work-outer` domina os
  incompletos, decidir: TTL mais curto, cap menor, ou descontar abandono da régua.
- **L2** — Instrumentar yield por lente no ledger CCE (findings novos por lente por
  rodada) e cortar lente com yield 0 duas rodadas seguidas — convergência mais barata
  sem mudar o contrato.
- **L3** — Verificar por que `plan_refine` itera só 1×: armamento, caminho do
  `.refine.json`, ou ausência de chamador (mesma classe do achado "rotina de retenção
  sem chamador" de 19/09).

**Padrão Jev que cabe (com modelo, depois do G3)**: Score-rúbrica barato no gate
VERIFY/REFLECT da fase (o vídeo: "checklist de 50–500 perguntas sobre o PR") como
**pré-filtro** antes do cross-audit caro — screening, não veredito; o juiz de
convergência permanece 100% determinístico (v2 §6.3: `loop_converged`/`judge_attest`
nunca por modelo). [I]

### 3.2 Fluxos ADW

**Estado medido [M]**: 113 runs · `router_accuracy` 1,0 · `zte_bypass_rate` 0,009 ·
`tiered_agent_share` 1,0 · factory router: **9 tickets na vida** (volume insuficiente
para modelo — a v2 já rebaixou C1 por isso).

**O achado dos vídeos que muda o detalhe**: screening System-1 **ajuda bugfix/review e
prejudica criação** (§2.2.2). Mapeamento [I, 0,85]:

| Classe de fluxo ADW | Screening Jev? | Evidência |
|---|---|---|
| bugfix / hotfix / review / audit | **candidato forte** — pré-filtro de diff + linter qualitativo | −41% output, −48% input, 25% mais rápido (Sonnet 5 bugfix) [F-3P] |
| feature / explore-plan / criação | **não aplicar** | +61% input, 83% mais lento (Opus 5 feature) [F-3P] |
| roteamento (factory) | esperar volume | 9 tickets [M] |

**Otimizações**:
- **A1** — Declarar a **classe do fluxo** nas specs (`[purpose] class = "fix" |
  "create" | "review"`) — metadado que hoje não existe e que decide se screening
  System-1 entra. Pré-condição de qualquer integração futura; custo zero; reversível. [I]
- **A2** — Manter o roteador por regra até volume: a v2 já decidiu; os vídeos não
  mudam (cascata regra → Jev → haiku quando houver tickets). [F]
- **A3** — Candidato novo vindo dos vídeos: **linter qualitativo** nas fases de
  revisão ("o nome descreve os side-effects?", "o log vaza PII/segredo?") como Score/
  Noul em lote sobre o diff — mas **só depois do G3** (aluno local) ou com ZDR; diff
  de repo público pode ir ao Jev, diff do `analise` não (v2 R2/R10). [I]

### 3.3 Recuperação de contexto

**Estado**: checkpointer triplo (DAG + memory + OKF bundle) · PreCompact snapshot ·
`loop_resume.py` (SessionStart/PostCompact) injeta artefatos pendentes + próximo
comando — **aditivo, portanto já alinhado ao padrão gateway de preservar cache** (§2.2.1).
Gotcha relevante da v2 §10.5: dentro do sandbox Landlock, leitura de transcripts
devolve zero **silencioso** — recuperação de contexto que varre `~/.claude/projects/*.jsonl`
tem de rodar **fora** do `touring run`. [F + M]

**Otimizações**:
- **C1** — Filtro de relevância no payload do resume: hoje o resume injeta tudo que
  está pendente. Com o aluno (G3), um Noul por item ("ajuda no primeiro prompt da nova
  sessão?") corta o que não serve — o padrão 0,85/0,50/0,50 dos vídeos aplicado ao
  checkpointer. Sem aluno: heurística de sobreposição léxica já dá metade do efeito. [I]
- **C2** — Medir a eficácia do resume (hoje não há régua): registrar quando a injeção
  do resume é seguida de ação relacionada vs ignorada — mesma disciplina do
  `suggestion_uptake` (0,33, medido). Sem essa régua, "recuperação funciona" é fé. [I]
- **C3** — Documentar no skill loop-engineering a regra operacional: **nunca** varrer
  transcripts de dentro do sandbox (o zero silencioso já enganou a v2). [F]

### 3.4 Memória

**Estado medido [M]**: `corpus_coverage` **0,597** (meta 0,95 — 40% das entradas
inalcançáveis pelo recall ANN; reindex disparado nesta sessão) ·
`curated_recall_share` 0,519 (piso 0,5, no fio) · `never_recalled_ratio` 0,093 (OK) ·
`graph_contract_share` **0,0** (meta 0,5 — nenhum nó curado novo com chave determinística
+ aresta tipada) · `edge_density` 0,023 (meta 0,05) · `tag_coverage` 1,0 (OK) ·
GPU embeddings ativa (doctor: cuda, 19 textos).

**Diagnóstico**: a biblioteca facetada v30.4 está **escrita mas não ligada em grafo** —
stores acontecem, links não (0 arestas novas/nó). É a mesma classe do achado
"infra desligada não é infra pronta": `memory link` existe e ninguém chama. [I]

**Otimizações**:
- **M1** — (feito nesta sessão) `touring memory reindex` → fechar corpus_coverage;
  agendar verificação semanal (o KPI já alerta; faltava o remédio automático).
- **M2** — `memory suggest-links` no `loop_phase_close.py`: fechamento de fase gera
  arestas sugeridas por derivação (o comando existe; falta o chamador — mesma classe
  do L3 acima). Sobe edge_density sem custo de modelo. [I]
- **M3** — (depois do G3/aluno ou juiz hospedado) filtro de relevância antes de injetar
  (candidato P3/C2 da v2, reforçado pelos vídeos §2.2.5): Noul por candidato no top-30,
  corte por P(sim), ordenação pelo próprio valor. Ataca `curated_recall_share` e a
  densidade de injeção (teto CILA). Evidência direta desta sessão: o recall para
  "jev…" trouxe outcomes de Edit em HTML — ruído num tema inédito. [F medido na sessão]
- **M4** — (background, barato) tags facetadas por lote hospedado só após G2 medir
  acurácia do professor; heurística atual acerta 0,067 nas 5 classes testadas (v2 P4).

### 3.5 Snippets (escada de trust + colheita)

**Estado medido [M]**: `code_mode_reuse` **FAIL — reuse_ratio 0,146** (piso 0,20) ·
`ladder_enrolled` 390 vs `ladder_reused` **10** · `harvested_runs` **0** ·
adherence success_rate 0,9175 (OK) · **838 pares de retry desperdiçados** ·
tmp privado: 260 MB em 17.196 runs.

**Diagnóstico**: **enrolar não é reusar** (lição já registrada) e o número hoje prova
que o gargalo é **descoberta/recall do snippet**, não a escada: 390 corpos sobem a
escada, mas quase nenhum é encontrado quando seria útil. E `--harvest` (a porta de
entrada declarada) tem **zero** usos — a afordância não dispara (mesma classe de
"afordância desligada quebra em silêncio"). [I, 0,85]

**Otimizações**:
- **S1** — Medir a descoberta: em runs que tocaram ≥2 alvos, quantos tinham snippet
  trusted aplicável (busca por propósito no acervo facetado)? A razão "reused /
  aplicável" é a régua que falta; hoje medimos só o denominador bruto. [I]
- **S2** — Recall de snippets por **propósito** no nudge do executor: quando o gate
  T3-B funde uma rajada, a resposta negada já entrega o programa fundido — passa a
  entregar também o **snippet trusted mais próximo** (1 consulta `#kind:snippet`
  facetada por domínio/linguagem, < 10 ms via índice). Afordância no executor (D8),
  não persuasão. [I]
- **S3** — `touring run --harvest` com nome por propósito: zero usos porque o passo é
  ótico. O `loop_phase_close` (ou o próprio run com `--brief`) pode propor harvest
  quando o mesmo corpo roda ≥ 3× com sucesso (a escada já mede isso: provisional ≥ 10
  exec @ ≥ 90%) — fechar o laço que a escada presume. [I]
- **S4** — Retry: 838 pares desperdiçados. A14 já exige "na 4ª muda de estratégia";
  instrumentar o contador por run e, na 2ª falha da mesma assinatura, o journal
  sugere a troca (medir primeiro; não endurecer o gate antes da régua). [I]
- **S5** — (depois do G3) Score-rúbrica de qualidade na entrada da escada ("este
  programa é genérico o bastante para virar snippet?") — screening do funil dos
  vídeos aplicado à colheita. [I, 0,65 — opcional]

---

## 4. Plano recomendado — em cima do G0–G5, não ao lado

> **ESTADO EM 23/09, fim do dia: N0–N6 ENTREGUES** (DAG `task_1790203324709596117`,
> 6/6 done; fases em [phases/](/phases/)). N0: corpus 1.0 ✓. N1: KPI compliance
> started-conditional + per-flow + arm_yield (55 testes, cross-check 2/2). N2: hipótese
> do corte MORTA pela data (1 rodada/174 ledgers); KPI por episódio (8.09→2.33 real) +
> `lens_yield` no payload. N3: produtor cabeado nos 2 fluxos — `plan_refine_iters`
> **2.0 PASS ao vivo**; explore-plan chamava arg inexistente há meses. N4: suggest-links
> com 1º chamador + chain edge (prova: N4→N3 `extends`) + régua resume_uptake com dados
> (10 injeções, uptake 0.0 honesto). N5: 23+5 specs classificadas + lint + 3 vermelhos
> pré-existentes fechados (guard escritores, curation, espelho). N6: hint jornalado por
> run + `hint_coverage` + snippet trusted nos 3 denies + harvest_candidates; 784+576+
> 1636 testes verdes, clippy limpo. Duas réguas mudam de leitura no pós-deploy:
> `flow.compliance*` e `explore_rounds_to_dry` passam a medir a verdade.

| # | Ação | Custo | Gate de saída (medido) | Depende de |
|---|---|---|---|---|
| **N0** ✅ | `touring memory reindex` | 1 comando | corpus_coverage ≥ 0,95 | — (disparado 23/09) |
| **N1** | Atribuição do compliance por flow (script sobre compliance.jsonl) | 1–2 h | relatório por flow; decisão sobre work-outer | — |
| **N2** | Yield por lente no CCE + corte de lente seca | ½ dia | explore_rounds_to_dry ≤ 8 com mesmo recall | — |
| **N3** | Diagnóstico plan_refine 1,0 (chamador? armamento?) | 1 h | causa nomeada + correção | — |
| **N4** | `suggest-links` no phase-close + régua de resume (C2) | ½ dia | edge_density ≥ 0,05; resume_uptake medido | — |
| **N5** | Classe de fluxo nas specs ADW (`[purpose] class`) | 1 h | 23 specs classificadas; lint exige o campo | — |
| **N6** | Descoberta de snippets: régua "reused/aplicável" + snippet trusted no deny do T3-B | 1–2 dias | code_mode_reuse ≥ 0,20 por comportamento | — |
| **G1** (v2) | **Revisão humana do corpus** (1.586 rótulos) | Gabriel | concordância ≥ 0,85 → fecha o gate | Gabriel |
| **G2** (v2) | Baselines: keyword × Laya zero-shot × professor (Jev só sem dado sensível e só se houver chave) | 1 dia | tabela acurácia/ECE/p99 | G1 |
| **G3–G5** (v2) | Aluno → integração atrás de flag → hospedado onde o aluno não serve | — | ver v2 §8.1 | G2 |

Ordem: **N0–N6 primeiro** (nada depende de modelo nem de chave; três deles atacam
régua FAIL/ADVISORY hoje), G1→G2 na sequência. Nenhum N compete com os G: os N são
engenharia de harness; os G são a trilha do modelo. [I]

## 5. Onde NÃO aplicar (reforço dos vídeos à v2 §6.3)

1. **Criação de features/código novo** — o único ponto onde os vídeos mediram
   **piora** real (+61% input, 83% mais lento). Screening System-1 fica fora de
   `feature.toml`/`explore-plan`. [F-3P]
2. Juiz de convergência, atestação, KPIs, contagens, CEG/gates de segurança,
   wiring/imports — determinísticos por contrato (D8/M3). [F]
3. Estados ambíguos/curtos demais (lição MS Now→Morgan Stanley): sem contexto
   suficiente no `state`, a decisão pertence ao Sistema 2. [F-3P]
4. Hooks síncronos de ~1 ms (`cli_suggester`, `pre-bash`): 70–500 ms + rede não cabe;
   o aluno local a 6–15 ms, sim — depois do G3. [F + M]

## 6. Cadeia causal

1. **Se** screening System-1 ajuda correção e prejudica criação, **então** a unidade de
   decisão é a **classe do fluxo** — e a classe precisa existir como metadado (N5)
   antes de qualquer integração.
2. **Se** 40% das memórias estão fora do corpus ANN e nenhuma aresta nova é criada,
   **então** a otimização mais barata da memória é fechar o que já existe (N0, N4),
   não um modelo novo.
3. **Se** 390 snippets estão na escada e 10 são reusados, **então** o gargalo é
   descoberta no momento da ação — e o lugar do snippet é o executor que já nega a
   rajada (N6), não mais um nudge.
4. **Se** as régua N0–N6 não dependem de modelo, **então** elas pagam agora e ainda
   sobem o valor do aluno quando o G3 entregar (memória limpa, fluxos classificados,
   snippets acháveis são o substrato que o modelo consulta).
5. **Se** a chave segue ausente, **então** todo gate até G2 roda com professor-Claude
   e nada fica bloqueado por fornecedor — o desenho já era assim e os vídeos não
   mudam. [I]

## 7. Cross-references

| Item | Local |
|---|---|
| Estratégia v2 (docs oficiais + Laya, G0–G5) | `docs/plans/2026-09-22-jev-system-one/strategy-2026-09-22-jev-system-one.md` |
| Relatório G1 (corpus 1.586 rótulos) | `docs/plans/2026-09-22-jev-system-one/experiments/g1-corpus/report.md` |
| Digests rodada 1–2 | `docs/plans/2026-09-22-jev-system-one/research/` |
| Notebook JEV (7 fontes) | https://notebooklm.google.com/notebook/af176c16-97d6-43e8-9a30-3bfa63d321ec |
| Diagnóstico desta sessão | [diagnostics/touring-20260923T185710.md](/diagnostics/touring-20260923T185710.md) |
| Skill loop-engineering | `~/.claude/skills/loop-engineering/SKILL.md` |
| Biblioteca de hashtags | `docs/memory-hashtag-library.md` |
