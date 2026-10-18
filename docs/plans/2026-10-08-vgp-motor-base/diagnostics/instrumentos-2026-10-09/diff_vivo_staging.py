"""Diff corpus VIVO x STAGING, por origem/classe. Leitura apenas.

Positivos inline: por `id` (derivado de session/file/first, independe do extrator).
Negativos (inline e tmp): por `tool_use_id` (o `id` deles embute o path/estrato, que o extrator novo muda).
Uso: python3 -I diff_vivo_staging.py <staging_dir> <saida.json>
"""

import json
import sys
from collections import Counter
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
sys.path.insert(0, str(SCRIPTS))
import corpus_v0d as c0
import corpus_v5_inline_neg as v5

VIVO = c0.VIVO
STAGING = Path(sys.argv[1])
SAIDA = Path(sys.argv[2])
assert STAGING.resolve() != VIVO.resolve(), "staging == vivo"


def out(*partes) -> None:
    print(" ".join(str(p) for p in partes))


def ler(arq: Path) -> list[dict]:
    return [json.loads(ln) for ln in arq.read_text(encoding="utf-8").splitlines() if ln.strip()]


def texto(base: Path, e: dict) -> str:
    return (base / e["snap"]).read_text(encoding="utf-8")


def resumo(t: str, n: int = 110) -> str:
    return f"{len(t)}c: " + t.strip()[:n].replace("\n", "\\n")


def positivos() -> dict:
    pv = {e["id"]: e for e in ler(VIVO / "entradas.jsonl") if e["rotulo"] == "positivo" and e["origem"] == "inline"}
    ps = {e["id"]: e for e in ler(STAGING / "entradas.jsonl") if e["rotulo"] == "positivo" and e["origem"] == "inline"}
    classes, motivos, mudou, reproz = Counter(), Counter(), [], Counter()
    for eid, e in pv.items():
        if eid not in ps:
            cmd = e.get("command_original") or e.get("command", "")
            _, motivo = c0.programa_inline_motivo(cmd, e.get("session", ""), e.get("ts", ""))
            classes["removida"] += 1
            motivos[motivo.split(":")[0] if motivo.startswith("ambíguo") else motivo] += 1
            reproz["removida e reproduzia" if e.get("reproduz") else "removida e nao reproduzia"] += 1
            continue
        antes, depois = texto(VIVO, e), texto(STAGING, ps[eid])
        if antes.strip() == depois.strip():
            classes["igual"] += 1
        else:
            classes["texto mudou"] += 1
            mudou.append({"onde": "positivo", "id": eid, "antes": antes, "depois": depois,
                          "cmd": (e.get("command_original") or e.get("command", ""))[:200]})
    novas = [eid for eid in ps if eid not in pv]
    classes["nova"] = len(novas)
    return {"vivo": len(pv), "staging": len(ps), "classes": dict(classes), "motivos": dict(motivos),
            "reproduz_vivo": sum(1 for e in pv.values() if e.get("reproduz")),
            "reproduz_staging": sum(1 for e in ps.values() if e.get("reproduz")),
            "removidas_reproz": dict(reproz), "mudou": mudou}


def negativos() -> dict:
    nv = {e["tool_use_id"]: e for e in ler(VIVO / "entradas_inline_neg.jsonl")}
    ns = {e["tool_use_id"]: e for e in ler(STAGING / "entradas_inline_neg.jsonl")}
    por_origem: dict[str, Counter] = {}
    motivos, mudou, trans, truncados = Counter(), [], Counter(), 0
    for tid, e in nv.items():
        chave = f"{e['origem']}/{e['extrator']}"
        c = por_origem.setdefault(chave, Counter())
        if tid not in ns:
            c["removida"] += 1
            estrato, alvo, prog = v5.resolve(e["command"])
            if len(e["command"]) >= 2000:
                truncados += 1
            motivos[str(alvo) if estrato is None else "programa vazio"] += 1
            continue
        n = ns[tid]
        antes, depois = texto(VIVO, e), texto(STAGING, n)
        if e["extrator"] != n["extrator"]:
            trans[f"{e['extrator']} -> {n['extrator']}"] += 1
        if antes.strip() == depois.strip():
            c["igual"] += 1
        else:
            c["texto mudou"] += 1
            mudou.append({"onde": f"negativo {chave}", "id": e["id"], "antes": antes, "depois": depois,
                          "cmd": e["command"][:200], "extrator_novo": n["extrator"]})
    novos = [t for t in ns if t not in nv]
    return {"vivo": len(nv), "staging": len(ns), "por_origem": {k: dict(v) for k, v in sorted(por_origem.items())},
            "motivos": dict(motivos), "transicoes_extrator": dict(trans), "novos": len(novos),
            "removidos_com_comando_cortado_em_2000c": truncados, "mudou": mudou}


def resto_intacto() -> bool:
    """As entradas que NAO sao inline positivas continuam byte a byte no staging."""
    def fora(base: Path) -> list[str]:
        return [ln for ln in (base / "entradas.jsonl").read_text(encoding="utf-8").splitlines()
                if not (json.loads(ln)["rotulo"] == "positivo" and json.loads(ln)["origem"] == "inline")]
    return fora(VIVO) == fora(STAGING)


r = {"positivos": positivos(), "negativos": negativos(), "resto_do_corpus_intacto": resto_intacto()}
SAIDA.write_text(json.dumps(r, ensure_ascii=False, indent=1), encoding="utf-8")
p, n = r["positivos"], r["negativos"]
out("POSITIVOS inline:", p["vivo"], "->", p["staging"], "| classes:", p["classes"])
out("  motivos das removidas:", p["motivos"], "| reproduz vivo/staging:", p["reproduz_vivo"], p["reproduz_staging"], p["removidas_reproz"])
out("NEGATIVOS:", n["vivo"], "->", n["staging"], "| novos:", n["novos"])
for k, v in n["por_origem"].items():
    out("  ", k, v)
out("  motivos das removidas:", n["motivos"], "| comando cortado em 2000c:", n["removidos_com_comando_cortado_em_2000c"])
out("  transicoes de estrato:", n["transicoes_extrator"])
out("resto do corpus intacto (byte a byte):", r["resto_do_corpus_intacto"])
out("exemplos 'texto mudou':", len(p["mudou"]), "positivos +", len(n["mudou"]), "negativos")
