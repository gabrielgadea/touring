"""Causa de cada falso deny do binario e935 no held-out disjunto: o nome esta DEFINIDO no snap? existe no disco de hoje?
a mensagem do motor? e o arquivo real (hoje) bate com o snap? Leitura apenas.
"""

import ast
import json
import re
from pathlib import Path

SP = Path(__file__).resolve().parent
HOLD = Path.home() / ".claude/touring/vgp/corpus_holdout_disjunto"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


ents = {}
for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
    for ln in (HOLD / nome).read_text(encoding="utf-8").splitlines():
        if ln.strip():
            e = json.loads(ln)
            ents[e["id"]] = e
d = json.loads((SP / "k_v2_disjunto.json").read_text(encoding="utf-8"))
for f in d["falsos"]:
    e = ents[f["id"]]
    texto = (HOLD / e["snap"]).read_text(encoding="utf-8")
    out("=" * 100)
    out(f["id"], f["balde"], "| path:", e["path"].replace(str(Path.home()), "~"), "| project_root:", Path(e["project_root"]).name, "| ts:", e["ts"], "| snap:", len(texto), "c")
    for a in f["achados"]:
        nome = a["nome"]
        defs = [n for n, l in enumerate(texto.splitlines(), 1) if re.search(rf"(^|\W)(def|class)\s+{re.escape(nome)}\b|(^|\W){re.escape(nome)}\s*(:[^=]+)?=[^=]|import\s+.*\b{re.escape(nome)}\b", l)]
        out(f"   achado {a['classe']} `{nome}` L{a['linha']} [{a['severidade']}] | {a.get('sugestao', '')[:140]!r} | reais: {a.get('reais')}")
        out(f"      definicoes de `{nome}` no snap (linhas): {defs[:6] or 'NENHUMA'} | star-import no snap: {bool(re.search(r'from\s+\S+\s+import\s+\*', texto))}")
    p = Path(e["path"])
    if not e["path"].startswith("<") and p.is_file():
        hoje = p.read_text(encoding="utf-8", errors="replace")
        out(f"      arquivo no disco hoje: {len(hoje)}c | igual ao snap: {hoje == texto} | nome definido no disco hoje: {any(re.search(rf'(^|\W){re.escape(a['nome'])}\b', hoje) for a in f['achados'])}")
    else:
        out("      arquivo no disco hoje: inexistente (ou inline)")
    if e["origem"] == "inline":
        out("      comando:", e["command"][:300].replace("\n", "\\n"))
