"""HORUST — o outro lado da conta: quantos NEGATIVOS Rust materializaveis ha em cada janela, antes e depois da regra do crate completo.
O falso deny precisa de n grande (CP95 <= 1% com 0 falsos pede n >= 299); um held-out de positivos sem negativos suficientes nao mede o
que o juiz exige. Mesmo gerador (`candidatos_negativos` + `materializar_negativo`), sem sortear: o POOL, nao a amostra.

Janelas: A = ts <= 2026-10-09T20:40Z (universo atual) | N = (A, 2026-10-10T00:00Z] (o novo). Cada uma com e sem as sessoes do plano VGP.
Uso (nativo): python3 -I -B negativos_janela.py <sessoes_vgp.json> <saida.json>
"""

import collections
import json
import os
import sys
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
sys.path.insert(0, str(B / "diagnostics/horust-2026-10-10"))
import corpus_rust_natural as crn  # noqa: E402
import estado_rust_no_instante as ers  # noqa: E402
import medir_horust as mh  # noqa: E402
import reconhecedor_cargo_estendido as rec_est  # noqa: E402
import replay  # noqa: E402


def main() -> int:
    if os.environ.get("HORUST_ESTENDIDO") == "1":
        rec_est.instalar()
    vgp = set(json.loads(Path(sys.argv[1]).read_text())["vgp"])
    dev_neg_tids = {json.loads(x)["tool_use_id"] for x in (mh.VGP_DIR / "corpus_rust_natural/entradas.jsonl").read_text().splitlines() if x.strip() and json.loads(x)["rotulo"] == "negativo"}
    mh.memoizar_git()
    uni = crn.Universo(mh.CORTE_C)
    for proj in crn.PROJETOS:
        uni.varrer(proj, replay.PROJECTS / f"-home-gabrielgadea-projects-{proj}")
    uni.fechar()
    ctx = crn.Ctx(crn.Config((), Path("/nonexistent"), (Path.home() / "projects",), corte=mh.CORTE_C), uni, crn.Reconstrutor(uni.idx, uni.erros))
    por_estrato, baratos = crn.candidatos_negativos(ctx)
    todos = [c for lista in por_estrato.values() for c in lista]
    janelas = {
        "A_todas": lambda c: c.edicao.ts <= mh.CORTE_A,
        "A_sem_VGP": lambda c: c.edicao.ts <= mh.CORTE_A and c.edicao.sid not in vgp,
        "N_todas (novo)": lambda c: mh.CORTE_A < c.edicao.ts <= mh.CORTE_B,
        "N_sem_VGP (novo)": lambda c: mh.CORTE_A < c.edicao.ts <= mh.CORTE_B and c.edicao.sid not in vgp,
        "FORA_DO_DEV_sem_VGP (qualquer data)": lambda c: c.edicao.sid not in vgp and c.edicao.tid not in dev_neg_tids,
        "FORA_DO_DEV_todas (qualquer data)": lambda c: c.edicao.tid not in dev_neg_tids,
    }
    saida: dict = {"candidatos_baratos_total": len(todos), "recusados_nos_filtros_baratos": dict(baratos.most_common())}
    cache: dict[str, tuple] = {}
    for nome, filtro in janelas.items():
        sel = [c for c in todos if filtro(c)]
        materializados, recusas, incompletos, motivos = 0, collections.Counter(), collections.Counter(), collections.Counter()
        for c in sel:
            if c.edicao.tid not in cache:
                item, motivo = crn.materializar_negativo(c, ctx)
                if item is None:
                    cache[c.edicao.tid] = (None, motivo, None)
                else:
                    ts = c.cargo.ts
                    crate = c.crate
                    est = ers.estado_do_crate(Path(crate.dir), ers.ProvedorReconstrutor(ctx.rec, ts), proprio=c.edicao.caminho,
                                              extras_raiz=ers.raizes_do_manifesto(Path(crate.dir)))
                    cache[c.edicao.tid] = (item, "", ers.motivo_incompleto(est))
            item, motivo, estado = cache[c.edicao.tid]
            if item is None:
                recusas[motivo] += 1
                continue
            materializados += 1
            if not estado[0]:
                incompletos[c.edicao.proj] += 1
                motivos[estado[1]] += 1
        saida[nome] = {"candidatos_baratos": len(sel), "materializaveis_pelo_gerador_atual": materializados,
                       "com_crate_completo (usaveis)": materializados - sum(incompletos.values()), "incompletos_por_projeto": dict(incompletos),
                       "incompletos_por_motivo": dict(motivos.most_common()), "recusados_na_materializacao": dict(recusas.most_common()),
                       "por_projeto (usaveis)": dict(collections.Counter(c.edicao.proj for c in sel if cache[c.edicao.tid][0] is not None and cache[c.edicao.tid][2][0]))}
        print(nome, saida[nome]["candidatos_baratos"], materializados, saida[nome]["com_crate_completo (usaveis)"], flush=True)
    Path(sys.argv[2]).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
