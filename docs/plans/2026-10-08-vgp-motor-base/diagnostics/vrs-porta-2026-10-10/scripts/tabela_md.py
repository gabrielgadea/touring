"""Tabela markdown cumulativa ANTES + variantes: recall por classe (a/b), falso deny e CP95 do juiz, avisos em negativos,
controle de 130 .rs e p95. Uso: touring run --file tabela_md.py --args '[SC, V0, V1, ...]' (sem rotulos = todos os medidos)."""

import json
import pathlib
import sys

SC = pathlib.Path(sys.argv[1])
ROTULOS = sys.argv[2:]
NOMES = {
    "ANTES": "a6 (original)",
    "V0": "porta estrita",
    "V1": "+(a) lib do pacote",
    "V2": "+(b) item associado",
    "V3": "+(c) preludio+locais",
    "V4": "+(d) atributos irmaos",
    "V5": "+(e) glob de modulo",
    "V6": "+(f) #[path] dir",
    "V7": "+(g) Unit::assoc",
    "V8": "+(h) pub use dep",
    "V9": "+(i) const/static",
    "V10": "+(j) inline aninhado",
    "V11": "+(l) glob em cadeia",
    "V12": "+(m) use Enum em fn",
    "V13": "+(o) macro;",
    "V14": "+(p) dinamico=nao_medido",
    "V15": "+(q) reexport de modulo",
    "V16": "+(r) reexport inline",
    "V17": "+(s) comentario em args",
    "V18": "+(t) fn sombreada",
    "V19": "+(v) import privado",
    "V20": "+(w) use autorreferente",
    "V21": "+(x) ausencia exige modulo enumeravel",
    "V22": "+(D) escopo por modulo inline",
    "V23": "+(z) bloco extern declara itens (FINAL)",
    "VB": "FINAL sem #[path] declarante",
}


def ler(nome):
    p = SC / nome
    return json.loads(p.read_text()) if p.is_file() else None


def recall(d):
    t = {}
    fd = [0, 0]
    avisos = 0
    for r in d.values():
        if "erro" in r:
            continue
        if r["rotulo"] == "positivo":
            c = r.get("classe") or "?"
            t.setdefault(c, [0, 0])
            t[c][1] += 1
            t[c][0] += any(f["nome"] == r["nome_esperado"] for f in r["findings"])
        else:
            fd[1] += 1
            fd[0] += any(f["severidade"] == "deny" for f in r["findings"])
            avisos += bool(r["findings"])
    return t, fd, avisos


def linha(rot):
    arq = "ids_antes_natural.json" if rot == "ANTES" else "var_%s_ids_natural.json" % rot
    d = ler(arq)
    if not d:
        return None
    t, fd, avisos = recall(d["ids"])
    g = lambda c: "%d/%d" % tuple(t.get(c, [0, 0]))
    j = ler("juiz_antes_natural.json" if rot == "ANTES" else "var_%s_juiz_natural.json" % rot)
    cp = "%.2f%%" % (100 * j["falso_deny_por_balde"]["arquivo"]["limite_superior_95"]) if j else "?"
    p95 = "%.0f" % j["p95_ms"] if j else "?"
    c = None
    cp_arq = "ctl_antes.txt" if rot == "ANTES" else "var_%s_controle130.txt" % rot
    pth = SC / cp_arq
    if pth.is_file():
        try:
            c = json.loads(pth.read_text())
        except Exception:
            c = None
    ctl = "%s/%s" % (c["arquivos_com_achado"], c["achados"]) if c else "?"
    return "| %s | %s | %s | %s | %s | %s | %d/%d (%s) | %d | %s | %s |" % (
        rot, NOMES.get(rot, ""), g("nome"), g("import"), g("atributo"), g("kwarg"), fd[0], fd[1], cp, avisos, ctl, p95,
    )


print("| rot | correcao | nome | import | atributo | kwarg | falso deny (CP95) | avisos em neg. | controle 130 (arq/achados) | p95 ms (debug) |")
print("|---|---|---|---|---|---|---|---|---|---|")
for r in ROTULOS or ["ANTES"] + ["V%d" % i for i in range(24)] + ["VB"]:
    ln = linha(r)
    if ln:
        print(ln)
