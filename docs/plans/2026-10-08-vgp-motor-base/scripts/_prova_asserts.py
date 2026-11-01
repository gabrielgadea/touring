"""Prova que os TRES asserts novos discriminam: um `assert main() == 0` que nunca reprova e decoracao.

O QUE ESTE SCRIPT FAZ, para cada um dos tres arquivos:
  1. copia o arquivo de teste para /tmp (o original do bundle NAO e tocado);
  2. substitui o `main()` por um que devolve 1 — o mutante que faz o selftest falhar;
  3. roda o pytest na copia e exige que ele REPROVE.

Um `pytest` que passa com o `main` quebrado esta provando que nao testa nada.

O `test_corpus_rust_contrato_juiz.py` tem um detalhe: o `main()` dele chama `falha()`, que faz
`sys.exit`. A mutacao aqui troca o corpo inteiro do `main`, entao nao ha `sys.exit` no caminho — o
que e justo, porque o mutante tem de ser observavel pelo pytest, e `SystemExit` dentro de um teste
e o pytest convertendo em falha do proprio teste, nao em `assert`.
"""

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

BUNDLE = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")

ALVOS = (
    ("test_vgp_lock.py", "test_selftest_do_lock"),
    ("test_chama_oraculo.py", "test_selftest_do_executor"),
    ("test_corpus_rust_contrato_juiz.py", "test_contrato_do_gerador_com_o_juiz"),
)


def muta(texto: str) -> str:
    """Troca o `return 0 if not falhas else 1` (ou o `return 0` final) por `return 1`.

    Corta no `def test_`: o mutante so mexe no `main`, nunca no assert — mexer no assert seria
    trocar o veredito por outro veredito, que nao prova nada.
    """
    i = texto.index("\ndef test_")
    cabeca, cauda = texto[:i], texto[i:]
    cabeca = re.sub(r"return 0 if not falhas else 1", "return 1", cabeca)
    cabeca = re.sub(r'print\("PASSOU: [^"]*"\)\n    return 0', 'print("MUTANTE")\n    return 1', cabeca)
    return cabeca + cauda


def main() -> int:
    falhas = 0
    with tempfile.TemporaryDirectory(prefix="vgp-mut-asserts-") as tmp:
        raiz = Path(tmp)
        for arquivo, funcao in ALVOS:
            orig = BUNDLE / arquivo
            texto = orig.read_text(encoding="utf-8")
            if f"def {funcao}(" not in texto:
                print(f"  FALHOU {arquivo}: nao achei `def {funcao}(`")
                falhas += 1
                continue
            if muta(texto) == texto:
                print(f"  FALHOU {arquivo}: o mutante nao mudou nada — o `return` nao casou")
                falhas += 1
                continue
            copia = raiz / arquivo
            copia.write_text(muta(texto), encoding="utf-8")
            r = subprocess.run(
                [sys.executable, "-m", "pytest", arquivo, "-q"],
                capture_output=True,
                text=True,
                cwd=raiz,
                timeout=1800,
            )
            ok = r.returncode != 0
            print(
                f"  {'ok  ' if ok else 'FALHA'} {arquivo:38s} pytest com o main mutado: "
                f"{'REPROVOU' if ok else 'PASSOU (o assert e decorativo)'}"
            )
            if not ok:
                falhas += 1
    print()
    total = len(ALVOS)
    print(f"{total - falhas}/{total} asserts discriminam (reprovam com o `main` quebrado)")
    return 0 if falhas == 0 else 1


if __name__ == "__main__":
    sys.exit(main())