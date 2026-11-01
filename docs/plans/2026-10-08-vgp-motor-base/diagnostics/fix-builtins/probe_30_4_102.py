#!/usr/bin/env python3
"""Prove the installed 30.4.102 binary by behavior and prepare a hot-daemon probe."""

import hashlib
import json
import shutil
import subprocess
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
BIN = Path.home() / ".touring/toolchains/30.4.102/bin/touring"
CTL = Path("/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad/j14_hibrido_20261031b")
OUT = B / "diagnostics/fix-builtins/probe_30_4_102.json"
HOT = Path("/tmp/claude-1000/-home-gabrielgadea-projects-touring/064bba23-c3ac-461e-b4f6-e2a4d998460e/scratchpad/probe102hot")


def entradas(corpus: Path) -> list[dict]:
    out = [json.loads(x) for x in (corpus / "entradas.jsonl").read_text().splitlines() if x.strip()]
    extra = corpus / "entradas_inline_neg.jsonl"
    if extra.is_file():
        out += [json.loads(x) for x in extra.read_text().splitlines() if x.strip()]
    return out


def origem_snap(entrada: dict, corpus: Path) -> Path:
    origem = Path(entrada["snap"])
    return origem if origem.is_absolute() else corpus / origem


def chamada(entrada: dict, corpus: Path, camadas: str) -> dict:
    cmd = [
        str(BIN), "vgp", "check", "--path", entrada["path"],
        "--content-file", str(corpus / entrada["snap"]),
        "--project-root", entrada["project_root"], "--lang", "python",
        "--camadas", camadas, "-j",
    ]
    manifesto = json.loads((corpus / "estado_no_instante/manifesto.json").read_text()) if (corpus / "estado_no_instante/manifesto.json").is_file() else {}
    estado = manifesto.get(entrada["id"], {})
    if estado.get("disk_file"):
        cmd += ["--disk-file", str(corpus / estado["disk_file"])]
    for overlay in estado.get("overlays", []):
        cmd += ["--overlay", f"{overlay['path']}={corpus / overlay['file']}"]
    for contexto in entrada.get("contexto") or []:
        cmd += ["--overlay", f"{contexto['path']}={corpus / contexto['snap']}"]
    run = subprocess.run(cmd, capture_output=True, text=True, timeout=60, check=False)
    return json.loads(run.stdout)


def main() -> None:
    todos = entradas(CTL)
    caso = next(e for e in todos if e["id"] == "2106f0ebf279")
    sinteticos = []
    for nome in ("exit", "quit"):
        fixture = B / f"diagnostics/fix-builtins/synthetic/{nome}.py"
        sinteticos.append(dict(caso, id=f"sintetico-{nome}", snap=fixture))
    resultado = {
        "binary": str(BIN),
        "sha256": hashlib.sha256(BIN.read_bytes()).hexdigest(),
        "version": subprocess.run([str(BIN), "--version"], capture_output=True, text=True, check=False).stderr,
        "caso_2106f0ebf279": chamada(caso, CTL, "1,2"),
        "sinteticos": {e["id"]: chamada(e, Path("/"), "1,2") for e in sinteticos},
    }
    OUT.write_text(json.dumps(resultado, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    HOT.mkdir(parents=True, exist_ok=True)
    for entrada in [caso, *sinteticos]:
        origem = origem_snap(entrada, CTL)
        destino = HOT / origem.name
        shutil.copy2(origem, destino)
        entrada["snap"] = destino.name
    (HOT / "entradas_inline_neg.jsonl").write_text(
        "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in [caso, *sinteticos]), encoding="utf-8"
    )
    (HOT / "entradas.jsonl").write_text("", encoding="utf-8")
    print(json.dumps({"caso": resultado["caso_2106f0ebf279"], "sinteticos": resultado["sinteticos"], "hot_corpus": str(HOT)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
