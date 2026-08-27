---
okf_version: "1.0"
type: Strategy
title: "Afordância de code mode — o transporte, o juiz, e o fim da guerra ao caso comum"
description: "Diagnóstico exaustivo da subutilização do code mode Touring + exploração profunda do code mode do deepseek-harness (clone /tmp/deepseek-harness @ dsh-0.1.1-rc.2) + 10 movimentos ordenados por U(a)=P·V−C."
plan_id: 2026-08-26-code-mode-diagnostico-afordancia
tags: [code-mode, afordancia, deepseek-harness, diagnostico, gates, n5-metric, orchestrate-sdk]
timestamp: 2026-08-26T21:05:00-03:00
authority: Gabriel Gadea
status: proposta — aguarda gate humano
---

# Afordância de code mode — o transporte, o juiz, e o fim da guerra ao caso comum

## §0 — O que este documento é

Diagnóstico completo pedido por Gabriel (26/08): *"code mode ainda está falhando e
sendo subutilizado"* + exploração profunda do code mode do deepseek-harness (4 fontes
nomeadas, lidas na íntegra no clone local) + *"precisamos de mais afordância"*.

Evidência: N5 miner (24 sessões, 31 transcripts) · arm.json (ledger de oferta em disco)
· gate-metrics vivo · 6 Agent Notes do dsh lidas na íntegra (code-mode 06-15,
typed-tool-returns 07-20, live-parallel 07-26, dispatch-log-spill 07-26,
language-dispatch 07-31, chat-subcall-rows 07-26) + fontes (ts-types.ts, py-types.ts,
code-runtime/src/types.ts, workflow-worker-thread README) + observação comportamental
desta própria sessão (denies, rewrites, advisories — ao vivo).

## §1 — O diagnóstico em números (medido, não inferido)

| Medida | Valor | Fonte |
|---|---|---|
| Adoção espontânea elegível | **26-28%** (alvo 60%) | auditoria 08-25, 1.528 calls |
| Rota seguida quando o gate dispara | **50%** (21/42, braço code) | `.claude/touring/code_mode_arm.json` |
| Headline N5 "injeção seguida" | 63.9% — **semântica suspeita** (§2) | `n5_injection_kpi.py` |
| Taxa verdadeira de rota-code no eixo | **52.8%** (877/1.660) | recomputado do N5 |
| Gate-fatigue (TOURING_GATE_OK) | 26 usos / 24 sessões (~1/sessão) | N5 S5b — saudável |
| `t3_turn_fused` | **0 — o T3-B nunca dispara** (PostToolUse fecha o turno) | medido 26/08 (P2c) |
| READONLY_HOOKS do orchestrate | **8 hooks** — o modelo é mais pobre DENTRO do sandbox | `run.rs:56-60` |
| X6 advisories | 5+ nesta sessão, 1 por capability | observação viva |
| `--brief` em arquivo de 60 linhas | elidiu 30 (sem piso) | observação viva |

## §2 — Defeito de medição: o juiz está medindo a coisa errada

O classificador do N5 (`n5_injection_kpi.py:137-148`):
- `Grep`/`Glob`/`Read` nativos → **"resisted"**
- Bash de inspeção (grep/cat) re-emitido → **"followed"**
- Bash com rota code (`touring run`/ctx_execute) → "code_route"

Ou seja: "followed" conta **ficar no Bash nativo** — o oposto da rota code. O headline
"63.9% de seguimento" mede **resistência disfarçada**. E a ferramenta nativa (muitas
vezes a resposta CERTA — o dsh prova, §3) é contada como falha. Toda afirmação de
adoção derivada deste KPI é suspeita até a correção. **Juiz antes do julgado.**

## §3 — O que o deepseek-harness tem que o Touring não tem (o coração da afordância)

Lidos na íntegra; cada item com evidência de arquivo.

1. **`run_code` É UMA TOOL no wire list** (feature note 06-15, decisão 1): nos modos
   `code`/`both` o registry contribui o transporte `run_code` + SDK `.d.ts` gerado no
   system prompt. **No Touring, code mode é lembrar de escrever
   `touring run --lang python --code '...'` dentro de uma string Bash** — com o
   escaping de aspas que me custou 2 erros nesta sessão. A hierarquia de custo está
   INVERTIDA: o caminho code custa mais tokens para digitar que a chamada atômica
   que substitui. `U(a)=P·V−C`: o C do code mode Touring é MAIOR que o nativo.
   (TOURING_MCP_CODE_MODE existe — W7, 3 tools — mas env-gated OFF por default.)
