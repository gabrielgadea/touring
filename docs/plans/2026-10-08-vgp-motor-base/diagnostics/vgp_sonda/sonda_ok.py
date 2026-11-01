import json
import os
from collections import Counter, defaultdict
from pathlib import Path

from scripts.memoria.grafo_modelo import No



def campo(n: No) -> str:


st = {"a": Counter(), "b": defaultdict(Counter), "n": 0}
st["n"] += 1
print(st["a"].most_common())
def campo(n: No) -> str:
    return n.slug
