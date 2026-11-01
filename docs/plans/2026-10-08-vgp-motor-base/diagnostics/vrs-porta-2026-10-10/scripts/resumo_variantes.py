import collections
import json
import pathlib
import sys

SC = pathlib.Path(sys.argv[1])
ROTULOS = sys.argv[2:] or ["V0", "V1", "V2", "V3", "V4"]


def ids(nome):
    return json.loads((SC / nome).read_text())["ids"]


def arq(rot, corpus):
    return "ids_antes_%s.json" % corpus if rot == "ANTES" else "var_%s_ids_%s.json" % (rot, corpus)


def conj(r):
    return {(f["classe"], f["nome"], f["linha"]) for f in r["findings"]}


def recall(d):
    t = collections.defaultdict(lambda: [0, 0])
    fd = [0, 0]
    com_aviso = 0
    for r in d.values():
        if "erro" in r:
            continue
        if r["rotulo"] == "positivo":
            c = r.get("classe") or "?"
            t[c][1] += 1
            t[c][0] += any(f["nome"] == r["nome_esperado"] for f in r["findings"])
        else:
            fd[1] += 1
            fd[0] += any(f["severidade"] == "deny" for f in r["findings"])
            com_aviso += bool(r["findings"])
    return {k: f"{a}/{b}" for k, (a, b) in sorted(t.items())}, f"{fd[0]}/{fd[1]}", com_aviso


def controle(rot):
    p = SC / ("ctl_antes.txt" if rot == "ANTES" else "var_%s_controle130.txt" % rot)
    if not p.is_file():
        return "?"
    o = json.loads(p.read_text())
    return (
        f"{o['arquivos_com_achado']}/{o['arquivos']} arq · {o['achados']} achados {o['achados_por_classe']} "
        f"· veredito {o['veredito']}"
    )


anterior = {}
for rot in ["ANTES"] + ROTULOS:
    linha = {"rot": rot}
    for corpus in ("sanidade", "natural"):
        try:
            d = ids(arq(rot, corpus))
        except FileNotFoundError:
            continue
        rc, fd, avisos = recall(d)
        linha[corpus] = {"recall": rc, "falso_deny": fd, "negativos_com_achado": avisos}
        if corpus in anterior:
            a = anterior[corpus]
            rem = add = ids_dif = vd = 0
            for i in set(a) & set(d):
                ca, cd = conj(a[i]), conj(d[i])
                rem += len(ca - cd)
                add += len(cd - ca)
                ids_dif += ca != cd
                vd += a[i]["verdict"] != d[i]["verdict"]
            linha[corpus]["vs_anterior"] = {
                "ids_com_achados_diferentes": ids_dif,
                "achados_removidos": rem,
                "achados_adicionados": add,
                "ids_com_veredito_diferente": vd,
            }
        anterior[corpus] = d
    try:
        j = json.loads((SC / ("juiz_antes_natural.json" if rot == "ANTES" else "var_%s_juiz_natural.json" % rot)).read_text())
        linha["juiz_natural"] = {
            "recall_arquivo": j["positivos_simbolo"]["arquivo"],
            "limite_sup_95": j["falso_deny_por_balde"]["arquivo"]["limite_superior_95"],
            "avisos_em_negativos": j["avisos_em_negativos"],
            "p95_ms": round(j["p95_ms"], 1),
        }
    except (FileNotFoundError, KeyError):
        pass
    linha["controle130"] = controle(rot)
    print(json.dumps(linha, ensure_ascii=False))