2. **SDK tipado gerado por assembly** (typed-tool-returns 07-20): `ToolArgsMap`/
   `ToolOutputMap` com tipos EXATOS por tool visível, byte-estável, degradando para
   `unknown` sem quebrar a assembly. Touring: stub ESTÁTICO de 9 métodos + 8 hooks.
3. **O SDK ensina economia, não só mecânica** (`ts-types.ts:250`, `py-types.ts:734`):
   bullets explícitos — *"Only what you print or return is program output… every
   other intermediate result stays out of the conversation, so extract just what you
   need"* + contrato de paralelismo + contrato `ToolCallError`. O stub Touring herdou
   o aviso STATIC STUB (mesmo texto do dsh!) mas não os bullets de economia.
4. **O dsh REJEITOU o modo sempre-exclusivo** (feature note, "Alternatives considered"):
   *"a coding agent's bread-and-butter single calls (bash, read, edit) are already
   ideal as native calls, and forcing every edit through a program taxes the common
   case."* O piloto `mode=code` do Touring NEGA chamada isolada de inspeção
   (cat/grep/find colapsam; o próprio texto do executor declara) — enquanto Read/Grep
   nativos ficam abertos. O deny empurra para a ferramenta nativa (contada como
   "resisted"!) ou cobra 1 round-trip de re-emissão. **O piloto luta contra a
   evidência dsh.**
5. **Observabilidade de sub-chamada** (live-parallel 07-26 + chat-subcall-rows 07-26):
   par `tool/code-dispatch-start`/settle por sub-chamada, linhas UI aninhadas
   (*"sub-calls ARE the story — hiding them re-creates the opacity"*), e **spill por
   sub-chamada** com preview+locator e backpressure (dispatch-log-spill 07-26: spill
   lento limita starts futuros). Touring: spill só do output externo; journal tem
   `origin: run_id:code:N` (a fiação existe) mas sem par start/settle, sem spill por
   sub-chamada.
6. **Paralelismo de sub-dispatch com o MESMO classificador do loop nativo**
   (`isConcurrencySafe`, `maxParallelSubCalls=10`, exclusivo drena o pool; o prompt
   SDK foi atualizado para o contrato VERDADEIRO — D8 puro). Touring: SDK síncrono,
   R9 sequencial, **um socket NOVO por `query()`** (`run.rs:68-71`) — K queries =
   K×RTT serial.
7. **Trust posture bash-equivalente POR DESIGN** (feature note §Trust): sem cerimônia
   de unsafe-acknowledgement, porque o bash já tem MAIS autoridade. O CEG do Touring
   X6-nega subprocess/file-write DENTRO do programa → advisory em quase todo programa
   bash não-trivial ('for', 'echo', 'python3', 'redirection' — 5+ nesta sessão).
   Advisory que dispara em tudo vira ruído.
8. **Fresh worker por run, env `{}`, sem pooling** — paridade com o CEG Touring
   (rlimits+landlock+env-clear). **Já temos.**
9. **Budget duplo** `computeMs` (busy-time, `eventLoopUtilization` — tool lenta não
   queima compute; loop quente morre) + `maxWallMs`. Touring W1 entregou
   compute_ms via /proc + wall. **Paridade.**
10. **`tools/pre-execute` NÃO reescreve args no dsh** (core/tools README: logged args
    desyncariam do que rodou; rewrite é nota PROPOSTA, não shipped). O Touring SHIPOU
    rewrite (G2/G8 via `updatedInput`) — divergência consciente a documentar: o CC
    registra `updatedInput` no transcript, a reconstrução sobrevive; fricção-zero é a
    doutrina D8. **Fica, documentado.**

## §4 — O que funciona no Touring (preservar)

Deny-com-rota-derivada (a rota carrega o programa REAL — segui toda vez nesta sessão)
· rewrite G8 fricção-zero (provado ao vivo 2×) · G6/G9/G10 com precisão correta ·
arm.json em disco (à prova de restart) · o minerador N5 existir (consertam-se as
classes, não o instrumento) · apresentação por escopo + kill switches + relax
por-comando · budget duplo (W1) · sandbox CEG real.

