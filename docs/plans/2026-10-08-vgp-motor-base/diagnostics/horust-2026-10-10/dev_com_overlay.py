"""HORUST — prova COMPORTAMENTAL no DEV (nao e held-out, nao sela nada): o `corpus_rust_natural` com o crate inteiro sobreposto no
instante, rodado no binario congelado da vrs-porta. Responde: o overlay de crate faz o motor ver o estado que o rustc viu (os
positivos B/C passam a ser achados) sem criar falso deny nos negativos?

Gera um corpus NOVO (`<saida>/corpus`, mesmo esquema do `corpus_rust_natural`, snaps copiados, `overlays` do manifesto preenchidos
com o crate no instante, conteudo ENDERECADO por sha para nao duplicar) so com as entradas cujo crate e completo. Positivos da
classe A e entradas de crate incompleto ficam de fora e sao contados. Nao toca `corpus_rust_natural/`.

Modo `crate` (o pedido): so o crate da entrada. Modo `ws` (extra): tambem os crates IRMAOS do workspace que o snapshot cita
(`use touring_foundation::...`), que o motor tambem le; entrada com irmao incompleto fica de fora.

Uso (nativo): python3 -I -B dev_com_overlay.py <saida_dir> [crate|ws]
Depois: python3 -I -B vrs-porta-2026-10-10/scripts/medir_ids.py --touring <bin> --corpus <saida>/corpus --out <ids.json>
"""

import collections
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
sys.path.insert(0, str(B / "diagnostics/horust-2026-10-10"))
import corpus_rust_natural as crn  # noqa: E402
import estado_rust_no_instante as ers  # noqa: E402
import medir_horust as mh  # noqa: E402
import replay  # noqa: E402

DEV = Path.home() / ".claude/touring/vgp/corpus_rust_natural"


def main() -> int:
    saida = Path(sys.argv[1])
    modo = sys.argv[2] if len(sys.argv) > 2 else "crate"
    assert modo in ("crate", "ws")
    corpus = saida / "corpus"
    assert not corpus.exists(), f"{corpus} ja existe"
    (corpus / "snap").mkdir(parents=True)
    (corpus / "estado_no_instante/antes").mkdir(parents=True)
    (corpus / "estado_no_instante/modulos").mkdir(parents=True)
    mh.memoizar_git()
    uni = crn.Universo(mh.CORTE_B)
    for proj in crn.PROJETOS:
        uni.varrer(proj, replay.PROJECTS / f"-home-gabrielgadea-projects-{proj}")
    uni.fechar()
    ctx = crn.Ctx(crn.Config((), saida, (Path.home() / "projects",), corte=mh.CORTE_B), uni, crn.Reconstrutor(uni.idx, uni.erros))
    entradas = [json.loads(x) for x in (DEV / "entradas.jsonl").read_text().splitlines() if x.strip()]
    manifesto_dev = json.loads((DEV / "estado_no_instante/manifesto.json").read_text())
    ws: dict = {}
    mantidas, fora, manifesto = [], collections.Counter(), {}
    conteudos: dict[str, str] = {}
    for e in entradas:
        snap = (DEV / e["snap"]).read_text(encoding="utf-8")
        ts = e["ts"] if e["rotulo"] == "positivo" else uni.chamadas[e["cargo_tool_use_id"]].ts  # negativo: o estado em que o cargo compilou
        crate = crn.crate_de(e["path"], ctx.manifestos)
        if crate is None:
            fora["crate não encontrado"] += 1
            continue
        est = ers.estado_do_crate(Path(crate.dir), ers.ProvedorReconstrutor(ctx.rec, ts), proprio=e["path"], extras_raiz=ers.raizes_do_manifesto(Path(crate.dir)))
        completo, motivo = ers.motivo_incompleto(est)
        if not completo:
            fora[f"{e['rotulo']}: {motivo}"] += 1
            continue
        if e["rotulo"] == "positivo" and mh.APLICA_A.search(e["mensagem"]) and ers.define_no_escopo(snap, e["linha"], e["coluna"], e["nome_esperado"]):
            fora["positivo: classe A"] += 1
            continue
        modulos = ers.overlays_do_crate(est, e["path"])
        if modo == "ws":
            membros = mh.membros_do_workspace(Path(e["project_root"]), ws)
            irmaos = [n for n in sorted(membros) if n != crate.nome.replace("-", "_") and re.search(rf"(?<![\w]){re.escape(n)}::", ers.limpar(snap))]
            incompleto = None
            for n in irmaos:
                est_i = ers.estado_do_crate(Path(membros[n]), ers.ProvedorReconstrutor(ctx.rec, ts), extras_raiz=ers.raizes_do_manifesto(Path(membros[n])))
                if not ers.motivo_incompleto(est_i)[0]:
                    incompleto = f"{n}: {ers.motivo_incompleto(est_i)[1]}"
                    break
                modulos += ers.overlays_do_crate(est_i, None)
            if incompleto:
                fora[f"{e['rotulo']}: crate irmão incompleto"] += 1
                continue
        over = []
        for p, texto in dict(modulos).items():
            h = hashlib.sha256(texto.encode()).hexdigest()[:20]
            rel = f"estado_no_instante/modulos/{h}.rs"
            if h not in conteudos:
                (corpus / rel).write_text(texto, encoding="utf-8")
                conteudos[h] = rel
            over.append({"path": str(p), "file": rel, "sha256": hashlib.sha256(texto.encode()).hexdigest()})
        shutil.copy2(DEV / e["snap"], corpus / e["snap"])
        antigo = manifesto_dev[e["id"]]
        if antigo.get("disk_file"):
            shutil.copy2(DEV / antigo["disk_file"], corpus / antigo["disk_file"])
        manifesto[e["id"]] = {"disk_file": antigo.get("disk_file"), "overlays": over, "motivo": antigo.get("motivo"), "crate_arquivos": len(est.arquivos)}
        mantidas.append(e)
    (corpus / "entradas.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in mantidas), encoding="utf-8")
    (corpus / "estado_no_instante/manifesto.json").write_text(json.dumps(manifesto, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8")
    resumo = {"entradas_do_dev": len(entradas), "mantidas": len(mantidas), "positivos_mantidos": sum(e["rotulo"] == "positivo" for e in mantidas),
              "negativos_mantidos": sum(e["rotulo"] == "negativo" for e in mantidas), "fora_por_motivo": dict(fora.most_common()),
              "modulos_unicos_gravados": len(conteudos), "bytes": sum((corpus / r).stat().st_size for r in conteudos.values()),
              "maiores_overlays_por_entrada": sorted(len(m["overlays"]) for m in manifesto.values())[-5:]}
    (saida / "resumo.json").write_text(json.dumps(resumo, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(resumo, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
