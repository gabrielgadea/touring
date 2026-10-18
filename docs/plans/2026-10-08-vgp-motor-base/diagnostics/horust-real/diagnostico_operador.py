"""HORUST-REAL — por que um operador descarta tanto? Refaz os primeiros `n` descartados de cada operador dado (do `resultados.jsonl`) e
mostra o que o rustc DISSE em cada um. Só diagnóstico: nada daqui entra no corpus. Dentro de um hw-slot (HW_SLOT_HELD).

Uso (dentro do slot): python3 -I -B diagnostico_operador.py <n> <operador> [<operador> ...]
"""

from __future__ import annotations

import collections
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construir_horust_real as cr  # noqa: E402
import construtor_horust as ch  # noqa: E402


def main() -> int:
    cr.exigir_slot()
    cr.worktree_limpo()
    n, operadores = int(sys.argv[1]), sys.argv[2:]
    alvo = []
    for op in operadores:
        alvo += [r for r in cr.ler_registros() if r["operador"] == op and r["motivo"]][:n]
    tentativas = []
    for r in alvo:
        tentativas.append(cr.tentativa_do_registro(r))
    meta = ch.metadados(cr.RAIZ)
    camadas = ch.mapa_camadas(meta, {(t.pkg, t.alvo) for t in tentativas})
    obs: dict = {}
    res = ch.executar(tentativas, cr.RAIZ, cr.TARGET, via_slot=False, camadas=camadas, observados_out=obs)
    cr.worktree_limpo()
    saida = []
    for t, r in zip(tentativas, alvo):
        d = obs.get(t.inj.id, [])
        saida.append({"operador": r["operador"], "motivo_antes": r["motivo"], "motivo_agora": res[t.inj.id], "rel": r["rel"], "linha": r["linha"],
                      "nome": f"{r['nome_orig']} -> {r['nome_novo']}", "esperado": r["codigo"], "observado": [f"{x.codigo}@{x.linha}: {x.msg[:100]}" for x in d][:3]})
    (HERE / "estado" / "diagnostico_operador.json").write_text(json.dumps(saida, ensure_ascii=False, indent=1))
    for s in saida:
        print(s["operador"], "|", s["motivo_agora"] or "VALIDO", "|", s["rel"].split("/src/")[-1], s["linha"], s["nome"], "| esperado", s["esperado"], "| viu", s["observado"])
    cont = collections.Counter((s["operador"], tuple(x.split("@")[0] for x in s["observado"])) for s in saida)
    print(dict(cont))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
