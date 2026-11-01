---
type: Criacao
title: "complementacao-hooks-touring — estender a cadeia de 27 eventos × 82 handlers com handlers reativos para os 15 sinais gap"
plan_id: 2026-08-31-complementacao-hooks
tags: [briah, criacao, hooks, complementacao, settings-json, signal-injection, code-mode]
timestamp: "2026-08-31T18:46:00-03:00"
okf_version: "0.1"
---

# complementacao-hooks-touring — a concepção (Briah)

## A Emanação

Quero que venha a existir **uma extensão da cadeia atual de hooks do touring** que adicione, aos eventos que JÁ existem (PreToolUse/PostToolUse/SessionStart), handlers novos que invoquem os módulos Rust já implementados (5 bridges × 146 pub_symbols + 42 crates × 6449 symbols) para injetar **15 sinais reativos** que hoje são gaps — qualidade pós-edit, símbolos pré-read, dependentes pré-edit, gotchas pré-edit, vulnerabilidades pós-write, code mode status no session-start, orphans pós-edit (REGRA #0), impact BFS pré-edit, audit_unsafe pós-edit, find_references pós-write, EntityId pós-write (REGRA #17), temporal_drift session-start, evolution_status session-start, scan_vulnerabilities pós-write, pub_api_diff pós-edit.

A criação nasce para **fechar o abismo mensurável** que a análise comparativa 57-vs-hooks revelou em 31/08 15:35 — **41 gaps (72%)** dos sinais que o CLI touring entrega nunca chegam ao contexto do LLM. Desses 41, 15 são reativos e merecem virar handlers novos. Os outros 27 são sob-demanda e ficam via SDK tipada no code mode (F2 code-mode-sinal).

## O Telos

**Para que** o programador humano e o agente IA que usam o touring tenham, aoEditar/Ler/Escrever código, **a mesma visão estrutural** que uma chamada CLI enriquecida tem hoje — qualidade antes de commitar, símbolos antes de ler, dependentes antes de refatorar, vulnerabilidades antes de aceitar.

**Para quem** o agente IA Claude Code / TACO (que hoje opera via hooks e decisions) — hoje ele vê 17% dos sinais via hooks; com a complementação, vê ~57% (17% já existentes + 41% dos 15 reativos novos = 41/57 = 72% dos sinais MUST+SHOULD cobertos).

A finalidade última: **o touring deixa de ser um CLI com curadoria, e vira um ambiente assistido por sinais estruturais em cada tool_use** — o equivalente a "modo avançado" permanente.

## A Imagem

Fecho os olhos e a criação está pronta: o programador abre um arquivo `.rs` para editar. O PreToolUse(Edit) injeta `dependents=12, gotchas=2 (F2.1 avoid shell=True, F4.3 no deprecated), wiring_impact=14 consumers, blast_radius=4`. O programador edita. O PostToolUse(Edit) injeta `quality_delta=-0.02 (was 0.92, now 0.90), pub_api_diff=+1fn/-0fn (additive), wiring_orphans=0 (REGRA #0 OK), audit_unsafe=0 (no unsafe added)`. O SessionStart da próxima sessão injeta `code_mode_status=Enabled, evolution_status=Converging (52 arms), temporal_drift=None`. A cena: o programador **vê cada decisão estrutural antes de cometê-la** — sem precisar rodar CLI à mão.

## A Fronteira

Esta criação **NÃO** é, **NÃO** será, e **NÃO** fará:

1. **NÃO** é criar touring-hook subcmds novos — o v11.0 do shim está congelado; novos handlers vão DIRETO em `~/.claude/settings.json` como handlers adicionais em eventos que JÁ EXISTEM (P4-arquitetural: preservar o shim).
2. **NÃO** é duplicar handlers — cada novo handler é ÚNICO em seu evento (PostToolUse com matcher `Edit|Write` recebe UM handler por sinal, não três).
3. **NÃO** é acoplar via Rust import direto — handlers invocam binários/scripts Rust via subprocess (padrão `arch:generator-hooks-integration:pattern` — fire-and-forget tokio::spawn).
4. **NÃO** é otimizar latência dos hooks existentes — otimização é projeto irmão.
5. **NÃO** é mexer na SDK do code mode (F2-F6 code-mode-sinal) — esses são projetos IRMÃOS, com DAGs separadas.
6. **NÃO** é opt-in — assim como o código mode complementação (F2), a complementação dos hooks é parte da esteira; cada handler novo é registrado no `settings.json` e fica ativo por default.

## A Cadeia

Do estado atual à Imagem, **se** cada elo, **então** o seguinte:

1. **se** catalogarmos os 15 sinais reativos com seu evento hook ideal (PostToolUse/PreToolUse/SessionStart) → **então** sabemos o ponto exato onde cada handler entra no `settings.json` (matriz já em `analise-comparativa-57-vs-hooks.md` v1.0).
2. **se** mapeamos cada sinal ao módulo Rust que JÁ o produz (5 bridges + crates que JÁ implementam o cálculo) → **então** o handler é só um `command` shim que invoca o módulo existente via subprocess — zero código novo de cálculo.
3. **se** cada handler novo respeitar o contrato do shim (`additionalContext` JSON canônico tipado + latência budget <100ms) → **então** o sinal chega ao LLM sem custo estrutural novo.
4. **se** os 15 handlers novos forem registrados em `settings.json` como entradas adicionais nos arrays `.hooks.PreToolUse`/`.hooks.PostToolUse`/`.hooks.SessionStart` (nunca substituindo) → **então** coexistem com os handlers atuais sem regressão.
5. **se** cada handler novo tiver teste (`scripts/test_complementacao_hooks.py`) que valida o JSON injetado contra um golden file → **então** o sinal é verificável por execução, não por fé.
6. **se** a BestPracticesGate cobrir aderência aos 15 novos handlers (warn-severo, não fail-closed, decidido na F5 code-mode-sinal) → **então** o programador é alertado quando um tool passa sem injetar o sinal esperado, sem bloquear a esteira.

## O Custo Invisível

Pré-mortem: daqui a 6 meses, a manchete é **"Complementação dos hooks quebrou o pre-edit gate"** — 15 handlers novos rodam em todo Edit/Write; a latência composta passa de 80ms para 1.5s por tool_use; o programador desliga 8 dos 15 via `~/.claude/settings.local.json`; o restante degrada; o gate `BestPracticesGate` reclama de aderência < 1.0 e ninguém sabe qual handler é o culpado.

**As alternativas descartadas** (com o porquê):

- (a) **Criar um mega-handler único que injeta os 15 sinais** — descartada: blast radius alto (1 arquivo, 15 subsistemas), debug impossível, acoplamento daninho.
- (b) **Adicionar handlers via `touring-hook <subcmd>` novo** — descartada: shim v11.0 congelado, criar subcmd novo quebra o contrato pinado.
- (c) **Implementar os cálculos em Python dentro do handler** — descartada: duplica lógica Rust já testada; latência maior; drift de implementação.
- (d) **Tornar os 15 handlers opt-in via env var** — descartada: violaria o princípio "esteira, não puxadinho" da complementação do code mode; degrade silenciosa.

O que mata a criação é **latência acumulada** — os 15 handlers precisam caber no budget <100ms cada, ou a UX quebra.

## O Pronto

**Pronto** é uma conjunção (AND) de 4 critérios medidos — nenhum sozinho basta:

1. **Comando verde**: `python3 scripts/test_complementacao_hooks.py --verbose` retorna **exit 0** com **N=15 testes verdes** (cada teste exercita UM handler novo: PostToolUse(Edit) injeta quality_delta, PreToolUse(Edit) injeta dependents, SessionStart injeta code_mode_status, etc).
2. **Settings.json**: o array `.hooks.PostToolUse` cresce de 19 para **26** (15 handlers novos com matcher `Edit|Write`), `.hooks.PreToolUse` cresce de 25 para **30**, `.hooks.SessionStart` cresce de 9 para **12**. Verificável por `jq '.hooks.PostToolUse | length'` etc.
3. **KPI composto**: `touring kpi -j hooks_complement.composite ≥ 0.80` em produção ≥ 7 dias consecutivos (cobre cold-start).
4. **Latência budget**: cada handler novo roda em **<100ms p95** (medido via `touring gate-metrics -j | jq .hook_latency_p95`); caso contrário fail-fast.

Pronto é quando **todos os 4** passam — não sensação, não porcentagem, não "está bonito".

## O Prompt Perfeito

> Estenda a cadeia atual de hooks do touring adicionando 15 handlers novos aos eventos que JÁ EXISTEM (`PreToolUse`/`PostToolUse`/`SessionStart`), invocando os módulos Rust já implementados (5 bridges × 146 pub_symbols + 42 crates × 6449 symbols) via subprocess fire-and-forget (padrão `arch:generator-hooks-integration`). Os 15 sinais reativos: (1) `touring.quality` em PostToolUse(Edit|Write) — quality_delta; (2) `touring.symbols` em PreToolUse(Read); (3) `touring.dependents` em PreToolUse(Edit); (4) `touring.pub_api_diff` em PostToolUse(Edit|Write); (5) `touring.gotchas` em PreToolUse(Edit); (6) `touring.scan_vulnerabilities` em PostToolUse(Write); (7) `touring.code_mode_status` em SessionStart; (8) `touring.wiring_orphans` em PostToolUse(Edit|Write); (9) `touring.wiring_impact` em PreToolUse(Edit); (10) `touring.gotcha_match` em PreToolUse(Edit); (11) `touring.find_references` em PostToolUse(Edit|Write); (12) `touring.entity_id` em PostToolUse(Write); (13) `touring.audit_unsafe` em PostToolUse(Edit|Write); (14) `touring.temporal_drift` em SessionStart; (15) `touring.evolution_status` em SessionStart.
>
> **Restrições firmes**: NÃO criar touring-hook subcmds novos (shim v11.0 congelado); handlers novos vão DIRETO em `~/.claude/settings.json`; cada handler é ÚNICO em seu evento; invocação via subprocess fire-and-forget; latência budget <100ms p95; coexistência sem regressão com handlers atuais.
>
> **Pronto** (AND dos 4): (a) `python3 scripts/test_complementacao_hooks.py --verbose` exit 0 com N=15 testes verdes; (b) `jq '.hooks.PostToolUse | length'` = 26, `jq '.hooks.PreToolUse | length'` = 30, `jq '.hooks.SessionStart | length'` = 12; (c) `touring kpi -j hooks_complement.composite ≥ 0.80` ≥ 7 dias; (d) `touring gate-metrics -j | jq .hook_latency_p95` < 100ms por handler.
>
> **Anti-goals firmes**: NÃO criar touring-hook subcmd novo; NÃO duplicar handlers; NÃO acoplar via Rust import direto; NÃO mexer em hooks existentes; NÃO tocar SDK do code mode (projeto irmão); NÃO tornar opt-in.

---

_v0.1 — 2026-08-31 18:46 BRT | Briah da complementação-hooks (nova frente, separada de code-mode-sinal) | Ratio 1.0 — todas as 7 operações presentes_