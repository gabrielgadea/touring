"""HORUST — fontes ALEM de analise/touring: quantos positivos Rust os OUTROS diretorios de transcript dariam (so contagem; o
gerador atual so le analise e touring). Mesmo `construir_positivos` e mesmo funil (crate no instante + classe A). So leitura.

Uso (nativo): python3 -I -B outros_projetos.py <sessoes_vgp.json> <saida.json>
"""

import collections
import json
import sys
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
sys.path.insert(0, str(B / "diagnostics/horust-2026-10-10"))
import corpus_rust_natural as crn  # noqa: E402
import medir_horust as mh  # noqa: E402
import replay  # noqa: E402

DIRS = ("kazuba-cargo", "touring-wt-q50", "touring-wt-wiring", "konverter", "claude-trading", "2026-04-layerzero", "transferegov-pipeline")


def main() -> int:
    vgp = set(json.loads(Path(sys.argv[1]).read_text())["vgp"])
    mh.memoizar_git()
    saida = {}
    for nome in DIRS:
        raiz = replay.PROJECTS / f"-home-gabrielgadea-projects-{nome}"
        if not raiz.is_dir():
            saida[nome] = {"erro": "diretorio inexistente"}
            continue
        uni = crn.Universo(mh.CORTE_B)
        uni.varrer(nome, raiz)
        uni.fechar()
        cfg = crn.Config((), Path("/nonexistent"), (Path.home() / "projects",), corte=mh.CORTE_B)
        ctx = crn.Ctx(cfg, uni, crn.Reconstrutor(uni.idx, uni.erros))
        brutas = collections.Counter(o.diag.codigo for o in uni.ocorrencias)
        itens, motivos, stats = crn.construir_positivos(ctx)
        ws: dict = {}
        linhas = [mh.avaliar(i.entrada, i.snap, ctx, uni, ws) for i in itens]
        saida[nome] = {"transcripts": uni.stats.get(f"transcripts:{nome}"), "edicoes_rs": len(uni.edicoes), "chamadas_cargo": uni.stats.get("chamadas_cargo"),
                       "ocorrencias_brutas_por_codigo": dict(brutas), "positivos_do_gerador_atual": len(itens), "descartados": dict(motivos.most_common(6)),
                       "funil": mh.funil(linhas) if linhas else None, "sessoes_vgp_entre_eles": sum(1 for i in itens if i.entrada["session"] in vgp)}
        print(nome, saida[nome]["positivos_do_gerador_atual"], flush=True)
    Path(sys.argv[2]).write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
