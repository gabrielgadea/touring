"""HORUST — compara o MESMO binario (vrs-porta, 42830beb) no DEV SEM overlay (`ids_depois_natural.json`) e COM o crate (e os irmaos) no
instante: nos MESMOS ids (os que sobraram das regras novas), o recall por classe, os positivos recuperados, os negativos com achado
e o falso deny. So leitura.

Uso: python3 -I -B comparar_dev.py <ids_com_overlay.json> <saida.json>
"""

import collections
import json
import sys
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
ANTES = json.loads((B / "diagnostics/vrs-porta-2026-10-10/dados/ids_depois_natural.json").read_text())["ids"]
DEPOIS = json.loads(Path(sys.argv[1]).read_text())["ids"]


def achou(r: dict) -> bool:
    return any(f["nome"] == r["nome_esperado"] for f in r["findings"])


def main() -> int:
    ids = sorted(DEPOIS)
    pos = [i for i in ids if DEPOIS[i]["rotulo"] == "positivo"]
    neg = [i for i in ids if DEPOIS[i]["rotulo"] == "negativo"]
    por_classe: dict = {}
    for c in sorted({DEPOIS[i]["classe"] for i in pos}):
        grupo = [i for i in pos if DEPOIS[i]["classe"] == c]
        por_classe[c] = {"n": len(grupo), "achados_sem_overlay": sum(achou(ANTES[i]) for i in grupo), "achados_com_overlay": sum(achou(DEPOIS[i]) for i in grupo)}
    tot = {"n": len(pos), "achados_sem_overlay": sum(achou(ANTES[i]) for i in pos), "achados_com_overlay": sum(achou(DEPOIS[i]) for i in pos)}
    mudou = [{"id": i, "classe": DEPOIS[i]["classe"], "nome": DEPOIS[i]["nome_esperado"], "sem": achou(ANTES[i]), "com": achou(DEPOIS[i])} for i in pos if achou(ANTES[i]) != achou(DEPOIS[i])]
    perdidos_antes = [i for i in pos if not achou(ANTES[i])]
    saida = {
        "positivos": tot, "por_classe": por_classe,
        "perdidos_sem_overlay": len(perdidos_antes), "recuperados_com_overlay": sum(achou(DEPOIS[i]) for i in perdidos_antes),
        "ainda_perdidos": [{"id": i, "classe": DEPOIS[i]["classe"], "nome": DEPOIS[i]["nome_esperado"], "veredito": DEPOIS[i].get("verdict"), "outros": [f["nome"] for f in DEPOIS[i]["findings"]][:3]}
                           for i in perdidos_antes if not achou(DEPOIS[i])],
        "perdidos_depois_que_antes_achava": [m for m in mudou if m["sem"] and not m["com"]],
        "negativos": {"n": len(neg), "deny_sem_overlay": sum(ANTES[i].get("verdict") == "deny" for i in neg), "deny_com_overlay": sum(DEPOIS[i].get("verdict") == "deny" for i in neg),
                      "com_aviso_sem_overlay": sum(bool(ANTES[i]["findings"]) for i in neg), "com_aviso_com_overlay": sum(bool(DEPOIS[i]["findings"]) for i in neg),
                      "erros_do_motor": sum("erro" in DEPOIS[i] for i in neg)},
        "veredito_por_rotulo_com_overlay": {"positivos": dict(collections.Counter(DEPOIS[i].get("verdict") for i in pos)), "negativos": dict(collections.Counter(DEPOIS[i].get("verdict") for i in neg))},
        "erros_do_motor_nos_positivos": sum("erro" in DEPOIS[i] for i in pos),
    }
    Path(sys.argv[2]).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(saida, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
