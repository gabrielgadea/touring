"""Verifica no codigo-fonte as hipoteses de causa dos 7 falsos deny do e935 no held-out disjunto. Leitura apenas."""

import ast
import json
import re
import subprocess
from pathlib import Path

HOME = Path.home()
HOLD = HOME / ".claude/touring/vgp/corpus_holdout_disjunto"
ANALISE = HOME / "projects/analise"
SITE = next((ANALISE / ".venv/lib").glob("python3*/site-packages"))


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def onde_liga(arvore: ast.Module, nome: str) -> str:
    """Em que posicao do modulo o nome e ligado: topo, dentro de try/if/with/for (composto) ou em lugar nenhum."""
    achados = []
    for no in ast.walk(arvore):
        alvo = None
        if isinstance(no, ast.Assign) and any(isinstance(t, ast.Name) and t.id == nome or (isinstance(t, ast.Tuple) and any(isinstance(x, ast.Name) and x.id == nome for x in t.elts)) for t in no.targets):
            alvo = "atribuicao" + (" ENCADEADA" if len(no.targets) > 1 else "")
        elif isinstance(no, (ast.ImportFrom, ast.Import)) and any((a.asname or a.name).split(".")[0] == nome for a in no.names):
            alvo = "import"
        elif isinstance(no, (ast.FunctionDef, ast.ClassDef, ast.AsyncFunctionDef)) and no.name == nome:
            alvo = "def/class"
        if alvo:
            achados.append((alvo, no.lineno))
    topo = {id(n) for n in arvore.body}
    res = []
    for alvo, ln in achados:
        no_topo = any(getattr(n, "lineno", -1) == ln and id(n) in topo for n in arvore.body)
        res.append(f"{alvo}@L{ln}{'' if no_topo else ' (DENTRO de bloco composto)'}")
    return "; ".join(res[:4]) or "NAO LIGADO em lugar nenhum do modulo"


# (a) numpy
npinit = SITE / "numpy/__init__.py"
if npinit.is_file():
    arv = ast.parse(npinit.read_text(encoding="utf-8"))
    topo = [a.name for n in arv.body if isinstance(n, ast.Import) for a in n.names] + [a.asname or a.name for n in arv.body if isinstance(n, ast.ImportFrom) for a in n.names]
    out("(a) numpy", (SITE / "numpy/version.py").read_text().strip().splitlines()[-1][:60] if (SITE / "numpy/version.py").is_file() else "",
        "| imports no TOPO do __init__:", topo[:10], "| `array`:", onde_liga(arv, "array"), "| `where`:", onde_liga(arv, "where"))
else:
    out("(a) numpy: nao instalado no .venv da analise")
# (b) PIL
img = SITE / "PIL/Image.py"
if img.is_file():
    arv = ast.parse(img.read_text(encoding="utf-8"))
    out("(b) PIL.Image: `LANCZOS`:", onde_liga(arv, "LANCZOS"), "| `NEAREST`:", onde_liga(arv, "NEAREST"))
    m = re.search(r"^.*\bLANCZOS\b.*$", img.read_text(encoding="utf-8"), re.M)
    out("    linha:", m.group(0).strip()[:100] if m else None, "| Pillow:", next(iter(SITE.glob("pillow-*")), Path("?")).name)
# (c) modulos do projeto: hoje, no git do instante e no overlay do manifesto
man = json.loads((HOLD / "estado_no_instante/manifesto.json").read_text(encoding="utf-8"))
for eid, modulo, nome in (("7a4ad8f765aa", "scripts/external_apis/tse.py", "TSEClient"), ("bd4cfc7dd1e1", "scripts/aco/rust_bridge.py", "RUST_AVAILABLE")):
    p = ANALISE / modulo
    hoje = p.read_text(encoding="utf-8") if p.is_file() else ""
    ts = next(json.loads(ln)["ts"] for ln in (HOLD / "entradas.jsonl").read_text().splitlines() if ln.strip() and json.loads(ln)["id"] == eid)
    rev = subprocess.run(["git", "-C", str(ANALISE), "rev-list", "-1", f"--before={ts}", "HEAD"], capture_output=True, text=True).stdout.strip()
    antes = subprocess.run(["git", "-C", str(ANALISE), "show", f"{rev}:{modulo}"], capture_output=True, text=True).stdout if rev else ""
    ov = [o for o in man[eid]["overlays"] if o["path"].endswith(modulo)]
    ov_txt = (HOLD / ov[0]["file"]).read_text(encoding="utf-8") if ov else ""
    out(f"(c) {eid} {modulo}: hoje liga `{nome}`? {onde_liga(ast.parse(hoje), nome) if hoje else 'sem arquivo'} | no git em {ts[:16]} ({rev[:8]}): "
        f"{onde_liga(ast.parse(antes), nome) if antes else 'arquivo ausente'} | overlay do manifesto: {'SIM' if ov else 'NAO'} {onde_liga(ast.parse(ov_txt), nome) if ov_txt else ''}")
# (d) os 3 snapshots com nome sem definicao: o nome aparece em algum Write/Edit/heredoc da sessao?
for eid, nome in (("ddbb2cfb4afc", "ComputeProcessContext"), ("74f04a24de12", "WS"), ("c35d78e3eb25", "WIRING")):
    e = next(json.loads(ln) for ln in (HOLD / "entradas.jsonl").read_text().splitlines() if ln.strip() and json.loads(ln)["id"] == eid)
    base = Path(e["path"]).name
    vezes, escritos = 0, 0
    for t in (HOME / ".claude/projects").glob("*/*.jsonl"):
        txt = t.read_text(encoding="utf-8", errors="replace")
        if base not in txt:
            continue
        for ln in txt.splitlines():
            if base in ln and re.search(rf"\b{re.escape(nome)}\b", ln):
                vezes += 1
                if '"tool_use"' in ln and any(k in ln for k in ('"Write"', '"Edit"', '"MultiEdit"', "<<")):
                    escritos += 1
    out(f"(d) {eid} {base}: `{nome}` aparece em {vezes} linhas de transcript que citam o arquivo, {escritos} delas em Write/Edit/heredoc (conteudo escrito)")
