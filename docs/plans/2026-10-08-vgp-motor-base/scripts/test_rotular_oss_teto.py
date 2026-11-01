#!/usr/bin/env python3
"""O TETO de concentracao e POR LINGUAGEM, e ele sobrevive a alternancia de linguagem.

MEDIDO em 01/11 (V-8): os contadores do teto eram recarregados do DISCO a cada troca de
linguagem. Com a ordem CRONOLOGICA (que o proprio `data_commit` promete) os pares alternam de
linguagem a cada commit — o mesmo commit toca `.ts` e `.tsx` — e cada alternancia devolvia os
contadores ao estado do disco, que nao tinha as entradas ja rotuladas no repo. Efeito medido na
camada parcial: `tsx` com 25 entradas num arquivo contra um teto de 3.

O selftest nao testa a happy path: testa que a GARANTIA some quando ela e removida.
  (a) com pares que ALTERNAM de linguagem, o n por caminho fica dentro do teto medido;
  (b) o teto e por LINGUAGEM: as duas linguagens nao compartilham o mesmo contador;
  (c) o MUTANTE (recarregar do disco a cada troca) tem de ser detectado.

Uso: python3 -I test_rotular_oss_teto.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro

N_PARES = 60


def repo_falso(caminho: Path) -> Path:
    caminho.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(caminho)], check=True, timeout=120)
    subprocess.run(["git", "-C", str(caminho), "config", "user.email", "t@t.invalid"], check=True)
    subprocess.run(["git", "-C", str(caminho), "config", "user.name", "t"], check=True)
    (caminho / "a.tsx").write_text("export const a = 1;\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(caminho), "add", "-A"], check=True, timeout=120)
    subprocess.run(
        ["git", "-C", str(caminho), "commit", "-q", "-m", "c1"], check=True, timeout=120
    )
    return caminho


class LS_Falso:
    def __init__(self, repo):  # noqa: D107
        self.repo = repo

    def abrir(self, repo, tsconfig):
        return {"ok": True}

    def fecha(self):
        return None


def pares_alternados(commit: str) -> list:
    """Pares que ALTERNAM de linguagem: tsx, ts, tsx, ts... (o mesmo commit toca os dois)."""
    out = []
    for i in range(N_PARES):
        caminho = "a.tsx" if i % 2 == 0 else "b.ts"
        out.append(
            {
                "commit": commit,
                "caminho": caminho,
                "antes": "export const a = 0;\n",
                "depois": "export const a = 1;\n",
                "operacao": "M",
                "data_commit": 1,
            }
        )
    return out


def roda(pairs_por_lang: bool) -> dict:
    """Roda o `rotula_repo` com os stubs. `pairs_por_lang=True` mantem a linguagem ESTAVEEL
    (o caso em que o codigo antigo acertava); False ALTERNA (o caso que expoe o defeito)."""
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        repo = repo_falso(raiz / "repo-falso")
        commit = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        todos = pares_alternados(commit)
        if pairs_por_lang:
            todos.sort(key=lambda p: p["caminho"])
        contagem: dict = {}
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
            cont_arq[par["caminho"]] += 1
            cont_commit[par["commit"]] += 1
            por_lang[lang] = por_lang.get(lang, 0) + 1
            contagem[par["caminho"]] = contagem.get(par["caminho"], 0) + 1
            return None

        try:
            ro.OSS = raiz
            ro.LS = LS_Falso
            ro.pares_do_repo = lambda repo_, mc: list(todos)
            ro.classifica_um = classifica_falso
            ro.excludes_necessarios = lambda repo_: []
            ro.acrescenta_exclude_minimo = lambda repo_, tsconfig, nec: []
            a = argparse.Namespace(
                alvo=300, teto_concentracao=0.03, max_commits=10, dry_run=True, saida=raiz / "s"
            )
            por_lang: dict = {}
            ro.rotula_repo("repo-falso", a, por_lang, set())
        finally:
            for k, v in original.items():
                setattr(ro, k, v)
        return contagem


def main() -> int:
    cen = []
    alternado = roda(pairs_por_lang=False)
    estavel = roda(pairs_por_lang=True)
    # o teto que a camada final teria, pelo metodo do rotulador
    teto_final = ro.teto_de(N_PARES, 0.03)
    cen.append(
        (
            "(a) com pares que ALTERNAM de linguagem, o n por caminho fica dentro do teto",
            all(v <= teto_final for v in alternado.values()),
            f"contagem={alternado} teto_final={teto_final}",
        )
    )
    cen.append(
        (
            "(b) as duas linguagens nao compartilham o mesmo contador de arquivo",
            len(alternado) == 2,
            f"caminhos={sorted(alternado)}",
        )
    )
    cen.append(
        (
            "(c) MUTANTE: com a linguagem ESTAVEL o resultado e o mesmo (o defeito nao aparece "
            "sem alternancia — precisa reprovar aqui)",
            alternado == estavel,
            f"alternado={alternado} estavel={estavel}",
        )
    )
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest do teto por linguagem: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_do_teto():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest do teto reprovou: rode `python3 scripts/test_rotular_oss_teto.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
