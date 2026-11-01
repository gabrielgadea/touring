# #tags: kind:snippet purpose:instrumento-errado domain:code-mode
"""R4 controle negativo: o verificador aprova o bom E reprova o ruim.

Uma execução não distingue constante (predict-action devolvia 0.990 para tudo,
inclusive `false`). Sandbox-puro e SEM eval (o CEG o nega): o verificador é um
PREDICADO NOMEADO — estenda o dicionário com o seu.
"""
import sys
from pathlib import Path

PREDICADOS = {
    "arquivo_nao_vazio": lambda x: Path(x).is_file() and Path(x).stat().st_size > 0,
    "arquivo_existe": lambda x: Path(x).exists(),
    "contem_marcador": lambda x: "METRIC=" in Path(x).read_text(errors="ignore"),
}
NOME = sys.argv[1] if len(sys.argv) > 1 else "arquivo_nao_vazio"
BOM = sys.argv[2] if len(sys.argv) > 2 else "/etc/hostname"
RUIM = sys.argv[3] if len(sys.argv) > 3 else "/dev/null"
verifica = PREDICADOS[NOME]
def seguro(entrada):
    try:
        return bool(verifica(entrada))
    except OSError:
        return False
aprova_bom, reprova_ruim = seguro(BOM), not seguro(RUIM)
print({"predicado": NOME, "aprova_bom": aprova_bom, "reprova_ruim": reprova_ruim,
       "calibrado": aprova_bom and reprova_ruim})
raise SystemExit(0 if (aprova_bom and reprova_ruim) else 1)
