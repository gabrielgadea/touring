---
okf_version: "1.0"
type: Strategy
title: "Graph Engineering + Wayfinder + Gauntlet no ADW e na loop-engineering"
description: "Estratégia para incorporar topologia de grafo (fan-out/join), planejamento sob fog-of-war e crítica adversarial ao runner ADW, mais um portfólio de fluxos modulares e composíveis."
plan_id: 2026-08-18-graph-engineering-flow-portfolio
tags: ["#kind:strategy", "#domain:adw", "#domain:loop-engineering", "#process:outer", "#artifact:strategy"]
timestamp: 2026-08-18T09:50:00-03:00
sources:
  - "https://youtu.be/JWhICz1QR8M — Greg Isenberg, Why Graph Engineering will 10x your Claude/Codex (26:28)"
  - "https://youtu.be/F3lL98Pj90o — Matt Pocock, /wayfinder: Nothing is too big to plan anymore (15:09)"
  - "https://youtu.be/BNjzXcEXmg4 — Jay E, This NEW Claude Prompting Technique (gauntlet-loop) (13:30)"
  - "https://arxiv.org/pdf/2505.22954 — Zhang et al., Darwin Godel Machine (ICLR 2026, 16pp) — analisada em 19/08/2026, ver o-juiz-e-o-arquivo.html"
---

# Estratégia — Graph Engineering, Wayfinder e Gauntlet no ADW

## 1. Intento

Incorporar ao runner ADW e à `/loop-engineering` três disciplinas convergentes extraídas
das fontes, e materializar o pedido do Gabriel: **um portfólio de fluxos com partes
modulares, personalizáveis, mais uma estrutura para criar um fluxo novo para uma
funcionalidade específica de um projeto/sistema**.

## 2. As três fontes — o que cada uma aporta

| Fonte | Camada | Aporte não-redundante |
|---|---|---|
| **Graph Engineering** (Isenberg) | topologia | jobs+arrows+state; padrão **diamante** (split→paralelo→check→merge); **separar worker de checker**; menor grafo que melhora a qualidade; o grafo produz **memória**, não só trabalho |
| **Wayfinder** (Pocock) | planejamento | **fog of war** + **frontier**; decision tickets ≠ implementation tickets; 4 tipos (research/prototype/grilling/task); planejamento **multi-sessão**; spec como destino **não-persistente** que linka à fonte primária |
| **Gauntlet Loop** (Jay E) | qualidade | fan-out worker↔**critic cego**; **bar to hit** explícita como critério de parada; ressalva crítica: sem brief/MVP correto, o loop **otimiza na direção errada** |

## 3. Estado atual — evidência de código (FACT [1.0])

| Capacidade | Estado | Evidência |
|---|---|---|
| Nós tipados, journal durável, resume | ✅ | `adw.py:57` `NODE_TYPES`; `Journal`; `--resume-run` |
| Terminação por código (Lei L2) | ✅ | `run_loop` + `NEW_FINDINGS`; `_next_edge` retry budget |
| Class-D (narrativa ≠ veredito) | ✅ | `_track_class_d` |
| Human gate + ZTE conformal | ✅ | `_human_node`, `_zte_bypass` |
| **Fan-out paralelo / join** | ❌ **ausente** | `execute()` mantém `current: str`; `_next_edge` devolve **uma** string |
| **Composição / fragmentos** | ❌ ausente | `load_spec` lê **um** TOML; sem `include`/`extends` |
| **Critic separado do worker** | ❌ ausente | nenhum spec da library tem critic-agent; só `gate` determinístico |
| **Fog / frontier tipados** | ❌ ausente | `decompose` tem deps e `ready`, não tem incerteza tipada |
| **Portfólio de fluxos recuperável** | ⚠️ quebrado | `portfolio status` conta `adw 8`, mas busca por intento não os recupera |
| **Criar fluxo novo** | ⚠️ `cp` | `cmd_from_template` copia o template literalmente |
| Racing | ✅ (≠ fan-out) | `cmd_race`: N lanes do **mesmo** spec, redundância competitiva |

**Raiz mecânica da invisibilidade do portfólio**: `adw_description`
(`touring-server/src/portfolio/miner.rs:237`) extrai **apenas** `[adw].description` — uma
frase. `shell_header` extrai o bloco de comentários inteiro. Um ADW entra no corpus BM25
como documento de ~10 palavras contra scripts de 200+. Reproduzido: o intento "corrigir um
bug com memória institucional e gate de verificação" — descrição literal de `bugfix.toml` —
devolveu um script de outro projeto, e nenhum ADW.

## 4. Decisão estratégica — 6 movimentos

| # | Movimento | Fonte | Natureza |
|---|---|---|---|
| **P1** | Nó `parallel` + `join` (padrão diamante) | Graph | motor |
| **P2** | Fragmentos composíveis (`[[use]]` com inlining namespaced) | pedido Gabriel | loader |
| **P3** | Bloco `[purpose]` + miner enriquecido → fluxos recuperáveis por intento | Graph (memória) | descoberta |
| **P4** | `touring flow new` — criação guiada a partir de fragmentos, nasce lintada e testada | pedido Gabriel | criação |
| **P5** | Wayfinder sobre `decompose` — fog/frontier, decision vs implementation tickets | Wayfinder | planejamento |
| **P6** | Fragmento `critic-panel` (cego, com bar, quorum) gated por brief aprovado | Gauntlet | qualidade |

**Princípio ordenador** (Isenberg, verbatim): *"the goal is to make the smallest graph that
improves the quality of work"*. P1–P6 não são licença para grafos maiores; são a afordância
para que o grafo **certo** seja barato de expressar, descobrir e reusar.

**Princípio de sequência** (Jay E): o gauntlet entra **depois** que a fundação está correta.
No loop-engineering isso é literal: `critic-panel` pertence ao passo 13 (cross-audit), nunca
ao passo 12 (execute) sem brief aprovado no gate humano do passo 9.

## 5. Ordem de implementação (dependências reais)

```
P2 (fragmentos) ─┬─> P4 (flow new) ─> P3 (portfólio) ─> adoção medida
P1 (parallel) ───┘        │
P6 (critic-panel) ────────┘
P5 (wayfinder) ── independente, acopla no OUTER
```

P2 antes de P1: fragmentos são inlining puro (zero mudança no motor) e destravam P4/P6.
P1 é a única mudança no motor de execução e carrega o risco de durabilidade (journal por
branch, resume parcial).

## 6. Riscos declarados

| Risco | Mitigação |
|---|---|
| `parallel` quebra a durabilidade do journal | `exec_key` por branch; `parallel_started`/`branch_completed`/`parallel_joined`; resume pula branches completas |
| Fragmentos viram indireção ilegível | inlining resolvido é **materializável** (`touring flow explain <name>` imprime o spec plano) |
| Mais agentes = mais ruído (anti-padrão explícito da fonte) | `critic-panel` com quorum e lentes **distintas**, nunca N críticos idênticos |
| Portfólio enriquecido vira propaganda | `when_not_to_use` é campo obrigatório; a lacuna é exibida (E4 honestidade) |
| Wayfinder vira waterfall | tickets `prototype` são o antídoto declarado pela fonte |

## 7. Convergência

Nenhum movimento é "pronto" por narrativa. Gate por artefato:
`touring adw lint` verde para todo fluxo do portfólio · `adw test` (mock) por fluxo ·
Teste A do portfólio passa (intento de bugfix recupera `bugfix`) · `loop_converged.py` exit 0.

---

_OKF Strategy · plan `/plan.md` · bundle `2026-08-18-graph-engineering-flow-portfolio`_
