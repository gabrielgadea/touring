"""Para cada perda de `nome` (id, nome): o nome esta DEFINIDO no proprio snapshot? E onde, hoje, no crate?

Uso: touring run --file def_perdas.py --args '[SC]'   (le perdas_nome_V22.jsonl)
"""

import json
import pathlib
import re
import subprocess
import sys

SC = pathlib.Path(sys.argv[1])
N = pathlib.Path("/home/gabrielgadea/.claude/touring/vgp/corpus_rust_natural")
ent = {}
for ln in (N / "entradas.jsonl").read_text().splitlines():
    e = json.loads(ln)
    ent[e["id"]] = e
res = json.loads((SC / "var_V22_ids_natural.json").read_text())["ids"]
for i, e in ent.items():
    if e["rotulo"] != "positivo" or e.get("classe") != "nome":
        continue
    r = res.get(i, {})
    if any(f["nome"] == e["nome_esperado"] for f in r.get("findings", [])):
        continue
    nome = e["nome_esperado"]
    src = (N / e["snap"]).read_text()
    pad = re.compile(r"\b(fn|const|static|struct|enum|type|trait|union|mod|macro_rules!)\s+%s\b" % re.escape(nome))
    no_snap = [src[: m.start()].count("\n") + 1 for m in pad.finditer(src)]
    uso_import = [src[: m.start()].count("\n") + 1 for m in re.finditer(r"\buse\b[^;]*\b%s\b" % re.escape(nome), src)]
    crate = pathlib.Path(e["path"]).parents[1] if "/src/" in e["path"] else pathlib.Path(e["path"]).parent
    try:
        g = subprocess.run(
            ["grep", "-rnE", r"(fn|const|static|struct|enum|type|trait|macro_rules!)\s+%s\b" % nome, str(crate), "--include=*.rs"],
            capture_output=True, text=True, timeout=20,
        ).stdout.splitlines()
    except Exception as ex:
        g = [str(ex)]
    print(
        json.dumps(
            {
                "id": i[:6],
                "nome": nome,
                "linha_erro": e["linha"],
                "def_no_snapshot": no_snap[:3],
                "use_no_snapshot": uso_import[:3],
                "ts": e["ts"][:16],
                "introduzido": e.get("introduzido"),
                "def_hoje": [x.replace("/home/gabrielgadea/projects/touring/crates/", "c/")[:110] for x in g[:2]],
            },
            ensure_ascii=False,
        )
    )
