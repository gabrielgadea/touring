"""Mede o shell_inline no universo real: unicidade, candidatos e a taxa de `compila` (NATIVO).

Escreve medir_candidatos.txt (resumo) e medir_nao_compila.txt (3 amostras por categoria que não compila).
Uso: cd ~/projects/analise && .venv/bin/python <este arquivo>
"""

from __future__ import annotations

import collections
import random
import sys
import warnings
from pathlib import Path

SC = Path(__file__).parent
sys.path.insert(0, str(SC))
sys.path.insert(0, str(Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"))
from medir_programa_inline import comandos  # noqa: E402

import shell_inline as si  # noqa: E402


def erro_de_compilacao(texto: str) -> str | None:
    """None se compila; senão `Tipo: mensagem` (SyntaxWarning de texto que nem é Python não conta)."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        try:
            compile(texto, "<x>", "exec")
        except (SyntaxError, ValueError) as e:
            return f"{type(e).__name__}: {str(e)[:70]}"
    return None


def medir(cmds: dict[str, str], subst: bool, saida: list[str], amostras: dict) -> None:
    unicos = comp_unicos = 0
    n_cand = collections.Counter()
    por_via: dict[tuple[str, bool], list[int]] = collections.defaultdict(lambda: [0, 0])
    sem_cand_py = 0
    motivos_sem = collections.Counter()
    for cmd in cmds.values():
        cands = [c for c in si.candidatos(cmd, subst) if c.texto is not None]
        n_cand[min(len(cands), 3)] += 1
        for c in cands:
            erro = erro_de_compilacao(c.texto or "")
            chave = (c.via, c.prof > 0)
            por_via[chave][0] += 1
            por_via[chave][1] += erro is None
            if erro is not None:
                amostras[(subst, c.via, c.prof > 0, erro.split(":")[1].strip()[:38])].append(cmd)
        v = si.analisar(cmd, subst)
        if v.texto is not None:
            unicos += 1
            comp_unicos += erro_de_compilacao(v.texto) is None
        elif not cands and ("python" in cmd):
            sem_cand_py += 1
            motivos_sem[v.motivo[:55]] += 1
    saida.append(f"\n===== substituicoes={subst}  (universo {len(cmds)} comandos Bash)")
    saida.append(f"analisar devolve texto único em {unicos}; compilam {comp_unicos} ({100 * comp_unicos / max(unicos, 1):.1f}%)")
    saida.append(f"nº de candidatos por comando: 0={n_cand[0]} 1={n_cand[1]} 2={n_cand[2]} 3+={n_cand[3]}")
    tot = sum(v[0] for v in por_via.values())
    ok = sum(v[1] for v in por_via.values())
    saida.append(f"TODOS os candidatos com texto: {tot}; compilam {ok} ({100 * ok / max(tot, 1):.1f}%)")
    for (via, aninhado), (n, c) in sorted(por_via.items(), key=lambda kv: -kv[1][0]):
        saida.append(f"   via={via:18} aninhado={str(aninhado):5}  n={n:5}  compila={c:5} ({100 * c / n:.1f}%)")
    saida.append(f"comandos que mencionam 'python', SEM candidato: {sem_cand_py}")
    for m, k in motivos_sem.most_common(8):
        saida.append(f"   {k:5}  {m}")


def main() -> int:
    cmds = comandos()
    saida: list[str] = []
    amostras: dict = collections.defaultdict(list)
    for subst in (False, True):
        medir(cmds, subst, saida, amostras)
    (SC / "medir_candidatos.txt").write_text("\n".join(saida))
    rnd = random.Random(20261009)
    linhas = []
    for chave, lista in sorted(amostras.items(), key=lambda kv: -len(kv[1])):
        subst, via, aninhado, erro = chave
        if not subst:
            continue
        linhas.append(f"\n#### via={via} aninhado={aninhado} erro≈{erro!r}: {len(lista)} candidatos")
        for cmd in rnd.sample(lista, min(3, len(lista))):
            cands = [c for c in si.candidatos(cmd, True) if c.texto is not None]
            c = next(x for x in cands if erro_de_compilacao(x.texto or "") and erro in (erro_de_compilacao(x.texto or "") or ""))
            linhas.append(f"--- programa extraído (início): {c.texto[:420]!r}")
            linhas.append(f"    erro: {erro_de_compilacao(c.texto or '')}")
    (SC / "medir_nao_compila.txt").write_text("\n".join(linhas))
    print("\n".join(saida))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
