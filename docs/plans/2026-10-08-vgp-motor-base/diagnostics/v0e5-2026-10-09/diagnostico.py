"""Por que o controle positivo NAO foi achado: o detector nao enxerga, ou o corpus nao tem?

Distingue as tres possibilidades, cada uma por evidencia propria:
  (a) o transcript e7c33fba existe e tem o `pgrep` — o problema e o MEU filtro;
  (b) o transcript existe mas o tool_result do pgrep nao tem envelope — o problema e a assinatura;
  (c) o transcript nao tem o pgrep no periodo — o problema e a janela.

Alem disso: a conservacao 1854 vs 1855 (falta 1 negacao) e onde ela esta.

Uso: python3 diagnostico.py
"""

import json
import re
import sys
from datetime import datetime
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS, _text  # noqa: E402
from v0e_negacoes import HOOK_DENIAL, COMPILED  # noqa: E402

G1_PAT = COMPILED[0][1]
DEPLOY_94 = "2026-10-09T14:16:43Z"
ALVO = "e7c33fba"


def main():
    arqs = [f for root in ROOTS for f in root.rglob(f"{ALVO}*.jsonl")]
    print(f"(a) transcript {ALVO}: {[str(f) for f in arqs]}")
    for f in arqs:
        print(f"    mtime={datetime.fromtimestamp(f.stat().st_mtime).isoformat()} "
              f"size={f.stat().st_size / 1e6:.1f}MB")

    if arqs:
        f = arqs[0]
        pgrep = env = perto = 0
        for line in f.open(encoding="utf-8", errors="replace"):
            if '"tool_result"' not in line and '"tool_use"' not in line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            ts = str(rec.get("timestamp", ""))
            for c in (rec.get("message") or {}).get("content") or []:
                if not isinstance(c, dict):
                    continue
                if c.get("type") == "tool_use" and "pgrep" in json.dumps(c.get("input") or {}):
                    pgrep += 1
                    perto += int("2026-10-09T14:1" in ts or "2026-10-09T14:2" in ts)
                    print(f"    tool_use pgrep @ {ts}: {str((c.get('input') or {}).get('command'))[:110]!r}")
                if c.get("type") == "tool_result" and c.get("is_error"):
                    t = _text(c)
                    if "X6 denied" in t or '"duration_ms"' in t:
                        env += 1
                        if env <= 5:
                            print(f"    tool_result envelope @ {ts}: {t[:190]!r}")
        print(f"    -> tool_use com pgrep={pgrep} (dos quais na janela 14:1x-14:2x: {perto}); "
              f"tool_result com envelope/X6={env}")

    # (c) o corpus TODO depois do deploy: existe algum envelope de touring run? algum pgrep?
    tot_env = tot_pgrep = tot_iserr = 0
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < datetime.fromisoformat(DEPLOY_94.replace("Z", "+00:00")).timestamp():
                continue
            for line in f.open(encoding="utf-8", errors="replace"):
                if '"tool_result"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if str(rec.get("timestamp", "")) < DEPLOY_94:
                    continue
                if '"tool_use"' in line and "pgrep" in line:
                    tot_pgrep += 1
                for c in (rec.get("message") or {}).get("content") or []:
                    if isinstance(c, dict) and c.get("type") == "tool_result" and c.get("is_error"):
                        tot_iserr += 1
                        if '"duration_ms"' in _text(c) or "X6 denied" in _text(c):
                            tot_env += 1
    print(f"\n(c) corpus TODO desde {DEPLOY_94}: linhas com pgrep={tot_pgrep} · "
          f"tool_result is_error={tot_iserr} · com envelope/X6={tot_env}")

    # a conservacao: qual negacao do balde NAO foi classificada
    faltando = []
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < datetime.fromisoformat("2026-09-17T00:00:00").timestamp():
                continue
            for line in f.open(encoding="utf-8", errors="replace"):
                if '"tool_result"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not (isinstance(c, dict) and c.get("type") == "tool_result" and c.get("is_error")):
                        continue
                    t = _text(c)
                    if HOOK_DENIAL.search(t) and G1_PAT.search(t):
                        if not re.search(r"CODE MODE · rajada|\[G10 exec-burst\]|\[G1 rajada-de-inspe",
                                         t.splitlines()[0] if t.splitlines() else ""):
                            faltando.append({"ts": rec.get("timestamp"), "sessao": f.stem,
                                             "linha": (t.splitlines() or [""])[0][:220]})
    print(f"\n(b) negacoes do balde que NAO casaram nenhum predicado: {len(faltando)}")
    for x in faltando[:5]:
        print(f"    {x['ts']}  {x['sessao'][:8]}  {x['linha']!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())