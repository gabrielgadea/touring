---
type: Strategy
title: "Strategy — canal de leitura read-only expondo sinais dos hooks no code mode"
description: "Plano estratégico derivado do O Prompt Perfeito (Briah) + 25 findings do explore multi-lente (Yetzirah). F1-F6 fases + 4 critérios AND para pronto."
plan_id: 2026-08-31-code-mode-sinal
bundle: docs/plans/2026-08-31-code-mode-sinal
okf_version: "0.1"
tags: [strategy, yetzirah, code-mode, signal-injection, hooks, landlock-seg2, s4-surface, sdk]
timestamp: 2026-08-31T13:40:00-03:00
---

# Strategy — code-mode-sinal-canal (Yetzirah)

## TL;DR

Construir um **canal de leitura read-only** dentro do sandbox do code mode que exponha exatamente os mesmos sinais que os 24 hooks do touring injetam no contexto do LLM — meta-informação, predicate gates, sugestões, métricas, memory recall, gotcha match, pre-edit scores, file metadata, wiring impact, learning rewards. API: SDK tipada no prompt. Contenção: Landlock SEG-2 (`~/.claude/{skills,rules,agents,commands}` RO). Gate fail-closed: `BestPracticesGate`. Pronto: 4 critérios AND.

## Estado da infraestrutura (ground truth via 25 findings)

| # | Achado | Evidência | Implicação |
|---|---|---|---|
| 1 | **SEG-2 já concedeu `~/.claude/{skills,rules,agents,commands}` RO** | release 30.4.28, lesson `landlock-superficie-instrucao-seg2` | O canal JÁ pode ler de onde precisa — sem batalha Landlock nova |
| 2 | **S4 surface allowlist gerada (DAG `task_1787779302447443970`)** | `s4-surface-allowlist-gerada:2026-08-27` | Já existe mapeamento dos hooks read-only; falta estender de 8 para N |
| 3 | **Pillar induction armado (29/08)** | `decisao:pillar-induction-armado:2026-08-29` | cli_suggester induz Master CLI + Learning Memory em runtime |
| 4 | **CEG waiver subprocess-only vale em toda linguagem (30/08)** | `decisao:waiver-subprocess-toda-linguagem:2026-08-30` | Sandbox pode chamar subprocess sem bloqueio para read hooks |
| 5 | **Code mode `presentation=code`** (projeto piloto) | `.touring/touring.toml [code_mode] mode = "code"` | Inspeção atômica é colapsada; SDK precisa ser program-side |
| 6 | **Code mode ranhura 300s calibrada** | gate `S3 30.4.14+` | 1 `touring run` zera a janela; pipeline já medido |
| 7 | **BestPracticesGate já existe** (`crates/touring-quality/src/builtins/best_practices.rs`) | source-facts | Falta estender para cobrir aderência ao uso do canal |
| 8 | **F2.1 OWASP é P0 BLOCK** | touring-quality dim catalog | O canal NÃO pode introduzir shell=True (CMDi CWE-78) |

## Estratégia em 6 fases (F1-F6)

### F1 — Catálogo dos 24 hooks + classificação por tier

**Goal**: inventário exato + critério de prioridade mensurável.

- Listar todos os hooks ativos (`touring hook registry` + source)
- Classificar por tier:
  - **Essencial** (sempre): `pre-edit`, `pre-tool-use`, `cli-suggest`
  - **Útil** (default): `gate-metrics`, `file-metadata-first`, `wiring-orphans`
  - **Opcional** (sob demanda): `evolution drift`, `learning status`, `quality score`
- **Artefato**: `docs/plans/2026-08-31-code-mode-sinal/inventory-hooks.md` com tabela tier×sinal×expor?

**Dependência**: nenhuma (foundation).

### F2 — Estender S4 surface allowlist (8 → N hooks)

**Goal**: dos 8 hooks atuais no SDK `--orchestrate`, expandir para todos os read-only hooks (~71).

- Partir de `s4-surface-allowlist-gerada` (lesson 27/08)
- Listar todos os hooks read-only no daemon (~71 de ~198 totais)
- Diff: quais entram no SDK do code mode vs ficam fora (escrita, runtime state, etc.)
- **Artefato**: novo `crates/touring-code/src/sdk_hooks.rs` (ou similar) com lista tipada

**Dependência**: F1 (precisa do tier classification).

### F3 — Canal de leitura via hook PostToolUse + arquivo espelho

**Goal**: o sinal chega ao sandbox sem violar Landlock.

- Hook `PostToolUse` no touring dispara sync para arquivo espelho (read-only para o sandbox)
- Arquivo espelho em path que SEG-2 já liberou (provavelmente `/home/gabrielgadea/.claude/skills/` ou similar — verificar)
- **Sandbox lê do arquivo espelho**, não chama subprocess (defesa em profundidade)
- Sync incremental (diff, não copia total a cada hook)
- **Artefato**: hook handler + sync logic + arquivo espelho + teste de roundtrip

