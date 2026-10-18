"""Share of injected bytes per family in `touring kpi -j` → context_budget (checks the area-C claim)."""
import json
import subprocess
import sys

saida = subprocess.run(["touring", "kpi", "-j"], capture_output=True, text=True, check=False).stdout
cb = json.loads(saida[saida.find("{"):])["context_budget"]
fam = cb["by_family"]
total = sum(v["bytes"] for v in fam.values()) or 1
for nome, v in sorted(fam.items(), key=lambda kv: -kv[1]["bytes"])[:6]:
    print(f"{nome:22s} {v['bytes']:9d} {100 * v['bytes'] / total:5.1f}%  blocos={v['blocks']}  "
          f"sem_proposicao={v.get('zero_proposition_blocks')}")
print(f"total={total} · por turno={cb.get('injected_bytes_per_turn')} (teto {cb.get('injected_bytes_per_turn_ceil')}) "
      f"· status={cb.get('status')} · sessões={len(cb.get('by_session', {}))}")
sys.exit(0)