## §5 — A estratégia (10 movimentos, ordenados por U(a))

| # | Movimento | Conteúdo | KPI |
|---|---|---|---|
| **S2** | **FIX-METRIC (P0, juiz primeiro)** | Renomear classes N5 para o que SÃO (`bash_native`/`native_tool`/`code_route`); KPI verdadeiro = `code_route/(3 classes)` entre elegíveis + outcome pós-deny (rota tomada \| ferramenta nativa \| re-emissão \| bypass) | headline honesto |
| **S1** | **TRANSPORT (P0)** | (a) `TOURING_MCP_CODE_MODE` ON por default no MCP (as 3 tools existem, W7); (b) matar o quoting: código por **stdin/heredoc** (`touring run --lang python <<'EOF'`) e/ou `--code-file` | code_route share ≥70% (de 52.8%) |
| **S3** | **RECALIBRATE (P1)** | Parar de negar inspeção ISOLADA (posição dsh: single call é o caso ideal nativo); deny só para rajada/laço/agregado — absorver o predicado de rajada no caminho por-chamada (T3-B morto) | native_tool-pós-deny ↓; credibilidade dos denies reais ↑ |
| **S4** | **SURFACE (P1)** | READONLY_HOOKS 8→conjunto read completo (wiring-orphans/audit, gate-metrics, decompose ready, portfolio, explore-status, ast-meta/tdg/rust-semantic, memory-query, gotcha-match, health-delta, learning-status); stub GERADO da allowlist (fonte única) + bullets de economia (adaptação do SDK_INSTRUCTIONS) | queries/programa ↑; "hook not in allowlist" ↓ |
| **S5** | **OBSERVABILITY (P2)** | Par start/settle por `query()` no journal (origin já existe) + spill por sub-chamada (preview+locator, padrão D5) | journal com t/bytes/hook por sub-chamada |
| **S6** | **X6-FRICTION (P2)** | Agregar advisory a **1 por run** (não por capability) | advisories ≤1/run |
| **S7** | **BRIEF-FLOOR (P2)** | `--brief` não elide abaixo de ~200 linhas | 0 elisões espúrias |
| **S8** | **CONN-REUSE (P2)** | 1 socket por programa no orchestrate (barato; async `gather` fica para decisão medida de RTT) | RTT share do wall ↓ |
| **S9** | **DOCS-DIVERGENCE (P3)** | Documentar em `docs/code-mode.md`: rewrite fica (updatedInput no transcript preserva reconstrução); T3-B morto removido/absorvido | drift zero |
| **S10** | **T3-BURIAL (P3)** | Remover ou absorver o caminho T3-B (`t3_turn_fused=0` medido — gate morto é débito de doutrina) | 0 counters mortos |

## §6 — Compromissos de medição (standing)

code_route share ≥70% do eixo · outcome pós-deny "rota tomada" ≥70% (rebaseline pós-S2)
· native_tool-pós-deny ↓ após S3 · gate-fatigue <5% (manter) · X6 ≤1/run · adoção
elegível ≥60% (meta do piloto S2, reexpressa com a métrica corrigida).

## §7 — Riscos

(i) S3 relaxar demais → adoção cai por falta de gatilho — mitigar medindo code_route
share antes/depois no piloto analise. (ii) MCP default ON infla tool-list tokens —
custo conhecido do modo 'both' no dsh (prefix stability + cache amortizam); medir.
(iii) stdin/heredoc = escrita cega? Não — heredoc já passa hoje; o programa continua
no sandbox CEG. (iv) READONLY_HOOKS maior vaza escrita? Não — só verbos read-only,
allowlist fail-closed com erro-que-ensina.

## §8 — Ordem de execução proposta

S2 (juiz) → S1 (transporte) → S3 (parar denies falsos) → S4 (superfície+economia) →
S5 (observabilidade) → S6/S7 (fricção) → S8 (latência) → S9/S10 (docs/dívida).

Parte do [bundle](/index.md). Diagnóstico: [diagnostics/touring-20260826T173710.md](/diagnostics/touring-20260826T173710.md).
