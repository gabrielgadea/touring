---
type: Strategy
title: Touring Autoresearch — inteligência e RL que melhoram a cada execução
description: Estratégia consolidada do OUTER (2026-08-25) — diagnóstico com ground truth + pesquisa externa (Karpathy autoresearch, self-improving agents, DSPy/Agent RL) → 3 sistemas de melhoria contínua.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [loop, strategy, autoresearch, rl, code-mode]
timestamp: 2026-08-25T21:45:00-03:00
okf_version: "0.1"
---

# Touring Autoresearch — Estratégia

Part of the [bundle](/index.md). Ledger CCE: `.touring-explore/autoresearch--camadas-de-inteligencia-e-rl-em-lo.ledger.json` (converged, 2 dry rounds).

## 1. Diagnóstico (ground truth executado, não citação)

### Camada 1 — o canal de recall NÃO está entupido como citado; o sinal de recompensa está vazio

A citação de entrada dizia `corpus_coverage 0,127` e `never_recalled_ratio 0,810`. Verificado ao vivo:

| Métrica | Citado | Medido hoje | Fonte |
|---|---|---|---|
| corpus_coverage | 0,127 | **0,861** | `touring kpi -j` |
| never_recalled_ratio | 0,810 | **0,199** | `touring kpi -j` + probe SQL direto |
| never-recalled (DB projeto) | — | 1.717/8.641 = **19,9%** | `memory_entries.access_count=0` |

Os pares são complementares (1−0,127≈0,86; 1−0,810≈0,19): a citação media a fração **recuperada** com o nome da fração **perdida** — sinal invertido (lição `sinais-de-progresso-que-mentem`: provar o instrumento antes de acusar o sistema).

O entupimento REAL está uma camada abaixo: **`outcome_reward IS NOT NULL` em 1,7% das memórias** (147/8.641 projeto; 4/241 global). Memórias são escritas e lidas — mas quase nenhuma carrega o veredito que permitiria ao aprendizado rankeá-las. O feromônio ACO existe como coluna e está vazio em 98,3%. `touring learning status`: `update_count=7` (LinUCB), `agentic_rl.update_count=5` — o substrato foi alimentado uma dúzia de vezes na história do daemon.

### Camada 2 — substrato existe e está parcialmente ligado (citação desatualizada)

| Afirmação citada | Verdade medida (varredura sandbox em `crates/`) |
|---|---|
| `learning.linucb` → só testes | **95 arquivos prod** referenciam LinUCB (bandit, hooks, server, CEG) |
| QTable observa o teclado | 70 arquivos prod; alimentação majoritária via telemetria de ferramenta (`post_tool_rl`) — **sem join teclado→nota** |
| `dspy_cluster` → 0 importadores | **Confirmado e pior: 0 arquivos em `crates/`** — não existe no Rust; dspy 3.2.1 instalado em Python, nada o invoca |

### Camada 3 (nova) — loops e ADWs não fecham outcome→reward

- `adw.py` (3.823 linhas): **exatamente 1 callsite de reward — o path `campaign`**. O `adw run` comum produz `RunOutcome` + journal fsync'd e **nenhum reward entra no learning**. 35 runs registrados (`touring.adw.runs`), zero aprendizado por run.
- `touring.adw.router_accuracy` = **STUB** — o factory router nunca recebeu feedback do que roteou.
- Code mode: `t3_turn_fused/first_passed` existem em 3 arquivos (record + display apenas) — **nenhum consumidor de aprendizado**. `touring.code_mode.adoption_ratio = 0,083` ADVISORY: a medição existe, a realimentação não.
- `loop_phase_close.py --reward` é o ÚNICO ponto onde outcome vira reward com disciplina — a exceção que prova a regra.

### Camada 4 (nova) — não existe o experimentador

Não há entidade que proponha mutação no harness → rode o experimento → meça escalar → keep/discard → registre → repita. O `variant_archive.py` (arXiv:2505.22954, w=s·h) existe com 13 testes — mas só registra quando `loop_phase_close --variant` é chamado manualmente, o que quase nunca acontece. Temos o arquivo de stepping-stones sem o jardineiro.

## 2. Pesquisa externa (lente `external` marcada no ledger)

### Karpathy/autoresearch (o modelo-alvo) — 9 fontes

