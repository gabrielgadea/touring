"""Censo de SUBMODULOS: o namespace estatico (coletor VGP) contra dir() do modulo importado, em profundidade <= 2.

Fase `lista <n_por_pacote>`: caminhos .py (nao-teste, nao-_privado) dos pacotes do allowlist, profundidade <= 2.
Fase `comparar <censo.tsv> <saida.json>`: um subprocesso por pacote importa seus submodulos listados e compara.
BURACO = nome publico de dir(modulo) ausente do namespace estatico num modulo NAO marcado (star/getattr/dynamic).
"""

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import censo  # noqa: E402

SP = censo.SP
PY = censo.PY
PACOTES = (
    "requests urllib3 httpx pydantic yaml attr click rich jinja2 markupsafe dateutil pytz tqdm sympy matplotlib "
    "openpyxl bs4 lxml tabulate packaging certifi idna charset_normalizer anyio starlette fastapi sqlalchemy "
    "alembic typer tenacity regex pygments psutil cachetools filelock markdown joblib tiktoken openai anthropic "
    "httpcore h11 websockets msgpack cloudpickle dill wrapt structlog loguru platformdirs docx pptx pypdf fontTools "
    "seaborn statsmodels shapely pyproj redis jsonschema referencing mako pluggy iniconfig pyparsing tomlkit tomli "
    "xlsxwriter multidict yarl frozenlist aiohttp huggingface_hub fsspec traitlets docker hypothesis pytest "
    "coverage jedi parso cffi pycparser cryptography rsa ecdsa jwt bcrypt numpy pandas scipy PIL networkx"
).split()


def lista(limite):
    for pkg in PACOTES:
        raiz = SP / pkg
        if not raiz.is_dir():
            continue
        arqs = []
        for p in sorted(raiz.rglob("*.py")):
            rel = p.relative_to(raiz)
            partes = rel.parts
            if len(partes) > 2 or any(x.startswith("_") and x != "__init__.py" for x in partes):
                continue
            if any(x in ("tests", "test", "testing", "conftest.py") for x in partes) or p.name.startswith("test_"):
                continue
            arqs.append(p)
        for p in arqs[: int(limite)]:
            print(p)


def modulo_de(path):
    rel = Path(path).relative_to(SP).with_suffix("")
    partes = list(rel.parts)
    if partes[-1] == "__init__":
        partes = partes[:-1]
    return ".".join(partes)


PROG = """
import importlib, json, sys
out = {}
for m in sys.argv[1:]:
    try:
        mod = importlib.import_module(m)
        out[m] = sorted(dir(mod))
    except BaseException as e:  # noqa
        out[m] = None
sys.stdout.write("\\n@@JSON@@" + json.dumps(out) + "\\n")
"""


def runtime_pacote(mods):
    try:
        r = subprocess.run([PY, "-I", "-c", PROG, *mods], capture_output=True, text=True, timeout=240,
                           env={"PYTHONDONTWRITEBYTECODE": "1", "HOME": "/tmp/censo-home", "PATH": "/usr/bin"})
    except subprocess.TimeoutExpired:
        return {}
    if "@@JSON@@" not in r.stdout:
        return {}
    return json.loads(r.stdout.rsplit("@@JSON@@", 1)[1].strip())


def comparar(censo_tsv, saida):
    por_pkg = {}
    estatico = {}
    for ln in Path(censo_tsv).read_text().splitlines():
        if not ln.startswith("CENSO\t"):
            continue
        _, path, star, ga, dyn, nomes = ln.split("\t", 5)
        mod = modulo_de(path)
        estatico[mod] = (path, star == "1", ga == "1", dyn == "1", set(filter(None, nomes.split(","))))
        por_pkg.setdefault(mod.split(".")[0], []).append(mod)
    out = {}
    for pkg, mods in sorted(por_pkg.items()):
        rt = runtime_pacote(mods)
        for m in mods:
            path, star, ga, dyn, nomes = estatico[m]
            dirs = rt.get(m)
            if dirs is None:
                continue
            irmaos = {p.name.split(".")[0] for p in Path(path).parent.iterdir()}
            publicos = {n for n in dirs if not n.startswith("_")} - irmaos
            buracos = sorted(publicos - nomes)
            out[m] = {"marcado": star or ga or dyn, "star": star, "getattr": ga, "dynamic": dyn,
                      "n_runtime": len(publicos), "n_buracos": len(buracos), "buracos": buracos[:20]}
    Path(saida).write_text(json.dumps(out, indent=1, ensure_ascii=False))
    medidos = len(out)
    marcados = sum(v["marcado"] for v in out.values())
    sem_marca = {k: v for k, v in out.items() if v["n_buracos"] and not v["marcado"]}
    sys.stdout.write(f"modulos medidos: {medidos} | marcados (abstem): {marcados} | com buraco E sem marca: {len(sem_marca)}\n")
    for k, v in sorted(sem_marca.items())[:60]:
        sys.stdout.write(f"  BURACO {k}: {v['n_buracos']}/{v['n_runtime']} ex {v['buracos'][:6]}\n")


if __name__ == "__main__":
    if sys.argv[1] == "lista":
        lista(sys.argv[2])
    else:
        comparar(sys.argv[2], sys.argv[3])
