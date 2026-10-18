"""Juiz V0d: vivo x staging, atribuido por entrada. Leitura apenas.

Casa os negativos por `tool_use_id` (o `id` deles embute path/estrato, que o extrator novo muda) e diz, para
cada mudanca de numero, a causa: entrada REMOVIDA, texto MUDOU, ESTRATO mudou, ou nada mudou na entrada.
Uso: python3 -I juiz_lado_a_lado.py <staging_dir> <saida.json>
"""

import json
import sys
from collections import Counter
from pathlib import Path

SP = Path(__file__).resolve().parent
VIVO = Path.home() / ".claude/touring/vgp/corpus_v0d"
STAGING = Path(sys.argv[1])
SAIDA = Path(sys.argv[2])


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()]


def juiz(nome: str) -> dict:
    return json.loads((SP / f"juiz_v2_{nome}.json").read_text(encoding="utf-8"))


def negativos_por_tid(base: Path) -> dict[str, dict]:
    return {e["tool_use_id"]: e for e in ler(base / "entradas_inline_neg.jsonl")}


def texto(base: Path, e: dict) -> str:
    return (base / e["snap"]).read_text(encoding="utf-8")


def atribui_negativos(jv: dict, js: dict) -> dict:
    fv = {f["id"] for f in jv["falsos"]}
    fs = {f["id"] for f in js["falsos"]}
    nv, ns = negativos_por_tid(VIVO), negativos_por_tid(STAGING)
    tab: dict[str, Counter] = {}
    for tid, e in nv.items():
        grupo = "tmp" if e["origem"] == "tmp" else f"inline/{e['extrator']}"
        c = tab.setdefault(grupo, Counter())
        era_falso = e["id"] in fv
        if tid not in ns:
            c["removida e era falso deny" if era_falso else "removida e estava limpa"] += 1
            continue
        n = ns[tid]
        mudou = texto(VIVO, e).strip() != texto(STAGING, n).strip()
        virou = n["id"] in fs
        fluxo = {(True, True): "falso -> falso", (True, False): "falso -> limpa",
                 (False, True): "limpa -> falso", (False, False): "limpa -> limpa"}[(era_falso, virou)]
        c[f"{fluxo} ({'texto mudou' if mudou else 'texto igual'})"] += 1
        if e["extrator"] != n["extrator"]:
            c[f"mudou de estrato {e['extrator']} -> {n['extrator']}"] += 1
    atrib = {e["id"] for e in ler(VIVO / "entradas.jsonl") if e["rotulo"] == "negativo"}
    return {"por_grupo": {k: dict(v) for k, v in sorted(tab.items())},
            "controle_atribuido_igual": sorted(fv & atrib) == sorted(fs & atrib),
            "falsos_vivo": len(fv), "falsos_staging": len(fs)}


def atribui_positivos(jv: dict, js: dict) -> dict:
    pv = {e["id"]: e for e in ler(VIVO / "entradas.jsonl") if e["rotulo"] == "positivo" and e["origem"] == "inline"}
    ps = {e["id"] for e in ler(STAGING / "entradas.jsonl") if e["rotulo"] == "positivo" and e["origem"] == "inline"}
    removidas = [e for i, e in pv.items() if i not in ps]
    novas = [i for i in ps if i not in pv]
    return {"removidas": len(removidas), "removidas_que_reproduziam": sum(bool(e.get("reproduz")) for e in removidas),
            "novas": len(novas)}  # a populacao EXATA do recall esta em populacao_recall.py (funcoes do juiz)


TAG = sys.argv[3] if len(sys.argv) > 3 else "staging"  # juiz_v2_<TAG>.json
jv, js = juiz("vivo"), juiz(TAG)
linhas = [
    ("recall simbolo arquivo", jv["positivos_simbolo"]["arquivo"], js["positivos_simbolo"]["arquivo"]),
    ("recall simbolo inline", jv["positivos_simbolo"]["inline"], js["positivos_simbolo"]["inline"]),
    ("negativos arquivo/inline", [jv["negativos"]["arquivo"], jv["negativos"]["inline"]], [js["negativos"]["arquivo"], js["negativos"]["inline"]]),
    ("falsos deny (total)", len(jv["falsos"]), len(js["falsos"])),
    ("falsos deny por balde", dict(Counter(f["balde"] for f in jv["falsos"])), dict(Counter(f["balde"] for f in js["falsos"]))),
    ("avisos em negativos", jv["avisos_em_negativos"], js["avisos_em_negativos"]),
    ("p95 ms", round(jv["p95_ms"], 1), round(js["p95_ms"], 1)),
    ("chamadas", jv["chamadas"], js["chamadas"]),
    ("defeito de corpus (snap nao compila)", jv["defeito_de_corpus"], js["defeito_de_corpus"]),
    ("erros do juiz", len(jv["erros"]), len(js["erros"])),
    ("pendencias", jv["pendencias"], js["pendencias"]),
]
for nome, a, b in linhas:
    out(f"{nome:42s} VIVO {a}  |  STAGING {b}")
neg, pos = atribui_negativos(jv, js), atribui_positivos(jv, js)
out("--- positivos removidos:", pos)
out("--- negativos (falso deny) por grupo; controle dos 300 atribuido intocados:", neg["controle_atribuido_igual"])
for g, c in neg["por_grupo"].items():
    out(f"  {g}: {c}")
SAIDA.write_text(json.dumps({"tabela": linhas, "negativos": neg, "positivos": pos}, ensure_ascii=False, indent=1), encoding="utf-8")
