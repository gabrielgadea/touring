"""Nao-regressao do juiz: o juiz ANTIGO e o NOVO (sem a flag nova) sobre o mesmo corpus e o mesmo binario.

Compara (1) o veredito do motor por id (`rodar_juiz_instrumentado.py`) — verdict + (classe, nome, linha, severidade) de cada
achado; `reais` e `sugestao` ficam de fora porque o MOTOR e nao-deterministico nelas (conjunto nao ordenado: o mesmo binario,
o mesmo id, 10 chamadas -> 10 ordens; medido em `motor_ordem_de_reais.py`) e sao reportados a parte, (2) o JSON do juiz campo a campo, ignorando os
campos NOVOS (`exigir_limite_superior`, `falso_deny_por_balde`) e o tempo (`p95_ms`; a pendencia de p95 e comparada
a parte, porque carrega o numero), (3) o stdout sem as linhas novas da tabela e sem o numero do p95, e (4) o exit code.
Tambem exige que o juiz novo TENHA os campos novos e o antigo NAO — senao seria comparar um arquivo com ele mesmo.

Uso: python3 -I comparar_juizes.py <orig.json> <novo.json> <orig.porid.json> <novo.porid.json> <orig.out> <novo.out> <saida.json>
"""

import json
import re
import sys
from pathlib import Path

orig_j, novo_j, orig_p, novo_p, orig_o, novo_o, saida = (Path(a) for a in sys.argv[1:8])
NOVOS = {"exigir_limite_superior", "falso_deny_por_balde"}
TEMPO = {"p95_ms"}


def ler(p: Path) -> dict:
    return json.loads(p.read_text(encoding="utf-8"))


def veredito(rec: dict) -> tuple:
    """O que decide: o verdict e, por achado, (classe, nome, linha, severidade). Erro do motor entra inteiro."""
    if "_erro" in rec:
        return ("erro", rec["_erro"])
    return (rec.get("verdict"), tuple((f.get("classe"), f.get("nome"), f.get("linha"), f.get("severidade")) for f in rec.get("findings", [])))


def texto_livre(rec: dict) -> tuple:
    """O que o motor escreve sem ordem fixa: `reais` (como conjunto) e `sugestao`."""
    return tuple((tuple(sorted(f.get("reais") or [])), f.get("sugestao")) for f in rec.get("findings", []))


def sem_p95(pendencias: list[str]) -> list[str]:
    return [p for p in pendencias if not p.startswith("p95 ")]


def limpa_stdout(texto: str) -> list[str]:
    """Sem as linhas da tabela nova, sem o rc impresso pelo shell e com o numero do p95 mascarado."""
    linhas = []
    for ln in texto.splitlines():
        if ln.startswith("  falso deny (") and "limite_superior_95" in ln:
            continue
        if ln.startswith(("rc=", "[instrumentado]", "hw-slot:")):
            continue
        linhas.append(re.sub(r"p95 [\d.]+ ms", "p95 <ms> ms", re.sub(r"PENDENTE: p95 [\d.]+ ms", "PENDENTE: p95 <ms> ms", ln)))
    return linhas


jo, jn = ler(orig_j), ler(novo_j)
po, pn = ler(orig_p), ler(novo_p)
resultado = {
    "campos_novos_so_no_novo": all(k in jn for k in NOVOS) and not any(k in jo for k in NOVOS),
    "ids_comparados_orig": len(po["por_id"]),
    "ids_comparados_novo": len(pn["por_id"]),
    "mesmo_conjunto_de_ids": set(po["por_id"]) == set(pn["por_id"]),
}
diverg = sorted(i for i in po["por_id"] if veredito(po["por_id"][i]) != veredito(pn["por_id"].get(i, {"_erro": "ausente"})))
resultado["ids_com_veredito_do_motor_diferente"] = diverg
resultado["ids_com_reais_ou_sugestao_diferentes (nao-determinismo do motor, nao do juiz)"] = sorted(
    i for i in po["por_id"] if i in pn["por_id"] and texto_livre(po["por_id"][i]) != texto_livre(pn["por_id"][i]))
resultado["ids_com_registro_bruto_diferente"] = sum(1 for i in po["por_id"] if po["por_id"][i] != pn["por_id"].get(i))
base_o = {k: v for k, v in jo.items() if k not in NOVOS | TEMPO | {"pendencias"}}
base_n = {k: v for k, v in jn.items() if k not in NOVOS | TEMPO | {"pendencias"}}
resultado["json_igual_fora_dos_campos_novos_e_do_tempo"] = base_o == base_n
resultado["campos_do_json_que_diferem"] = sorted(k for k in set(base_o) | set(base_n) if base_o.get(k) != base_n.get(k))
resultado["pendencias_iguais_fora_do_p95"] = sem_p95(jo["pendencias"]) == sem_p95(jn["pendencias"])
resultado["pronto"] = {"orig": jo["pronto"], "novo": jn["pronto"]}
resultado["pendencias_de_p95"] = {"orig": [p for p in jo["pendencias"] if p.startswith("p95 ")], "novo": [p for p in jn["pendencias"] if p.startswith("p95 ")]}
resultado["stdout_igual_fora_da_tabela_nova_e_do_p95"] = limpa_stdout(orig_o.read_text(encoding="utf-8")) == limpa_stdout(novo_o.read_text(encoding="utf-8"))
resultado["rc"] = {"orig": re.findall(r"^rc=(\d+)", orig_o.read_text(encoding="utf-8"), re.M)[-1:], "novo": re.findall(r"^rc=(\d+)", novo_o.read_text(encoding="utf-8"), re.M)[-1:]}
resultado["limite_superior_95_do_novo"] = {b: {"falsos": t["falsos"], "n": t["n"], "taxa": t["taxa"], "limite_superior_95": t["limite_superior_95"]} for b, t in jn["falso_deny_por_balde"].items()}
resultado["NAO_REGRESSAO"] = (
    resultado["campos_novos_so_no_novo"] and resultado["mesmo_conjunto_de_ids"] and not diverg
    and resultado["json_igual_fora_dos_campos_novos_e_do_tempo"] and resultado["pendencias_iguais_fora_do_p95"]
    and resultado["stdout_igual_fora_da_tabela_nova_e_do_p95"] and resultado["rc"]["orig"] == resultado["rc"]["novo"]
)
saida.write_text(json.dumps(resultado, ensure_ascii=False, indent=1), encoding="utf-8")
for k, v in resultado.items():
    print(f"{k}: {v}")
raise SystemExit(0 if resultado["NAO_REGRESSAO"] else 1)
