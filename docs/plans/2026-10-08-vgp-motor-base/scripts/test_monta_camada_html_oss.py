#!/usr/bin/env python3
"""Os testes do montador da camada html — ANTES de rotular qualquer coisa.

O que eles seguram (o plano do D-8, §4):

  (a) CONTROLE POSITIVO — um commit que introduz uma URL de CDN sem versao exata produz uma
      entrada POSITIVA, classe `cdn_pin`, com `nome_esperado` == a URL COMO ESCRITA;
  (b) CONTROLE NEGATIVO — um commit que toca `.html` e nao introduz nada produz uma entrada
      NEGATIVA (e o commit que so introduce uma versao EXATA tambem e negativo: versao exata nao
      e `cdn_pin`);
  (c) MUTANTE DA BASE DO DELTA — **toda** entrada carrega `snap_antes`, o arquivo existe, e no
      positivo o `snap_antes` e o ESTADO ANTES. Este e o defeito que o corpus de 10/10 tinha (as
      entradas nao carregavam o "antes", e sem ele o juiz nao tem contra o que comparar: o falso
      deny mediria a base ausente, nao o motor);
  (d) MUTANTE DO `nome_esperado` — o nome e a URL como escrita, nao o valor cru nem normalizado;
  (e) IDEMPOTENCIA — duas execucoes sobre o mesmo repo dao os mesmos ids;
  (f) MUTANTE DO TETO — a poda para o teto e um ponto fixo (podar baixa o n, e baixar o n baixa
      o teto).

Uso: python3 -I test_monta_camada_html_oss.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import monta_camada_html_oss as M  # noqa: E402

CORTE = "2099-01-01T00:00:00Z"
URL_PIN = "https://cdn.jsdelivr.net/npm/gsap@3.13.0/dist/gsap.min.js"  # versao EXATA -> `cdn`
URL_SEM_PIN = "https://unpkg.com/react"  # sem versao -> `cdn_pin`
HTML_1 = f'<html><body><script src="{URL_PIN}"></script></body></html>\n'
HTML_2 = f'<html><body><script src="{URL_SEM_PIN}"></script></body></html>\n'
HTML_3 = '<html><body><p>texto novo</p></body></html>\n'


def git(d: Path, *a: str) -> str:
    return subprocess.run(
        ["git", "-C", str(d), *a], capture_output=True, text=True, check=False
    ).stdout


def commit(d: Path, conteudo: str, msg: str) -> None:
    (d / "a.html").write_text(conteudo, encoding="utf-8")
    git(d, "add", "-A")
    subprocess.run(
        ["git", "-C", str(d), "commit", "-q", "-m", msg], check=True, timeout=120
    )


def repo_sintetico(raiz: Path) -> Path:
    """Tres commits: (1) cria o html com versao EXATA (nao e `cdn_pin`); (2) troca por uma URL
    sem versao (e um POSITIVO); (3) muda so o texto (e um NEGATIVO)."""
    d = raiz / "repo"
    d.mkdir(parents=True)
    subprocess.run(["git", "init", "-q", str(d)], check=True, timeout=120)
    git(d, "config", "user.email", "t@t.invalid")
    git(d, "config", "user.name", "t")
    commit(d, HTML_1, "c1")
    commit(d, HTML_2, "c2")
    commit(d, HTML_3, "c3")
    return d


def roda(d: Path, saida: Path) -> list:
    (saida / "snap").mkdir(parents=True, exist_ok=True)
    (saida / "snap_antes").mkdir(parents=True, exist_ok=True)
    entradas, _counts, _x = M.seleciona(d, CORTE, 100, set(), saida)
    return entradas


def main() -> int:
    cen: list[tuple] = []
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        d = repo_sintetico(raiz)
        saida = raiz / "saida"
        entradas = roda(d, saida)
        por_rotulo = {}
        for e in entradas:
            por_rotulo.setdefault(e["rotulo"], []).append(e)
        positivos = por_rotulo.get("positivo", [])
        negativos = por_rotulo.get("negativo", [])

        cen.append(
            (
                "(a) CONTROLE POSITIVO: o commit que introduz a URL sem versao da 1 entrada "
                "positiva de classe `cdn_pin`",
                len(positivos) == 1 and positivos[0]["classe"] == "cdn_pin",
                f"positivos={len(positivos)} classes={[e['classe'] for e in positivos]}",
            )
        )
        cen.append(
            (
                "(b) CONTROLE NEGATIVO: os outros dois commits (versao exata e mudanca de texto) "
                "sao negativos",
                len(negativos) == 2,
                f"negativos={len(negativos)}",
            )
        )

        # (c) MUTANTE DA BASE DO DELTA: toda entrada carrega o "antes", e no positivo o
        # `snap_antes` e o estado ANTES (a URL com versao exata).
        com_antes = [e for e in entradas if not e.get("snap_antes")]
        arquivo_existe = all(
            (saida / e["snap_antes"]).is_file()
            for e in entradas
            if e.get("snap_antes")
        )
        antes_e_o_antes = False
        if positivos:
            e = positivos[0]
            antes_e_o_antes = URL_PIN in (saida / e["snap_antes"]).read_text(
                encoding="utf-8"
            ) and URL_SEM_PIN not in (saida / e["snap_antes"]).read_text(encoding="utf-8")
        cen.append(
            (
                "(c) MUTANTE DA BASE DO DELTA: toda entrada carrega `snap_antes`, o arquivo "
                "existe, e no positivo ele e o estado ANTES",
                not com_antes and arquivo_existe and antes_e_o_antes,
                f"sem snap_antes={len(com_antes)} arquivo_existe={arquivo_existe} "
                f"antes_e_o_antes={antes_e_o_antes}",
            )
        )

        # (d) MUTANTE DO `nome_esperado`: a URL COMO ESCRITA, nao o valor cru nem normalizado.
        nome_certo = bool(positivos) and positivos[0]["nome_esperado"] == URL_SEM_PIN
        cen.append(
            (
                "(d) MUTANTE DO `nome_esperado`: e a URL como escrita no atributo",
                nome_certo,
                f"nome_esperado={positivos[0]['nome_esperado'] if positivos else None!r}",
            )
        )

        # (e) IDEMPOTENCIA: duas execucoes dao os mesmos ids.
        ids_1 = sorted(e["id"] for e in entradas)
        saida2 = raiz / "saida2"
        ids_2 = sorted(e["id"] for e in roda(d, saida2))
        cen.append(
            (
                "(e) IDEMPOTENCIA: duas execucoes dao os mesmos ids",
                ids_1 == ids_2 and len(ids_1) > 0,
                f"n={len(ids_1)} iguais={ids_1 == ids_2}",
            )
        )

        # (f) MUTANTE DO TETO: a poda corta no teto DADO e e um ponto fixo (re-podar nao muda).
        lote = [
            {"id": f"x{i}", "path": "repo/a.html", "classe": "", "rotulo": "negativo"}
            for i in range(10)
        ]
        podadas, n_podadas = M.podar_por_teto(lote, 3)
        podadas2, n_podadas2 = M.podar_por_teto(podadas, 3)
        cen.append(
            (
                "(f) MUTANTE DO TETO: a poda corta no teto DADO e e um ponto fixo",
                len(podadas) == 3 and n_podadas == 7 and len(podadas2) == 3 and n_podadas2 == 0,
                f"podadas={len(podadas)} n_podadas={n_podadas} fixo={len(podadas2)} "
                f"segunda_poda={n_podadas2}",
            )
        )

    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest do montador da camada html: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_do_montador_html():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest do montador html reprovou: rode `python3 scripts/test_monta_camada_html_oss.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
