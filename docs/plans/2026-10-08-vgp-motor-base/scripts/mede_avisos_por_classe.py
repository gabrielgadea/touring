"""AVISOS EM NEGATIVOS, POR CLASSE — a medida util no HTML, sem tocar no juiz.

A coordenadora (10/10): "no HTML a politica quase nunca chega a deny (um recurso local ausente em
arquivo do projeto vira aviso), entao o falso deny tende a 0 por construcao. Nos 833 negativos, a
medida util e «AVISOS em negativos» por classe."

O juiz tem `avisos_em_negativos` (L248-249) mas como contador UNICO — nao por classe. Como o juiz
nao se mexe, este script conta direto: chama o motor uma vez por entrada negativa e agrega os
`findings` de severidade `warn` pela classe do ACHADO.

USO (a coordenadora usa este instrumento no J, com o binario INTEGRADO):
    python3 mede_avisos_por_classe.py --touring <binario-integrado> --corpus <dir-do-corpus>

O `--touring` padrao e o f55ec4b5, que e o binario do P0: motor PYTHON, anterior a VL0, SEM
adaptador de HTML. MEDIDO com este script e com `scratch/prova_motor_fala_html.py`: ele produz
zero achado ate em paginas com defeito verificavel a olho (recurso ausente, ancora quebrada,
classe sem estilo). Entao o silencio que ele devolve NAO e medida de corpus — e ausencia de
adaptador. Com o binario integrado, o mesmo script passa a medir o que interessa.

A REGRA ZERO deste instrumento: o erro da chamada e contado SEPARADO do silencio. `rodar_motor`
devolve `{"_erro": ...}` quando a chamada falha, e um dict sem `findings` tem `.get(...)` vazio —
sem a separacao, um motor que ERRASSE em todas as chamadas se reportaria como "0 avisos", que e
exatamente o numero de quem viu e nao falou. Erro e silencio nao se somam.
"""
import json
import subprocess
from collections import Counter
from pathlib import Path

CORPUS_PADRAO = Path.home() / ".claude/touring/vgp/corpus_semeado/html"
# PADRAO = binario congelado f55ec4b5 (motor PYTHON, anterior a VL0, SEM adaptador de HTML —
# medido: produz zero achado ate em pagina com defeito a olho). Parametrize com `--touring` para
# o binario INTEGRADO; a coordenadora usa este instrumento no J.
PADRAO_MOTOR = Path.home() / ".claude/touring/vgp/binarios/f55ec4b5/touring"
JUIZ = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/juiz_motor_vgp.py"
)


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser(description="avisos em negativos, por classe")
    ap.add_argument(
        "--touring",
        default=str(PADRAO_MOTOR),
        help="binario do motor; o integrado e o que produz achado em HTML",
    )
    ap.add_argument(
        "--corpus",
        default=str(CORPUS_PADRAO),
        help="diretorio do corpus (o que tem entradas.jsonl e snap/)",
    )
    a = ap.parse_args()
    motor = a.touring
    corpus = Path(a.corpus)
    """Reusa a funcao do juiz em vez de reimplementar a chamada: uma divergencia aqui seria
    uma medida de outra coisa. Importa o juiz como modulo e chama `rodar_motor` dele."""
    import importlib.util

    spec = importlib.util.spec_from_file_location("juiz", JUIZ)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)  # noqa: E402

    entradas = [
        json.loads(x)
        for x in (corpus / "entradas.jsonl").read_text().splitlines()
        if x.strip()
    ]
    negativos = [e for e in entradas if e["rotulo"] == "negativo"]
    print(f"=== {len(negativos)} negativos em {corpus} ===")

    warn_por_classe = Counter()
    deny_por_classe = Counter()
    achados_total = 0
    por_id = []
    erros = Counter()
    n_erro = 0
    for e in negativos:
        saida = mod.rodar_motor([motor], e, corpus)
        # MEDIDO em 10/10: `rodar_motor` devolve {"_erro": ...} quando a chamada falha, e um
        # dict sem `findings` tem `.get("findings", [])` = vazio. Sem esta contagem, um motor que
        # ERROU em todas as 200 chamadas se reportaria como "0 avisos" — o numero exato de quem
        # viu e nao falou. Erro e silencio sao coisas diferentes e nao se somam.
        if isinstance(saida, dict) and saida.get("_erro"):
            n_erro += 1
            erros[str(saida["_erro"])[:120]] += 1
            continue
        achados = saida.get("findings", []) if isinstance(saida, dict) else []
        n_warn = sum(1 for f in achados if f.get("severidade") == "warn")
        n_deny = sum(1 for f in achados if f.get("severidade") == "deny")
        achados_total += len(achados)
        # o campo `classe` do corpus esta vazio nos negativos; o que identifica a classe do
        # ACHADO e o proprio achado (`classe`/`regra`). Sem ele, o corte e so global.
        for f in achados:
            if f.get("severidade") == "warn":
                warn_por_classe[f.get("classe") or f.get("regra") or "?"] += 1
            if f.get("severidade") == "deny":
                deny_por_classe[f.get("classe") or f.get("regra") or "?"] += 1
        if n_warn or n_deny:
            por_id.append((e["id"], n_warn, n_deny))

    print(f"  chamadas com ERRO = {n_erro}/{len(negativos)}  <- nao contam para nenhuma media")
    for k, v in erros.most_common(3):
        print(f"      {v:>4d}x {k}")
    print(f"  chamadas que responderam = {len(negativos) - n_erro}")
    print(f"  achados no total = {achados_total}")
    print(f"  avisos em negativos (contador global do juiz) = {sum(warn_por_classe.values())}")
    print(f"  denies em negativos = {sum(deny_por_classe.values())}")
    if n_erro == len(negativos):
        print()
        print("  VEREDITO: o motor NAO respondeu em nenhuma chamada. Os 0 avisos acima sao")
        print("  FALHA FECHADA do juiz, nao comportamento do motor. Nao ha medida aqui.")
    print()
    print("=== avisos por classe do ACHADO ===")
    if not warn_por_classe:
        print("  (nenhum — o motor nao produz warn nenhum nestes negativos)")
    for k, v in warn_por_classe.most_common():
        print(f"  {k:28s} {v}")
    print()
    print("=== denies por classe do ACHADO ===")
    if not deny_por_classe:
        print("  (nenhum)")
    for k, v in deny_por_classe.most_common():
        print(f"  {k:28s} {v}")
    if por_id:
        print()
        print("=== negativos que produziram achado (primeiros 5) ===")
        for i, w, d in por_id[:5]:
            print(f"  {i}  warn={w} deny={d}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())