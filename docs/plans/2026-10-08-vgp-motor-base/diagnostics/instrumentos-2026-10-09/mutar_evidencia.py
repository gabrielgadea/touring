"""Mutantes da escolha por evidencia em corpus_v0d.py. Morto = pytest dos testes `evidencia` com rc != 0.

Cada mutante troca UM trecho que ocorre exatamente uma vez; roda numa COPIA do diretorio de scripts (o original
nunca e tocado). Uso: python3 -I mutar_evidencia.py
"""

import hashlib
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
SP = Path(__file__).resolve().parent

MUTANTES = {
    "M1 ignora a linha (so contem o nome em qualquer linha)": (", linhas[linha - 1]) is not None", ", texto) is not None"),
    "M2 substring em vez de palavra inteira": ('rf"(?<!\\w){re.escape(nome)}(?!\\w)"', "re.escape(nome)"),
    "M3 linha 0-based": (", linhas[linha - 1]) is not None", ", linhas[linha]) is not None"),
    "M4 com >=2 que casam escolhe o primeiro": ("if len(casam) != 1:", "if len(casam) < 1:"),
    "M5 com 0 que casam escolhe assim mesmo": ("if len(casam) != 1:", "if len(casam) > 1:"),
    "M6 _origem_e_base nao passa a evidencia": ("first, _evidencia_do_inventario(row)", "first, None"),
    "M7 quebra de linha do splitlines": ('re.split(r"\\r\\n|\\r|\\n", texto)', "texto.splitlines()"),
    "M8 linha 0 vale (so rejeita negativas)": ("if not texto or linha < 1:", "if not texto or linha < 0:"),
    "M9 candidato --file sem texto reconstruido": ("return _reconstroi_arquivo_sessao(c.arquivo or \"\", session, first)\n\n\ndef _escolhe", "return None\n\n\ndef _escolhe"),
}


def rodar(copia: Path) -> tuple[int, str]:
    r = subprocess.run(
        [sys.executable, "-I", "-m", "pytest", "-q", "-p", "no:cacheprovider", "-x", str(copia / "test_corpus_v0d.py")],
        capture_output=True, text=True, timeout=600,
    )
    ult = (r.stdout + r.stderr).strip().splitlines()
    return r.returncode, ult[-1][:100] if ult else ""


def main() -> int:
    alvo = SCRIPTS / "corpus_v0d.py"
    orig = alvo.read_text(encoding="utf-8")
    h = hashlib.sha256(orig.encode()).hexdigest()[:16]
    vivos = 0
    with tempfile.TemporaryDirectory(dir=SP) as d:
        copia = Path(d) / "scripts"
        shutil.copytree(SCRIPTS, copia, ignore=shutil.ignore_patterns("__pycache__"))
        rc0, msg0 = rodar(copia)
        print(f"alvo sha {h} · controle (sem mutacao): rc {rc0} · {msg0}")
        if rc0 != 0:
            return 2
        for nome, (de, para) in MUTANTES.items():
            n = orig.count(de)
            if n != 1:
                print(f"  ANCORA {n}x: {nome}")
                vivos += 1
                continue
            (copia / "corpus_v0d.py").write_text(orig.replace(de, para), encoding="utf-8")
            rc, msg = rodar(copia)
            vivos += rc == 0
            print(f"  {'MORTO' if rc != 0 else 'VIVO '}  {nome}  (rc {rc} · {msg})")
    assert hashlib.sha256(alvo.read_text().encode()).hexdigest()[:16] == h, "alvo mudou durante a mutacao"
    print(f"vivos: {vivos}/{len(MUTANTES)}")
    return 1 if vivos else 0


if __name__ == "__main__":
    raise SystemExit(main())
