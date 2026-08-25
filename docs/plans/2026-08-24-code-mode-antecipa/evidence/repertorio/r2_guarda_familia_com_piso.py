# #tags: kind:snippet purpose:familia-parcial domain:code-mode
"""R2 guarda-família-com-piso: assere sobre a FAMÍLIA inteira, com piso.

Um guard que valida 1 sítio deixa os outros N-1 regredirem; um guard sem piso
aceita família vazia (o 0/0 do auditor). Parametrize GLOB, PADRAO e PISO.
"""
import glob, re, sys
GLOB = sys.argv[1] if len(sys.argv) > 1 else "crates/touring-foundation/src/gate_metrics*.rs"
PADRAO = sys.argv[2] if len(sys.argv) > 2 else r"bash_calls_total_count"
PISO = int(sys.argv[3]) if len(sys.argv) > 3 else 2
sitios = [f for f in glob.glob(GLOB, recursive=True)
          if re.search(PADRAO, open(f, encoding="utf-8", errors="ignore").read())]
ok = len(sitios) >= PISO
print({"sitios": len(sitios), "piso": PISO, "ok": ok, "lista": sitios[:8]})
raise SystemExit(0 if ok else 1)
