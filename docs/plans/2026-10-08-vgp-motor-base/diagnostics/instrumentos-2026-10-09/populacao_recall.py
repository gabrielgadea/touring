"""Populacao exata do recall inline (as funcoes do PROPRIO juiz) no vivo e num staging, entrada por entrada.

Uso: python3 -I populacao_recall.py <staging_dir> <juiz_staging.json> <saida.json>
Membro da populacao = positivo inline com snap que compila e `classe_erro(mensagem) == "simbolo"`.
Acerto/perda vem do `perdidos` do JSON do juiz de cada corpus. Leitura apenas.
"""

import json
import sys
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
import juiz_motor_vgp as jm

SP = Path(__file__).resolve().parent
VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"
STAGING, JSON_S, SAIDA = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(p: Path) -> list[dict]:
    return [json.loads(ln) for ln in p.read_text(encoding="utf-8").splitlines() if ln.strip()]


def populacao(base: Path, perdidos: set[str]) -> dict[str, dict]:
    """id -> {'acerto': bool, 'classe': ...} dos positivos inline de simbolo que compilam."""
    r = {}
    for e in ler(base / "entradas.jsonl"):
        if e["rotulo"] != "positivo" or e["origem"] != "inline" or str(e.get("reproduz")).lower() != "true":
            continue  # o carregar() do juiz so mede positivos que REPRODUZEM
        if not jm.snap_compila(base, e) or jm.classe_erro(e.get("mensagem") or "") != "simbolo":
            continue
        r[e["id"]] = {"acerto": e["id"] not in perdidos, "classe": e.get("classe"), "nome": e["nome_esperado"]}
    return r


pv = populacao(VIVO, {p["id"] for p in json.loads((SP / "juiz_v2_vivo.json").read_text())["perdidos"]})
ps = populacao(STAGING, {p["id"] for p in json.loads(JSON_S.read_text())["perdidos"]})
saem, entram = sorted(set(pv) - set(ps)), sorted(set(ps) - set(pv))
mudam = sorted(i for i in set(pv) & set(ps) if pv[i]["acerto"] != ps[i]["acerto"])
out("populacao vivo:", len(pv), "acertos", sum(v["acerto"] for v in pv.values()),
    "| staging:", len(ps), "acertos", sum(v["acerto"] for v in ps.values()))
out("saem da populacao:", [(i, pv[i]["classe"], "acerto" if pv[i]["acerto"] else "perda") for i in saem])
out("entram na populacao:", [(i, ps[i]["classe"], "acerto" if ps[i]["acerto"] else "perda") for i in entram])
out("ficam e mudam de resultado:", [(i, "acerto" if pv[i]["acerto"] else "perda", "->", "acerto" if ps[i]["acerto"] else "perda") for i in mudam])
SAIDA.write_text(json.dumps({"vivo": pv, "staging": ps, "saem": saem, "entram": entram, "mudam": mudam}, ensure_ascii=False, indent=1), encoding="utf-8")
