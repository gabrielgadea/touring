---
type: MeasurementPlan
title: S2 — medição do piloto mode=code (analise)
description: baseline, prova comportamental e o gate de adoção elegível >60% do piloto code-mode do analise (30.4.15)
tags: [s2, piloto, code-mode, medicao]
plan_id: task_1787767900017576294
---

# S2 — Piloto `mode = "code"` no analise: medição

## Baseline pré-piloto (26/08 ~15:50, daemon do analise, 30.4.14)

- `adoption_touring_count = 0` · `adoption_antipattern_count = 10` · `bash_calls_total = 42`
  (counters do daemon per-project desde o último restart — janela curta)
- Histórico (auditoria 25/08): adoção elegível 26-28%; sessões-referência 67 Bash / 0 `touring run`.
- Eixo injeção nativa (N5, corpus touring): overall 52,5% (63,7% com classificador S1-completo);
  gate-fatigue: `TOURING_GATE_OK=1` ×24.

## Prova comportamental pós-deploy (regra 10 — nunca por rótulo)

```bash
cd /home/gabrielgadea/projects/analise
echo '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"grep foo src/"},"session_id":"s2-proof"}' \
  | ./.touring/bin/touring-hook cli-suggest     # → deny [CODE MODE] com a rota derivada
./.touring/bin/touring run --lang bash --code 'echo hi' \
  2>/tmp/s2_stderr.txt | python3 -c 'import json,sys; print("ceg_advisory" in json.load(sys.stdin))'  # → True
grep -c "advisory deny" /tmp/s2_stderr.txt       # → 0 (S5a)
TOURING_DAEMON_SOCKET=$PWD/.touring/daemon.sock touring gate-metrics -j \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print("gate_fatigue" in d, "native_injection_followed_count" in d)'  # → True True
```

## Gate do piloto (fecha nas próximas sessões do analise)

- **adoção elegível > 60%** (hoje 26-28%): ler `adoption_*` no socket do analise
  (elegível = antipattern + touring; meta: touring/(touring+antipattern) > 0.60);
- **gate-fatigue < 5%**: `gate_fatigue.bypass_ratio` no snapshot do daemon do analise;
- **injeção nativa followed → ~0** no corpus do analise:
  `python3 scripts/n5_injection_kpi.py --dir ~/.claude/projects/-home-gabrielgadea-projects-analise/`
  (rodar após N sessões de piloto e comparar com o baseline acima).
