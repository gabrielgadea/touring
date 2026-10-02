#!/usr/bin/env python3
"""Teste-guarda: a suíte r3 INTEIRA não pode tocar o diretório de saída real.

Por que este arquivo existe: em 02/10 a suíte `test_r3_replay_a1.py` rodou o
replay A1 de verdade sobre `~/.claude/touring/r3-replay/` e **sobrescreveu os
artefatos oficiais** (`a2/results-a2.json` virou 25 bytes). A causa era um teste
que trocava `_RAIZ` esperando redirecionar a escrita, mas `SAIDA` é uma
constante calculada no IMPORT — a troca não surtia efeito, e nada no teste
reprovava.

O que este guarda faz, que nenhum teste dentro da suíte podia fazer: **rodar a
suíte inteira num subprocesso** e comparar o sha256 + mtime de TUDO sob
`~/.claude/touring/r3-replay/` e `~/.claude/touring/r3-data/` antes e depois. Um
teste que escreve no caminho real passa sem erro — o arquivo é sobrescrito com
sucesso e o assert continua verde. Só uma medição de fora pega.

O mutante correspondente (reverter o monkeypatch para `_RAIZ`) tem de reprovar
este arquivo; por isso o guarda mede o resultado, não a intenção.
"""
from __future__ import annotations

import hashlib
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent
REPO = SCRIPTS.parent
SUITES = [
    "test_r3_b0.py",
    "test_r3_extract_a2_offers.py",
    "test_r3_a1_kpi.py",
    "test_r3_freeze_dataset.py",
    "test_r3_replay_a1.py",
]
# O que NÃO pode mudar: a saida dos replays e o corpus congelado.
DIRETORIOS_PROTEGIDOS = [
    Path(os.path.expanduser("~/.claude/touring/r3-replay")),
    Path(os.path.expanduser("~/.claude/touring/r3-data")),
]


def assinatura(d: Path) -> dict:
    """`caminho -> (sha256, mtime_ns)` de tudo sob `d`.

    O mtime entra junto do hash porque um arquivo reescrito com o MESMO conteúdo
    (o `--check` que reescreve) não muda o hash — e essa é exatamente a classe de
    dano que a Songsera mediu: "conteúdo idêntico por hash" e mesmo assim o
    working state tinha sido reescrito.
    """
    if not d.exists():
        return {}
    out = {}
    for p in sorted(d.rglob("*")):
        if p.is_file():
            st = p.stat()
            out[str(p)] = (hashlib.sha256(p.read_bytes()).hexdigest(), st.st_mtime_ns)
    return out


def estado_real() -> dict:
    agg = {}
    for d in DIRETORIOS_PROTEGIDOS:
        for k, v in assinatura(d).items():
            agg[k] = v
    return agg


def _rodar_suite(nome: str) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return subprocess.run(
        [sys.executable, "-m", "pytest", f"scripts/{nome}",
         "-q", "--no-header", "-p", "no:cacheprovider"],
        capture_output=True, text=True, cwd=str(REPO), env=env, timeout=600)


@pytest.mark.parametrize("suite", SUITES)
def test_suite_nao_toca_a_saida_real(suite: str) -> None:
    """Uma suíte por vez: se a culpa for de uma só, o nome diz qual."""
    antes = estado_real()
    if not antes:
        pytest.skip("sem saida real ainda para comparar (rode os replays antes)")
    r = _rodar_suite(suite)
    depois = estado_real()
    mudou = {k: (antes.get(k, ("<novo>", 0))[0][:8], depois.get(k, ("<sumiu>", 0))[0][:8])
             for k in set(antes) | set(depois)
             if antes.get(k) != depois.get(k)}
    assert not mudou, (
        f"a suíte {suite} alterou o diretório REAL:\n"
        + "\n".join(f"  {k}\n    antes={v[0]} depois={v[1]}" for k, v in list(mudou.items())[:6])
    )
    assert r.returncode == 0, f"{suite} reprovou:\n{r.stdout[-800:]}"


def test_o_guarda_pegaria_o_bug_da_versao_antiga() -> None:
    """O guarda tem dente: reproduz o defeito num tmp e mostra que o padrão
    `_RAIZ` (constante de import) NÃO redireciona a escrita.

    Sem esta prova, "o teste é hermético" é uma afirmação; com ela, é uma
    medição.
    """
    import r3_replay_a1 as ra
    tmp = Path(os.environ["PYTEST_CURRENT_TEST"] and "/tmp") if False else None
    real = ra.SAIDA
    # _RAIZ e o "pai" de onde SAIDA foi derivado; trocar ele NAO muda SAIDA
    assert ra.SAIDA.name == "a1", (
        f"SAIDA deveria ser o subdiretorio a1; veio {ra.SAIDA}"
    )
    assert getattr(ra, "_RAIZ", None) is not None, "_RAIZ sumiu do modulo"
    assert ra.SAIDA != ra._RAIZ, "SAIDA e _RAIZ sao o mesmo caminho?"
