#!/usr/bin/env python3
"""Kill timestamp-regression mutants in temporary copies, never edit the originals."""

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

AQUI = Path(__file__).resolve().parent
B = AQUI.parent.parent
CONSTRUTOR = B / "diagnostics/instrumentos-2026-10-09/construir_disjunto.py"
C0 = B / "scripts/corpus_v0d.py"
MUTANTES = [
    {
        "nome": "inline_prefere_primeiro",
        "fonte": CONSTRUTOR,
        "teste": "test_ts_inline.py::test_duplicata_datada_vence_blob_e_respeita_o_corte",
        "antes": 'if anterior is None or (r["ts"] and (not anterior["ts"] or r["ts"] < anterior["ts"])):',
        "depois": "if anterior is None:",
    },
    {
        "nome": "inline_aceita_ts_ausente",
        "fonte": CONSTRUTOR,
        "teste": "test_ts_inline.py::test_inline_sem_qualquer_instante_vira_exclusao_contada",
        "antes": 'if not r["ts"]:',
        "depois": "if False:",
    },
    {
        "nome": "cache_aceita_ts_ausente",
        "fonte": CONSTRUTOR,
        "teste": "test_ts_inline.py::test_barreira_do_cache_nao_deixa_ts_vazio_atravessar_o_corte",
        "antes": 'com_ts = [k for k in d["filtrados"] if k.rsplit("@", 1)[1]]',
        "depois": 'com_ts = list(d["filtrados"])',
    },
    {
        "nome": "arquivo_aceita_ts_ausente",
        "fonte": C0,
        "teste": "test_ts_arquivo.py::test_arquivo_preserva_evento_datado_e_exclui_blob_sem_ts",
        "antes": "if not ts:",
        "depois": "if False:",
    },
]


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    originais = {path: sha(path) for path in (CONSTRUTOR, C0)}
    resultados = []
    for mutante in MUTANTES:
        nome = mutante["nome"]
        with tempfile.TemporaryDirectory(prefix=f"vgp-{nome}-") as tmp:
            raiz = Path(tmp)
            fonte = mutante["fonte"]
            rel_fonte = fonte.relative_to(B)
            copia = raiz / rel_fonte
            copia.parent.mkdir(parents=True, exist_ok=True)
            for dependencia in fonte.parent.glob("*.py"):
                destino = raiz / dependencia.relative_to(B)
                destino.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(dependencia, destino)
            texto = fonte.read_text(encoding="utf-8")
            assert texto.count(mutante["antes"]) == 1, f"âncora ausente em {nome}"
            copia.write_text(texto.replace(mutante["antes"], mutante["depois"]), encoding="utf-8")
            nome_teste = mutante["teste"].split("::", 1)[0]
            copia_teste = raiz / "diagnostics/holdout-20261031" / nome_teste
            copia_teste.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(AQUI / nome_teste, copia_teste)
            alvo = f"{copia_teste}::{mutante['teste'].split('::', 1)[1]}"
            run = subprocess.run(
                [sys.executable, "-I", "-m", "pytest", "-q", alvo],
                cwd="/tmp",
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )
            (AQUI / f"mutante_{nome}.log").write_text(run.stdout + run.stderr, encoding="utf-8")
            morto = run.returncode == 1
            resultados.append({"nome": nome, "rc": run.returncode, "morto": morto})
    assert all(sha(path) == digest for path, digest in originais.items()), "original mudou"
    (AQUI / "mutantes_ts_ausente.json").write_text(
        json.dumps(resultados, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    print(json.dumps(resultados, ensure_ascii=False))
    return 0 if all(r["morto"] for r in resultados) else 1


if __name__ == "__main__":
    raise SystemExit(main())