Loop de ~630 linhas: agente lê `program.md` → edita `train.py` → treina 5 min → lê `val_bpb` (bits-per-byte de validação, independente de vocabulário) → **keep/discard binário** → repete. ~100 experimentos/noite; a escada de val_bpb conta a história. Elementos essenciais: **(1)** UMA métrica escalar comparável; **(2)** orçamento fixo por experimento (barato → muitos); **(3)** decisão binária sem "talvez"; **(4)** log de evidência append-only; **(5)** criatividade no agente, disciplina no loop. Shopify já generalizou: *"Autoresearch isn't just for training models"* — hypothesis→experiment→measure→keep/discard para qualquer coisa mensurável.

### Self-improving agents 2026

- **arXiv:2607.13104** — survey "Self-Improvements in Modern Agentic Systems" (self-improvement controlável em sistemas deployados).
- **Preprints 202608.0051 + ICLR 2026 Workshop RSI** — experiência+feedback → updates persistentes; a frente institucional.
- **arXiv:2505.22954** (já no nosso skill): arquivo de variantes > hill-climb (50,0% vs 39,7% SWE-bench) — caminhos bons passam por variantes piores.
- **Agent Lightning** (Microsoft, arXiv:2508.03680; v1.0 2608.17528, ~3,5k LOC): desacopla framework de agente do treino RL — RL sobre trajetórias sem reescrever o agente.
- **OpenPipe ART** — GRPO para agentes com harness ergonômico.
- **DSPy via Context7** (`stanfordnlp/dspy`, 34 KB indexados): GEPA (evolução reflexiva de prompts com feedback textual + Pareto — o mais alinhado a "melhorar a cada execução"), MIPROv2 (otimização bayesiana prompt+exemplos), BootstrapFewshot, BootstrapFinetune, tutorial Arbor RL. Instalado, 0 importadores no nosso código.

## 3. Estratégia — três sistemas de melhoria contínua

Tese: **toda execução vira experimento registrado com outcome; todo outcome alimenta recompensa; toda recompensa move uma política; e um research loop propõe e testa mutações do próprio harness contra métricas escalares.**

### Sistema A — Sinal de outcome universal (fechar o poço de 98,3%)

1. Reward retro-escrito: gates (G1/G6/G8/T3-B/code-mode) gravam `outcome_reward` na memória/snippet da rota quando o veredito da sessão é conhecido (rota seguida? executou? exit?).
2. `adw run` grava reward por run (`RunOutcome.status` + gates + duração → `touring learning reward adw:run:<spec>`) — hoje só `campaign` grava.
3. Factory router recebe feedback pós-run → `router_accuracy` sai de STUB.

### Sistema B — Políticas que consomem o sinal (ligar o substrato)

4. LinUCB ganha braços vivos: apresentação code-mode por escopo (native/both/code) com recompensa = adoção medida; T3-B window/threshold tunados por recompensa (fused→programa executado = +; fused→contornado via prefixo native = −).
5. QTable passa a receber outcome (join teclado→nota via run_id/journal), não só telemetria.
6. DSPy GEPA offline otimizando prompts de agentes ADW (critic lenses, personas de scout) com métrica = audit yield / phase quality.

### Sistema C — O research loop (o Karpathy do Touring)

7. `touring adw run autoresearch` (spec nova): propõe UMA mutação do harness (threshold, prompt, peso, apresentação), aplica em shadow, roda a bateria de escalares (KPI suite como val_bpb: `code_mode.adoption_ratio`, `memory.corpus_coverage`, `flow.compliance_ratio`, e2e composite, quality50), **keep/discard binário**, registra no `variant_archive` + memória com reward, repete. Orçamento fixo por experimento. Human gate para promover shadow→live.
8. Métrica escalar canônica por subsistema (o "val_bpb" de cada): code mode → adoção × compressão × outcome; loops → iterações-até-convergência (↓) × quality (↑); ADW → success_rate × rounds_to_dry (↓); memória → share com reward (↑ sobre 1,7%).

## 4. Fases propostas (para a DAG, após aprovação)

| Fase | Conteúdo | Critério de convergência (medido) |
|---|---|---|
| **P0** Sinal | Sistema A itens 1–2 | `outcome_reward` coverage sobe de 1,7%; reward por `adw run` provado ao vivo |
| **P1** Políticas | Sistema B itens 4–5 | braço code-mode aprende (update_count cresce com recompensa real); QTable com join outcome |
| **P2** Experimentador | Sistema C itens 7–8 | 1 campanha autoresearch real sobre T3-B com keep/discard registrado no variant_archive |
| **P3** DSPy/GEPA | Sistema B item 6 | 1 persona otimizada offline com métrica antes/depois |
| **P4** Consolidação | A/B, docs/skills/rules sync, propagação | `loop_converged.py --rust-full` exit 0 |

