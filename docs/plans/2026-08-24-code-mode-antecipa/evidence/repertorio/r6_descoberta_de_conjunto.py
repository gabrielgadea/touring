# #tags: kind:snippet purpose:familia-parcial domain:code-mode
"""R6 descoberta-de-conjunto: enumerar o universo ANTES de tratar (o m de n/m).

Tratar sem enumerar é como o conserto de-4-dos-5 nasce. Sandbox-puro (os.walk +
re, sem grep). Emite FACT=SITES= (contrato do probe/family-fix) + a lista.
"""
import sys, re, os
PADRAO = sys.argv[1] if len(sys.argv) > 1 else r"std::env::current_dir"
DIR = sys.argv[2] if len(sys.argv) > 2 else "crates/touring-server/src/cli"
rx = re.compile(PADRAO)
sitios = []
for raiz, _, arquivos in os.walk(DIR):
    for a in arquivos:
        caminho = os.path.join(raiz, a)
        try:
            if rx.search(open(caminho, encoding="utf-8", errors="ignore").read()):
                sitios.append(caminho)
        except OSError:
            pass
for s in sorted(sitios):
    print("SITIO:", s)
print(f"FACT=SITES={len(sitios)}")