**Dependência**: F2 (precisa da lista de hooks para sincronizar).

### F4 — SDK tipada no prompt do code mode

**Goal**: o modelo invoca via typed stub, não texto livre.

- Prompt byte-estável (ordem lexicográfica) — KV-cache do provider
- Typed methods: `touring.ast_meta(file)`, `touring.gotcha_match(file)`, `touring.wiring_orphans()`, `touring.memory_recall(topic)`, `touring.pre_edit(file)`, `touring.tantivy_search(query)`, `touring.parallel(calls)`, `touring.query(hook, payload)`
- Cada método retorna JSON canônico tipado (nunca prosa)
- Erros estruturados com taxonomia ortogonal (exception/timeout/abort/proc-exit/invalid-output/output-limit)
- **Artefato**: SDK gerada em `crates/touring-code/src/sdk.py` (Python) ou similar

**Dependência**: F2 (precisa da surface para tipar).

### F5 — BestPracticesGate cobrindo aderência

**Goal**: o agente é forçado a usar o canal quando escreve código que toca arquivos indexados.

- Estender `BestPracticesGate` (`crates/touring-quality/src/builtins/best_practices.rs`)
- Regra: invocação obrigatória de `touring.ast_meta()` antes de Write/Edit
- Regra: invocação obrigatória de `touring.pre_edit` quando `blast_radius > 5`
- Gate fail-closed (bloqueia se aderência < 1.0), não advisory
- **Artefato**: PR com diff + testes do gate + log de execução

**Dependência**: F4 (precisa do SDK existir para checar uso).

### F6 — 4 critérios AND para "pronto"

**Goal**: medido, não sensação.

| # | Tipo | Comando / KPI | Threshold | Janela |
|---|---|---|---|---|
| (a) | Comando verde | `python3 scripts/test_code_mode_signal_injection.py --verbose` | exit 0 com N≥8 testes | instantâneo |
| (b) | KPI composto | `touring kpi -j code_mode_signal_use.composite` | ≥ 0.80 | ≥ 7 dias produção |
| (c) | Gate elite | `python3 docs/elite_aggregate.py --check` (escopo `code-mode-sinal`) | ≥ Gold (0.80) | instantâneo |
| (d) | Evento release | release v30.5.x propagada + 1 ADW end-to-end | prova viva | 1 ciclo release |

**Artefato**: `scripts/test_code_mode_signal_injection.py` com 8+ testes + entry no `docs/elite_aggregate.py` + KPI em `touring kpi`.

## Riscos & Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|
| Hook sync introduz latência no code mode | média | alto | Sincronia incremental (diff), cache local, sync em background |
| Gate fail-closed bloqueia PR legítimo | média | médio | Threshold `blast_radius > 5` é generous; rodar shadow mode1 release antes de fail-closed |
| Arquivo espelho vira stale se hook falha | baixa | alto | Health-check do espelho no SDK (raise se versão < N-1) |
| SDK typed método diverge do daemon real | média | médio | Generate SDK from `touring query "hook:list"` no build (não manter paralelo) |
| F2.1 OWASP regressão (shell=True) | baixa | bloqueante | CI gate `touring-quality check --gate F2.1` no próprio SDK |

## Anti-goals (NÃO fazer)

1. NÃO forkear code mode com cópia total — Landlock + S4 surface mostram que dá para fazer read-only no código atual.
2. NÃO hook chain duplicado entre touring e code mode — drift dobrado.
3. NÃO transporte de todos os arquivos do touring — Landlock medido nega fora do allowlist SEG-2.
4. NÃO sistema novo — extensão orgânica do code mode.
5. NÃO puxadinho opt-in — gate fail-closed.
6. NÃO pós-processamento dos sinais — vira latência no turno.
7. NÃO só memory recall — perde os sinais estruturados (pre-edit, file metadata, wiring).

## Próximo passo (gate humano)

Para Gabriel aprovar a estratégia antes de F1 começar:

1. **Estratégia aprovada?** (gate humano)
2. **Quaisquer fases que devem pular ou reordenar?**
3. **Tier classification — concorda com os 3 tiers sugeridos?**

Quando aprovar, Fase F1 começa: `touring decompose create task` para F1-F6 com depends-on chain.

## Proveniência dos achados

- `touring memory recall` — Briah parcial + Briah fechado + SEG-2 + S4 surface + waiver subprocess + pillar induction (6 lições canônicas)
- `touring explore --until-dry` — 25 findings (rodada 1: 25, dry em 2 rounds)
- `cognicao_formal.py medir --arquivo` — ratio 1.0 no `criacao.md`
- Source facts: `crates/touring-quality/src/builtins/best_practices.rs` (BestPracticesGate), `.touring/touring.toml [code_mode] mode = "code"`, SEG-2 release 30.4.28.

---

_v1.0 — 2026-08-31 | Autor: Yetzirah (TACO Briah → Yetzirah) | Aguardando gate humano_