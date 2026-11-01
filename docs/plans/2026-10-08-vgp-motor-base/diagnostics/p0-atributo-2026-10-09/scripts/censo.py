"""Censo do namespace estatico (coletor VGP) contra o runtime (dir() do modulo importado no venv).

Fase 1 (`lista`): imprime os caminhos dos __init__.py dos pacotes do allowlist (um por linha).
Fase 2 (`comparar <censo.tsv> <saida.json>`): le a saida CENSO do teste Rust ignorado e, por pacote,
importa o modulo no venv (subprocesso, timeout) e compara dir() com os nomes estaticos.

Um BURACO = nome publico do runtime ausente do namespace estatico em modulo NAO marcado
(star/getattr/dynamic): e exatamente o que viraria falso deny.  Submodulos (arquivos/pastas do
pacote) sao descontados: o motor os resolve por arquivo, nunca pelo namespace.
"""

import json
import subprocess
import sys
from pathlib import Path

SP = Path("/home/gabrielgadea/projects/analise/.venv/lib/python3.12/site-packages")
PY = "/home/gabrielgadea/projects/analise/.venv/bin/python"
ALLOW = """numpy PIL pandas scipy requests httpx pydantic yaml attr attrs click rich jinja2 markupsafe dateutil pytz
tqdm networkx sympy matplotlib sklearn openpyxl bs4 lxml tabulate packaging urllib3 certifi idna charset_normalizer
anyio starlette fastapi sqlalchemy alembic typer tenacity regex pygments psutil cachetools filelock zipp
importlib_metadata markdown mpmath joblib tiktoken openai anthropic httpcore h11 websockets orjson msgpack
cloudpickle dill wrapt structlog loguru colorlog humanfriendly platformdirs docx pptx pypdf fontTools seaborn
statsmodels shapely pyproj skimage tifffile imageio redis jsonschema referencing rpds mako more_itertools
pluggy iniconfig pyparsing six_absent tomlkit tomli sortedcontainers xlsxwriter xlrd multidict yarl frozenlist
aiohttp huggingface_hub fsspec traitlets pexpect docker tree_sitter hypothesis pytest coverage mypy jedi parso
cffi pycparser cryptography rsa ecdsa jwt bcrypt""".split()


NEGADOS = {"torch", "torchvision", "torchgen", "triton", "nvidia", "cuda", "functorch", "transformers",
           "sentence_transformers", "accelerate", "datasets", "spacy", "thinc", "blis", "onnxruntime",
           "faiss", "patchright", "playwright", "selenium", "pt_core_news_lg", "setuptools", "pyright",
           "ruff", "maturin", "claude_learning_kernel", "tokenizers", "safetensors", "litellm", "dspy"}


def lista():
    """Todos os pacotes do venv com __init__.py (menos os pesados/perigosos), um caminho por linha."""
    for d in sorted(SP.iterdir()):
        if not d.is_dir() or d.name.startswith("_") or "-" in d.name or d.name in NEGADOS:
            continue
        if (d / "__init__.py").exists():
            print(d / "__init__.py")


def runtime(nome):
    cod = (
        "import importlib, json, sys\n"
        f"m = importlib.import_module({nome!r})\n"
        "print(json.dumps(sorted(dir(m))))\n"
    )
    try:
        r = subprocess.run([PY, "-I", "-c", cod], capture_output=True, text=True, timeout=40,
                           env={"PYTHONDONTWRITEBYTECODE": "1", "HOME": "/tmp/censo-home", "PATH": "/usr/bin"})
    except subprocess.TimeoutExpired:
        return None, "timeout"
    if r.returncode != 0:
        return None, r.stderr.strip().splitlines()[-1][:120] if r.stderr.strip() else f"rc={r.returncode}"
    ult = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else "[]"
    try:
        return set(json.loads(ult)), ""
    except json.JSONDecodeError:
        return None, "saida nao-JSON"


def comparar(censo, saida):
    out = {}
    for ln in Path(censo).read_text().splitlines():
        if not ln.startswith("CENSO\t"):
            continue
        _, path, star, getattr_, dyn, nomes = ln.split("\t", 5)
        pkg = Path(path).parent.name
        estatico = set(filter(None, nomes.split(",")))
        rt, erro = runtime(pkg)
        reg = {"star": star == "1", "getattr": getattr_ == "1", "dynamic": dyn == "1",
               "n_estatico": len(estatico)}
        if rt is None:
            reg["runtime_erro"] = erro
            out[pkg] = reg
            continue
        subs = {p.stem for p in (SP / pkg).iterdir() if p.suffix in (".py", ".so") or p.is_dir()}
        subs |= {p.name.split(".")[0] for p in (SP / pkg).iterdir()}
        publicos = {n for n in rt if not n.startswith("_")} - subs
        buracos = sorted(publicos - estatico)
        reg.update(n_runtime=len(publicos), buracos=buracos[:25], n_buracos=len(buracos),
                   marcado=reg["star"] or reg["getattr"] or reg["dynamic"])
        out[pkg] = reg
    Path(saida).write_text(json.dumps(out, indent=1, ensure_ascii=False))
    sem_marca = {k: v for k, v in out.items() if v.get("n_buracos") and not v["marcado"]}
    print(f"pacotes: {len(out)} | com runtime: {sum('n_runtime' in v for v in out.values())} | "
          f"marcados (abstem): {sum(v.get('marcado', False) for v in out.values())} | "
          f"com buraco E sem marca: {len(sem_marca)}")
    for k, v in sorted(sem_marca.items()):
        print(f"  BURACO {k}: {v['n_buracos']}/{v['n_runtime']} ex {v['buracos'][:6]}")


if __name__ == "__main__":
    {"lista": lista, "comparar": lambda: comparar(sys.argv[2], sys.argv[3])}[sys.argv[1]]()
