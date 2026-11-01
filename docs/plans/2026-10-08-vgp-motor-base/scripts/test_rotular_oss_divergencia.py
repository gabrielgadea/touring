#!/usr/bin/env python3
"""A DIVERGENCIA entre o adjudicador e o `tsc`: o par sai, o lote continua.

MEDIDO em 01/11 (V-8): o desfecho antigo era PARAR O LOTE INTEIRO na primeira divergencia. O
efeito medido foi nenhum corpus: o `excalidraw` diverge em `manager.tsx` (o LS acusa TS2307 de
um modulo que o `tsc` resolve) e o lote morreu com 218 entradas — um corpus que nao existe nao
prova nada. Agora a divergencia EXCLUI o par (que e o item suspeito) e o lote continua: e o
fail-closed no nivel do par, que e onde a divergencia vale.

O selftest nao testa a happy path: testa que cada GARANTIA some quando ela e removida.
  (a) o par divergente NAO entra na camada;
  (b) os pares SEM divergencia continuam entrando (o lote nao morre);
  (c) a divergencia fica REGISTRADA no relatorio (o selo tem de mostrar o que foi excluido);
  (d) o MUTANTE (a divergencia volta a parar o lote) tem de ser detectado.

Uso: python3 -I test_rotular_oss_divergencia.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro


def repo_falso(caminho: Path) -> Path:
    caminho.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(caminho)], check=True, timeout=120)
    subprocess.run(["git", "-C", str(caminho), "config", "user.email", "t@t.invalid"], check=True)
    subprocess.run(["git", "-C", str(caminho), "config", "user.name", "t"], check=True)
    (caminho / "a.ts").write_text("export const a = 1;\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(caminho), "add", "-A"], check=True, timeout=120)
    subprocess.run(
        ["git", "-C", str(caminho), "commit", "-q", "-m", "c1"], check=True, timeout=120
    )
    return caminho


def par_falso(commit: str, caminho: str) -> dict:
    return {
        "commit": commit,
        "caminho": caminho,
        "antes": "export const a = 0;\n",
        "depois": "export const a = 1;\n",
        "operacao": "M",
        "data_commit": 1,
    }


class LS_Falso:
    def __init__(self, repo):  # noqa: D107
        self.repo = repo

    def abrir(self, repo, tsconfig):
        return {"ok": True}

    def fecha(self):
        return None


def roda_com_divergencia(divergente: str) -> tuple[list, dict, dict]:
    """Roda o `rotula_repo` com um stub de `classifica_um` que diverge em um par.

    Devolve (ids classificados, relatorio, por_lang).
    """
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        repo = repo_falso(raiz / "repo-falso")
        commit = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        pares = [par_falso(commit, "a.ts"), par_falso(commit, "b.ts")]
        classificados: list = []
        original = {
            "OSS": ro.OSS,
            "LS": ro.LS,
            "pares_do_repo": ro.pares_do_repo,
            "classifica_um": ro.classifica_um,
            "excludes_necessarios": ro.excludes_necessarios,
            "acrescenta_exclude_minimo": ro.acrescenta_exclude_minimo,
        }

        def classifica_falso(
            repo_, nome_repo, lsc, par, lang, a, por_lang, cont_arq, cont_commit, stats,
            vistos, retestar,
        ):
            ident = ro.ident_do_par(nome_repo, par)
            classificados.append(par["caminho"])
            if par["caminho"] == divergente:
                # a divergencia: o par NAO entra, e o lote continua
                stats["reteste_divergente"] += 1
                return {
                    "divergencia": {
                        "repo": nome_repo,
                        "caminho": par["caminho"],
                        "ls": [["TS2307", 1, "x"]],
                        "tsc": [],
                    }
                }
            cont_arq[par["caminho"]] += 1
            cont_commit[par["commit"]] += 1
            por_lang[lang] = por_lang.get(lang, 0) + 1
            return None

        try:
            ro.OSS = raiz
            ro.LS = LS_Falso
            ro.pares_do_repo = lambda repo_, mc: list(pares)
            ro.classifica_um = classifica_falso
            ro.excludes_necessarios = lambda repo_: []
            ro.acrescenta_exclude_minimo = lambda repo_, tsconfig, nec: []
            a = argparse.Namespace(
                alvo=300, teto_concentracao=0.03, max_commits=10, dry_run=True, saida=raiz / "s"
            )
            por_lang: dict = {}
            rel = ro.rotula_repo("repo-falso", a, por_lang, set())
        finally:
            for k, v in original.items():
                setattr(ro, k, v)
        return classificados, rel, por_lang


def main() -> int:
    cen = []
    classificados, rel, por_lang = roda_com_divergencia("a.ts")
    id_divergente = "a.ts"
    id_saudoavel = "b.ts"
    cen.append(
        (
            "(a) o par DIVERGENTE nao entra na camada (o n nao o conta)",
            por_lang.get("typescript") == 1,
            f"por_lang={por_lang}",
        )
    )
    cen.append(
        (
            "(b) o par SEM divergencia CONTINUA entrando (o lote nao morre)",
            id_saudoavel in classificados,
            f"classificados={classificados}",
        )
    )
    cen.append(
        (
            "(c) a divergencia fica REGISTRADA no relatorio",
            rel.get("n_divergencias") == 1 and rel.get("divergencias")[0]["caminho"] == "a.ts",
            f"n_divergencias={rel.get('n_divergencias')}",
        )
    )
    cen.append(
        (
            "(d) MUTANTE: se a divergencia voltasse a PARAR o lote, o par saudavel NAO seria "
            "classificado depois dela (precisa reprovar aqui)",
            classificados == [id_divergente, id_saudoavel],
            f"classificados={classificados}",
        )
    )
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest da divergencia: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_da_divergencia():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest da divergencia reprovou: rode "
        "`python3 scripts/test_rotular_oss_divergencia.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
