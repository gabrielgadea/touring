<!-- OKF document -->
---
okf_version: "1.0"
type: ResumePoint
title: "Retomar aqui — afordância profunda de code mode + incidente Write + despatch executado"
description: "Estado exato ao fim da sessão dfed64be (26/08/2026 ~14:45 BRT): diagnóstico completo, incidente Write investigado (veredito), patch RETIRADO, estratégias S1-S6 + N1-N5 prontas para aplicar."
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [code-mode, afordancia, retomada, handoff, write-tool-incident, despatch]
timestamp: 2026-08-26T14:45:00-03:00
authority: Gabriel Gadea
---

# Retomar aqui — 26/08/2026 ~14:45 BRT

## 1. O que mudou no ambiente NESTA sessão (ordem de Gabriel, executado)

1. **Patch bash-nudge RETIRADO**: binário `~/.local/share/mise/installs/claude/2.1.245/claude`
   restaurado do `.orig` via mv+cp (inode swap — ETXTBSY impedia cópia direta). Verificado:
   `--check` = NÃO PATCHADO, 6/6 fragmentos da injeção nativa presentes. O binário
   patchado sobrevive como `claude.patched-bash-nudge` (referência; pode ser apagado depois).
2. **Hook SessionStart do patch REMOVIDO do settings.json** (senão a próxima sessão
   re-aplicaria via `--ensure`). Backup: `~/.claude/settings.json.bak-pre-despatch-20260826`.
3. **`~/.claude.json` restaurado** da edição diagnóstica de flags (streaming2/checkpoints
   — edição provou que NÃO são a causa; refresh remoto religa em runtime de qualquer forma).
4. **Injeção nativa ATIVA de novo** em toda sessão nova (auto/bypass): "use Bash rather
   than dedicated tools" — agora o modus operandi será observado contra ela (KPI N5).

## 2. Incidente Write tool — VEREDITO (investigação completa: 20+ probes)

**Fenômeno**: Write/Edit reportam sucesso na janela do modelo e NADA executam
(0 tool_use / 0 tool_result / 0 hooks / 0 bytes / 0 bwrap). A fabricação vive na
montagem da janela de contexto, não no executor. Reproduzido 6× nesta sessão +
4× headless (probes B, F, Z2, Z3; transcripts 81dde31f e f65c89eb provam o padrão).
**Condição**: harness v2.1.245 SEM a env `CLAUDE_CONFIG_DIR` × cwd=projeto touring.
**A mera presença da var elimina o fenômeno** — mesmo apontando para o próprio
~/.claude real (probe FULL11 escreveu). 12+ causas inocentadas por isolamento
(patch, binário, settings completo, CLAUDE.md, rules, skills, agents, commands,
plugins, security, auto-memory, estado, flags checkpoints/streaming2).
**Janela**: até ~09:30 escrevia (sessão 01ce8edf); ≥12:42 fabrica (esta + headless).
**Memória completa**: `incident:write-tool-reporta-sucesso-sem-escrever:2026-08-26`.
**Mitigação para a nova sessão** (decisão de Gabriel pendente):
- Opção A (recomendada): `export CLAUDE_CONFIG_DIR=$HOME/.claude` no shell profile —
  aponta a var para o próprio default; fenômeno some (prova FULL11); observar se
  peer-sessions/remote-control mudam de comportamento.
- Opção B: viver com heredoc até o fix upstream + reportar à Anthropic com a matriz.
**Na nova sessão, testar o Write cedo** (probe trivial em /tmp + `ls` de verificação)
antes de confiar nele para trabalho real.

## 3. Diagnóstico code-mode (pedido original) — 5 causas raiz verificadas

