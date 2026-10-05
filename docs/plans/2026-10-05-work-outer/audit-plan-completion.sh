#!/bin/bash
# audit-plan-completion.sh — release 30.4.92 (05/10/2026), provada por EXECUÇÃO.
#
# Consumido pela cláusula `cross_audit` do loop_converged.py (exit 0 = completo) para a tarefa
# task_1791211374258794852. Tudo aqui é leitura ou roda em diretório temporário: nenhum plano é criado no DAG vivo
# e nenhum projeto é atualizado (a propagação é do rito; aqui só se confere o que ela deixou).
set -euo pipefail
R=/home/gabrielgadea/projects/touring
B="$R/docs/plans/2026-10-05-work-outer"
fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }
cd "$R"

# 1. O relatório registra a release e o que a cláusula cross_audit achou.
grep -q '^### 9.2 A cláusula `cross_audit` passa a rodar' docs/audits/cross-audit-2026-10-04.md || fail "relatório sem o §9.2"
grep -q '^### 9.3 Release 30.4.92 e propagação' docs/audits/cross-audit-2026-10-04.md || fail "relatório sem o §9.3"
ok "relatório §9.2 e §9.3"

# 2. Os dois censos pelo binário de release: --help nunca executa, e -j/--json é aceito depois de todo subcomando.
python3 -m pytest -q -p no:cacheprovider scripts/test_help_never_executes.py >/dev/null 2>&1 \
  || fail "censo do --help ou do -j reprovou contra target/release/touring"
ok "censos do --help e do -j"

# 3. O binário de release aceita o -j depois do subcomando (sem daemon; o caso `--` é o teste unitário de cli::common).
out="$(env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK target/release/touring decompose get X -j --help 2>&1)" \
  || fail "decompose get X -j --help saiu não-zero"
echo "$out" | grep -q 'unexpected argument' && fail "decompose get X -j ainda rejeita a flag global"
ok "decompose get X -j --help responde ajuda"

# 4. A toolchain default é a 30.4.92 e os projetos pinados resolvem para ela, com o 30.4.90 reprovando as sondas.
[ "$(cat "$HOME/.touring/default")" = "30.4.92" ] || fail "toolchain default não é 30.4.92"
TOURING_CODE_MODE=native python3 "$B/prove_projects.py" 2>&1 | grep -q '^VEREDITO: PROVADO' \
  || fail "prova por projeto (binário do projeto + controle negativo 30.4.90) não deu PROVADO"
ok "default 30.4.92 e prova por projeto com controle negativo"

# 5. O texto do Stop hook nomeia um loop_marker.py que existe (a demanda de handoff ensina a correção).
python3 -m pytest -q -p no:cacheprovider "$HOME/.claude/skills/loop-engineering/scripts/hooks/test_flow_guard.py" \
  >/dev/null 2>&1 || fail "test_flow_guard.py vermelho"
python3 scripts/sync-client-skills.py --check >/dev/null 2>&1 || fail "espelho client/ diverge do vivo"
ok "guard de handoff e espelho"

# 6. Os juízes das duas tarefas do ciclo anterior convergiram com o cross_audit RODANDO (não N/A).
python3 - <<'EOF' || fail "juiz anterior não convergiu ou cross_audit ainda N/A"
import json
for p in ("docs/plans/2026-10-04-work-outer/judge-2026-10-05.json",
          "docs/plans/2026-10-04-integridade-prosa/judge-2026-10-05.json"):
    d = json.load(open(p))
    assert d.get("converged") is True, (p, d.get("unmet"))
    ca = d["clauses"]["cross_audit"]
    assert ca["result"] == "PASS", (p, ca)
EOF
ok "juízes do ciclo anterior convergidos com cross_audit real"

echo "audit-plan-completion: 6/6"