Guards novos seguem o padrão da casa: mutation-proof (0→1→0) e veredito por exit code, nunca narrativa.

## 5. Riscos e mitigações

- **Reward hacking** (o agente otimiza a métrica e não o propósito): `judge_attest.py` já é a defesa institucional (arXiv:2505.22954 App. H) — toda métrica canônica nova entra no judge of record com attestation.
- **Métrica escalar errada vira norte errado**: fase P2 começa com 1 subsistema (T3-B) cujo outcome é binário e observável (programa fundido executou ou não).
- **DSPy offline pode virar yak shaving**: P3 só começa se P0–P2 convergirem; kill switch = não instalar nada novo, dspy 3.2.1 já está no ambiente.

---

## 6. Adendo da retomada — 2026-08-25 20:18 BRT (OUTER re-executado nesta volta)

O piso de frescor do gate (`flow_armed_at`) exige artefatos **desta** volta, então o OUTER
foi re-executado em vez de herdado. O que a re-execução mediu e o que ela expôs:

### 6.1 Ground truth re-medido (probe SQL direto, `mode=ro`)

| Métrica | 18h (sessão 1) | 20h18 (retomada) | Leitura |
|---|---|---|---|
| `outcome_reward IS NOT NULL` (projeto) | 147/8.641 = 1,70% | **147/8.671 = 1,70%** | +30 memórias novas, **zero** com veredito — o poço não é histórico, é **corrente** |
| `outcome_reward` (global) | 4/241 = 1,66% | 4/242 = 1,65% | idem |
| `access_count = 0` (projeto) | 1.717/8.641 = 19,9% | 1.717/8.671 = 19,8% | estável |

O dado novo é o **fluxo**, não o estoque: nas ~2h entre as duas medições o corpus cresceu 30
entradas e **nenhuma** carregou outcome. Qualquer trabalho feito hoje entra no corpus mudo.

### 6.2 Três defeitos que a própria re-execução expôs (não estavam no diagnóstico de 18h)

- **D1 — `strategy-loop` falha estruturalmente em tema com lente externa.** O nó
  `explore_round` roda `touring explore "$topic" --scope "$scope"`, que **sai 1** enquanto a
  lente `external` está `PENDING`. O nó não tem rota para marcá-la (é decisão humana/manual),
  então `on_fail = "__fail__"` derruba o fluxo antes do `evidence_report`. Observado 2× (exit 3
  na sessão 1, exit 1 aqui). Efeito: o OUTER determinístico **nunca** completa sozinho num tema
  que exija pesquisa externa — exatamente a classe de tema que mais precisa dele.
- **D2 — o slug do ledger não normaliza diacríticos.** `…inteligencia…` e `…inteligência…`
  produzem dois ledgers distintos no mesmo escopo. A convergência de um não protege o outro:
  havia um ledger convergido (5 rodadas, `external` visitada com 11 fontes) e um fork
  não-convergido, para a mesma pergunta.
- **D3 — o gate do flow aceita ledger alheio.** O manifesto casa
  `{scope}/.touring-explore/*.ledger.json` — **qualquer** ledger fresco do escopo, sem exigir
  o tema do flow nem `verdict.converged`. Na primeira avaliação desta sessão o gate deu
  `present` apontando para o ledger **não-convergido** (D2), e teria deixado o turno fechar com
  a exploração incompleta. É a mesma classe já registrada em `piso-de-artefato-envelhece-com-o-marker`:
  o artefato certo pelo predicado errado.

D1–D3 são candidatos naturais ao escopo do plano: D1/D3 endurecem o próprio enforcement que a
Lei L3 promete, e D2 é uma linha de normalização.

### 6.3 Exploração

Ledger convergido nesta volta: 8 rodadas, 13 findings, `1 → 10 → 0 → 0 → 0 → 2 → 0 → 0`, todas
as 7 lentes visitadas (`external` reafirmada com as 11 fontes do §2). Os 2 findings da rodada 6
são **auto-referentes** — as duas memórias que a sessão anterior gravou. Sem substância nova:
o tema segue seco.
