#!/usr/bin/env python3
"""A DISJUNCAO entre corpora no rotulador OSS -- um MUTANTE por clausula do contrato.

MEDIDO em 01/11 (V-8): o rotulador nao tinha nenhuma disjuncao entre corporas. `estado_no_disco`
le SO o diretorio de SAIDA e o `grava_entrada` REESCREVE o id em vez de recusar o par -- ou seja,
um par que um corpus anterior ja rotulou seria rotulado DE NOVO, e a camada nova repetiria a
anterior par a par. Um held-out que repete o holdout anterior nao prova nada.

O selftest nao testa a happy path: testa que cada GARANTIA some quando ela e removida.

  (a) o par excluido NAO CHEGA AO ADJUDICADOR -- e o unico jeito de provar a disjuncao;
  (b) o par excluido NAO ENTRA NO TETO -- o teto mede o que foi ROTULADO, e contar o excluido
      daria um n maior que o n da camada;
  (c) SEM a flag, o comportamento e o de sempre -- nenhum par e recusado por uso anterior;
  (d) o `id` da exclusao e o MESMO nome do `id` gravado -- duas copias do calculo divergem em
      silencio, e uma divergencia silenciosa aqui significa um par repetido num held-out.

Uso: python3 -I test_rotular_oss_exclusao.py   (exit 0 = tudo verde)
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import rotular_oss as ro


# --------------------------------------------------------------------------------------
# um repo git de mentira: o teste precisa de `git log` de verdade, e nao de um mock
# --------------------------------------------------------------------------------------
def repo_falso(caminho: Path) -> Path:
    caminho.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(caminho)], check=True, timeout=120)
    subprocess.run(
        ["git", "-C", str(caminho), "config", "user.email", "t@t.invalid"], check=True
    )
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
    """O adjudicador, sem adjudicador: so o suficiente para o laco de selecao rodar."""

    def __init__(self, repo):  # noqa: D107
        self.repo = repo

    def abrir(self, repo, tsconfig):
        return {"ok": True}

    def fecha(self):
        return None


# --------------------------------------------------------------------------------------
# (a) e (b): o par excluido nao chega ao adjudicador e nao entra no teto
# --------------------------------------------------------------------------------------
def cenario_excluido_nao_chega_ao_oraculo() -> list:
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        raiz = Path(tmp)
        repo = repo_falso(raiz / "repo-falso")
        commit = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        pares = [
            par_falso(commit, "a.ts"),
            par_falso(commit, "b.ts"),
        ]
        id_a = ro.ident_do_par("repo-falso", pares[0])
        id_b = ro.ident_do_par("repo-falso", pares[1])

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
            classificados.append(ro.ident_do_par(nome_repo, par))
            # o `classifica_um` real CONTA no teto -- o stub precisa contar tambem, senao o
            # teste passaria por um teto que nunca foi medido
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
                alvo=300, teto_concentracao=0.03, max_commits=10, dry_run=True,
                saida=raiz / "saida",
            )
            por_lang: dict = {}
            rel = ro.rotula_repo("repo-falso", a, por_lang, {id_a})

            cen.append(
                (
                    "o par EXCLUIDO nao chega ao adjudicador",
                    id_a not in classificados,
                    f"classificados={classificados}",
                )
            )
            cen.append(
                (
                    "o par NAO excluudo chega ao adjudicador (controle positivo)",
                    id_b in classificados,
                    f"classificados={classificados}",
                )
            )
            cen.append(
                (
                    "o par excluido e CONTADO no relatorio do repo",
                    rel.get("pares_excluidos_ja_usados") == 1,
                    str(rel.get("pares_excluidos_ja_usados")),
                )
            )
            # (b): o teto mede o que foi ROTULADO. O par excluido nao entra em `por_lang`.
            cen.append(
                (
                    "o par excluido NAO entra no n da linguagem (o teto mede o rotulado)",
                    por_lang.get("typescript") == 1,
                    f"por_lang={por_lang}",
                )
            )
            cen.append(
                (
                    "o relatorio diz que UM par foi excluido, por linguagem",
                    rel.get("excluidos_por_lang") == {"typescript": 1},
                    str(rel.get("excluidos_por_lang")),
                )
            )
        finally:
            for k, v in original.items():
                setattr(ro, k, v)
    return cen


# --------------------------------------------------------------------------------------
# (c): SEM a flag, o comportamento e o de sempre
# --------------------------------------------------------------------------------------
def cenario_sem_exclusao_nao_filtra_nada() -> list:
    cen = []
    pares = [par_falso("c1", "a.ts"), par_falso("c2", "b.ts")]
    for rotulo, mascara in (
        ("excluidos=None", None),
        ("excluidos=vazio", set()),
    ):
        mantidos, fora, _ = ro.filtra_pares_excluidos("r", pares, mascara)
        cen.append(
            (
                f"com {rotulo} NENHUM par e descartado",
                len(mantidos) == 2 and len(fora) == 0,
                f"mantidos={len(mantidos)} fora={len(fora)}",
            )
        )
    id_a = ro.ident_do_par("r", pares[0])
    mantidos, fora, _ = ro.filtra_pares_excluidos("r", pares, {id_a})
    cen.append(
        (
            "com a mascara o par do id e o unico descartado",
            len(mantidos) == 1 and len(fora) == 1 and fora[0] is pares[0],
            f"mantidos={len(mantidos)} fora={len(fora)}",
        )
    )
    return cen


# --------------------------------------------------------------------------------------
# (d): o `id` da exclusao e o MESMO nome do `id` gravado
# --------------------------------------------------------------------------------------
def cenario_id_da_exclusao_e_o_id_gravado() -> list:
    cen = []
    with tempfile.TemporaryDirectory() as tmp:
        saida = Path(tmp) / "saida"
        par = par_falso("c1", "a.ts")
        vistos: set = set()
        ident = ro.grava_entrada(saida, "repo-falso", par, "typescript", "negativo", vistos)
        jl = saida / "typescript" / "entradas.jsonl"
        gravado = json.loads(jl.read_text(encoding="utf-8").splitlines()[0])
        cen.append(
            (
                "o id gravado no disco e o id que a exclusao compara",
                gravado["id"] == ident == ro.ident_do_par("repo-falso", par),
                f"gravado={gravado['id']} ident={ident}",
            )
        )
        # e o corpus anterior RECONHECE esse id
        ids, por_origem = ro.ids_excluidos([saida])
        cen.append(
            (
                "o corpus anterior devolve o id gravado como id excluido",
                ident in ids and por_origem.get(str(saida)) == 1,
                f"ids={len(ids)} por_origem={por_origem}",
            )
        )
        # (e) a saida nao se exclui a si mesma: uma reexecucao reescreve, nao pula
        ids2, _ = ro.ids_excluidos([saida], ignorar=saida)
        cen.append(
            (
                "a saida NAO se exclui a si mesma (a reexecucao continua idempotente)",
                len(ids2) == 0,
                f"ids={len(ids2)}",
            )
        )
    return cen


def main() -> int:
    cen = []
    cen.extend(cenario_excluido_nao_chega_ao_oraculo())
    cen.extend(cenario_sem_exclusao_nao_filtra_nada())
    cen.extend(cenario_id_da_exclusao_e_o_id_gravado())
    falhas = [f"{n}: {m}" for n, ok, m in cen if not ok]
    print(f"selftest da disjuncao do rotulador: {len(cen) - len(falhas)}/{len(cen)}")
    for n, ok, m in cen:
        print(f"  {'ok  ' if ok else 'FALHOU'} {n} -- {m}")
    return 0 if not falhas else 1


def test_selftest_da_disjuncao():
    """O selftest como o pytest o ve (exit 5 = 'nada a fazer' lido como verde)."""
    assert main() == 0, (
        "o selftest da disjuncao reprovou: rode "
        "`python3 scripts/test_rotular_oss_exclusao.py`"
    )


if __name__ == "__main__":
    raise SystemExit(main())
