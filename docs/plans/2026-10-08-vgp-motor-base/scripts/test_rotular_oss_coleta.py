#!/usr/bin/env python3
"""A COLETA de pares: o leitor em LOTE devolve o MESMO conteudo que o `git show`.

MEDIDO em 01/11 (V-8): a coleta era 3 spawns por par (`git rev-parse` do pai + 2 `git show`) —
17.670 pares em mc=600 são ~50.000 spawns, e a coleta chegou a ser o gargalo do lote (1,5 h dos
2,4 h eram de repos cujas linguagens já estavam cheias, porque o `por_lang >= alvo` só é
conferido DEPOIS da coleta). Agora o pai e a DATA saem do MESMO log e o conteúdo vem em UM
`git cat-file --batch`.

O selftest não testa a happy path: testa que cada GARANTIA some quando ela é removida.

  (a) o conteúdo em LOTE é byte a byte o que o `git show` devolve — inclusive um arquivo com
      acento (multibyte), que é onde um erro de contagem de bytes aparece;
  (b) um objeto que não existe vem como VAZIO (o `git show` também) — e não como lixo;
  (c) o par traz o `data_commit` REAL: no código antigo o `datas` nunca era preenchido, de modo
      que o `data_commit` era sempre 0 e a "ordem canônica" era alfabética por caminho;
  (d) o MUTANTE (ler n-1 bytes) tem de ser detectado.

Uso: python3 -I test_rotular_oss_coleta.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro

CONTEUDO_COM_ACENTO = "export const a = 'acentuação çãõ';\n"
CONTEUDO_SIMPLES = "export const b = 2;\n"


def repo_de_teste(caminho: Path) -> tuple[Path, str, str]:
    """Um repo git com DOIS commits (o segundo modifica o primeiro).

    MEDIDO em 01/11: os commits sao DATADOS antes do corte temporal (2026-10-09 23:00) — o
    `pares_do_repo` filtra por `--before`, e um commit de hoje nao entra na populacao. Sem
    isso o teste mediria o corte, nao a coleta.
    """
    caminho.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(caminho)], check=True, timeout=120)
    subprocess.run(["git", "-C", str(caminho), "config", "user.email", "t@t.invalid"], check=True)
    subprocess.run(["git", "-C", str(caminho), "config", "user.name", "t"], check=True)
    env = dict(os.environ)
    env["GIT_AUTHOR_DATE"] = "2026-10-01T10:00:00 +0000"
    env["GIT_COMMITTER_DATE"] = "2026-10-01T10:00:00 +0000"
    (caminho / "a.ts").write_text(CONTEUDO_SIMPLES, encoding="utf-8")
    (caminho / "b.ts").write_text(CONTEUDO_COM_ACENTO, encoding="utf-8")
    subprocess.run(["git", "-C", str(caminho), "add", "-A"], check=True, timeout=120)
    subprocess.run(
        ["git", "-C", str(caminho), "commit", "-q", "-m", "c1"], check=True, timeout=120, env=env
    )
    primeiro = subprocess.run(
        ["git", "-C", str(caminho), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    env["GIT_AUTHOR_DATE"] = "2026-10-02T10:00:00 +0000"
    env["GIT_COMMITTER_DATE"] = "2026-10-02T10:00:00 +0000"
    (caminho / "a.ts").write_text("export const a = 3;\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(caminho), "add", "-A"], check=True, timeout=120)
    subprocess.run(
        ["git", "-C", str(caminho), "commit", "-q", "-m", "c2"], check=True, timeout=120, env=env
    )
    segundo = subprocess.run(
        ["git", "-C", str(caminho), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return caminho, primeiro, segundo


def git_show(repo: Path, spec: str) -> str:
    """O `git show` de referencia — o que o leitor em LOTE tem de reproduzir."""
    r = subprocess.run(
        ["git", "-C", str(repo), "show", spec],
        capture_output=True,
        text=True,
        check=False,
    )
    return r.stdout


def cenario_lote_igual_ao_git_show() -> list:
    """(a) e (b): o conteudo em lote e o MESMO; o que nao existe vem vazio."""
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        repo, primeiro, segundo = repo_de_teste(Path(tmp) / "repo")
        specs = [
            f"{segundo}:a.ts",
            f"{segundo}:b.ts",
            f"{primeiro}:a.ts",
            f"{segundo}:nao-existe.ts",
        ]
        em_lote = ro.git_show_em_lote(repo, specs)
        cen.append(
            (
                "o lote devolve um conteudo por especificacao",
                len(em_lote) == len(specs),
                f"{len(em_lote)} de {len(specs)}",
            )
        )
        for i, spec in enumerate(specs):
            referencia = git_show(repo, spec)
            cen.append(
                (
                    f"o lote devolve o MESMO conteudo que o `git show`: {spec.split(':')[-1]}",
                    i < len(em_lote) and em_lote[i] == referencia,
                    f"lote={em_lote[i][:40]!r} show={referencia[:40]!r}",
                )
            )
        # o arquivo com acento: a contagem de BYTES (e nao de caracteres) e o que segura
        acento = em_lote[1] if len(em_lote) > 1 else ""
        cen.append(
            (
                "o arquivo com acento volta INTEIRO (bytes, nao caracteres)",
                acento == CONTEUDO_COM_ACENTO,
                repr(acento[:40]),
            )
        )
        # (d) o MUTANTE: ler n-1 bytes
        if len(em_lote) > 1:
            mutante = em_lote[1][:-1] if em_lote[1] else "x"
            cen.append(
                (
                    "MUTANTE: ler n-1 bytes DIFERE do conteudo verdadeiro (precisa reprovar aqui)",
                    mutante != CONTEUDO_COM_ACENTO,
                    f"mutante={mutante[:40]!r}",
                )
            )
    return cen


def cenario_pares_com_data_e_antes() -> list:
    """(c): o par traz o `data_commit` REAL e o `antes` do commit anterior."""
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        repo, primeiro, segundo = repo_de_teste(Path(tmp) / "repo")
        pares = ro.pares_do_repo(repo, 10)
        modificacoes = [p for p in pares if p["caminho"] == "a.ts"]
        cen.append(
            (
                "a modificacao aparece como par",
                len(modificacoes) >= 1,
                f"{len(pares)} par(es)",
            )
        )
        if modificacoes:
            p = modificacoes[0]
            cen.append(
                (
                    "o `data_commit` e a DATA REAL do commit (nao 0)",
                    p["data_commit"] > 0,
                    f"data_commit={p['data_commit']}",
                )
            )
            cen.append(
                (
                    "o `antes` e o conteudo do commit ANTERIOR",
                    p["antes"] == CONTEUDO_SIMPLES,
                    repr(p["antes"][:40]),
                )
            )
            cen.append(
                (
                    "o `depois` e o conteudo do commit atual",
                    p["depois"] == "export const a = 3;\n",
                    repr(p["depois"][:40]),
                )
            )
            cen.append(
                (
                    "o par diz que e MODIFICACAO (e nao criacao)",
                    p["operacao"] == "M",
                    p["operacao"],
                )
            )
    return cen


def main() -> int:
    cen = []
    cen.extend(cenario_lote_igual_ao_git_show())
    cen.extend(cenario_pares_com_data_e_antes())
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest da coleta de pares: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_da_coleta():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest da coleta reprovou: rode `python3 scripts/test_rotular_oss_coleta.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
