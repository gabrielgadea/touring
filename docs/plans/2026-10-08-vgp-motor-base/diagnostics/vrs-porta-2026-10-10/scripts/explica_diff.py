import collections
import json
import sys

antes = json.load(open(sys.argv[1]))["ids"]
depois = json.load(open(sys.argv[2]))["ids"]


def conj(r):
    return {(f["classe"], f["nome"], f["linha"]) for f in r["findings"]}


removidos = collections.Counter()
adicionados = collections.Counter()
ids_com_removido = set()
ids_com_adicionado = set()
verdict_trans = collections.Counter()
motivos = collections.Counter()
for i in sorted(set(antes) & set(depois)):
    a, d = antes[i], depois[i]
    ca, cd = conj(a), conj(d)
    verdict_trans[(a["verdict"], d["verdict"], ca == cd)] += 1
    for x in ca - cd:
        removidos[(x[0], x[1])] += 1
        ids_com_removido.add(i)
    for x in cd - ca:
        adicionados[(x[0], x[1])] += 1
        ids_com_adicionado.add(i)
    if d.get("motivo"):
        motivos[d["motivo"].split(":")[0] + " | " + d["motivo"].split(":", 1)[-1][:90]] += 1
print("transicoes (antes, depois, mesmos_achados):", dict(verdict_trans))
print(f"ids com achado removido: {len(ids_com_removido)}; com achado adicionado: {len(ids_com_adicionado)}")
print("ADICIONADOS (classe, nome):", dict(adicionados.most_common(30)))
print("REMOVIDOS por classe:", dict(collections.Counter(k[0] for k in removidos.elements())))
print("REMOVIDOS top:", [(k, v) for k, v in removidos.most_common(25)])
print("motivos nao_medido (depois), top:")
for k, v in motivos.most_common(12):
    print("  ", v, k)
