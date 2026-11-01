# #tags: kind:snippet purpose:staleness domain:code-mode
"""R5 delta antes/depois: a MESMA medição 2x; o delta é a evidência.

'Melhorou' sem baseline é narrativa. Sandbox-puro e SEM eval: a medida é uma
FUNÇÃO NOMEADA (estenda o dicionário); a ação sob medida entra entre as duas
leituras. Este esqueleto mede contagem de glob.
"""
import sys, glob
from pathlib import Path

MEDIDAS = {
    "conta_glob": lambda arg: len(glob.glob(arg, recursive=True)),
    "bytes_arquivo": lambda arg: Path(arg).stat().st_size,
}
NOME = sys.argv[1] if len(sys.argv) > 1 else "conta_glob"
ARG = sys.argv[2] if len(sys.argv) > 2 else "crates/*"
medir = MEDIDAS[NOME]
antes = medir(ARG)
# <a ação sob medida entra aqui — neste esqueleto, nenhuma>
depois = medir(ARG)
print({"medida": f"{NOME}({ARG})", "antes": antes, "depois": depois,
       "mudou": antes != depois})
