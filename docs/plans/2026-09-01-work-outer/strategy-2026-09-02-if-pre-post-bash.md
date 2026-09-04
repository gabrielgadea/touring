---
type: Strategy
title: "Decisão: o `if` de pre-bash/post-bash em ~/.claude/settings.json (F0.3)"
description: "Canvas de decisão (9 seções) — o filtro `if` é sintaxe inválida no Claude Code (regra única, sem operadores lógicos; fail-open em comando não parseável); recomendação B: remover dos dois hooks. Human gate: Gabriel."
tags: [strategy, decision-canvas, hooks, f0.3, settings-json, signal-layer]
timestamp: 2026-09-02T00:20:00-03:00
plan_id: 2026-08-31-complementacao-hooks
---

# Decisão: remover o `if` de `pre-bash` e `post-bash`

Bundle de origem: `../2026-08-31-complementacao-hooks/RETOMAR-AQUI.md` (F0.3). Memória: `f0.3:if-semantica-fechada:2026-09-02`.

## §1 Decisão

Escolher entre remover o filtro `if` dos dois hooks Bash do Touring, remover só do `post-bash`, reescrevê-lo em sintaxe válida, ou manter.

## §2 Contexto (evidência)

- Context7 `/websites/code_claude` — `hooks`: o `if` aceita **uma única regra de permissão sem operadores lógicos**; alternativas exigem handlers separados. `hooks-guide`: **falha aberto** se o comando não é parseável. `debug-your-config`: edições em `settings.json` valem na sessão corrente.
- Registro atual (ambos os hooks): `"if": "Bash(cargo *|rustc *|touring *|cd *rust*|*touring*|*cargo*|*rustc*)"` — inválido como regra única.
- Sondas controladas (trace `hook_trace.jsonl`, `stdin_bytes` como id): `touring --version`, `cargo --version`, `echo cargo`, `echo <controle>` → 0 `pre-bash` / 0 `post-bash`; `for … $(date) … | tr` sem cargo/touring → `pre-bash` disparou; heredoc e for-loop anteriores → `post-bash` disparou. O filtro seleciona por parseabilidade, não por conteúdo.
- Origem: `docs/internal/sessions/PLAN-diagnostic-precision-pln2.md` FIX-S6/RC9 — o próprio plano decidiu "fix no binário é mais robusto — o settings.json filter é fragile". Handlers atuais não spawnam `cargo`.
- Custo medido do `pre-bash` via shim: 14–17 ms/chamada; já rodam 5 hooks pré + 4 pós por Bash.

## §3 Opções

A. remover só do `post-bash` · **B. remover dos dois** · C. handlers separados com um `if` válido cada (`Bash(cargo *)`, `Bash(touring *)`, `Bash(rustc *)`) · D. status quo.

## §4 Trade-offs

| Opção | Ganho | Custo/risco |
|---|---|---|
| A | mirror, `bash_outcomes`, reward RL e métricas de teste veem todo Bash; zero deny novo | `pre-bash` segue infra morta |
| B | tudo de A + `pre-bash` como desenhado (recall silencioso, gate de comandos perigosos, pausa cargo sob memória RED) | +2 spawns/Bash; deny novo em comando que COMEÇA por `rm -rf/-r/-f`, `dd`, `killall`, `sudo rm`, `mkfs`, `chmod 000`, `curl … \| sh` |
| C | determinístico e restrito | contradiz o desenho do `pre-bash` (deny list mira `rm`/`dd`/`sudo`, nunca cargo); filtro por conteúdo já existe no binário (`classify_bash_command`) |
| D | nada muda | dois hooks disparando ao acaso; KPIs `hooks_complement`/`signal_use` sem dado |

## §5 Stakeholders

Gabriel (dono do `settings.json`, global: toda sessão e projeto, incl. `analise`/`konverter` via shim 30.4.30) · sessões CC concorrentes · daemons (+2 RPC/Bash) · consumidores dos KPIs e do laço RL.

## §6 Riscos da opção B

1. Deny falso-positivo do `pre-bash` — o validador olha só o primeiro comando (`extract_command_short`); `cd x && rm -rf y` passa, `rm -f arquivo` direto é negado. Mitigação: razão explícita; estreitar o regex `-f\s+` no binário sob TDD (D8).
2. Pausa de cargo sob memória RED (W-540) — protetivo; a razão nomeia a causa.
3. Latência/carga ~30 ms/Bash — se relevante, `async: true` no `post-bash` (docs).

## §7 Reversibilidade

Total e barata: recolocar a linha; efeito imediato na sessão corrente.

## §8 Recomendação

**Opção B, confiança 0,85.** Filtro inválido → comportamento aleatório; handlers desenhados para todo comando e já filtram por conteúdo onde importa; o motivo histórico (RC9) não existe mais e o plano que o propôs o rejeitou; custo = dois spawns de ~15 ms; riscos são fricção com remédio no binário. Para o `pre-bash` a escolha é binária (tudo ou nada): C não faz sentido para um gate que mira `rm`/`sudo`.

## §9 Perguntas abertas

1. Frequência de `rm -rf|-f` como primeiro comando — 1 semana de denies no trace decide se o regex estreita.
2. Latência real por turno em rajadas — p50 > 50 ms → `async: true` no `post-bash`.
3. `PostToolUse` só dispara em sucesso → caminho de falha do `post-bash` morto por construção; registrar em `PostToolUseFailure` é decisão separada.
4. Após a remoção, numerador do `hooks_complement` por origem (F9).

## Decisão e execução

**Gabriel aprovou a opção B em 02/09/2026 ~00:50 BRT** ("aprovo B, remova o if dos dois e prove com o trace").
Aplicado às ~00:55 BRT: 2 linhas `"if": …` removidas (round-trip JSON lossless, backup
`~/.claude/settings.json.bak-20260901T235541-if-removal`). Prova na mesma sessão, sem restart:
`touring index find SignalPipeline` lançou `pre-bash` (PreToolUse) e `post-bash` (PostToolUse) no `hook_trace.jsonl`;
mirror 75→76 (`index_find`); `hook_dispatch_by_name` `pre-bash=11`/`post-bash=6`. Perguntas abertas §9 seguem como
observação de 1 semana. Memória: `f0.3:if-removido-provado:2026-09-02`.
