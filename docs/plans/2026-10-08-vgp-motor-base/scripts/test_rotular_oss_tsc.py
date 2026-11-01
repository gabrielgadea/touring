#!/usr/bin/env python3
"""O `tsc` do retestemunho e resolvido por AMBIENTE, e falha ALTO quando nao encontra.

MEDIDO em 01/11 (V-8): o rotulador tinha `TSC = Path("/home/<usuario>/projects/analise/
frontend/node_modules/.bin/tsc")` -- um caminho ABSOLUTO desta maquina, VERSIONADO num repo
PUBLICO. A reescrita de 01/11 (66 caminhos com conteudo privado removidos) NAO pegou esse, e ele
continua no `origin/main`. Um caminho com o nome do usuario nao tem porque estar no historico, e
o `tsc` nao precisa de caminho fixo: ele se resolve por `TOURING_TSC`, pelo PATH, ou pelo
`node_modules/.bin/tsc` do proprio repo.

E o que este teste segura: a resolucao NAO pode silenciar. Sem `tsc`, o retestemunho nao roda e a
DELTA do `tsc` vira "confirmacao vazia" -- a mesma armadilha do formato de saida nao parseado
(medido em 09/10 no zod: 598 erros emitidos, 0 casando, e o oraculo de referencia passava a
confirmar nada). Entao: nao achado -> PARA COM MENSAGEM.

  (a) `TOURING_TSC` existente vence;
  (b) `TOURING_TSC` inexistente REPROVA (nao cai no PATH -- o erro do ambiente tem que ser o erro);
  (c) sem env, o `node_modules/.bin/tsc` do repo serve;
  (d) sem env, sem PATH e sem repo, PARA COM MENSAGEM (nunca devolve `None`);
  (e) MUTANTE: o ambiente MANDA sobre o cache (trocar `TOURING_TSC` troca o resultado);
  (f) o modulo nao carrega caminho absoluto de HOME -- a regressao do vazamento nao volta.

Uso: python3 -I test_rotular_oss_tsc.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import os
import re
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro

RE_ABSOLUTO_HOME = re.compile(r"[\"'](/home/[^\"'\s]+)[\"']")


@contextmanager
def ambiente_tsc(env: str | None, which: str | None):
    """`TOURING_TSC` e o `shutil.which` sob controle. NAO mexe no cache: quem precisa zera."""
    antigo = os.environ.get("TOURING_TSC")
    try:
        if env is None:
            os.environ.pop("TOURING_TSC", None)
        else:
            os.environ["TOURING_TSC"] = env
        with mock.patch.object(ro.shutil, "which", lambda _cmd: which):
            yield
    finally:
        if antigo is None:
            os.environ.pop("TOURING_TSC", None)
        else:
            os.environ["TOURING_TSC"] = antigo


def _reprovou(fn) -> str:
    """Chama `fn` e devolve a mensagem do SystemExit; se NAO reprovou, devolve o motivo."""
    try:
        got = fn()
    except SystemExit as e:
        return f"reprovou: {e}"
    return f"NAO reprovou: devolveu {got}"


def cenario_a(raiz: Path) -> tuple:
    tsc_a = raiz / "tsc-a"
    tsc_a.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    ro._TSC_CACHE = None
    with ambiente_tsc(env=str(tsc_a), which=None):
        got = ro._tsc()
    return ("(a) `TOURING_TSC` existente e o que volta", got == tsc_a, f"got={got}")


def cenario_b(raiz: Path) -> tuple:
    tsc_c = raiz / "tsc-c"
    tsc_c.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    ro._TSC_CACHE = None
    inexistente = str(raiz / "nao-existe")
    with ambiente_tsc(env=inexistente, which=str(tsc_c)):
        erro = _reprovou(lambda: ro._tsc())
    ok = "reprovou" in erro and "TOURING_TSC" in erro
    return ("(b) `TOURING_TSC` inexistente REPROVA (nao cai no PATH)", ok, erro)


def cenario_c(raiz: Path) -> tuple:
    ro._TSC_CACHE = None
    repo = raiz / "repo"
    (repo / "node_modules" / ".bin").mkdir(parents=True, exist_ok=True)
    local = repo / "node_modules" / ".bin" / "tsc"
    local.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    with ambiente_tsc(env=None, which=None):
        got = ro._tsc(repo)
    return (
        "(c) sem env e sem PATH, o `node_modules/.bin/tsc` do repo serve",
        got == local,
        f"got={got} esperado={local}",
    )


def cenario_d() -> tuple:
    ro._TSC_CACHE = None
    with ambiente_tsc(env=None, which=None):
        erro = _reprovou(lambda: ro._tsc())
    ok = "reprovou" in erro and "TOURING_TSC" in erro
    return ("(d) sem env, sem PATH e sem repo, PARA COM MENSAGEM", ok, erro)


def cenario_e(raiz: Path) -> tuple:
    tsc_b = raiz / "tsc-b"
    tsc_b.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    tsc_c = raiz / "tsc-c"
    tsc_c.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    ro._TSC_CACHE = None
    with ambiente_tsc(env=None, which=str(tsc_c)):
        ro._tsc()  # popula o cache do PATH
    with ambiente_tsc(env=str(tsc_b), which=None):
        got = ro._tsc()
    return (
        "(e) MUTANTE: trocar `TOURING_TSC` troca o resultado (o ambiente manda sobre o cache)",
        got == tsc_b,
        f"got={got} esperado={tsc_b}",
    )


def cenario_f() -> tuple:
    texto = Path(ro.__file__).read_text(encoding="utf-8")
    achados = RE_ABSOLUTO_HOME.findall(texto)
    return (
        "(f) o modulo nao carrega caminho absoluto de HOME (o vazamento nao volta)",
        not achados,
        f"achados={achados}",
    )


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        cen = [
            cenario_a(raiz),
            cenario_b(raiz),
            cenario_c(raiz),
            cenario_d(),
            cenario_e(raiz),
            cenario_f(),
        ]
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest do resolvedor de tsc: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_do_resolvedor_de_tsc():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest do resolvedor de tsc reprovou: rode `python3 scripts/test_rotular_oss_tsc.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
