#!/usr/bin/env bash
# Completion evidence for detector fixes, J18 and the 13-orphan task.
set -euo pipefail
R=${HOME}/projects/touring
TASK=task_1794353972767264831
fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }
cd "$R"

test -f docs/plans/2026-11-10-decisoes-pendentes/RELATORIO-4b.md || fail "relatório 4b"
test -f docs/plans/2026-10-08-vgp-motor-base/diagnostics/j18/RELATORIO.md || fail "relatório J18"
test -f docs/plans/2026-11-10-decisoes-pendentes/RELATORIO-orfaos.md || fail "tabela dos 13 órfãos"
ok "três relatórios presentes"

for commit in f558fb57 c9729981 a28615da; do
  git merge-base --is-ancestor "$commit" HEAD || fail "commit $commit ausente de HEAD"
done
ok "revisão 4b, correções e J18 em HEAD"

python3 - "$TASK" <<'EOF' || fail "decompose ready ainda tem trabalho"
import json
import subprocess
import sys

result = json.loads(
    subprocess.run(
        ["touring", "decompose", "ready", sys.argv[1]],
        capture_output=True,
        text=True,
        timeout=60,
        check=True,
    ).stdout
)
assert result.get("ready_subtasks") == [], result
assert result.get("blocked_subtasks") == [], result
EOF
ok "decompose ready e blocked vazios"

git diff --check || fail "whitespace/conflict markers"
ok "git diff --check"

echo "audit-plan-completion: 4/4"
