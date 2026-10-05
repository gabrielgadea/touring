#!/bin/bash
# audit-plan-completion.sh — os entregáveis do gate de integridade da prosa (04/10/2026), provados por EXECUÇÃO.
#
# Consumido pela cláusula `cross_audit` do loop_converged.py (exit 0 = completo). O modelo de 20/09 declarava o
# limite de checar texto com `grep`; aqui cada item roda o artefato: o hook com payload real, o critério sobre os
# rótulos commitados, o hash do pré-registro, a varredura do repo. Os fragmentos corrompidos vão como escapes
# \u, para que este arquivo não seja ele mesmo uma detecção do gate.
set -euo pipefail
R=/home/gabrielgadea/projects/touring
B="$R/docs/plans/2026-10-04-integridade-prosa"
GATE="$HOME/.claude/skills/Touring/scripts/hooks/prose_integrity_gate.py"
fail() { echo "FALTA: $1"; exit 1; }
ok() { echo "  ok: $1"; }

# 1. O pré-registro é o que foi commitado antes dos dados (sha256 registrado em 259ac249 e na memória).
[ "$(sha256sum "$B/pre-registro.md" | cut -d' ' -f1)" = 7fbb91ef979d2c534fa6782dc9252e0c11870899a4cb7ab6acf032bf67d4440d ] \
  || fail "pre-registro.md mudou depois de registrado"
ok "pré-registro intacto"

# 2. O hook existe, passa nos seus controles e na suíte própria.
python3 "$GATE" --controls >/dev/null || fail "controles do detector reprovaram"
python3 -m pytest -q -p no:cacheprovider "$(dirname "$GATE")/test_prose_integrity_gate.py" >/dev/null 2>&1 \
  || fail "suíte do gate vermelha"
ok "controles e suíte do gate"

# 3. Registrado no settings.json para as cinco ferramentas.
python3 - <<'EOF' || fail "hook ausente do settings.json"
import json, os
s = json.load(open(os.path.expanduser("~/.claude/settings.json")))
hits = [h for g in s["hooks"]["PreToolUse"] for h in g.get("hooks", [])
        if "prose_integrity_gate.py" in h.get("command", "")
        and all(t in g.get("matcher", "") for t in ("SendMessage", "Write", "Edit", "MultiEdit", "NotebookEdit"))]
assert len(hits) == 1, hits
EOF
ok "registro no settings.json"

# 4. O comportamento nos três modos, com log temporário (nunca o log vivo).
TMP=$(mktemp -d)
P='{"tool_name":"SendMessage","tool_input":{"to":"x","message":"a \u8def\u5f84 b"}}'
out=$(TOURING_PROSE_GATE_LOG="$TMP/e.jsonl" python3 "$GATE" <<<"$P"); rc=$?
[ $rc -eq 0 ] && grep -q U+8DEF <<<"$out" || fail "sombra não avisou ou bloqueou (rc $rc)"
set +e; TOURING_PROSE_GATE_MODE=block TOURING_PROSE_GATE_LOG="$TMP/e.jsonl" python3 "$GATE" <<<"$P" >/dev/null 2>&1; rc=$?; set -e
[ $rc -eq 2 ] || fail "block não bloqueou o SendMessage (rc $rc)"
out=$(TOURING_PROSE_GATE_DISABLED=1 TOURING_PROSE_GATE_MODE=block python3 "$GATE" <<<"$P")
[ -z "$out" ] || fail "kill switch não silenciou"
[ "$(wc -l < "$TMP/e.jsonl")" -eq 2 ] || fail "o log não tem os 2 eventos esperados"
rm "$TMP/e.jsonl" && rmdir "$TMP"
ok "sombra avisa, block bloqueia o SendMessage, kill switch silencia"

# 5. O veredito publicado é reproduzível a partir dos rótulos commitados.
v=$(python3 "$GATE" --criterion "$B/labels-replay-sendmessage.jsonl")
python3 -c "import json,sys; r=json.loads(sys.argv[1]); assert (r['n'],r['legit_or_unclear'],r['promote'])==(25,5,False), r" "$v" \
  || fail "critério não reproduz n 25 / LEGIT 5 / promote false: $v"
ok "critério reproduz o veredito do README"

# 6. A corrupção limpa não voltou ao repo, e o espelho é o vivo.
python3 - <<'EOF' || fail "corrupção limpa reapareceu"
import pathlib
R = pathlib.Path("/home/gabrielgadea/projects/touring")
gone = {
  "crates/touring-storage/src/embeddings/providers/voyage.rs": "the\u516c\u5171",
  "crates/touring-assists/tests/e2e_assist_pipeline.rs": "foo\u8fbe\u6210",
  "crates/touring-cortex/GAP_ANALYSIS.json": "\u8bef\u89e3",
  "client/skills/Touring/references/vp-scout-examples.md": "\u0441\u0432\u044f\u0437\u044b\u0432\u0430\u043d\u0438\u0435",
  "docs/2026-06-25-harness-consolidation-master-plan.md": "Para\u8282\u7701",
  "scripts/jev_tags_sample.py": "G1\u7d14\u7c8b",
}
bad = [f for f, frag in gone.items() if frag in (R / f).read_text(encoding="utf-8")]
assert not bad, bad
EOF
cmp -s "$GATE" "$R/client/skills/Touring/scripts/hooks/prose_integrity_gate.py" || fail "espelho do hook diverge do vivo"
ok "limpeza mantida e espelho idêntico"

echo "audit-plan-completion: 6/6"
