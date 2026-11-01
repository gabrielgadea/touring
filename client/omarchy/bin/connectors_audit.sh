#!/usr/bin/env bash
# connectors_audit.sh — censo de saúde dos conectores/MCP desta máquina.
#
# Uso: connectors_audit.sh [--out PATH] [--expect a,b,c]
#   --out     destino do JSON (default: ~/Work/state/connectors.json)
#   --expect  nomes que o plano nomeia e que devem constar mesmo se ausentes
#
# ── Por que um censo e não "os três do plano" ─────────────────────────────────
# O plano (S-8.2) nomeia Gmail/Calendar/Notion. Nenhum dos três está configurado
# nesta máquina (medido 23/08/2026). Auditar só três nomes ausentes produz um
# artefato verdadeiro e inútil; escolher a dedo três que estejam verdes seria
# selecionar o sujeito para caber no resultado. O censo completo evita as duas
# coisas: registra TODOS os conectores, e os três nomeados aparecem como
# `not-configured` — que é a resposta correta à pergunta que o plano faz.
#
# ── Por que `claude mcp list` é a chamada read-only ───────────────────────────
# Ele imprime "Checking MCP server health…" e tenta conectar em cada servidor:
# é uma sonda AO VIVO, não um cache — provado no mesmo dia por um servidor que
# devolveu `HTTP 502` em tempo real enquanto os demais respondiam. Para um MCP
# remoto autenticado, "Connected" já significa que o OAuth foi aceito no
# handshake, então uma chamada extra a uma ferramenta leria dados do usuário sem
# acrescentar prova. Sonda o transporte E a credencial, sem tocar no conteúdo.

set -euo pipefail

OUT="$HOME/Work/state/connectors.json"
EXPECT="Gmail,Google Calendar,Notion"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out)    OUT="${2:?--out needs a path}"; shift 2 ;;
        --expect) EXPECT="${2:?--expect needs a list}"; shift 2 ;;
        --help|-h) sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) printf 'connectors_audit.sh: unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

if ! command -v claude >/dev/null 2>&1; then
    printf 'connectors_audit.sh: claude not on PATH\n' >&2
    exit 1
fi

mkdir -p "$(dirname "$OUT")"
raw="$(claude mcp list 2>&1 || true)"

# Heredoc quoted (<<'PY'): o corpo Python contém apóstrofos em comentários, que
# dentro de `python3 -c '...'` fechariam a string do bash — foi exatamente o bug
# que impediu a primeira execução deste script (line 54: `o: command not found`).
RAW="$raw" OUT="$OUT" EXPECT="$EXPECT" python3 - <<'PY'
import json, os, re
from datetime import datetime, timezone

raw = os.environ["RAW"]
rows = []
seen = set()
# "  <name>: <target> - <verdict>"  (o verdict pode conter " - ", então split 1x
# a partir da direita e' errado; o marcador e' o símbolo de estado)
line_re = re.compile(r"^(?P<name>[^:]+):\s+(?P<target>.*?)\s+-\s+(?P<verdict>[✔✘!].*)$")
for line in raw.splitlines():
    m = line_re.match(line.strip())
    if not m:
        continue
    name = m.group("name").strip()
    verdict = m.group("verdict").strip()
    if verdict.startswith("✔"):
        status = "ok"
    elif verdict.startswith("!"):
        status = "needs-auth"
    else:
        status = "failed"
    seen.add(name.lower())
    rows.append({
        "name": name,
        "target": m.group("target").strip(),
        "status": status,
        "detail": verdict[:200],
    })

# Os nomes que o plano cobra entram mesmo ausentes: um conector que nao existe e'
# um achado, e omiti-lo faria o artefato responder outra pergunta.
for want in [w.strip() for w in os.environ["EXPECT"].split(",") if w.strip()]:
    if not any(want.lower() in s for s in seen):
        rows.append({
            "name": want,
            "target": "",
            "status": "not-configured",
            "detail": "named by the plan (S-8.2) but not present in `claude mcp list`",
        })

rows.sort(key=lambda r: (r["status"] != "ok", r["name"].lower()))
doc = {
    "ts": datetime.now(timezone.utc).isoformat(),
    "probe": "claude mcp list (live connection attempt per server)",
    "counts": {s: sum(1 for r in rows if r["status"] == s)
               for s in ("ok", "needs-auth", "failed", "not-configured")},
    "connectors": rows,
}
with open(os.environ["OUT"], "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2, ensure_ascii=False)
    fh.write("\n")
for r in rows:
    print(f'{r["status"]:15} connector:{r["name"]}')
print(f'-- {doc["counts"]["ok"]} ok, written to {os.environ["OUT"]}')
PY
