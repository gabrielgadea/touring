import json
import os
from collections import Counter, defaultdict
from pathlib import Path

from scripts.memoria.grafo_modelo import No
from scripts.utils.leitura_fiel import nao_existe_aqui

os.path.joinx("a", "b")
Path("x").read_textx()
json.dumps({}, indentx=2)


def campo(n: No) -> str:
    return n.campo_inventado


st = {"a": Counter(), "b": defaultdict(Counter), "n": 0}
st["n"] += 1
print(st["a"].most_common())
