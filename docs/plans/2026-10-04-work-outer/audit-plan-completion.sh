#!/bin/bash
# audit-plan-completion.sh — consertos do cross-audit de 04/10/2026 (X1..X19, R1..R6), provados por EXECUÇÃO.
#
# Consumido pela cláusula `cross_audit` do loop_converged.py (exit 0 = completo) para a tarefa
# task_1791142397213803492. Tudo aqui é leitura ou roda em diretório temporário: nenhum plano é criado no DAG vivo
# (sondas que criam fixtures no banco vivo foram o que deixou a linha 'validation-final-task' de maio).
set -euo pipefail
R=/home/gabrielgadea/projects/touring
H="$HOME"
TASK=task_1791142397213803492
fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }
cd "$R"

# 1. O relatório tem o fechamento e o veredito do juiz ficou no bundle.
grep -q '^### 9.1 Revisor de contexto limpo e juiz final' docs/audits/cross-audit-2026-10-04.md || fail "relatório sem o §9.1"
python3 -c "import json; assert json.load(open('docs/plans/2026-10-04-work-outer/judge-followups-2026-10-04.json'))['converged'] is True" \
  || fail "veredito do juiz ausente ou não convergido"
ok "relatório e veredito"

# 2. X1/X2: `workflow resume` chega ao handler próprio e conta subtarefas, não tentativas.
python3 - "$TASK" <<'EOF' || fail "workflow resume não responde pelo handler de resume"
import json, subprocess, sys
r = json.loads(subprocess.run(["touring", "workflow", "resume", sys.argv[1], "-j"], capture_output=True,
                              text=True, timeout=60).stdout)
assert r.get("event") == "workflow_resume", r.get("event")
assert r.get("total_count") == 12 and r.get("completed_count") == 12, (r.get("total_count"), r.get("completed_count"))
assert len(r.get("subtasks", [])) == 12, len(r.get("subtasks", []))
EOF
ok "workflow resume (12/12 pela tarefa do ciclo)"

# 3. X8/R3: uma tarefa concluída não lista nada como bloqueado.
python3 - "$TASK" <<'EOF' || fail "ready lista concluída como bloqueada"
import json, subprocess, sys
r = json.loads(subprocess.run(["touring", "decompose", "ready", sys.argv[1]], capture_output=True, text=True,
                              timeout=60).stdout)
assert r.get("blocked_subtasks") == [] and r.get("ready_subtasks") == [], r
EOF
ok "decompose ready sem terminais em blocked"

# 4. X13: o purge de palavra de escopo persistiu.
python3 - <<'EOF' || fail "purge-phantom ainda acha linhas"
import json, subprocess
r = json.loads(subprocess.run(["touring", "wiring", "repair", "--purge-phantom", "--dry-run"], capture_output=True,
                              text=True, timeout=120).stdout)
assert r.get("edges", 0) == 0, r
EOF
ok "purge-phantom em dry-run: 0"

# 5. X19/R1: nenhum comando da tabela executa ao receber --help (censo pelo binário de release).
python3 -m pytest -q -p no:cacheprovider scripts/test_help_never_executes.py >/dev/null 2>&1 \
  || fail "algum comando não respondeu --help com ajuda"
ok "censo do --help (141 comandos)"

# 6. X6/X11/R2, X9, X3/X15, X14/R5: as suítes herméticas dos consertos.
python3 -m pytest -q -p no:cacheprovider scripts/test_propagate_release_s1a_gate.py scripts/test_r3_a1_kpi.py \
  agent-harness/tests/test_core.py >/dev/null 2>&1 || fail "suíte de scripts/harness vermelha"
python3 -m pytest -q -p no:cacheprovider "$H/.claude/skills/loop-engineering/scripts/test_phase_close_index.py" \
  "$H/.claude/skills/loop-engineering/scripts/test_diagnose_index.py" \
  "$H/.claude/skills/Touring/scripts/test_okf_index_written.py" >/dev/null 2>&1 || fail "suíte de indexação dos escritores vermelha"
python3 scripts/sync-client-skills.py --check >/dev/null 2>&1 || fail "espelho client/ diverge do vivo"
ok "suítes dos consertos e espelho"

echo "audit-plan-completion: 6/6"