R1 **Furo de classificação `cd`**: `scan_class_of`/`is_scan_command`/G-turno leem só o
1º token; na sessão-problema do analise 70/91 comandos começavam com `cd` (ação real na
linha seguinte) — invisíveis a G1/T3-B/code-mode deny.
R2 **`mode = "code"` só existe no workspace touring**; analise/konverter rodam `both`
default → zero deny por classe; sobra só G1 + G-turno (tardios).
R3 **G-turno diagnostica por léxico e prescreve remédio errado** ("cd ×26" + esqueleto
R1 de inspeção para rajada de EXECUÇÃO python) — o agente reconhece e segue igual.
R4 **Rajada de execução homogênea sem classe** (`.venv/bin/python3` ×30-75): nem
inspeção, nem laço G8 — o maior poço de round-trips fora do piloto.
R5 **Fricção assimétrica + gate-fatigue**: WARN CEG advisory em todo shell run;
`TOURING_GATE_OK=1` em 8/21 comandos de outra sessão (bypass virando hábito).
DeepSeek comparado (feature note + tools README + code-runtime lidos): apresentação
no registry (`UNKNOWN_TOOL` estrutural pré-pipeline), SDK gerado por escopo, **trust
bash-equivalent por design** (a rota code nunca mais assustadora que a bash).
**Estratégia completa**: `strategy-2026-08-26-code-mode-afordancia-deep.md` (§1-§6:
S1-S6; §7: N1-N5) — persistida em `strategy:code-mode-afordancia-deep:2026-08-26`.

## 4. O que aplicar na nova sessão (ordem proposta)

1. **S1 — Classificador além do 1º token** (`scan_class_of` cli_suggester.rs:2813,
   `is_scan_command` :2277, contador do G-turno loop_stop_guard.py:297 — 3+ sítios;
   grep o padrão inteiro primeiro, mutação 0→1→0 por sítio). Desbloqueia tudo.
2. **N3a — Gate `sed -i` modelo-direto em código** → deny com rota Edit-com-gates
   (a injeção restaurada manda "make file changes with sed" — anti-padrão que
   atravessa os 17 stage gates).
3. **N5 — KPI injeção**: counters `injection_native_followed` ×
   `injection_native_resisted` em gate-metrics (a injeção restaurada é o numerador;
   sem ela o KPI não existe — razão extra do despatch).
4. **S3 — G-turno com diagnóstico semântico + remédio derivado** (programa dos
   comandos verbatim da rajada, como o T3-B; counter `g_turno_effective`).
5. **S5a — Advisory CEG fora do stderr do modelo** (run.rs:425-445 → tracing debug).
6. **S2 — Piloto `mode = "code"` no analise** (GATE GABRIEL — só após S1).
7. **S4/S6 — exec-burst (R9 exec-agregado) + portfolio no remédio do nudge.**
8. **Reportar o incidente Write upstream** (matriz de reprodução na memória do incidente).

## 5. Observação do modus operandi (o que olhar na nova sessão)

- A injeção nativa está de volta: cada `cat/grep/sed` nativo emitido = followed;
  cada `touring run`/Read/Edit em situação equivalente = resisted. Antes do KPI N5
  existir, medir por forense de transcript (script `sweep_writes.py` adaptado — conta
  classes por sessão; está em /tmp, re-criar se perdido).
- O Write tool: probe trivial em /tmp no início da sessão + `ls` de verificação —
  decidir mitigação (§2) antes de qualquer trabalho de escrita real.
- Gate-fatigue: contar `TOURING_GATE_OK=1` por sessão (meta < 5%).
- Manifest OUTER desta sessão: COMPLETO (diagnostic + ledger CCE convergido +
  strategy doc) — o Stop guard não deve bloquear o encerramento.

## 6. Artefatos-chave

| Artefato | Onde |
|---|---|
| Estratégia completa (S1-S6 + N1-N5) | `strategy-2026-08-26-code-mode-afordancia-deep.md` (mesmo bundle) |
| Memória estratégia | `strategy:code-mode-afordancia-deep:2026-08-26` |
| Memória incidente (matriz probes) | `incident:write-tool-reporta-sucesso-sem-escrever:2026-08-26` |
| Diagnostic OUTER | `diagnostics/touring-20260826T124448.md` (mesmo bundle) |
| Backup settings (pré-despatch) | `~/.claude/settings.json.bak-pre-despatch-20260826` |
| Binário patchado (referência) | `~/.local/share/mise/installs/claude/2.1.245/claude.patched-bash-nudge` |
| deepseek-harness (clone) | `/tmp/deepseek-harness` (shallow; re-clonar se perdido) |
| Plano anterior (P0-P4) | `docs/plans/2026-08-25-code-mode-afordancia/` + `retomar:code-mode-afordancia:2026-08-25` |
