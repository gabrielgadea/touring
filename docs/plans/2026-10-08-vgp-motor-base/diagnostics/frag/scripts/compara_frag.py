#!/usr/bin/env python3
"""Compara o binário BASE (espelho pristino) e o FRAG, por configuração e POR ID.

Entrada: <dir>/{base,frag}_{vivo,bak}_{v2,v3}.json (o JSON do juiz) e o `.porid.json` ao lado (juiz_por_id.py).
Saída (stdout + <dir>/comparacao.json): por configuração, as chaves do juiz que divergem e, por id, toda mudança de
veredito ou de severidade de achado, classificada:

  esperado_rebaixa  deny -> (warn|allow|unknown) num arquivo em /tmp ou no .cache do projeto (o que a ordem pede)
  INESPERADO        deny rebaixado fora do scratch (não é o item pedido)
  DENY_NOVO         não-deny -> deny (o que NÃO pode existir)
  outra             qualquer outra mudança (veredito ou achado), listada

Código de saída: 0 se não há DENY_NOVO e nenhum INESPERADO; 1 caso contrário (os esperados não reprovam).
"""

import json
import sys
from pathlib import Path

IGNORADAS = {"p95_ms", "falso_deny_por_balde", "pendencias", "chamadas"}


def scratch(path: str, raiz: str) -> bool:
    if path.startswith("/tmp/"):
        return True
    rel = path[len(raiz) :].lstrip("/") if path.startswith(raiz.rstrip("/") + "/") else path
    return rel.split("/", 1)[0] == ".cache"


def chaves(linha: dict) -> dict:
    return {(f["classe"], f["nome"]): f["severidade"] for f in linha["findings"]}


def classificar(i: str, b: dict, f: dict) -> tuple[str, str] | None:
    vb, vf = b["verdict"], f["verdict"]
    cb, cf = chaves(b), chaves(f)
    if vb == vf and cb == cf:
        return None
    if vb == "deny" and vf != "deny":
        return ("esperado_rebaixa" if scratch(b["path"], b["project_root"]) else "INESPERADO", f"{vb}->{vf}")
    if vb != "deny" and vf == "deny":
        return ("DENY_NOVO", f"{vb}->{vf}")
    novos_deny = [k for k, s in cf.items() if s == "deny" and cb.get(k) != "deny"]
    if novos_deny:
        return ("DENY_NOVO", f"achado novo deny {novos_deny[:3]}")
    caiu = [k for k, s in cb.items() if s == "deny" and cf.get(k) != "deny"]
    if caiu and scratch(b["path"], b["project_root"]):
        return ("esperado_rebaixa", f"achado deny->{[cf.get(k) for k in caiu][:3]} {caiu[:3]} (veredito segue {vf})")
    if caiu:
        return ("INESPERADO", f"achado deny caiu fora do scratch {caiu[:3]}")
    return ("outra", f"{vb}->{vf} achados {sorted(cb.items())[:3]} -> {sorted(cf.items())[:3]}")


def comparar(d: Path, corpus: str, modo: str, lado: str = "frag") -> dict:
    nome = f"{corpus}_{modo}"
    jb, jf = (json.loads((d / f"{l}_{nome}.json").read_text()) for l in ("base", lado))
    rb, rf = (json.loads((d / f"{l}_{nome}.porid.json").read_text()) for l in ("base", lado))
    difs = [k for k in sorted(set(jb) | set(jf)) if k not in IGNORADAS and jb.get(k) != jf.get(k)]
    por_id: list[dict] = []
    for i in sorted(set(rb) | set(rf)):
        if i not in rb or i not in rf:
            por_id.append({"id": i, "classe": "outra", "detalhe": "id só de um lado"})
            continue
        c = classificar(i, rb[i], rf[i])
        if c:
            por_id.append(
                {"id": i, "rotulo": rb[i]["rotulo"], "origem": rb[i]["origem"], "path": rb[i]["path"], "classe": c[0], "detalhe": c[1]}
            )
    return {"config": f"{nome} ({lado})", "pronto": [jb.get("pronto"), jf.get("pronto")], "chaves_divergentes": difs,
            "base": {k: jb.get(k) for k in difs}, "frag": {k: jf.get(k) for k in difs},
            "p95_ms": [jb.get("p95_ms"), jf.get("p95_ms")], "por_id": por_id,
            "erros": [len(jb.get("erros", [])), len(jf.get("erros", []))]}


def main(d: Path) -> int:
    ruim = 0
    todas = []
    for corpus in ("vivo", "bak"):
        for modo in ("v2", "v3"):
            if not (d / f"frag_{corpus}_{modo}.json").is_file():
                print(f"[{corpus}/{modo}] sem resultado FRAG")
                continue
            r = comparar(d, corpus, modo)
            todas.append(r)
            cont: dict[str, int] = {}
            for x in r["por_id"]:
                cont[x["classe"]] = cont.get(x["classe"], 0) + 1
            print(f"\n== {r['config']}: pronto base/frag={r['pronto']} erros={r['erros']} p95={r['p95_ms']}")
            print(f"   chaves do juiz que divergem: {r['chaves_divergentes'] or 'NENHUMA'}")
            for k in r["chaves_divergentes"]:
                print(f"     {k}: base={json.dumps(r['base'][k])[:200]} | frag={json.dumps(r['frag'][k])[:200]}")
            print(f"   ids que mudam: {len(r['por_id'])} {cont}")
            for x in r["por_id"]:
                print(f"     [{x['classe']}] {x['id']} ({x.get('origem')}, {x.get('rotulo')}) {x.get('path')}: {x['detalhe']}")
            ruim += cont.get("DENY_NOVO", 0) + cont.get("INESPERADO", 0)
            if (d / f"fragexec_{corpus}_{modo}.json").is_file():
                c = comparar(d, corpus, modo, "fragexec")
                todas.append(c)
                print(f"   CONTROLE (binario FRAG pedindo --contexto execucao) x base: chaves que divergem={c['chaves_divergentes'] or 'NENHUMA'}"
                      f" | ids que diferem={len(c['por_id'])} (esperado 0: execucao == o comportamento de antes)")
                for x in c["por_id"]:
                    print(f"     [CONTROLE-DIFERE] {x['id']} {x.get('path')}: {x['detalhe']}")
                ruim += len(c["por_id"]) + len(c["chaves_divergentes"])
    (d / "comparacao.json").write_text(json.dumps(todas, ensure_ascii=False, indent=1))
    print(f"\nDENY_NOVO + INESPERADO = {ruim}")
    return 0 if ruim == 0 else 1


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1])))
