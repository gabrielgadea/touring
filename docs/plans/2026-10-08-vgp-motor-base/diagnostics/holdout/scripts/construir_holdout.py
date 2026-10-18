"""Constroi ~/.claude/touring/vgp/corpus_holdout/ : negativos (e positivos, se houver) POSTERIORES ao corte.

Mesmo esquema e mesmo processo do corpus vivo — reaproveita corpus_v0d.construir / corpus_v5_inline_neg /
estado_no_instante_v0d, sem tocar nos tres arquivos: o recorte por tempo e feito por ajustes em tempo de execucao,
so neste processo. O corpus vivo e so leitura (manifesto de hashes conferido antes e depois).

  corte = maior `ts` em corpus_v0d/entradas.jsonl e entradas_inline_neg.jsonl, LIDO do arquivo, + 1 minuto
  ate   = o instante em que este script comecou (UTC), congelado no holdout_meta.json; os transcripts sao
          truncados em `ate` ANTES de decidir quais edicoes sao limpas, entao o corpus e reproduzivel.

Uso: nice -n 19 python3 -I construir_holdout.py [--jobs 2] [--saida DIR]
"""

import argparse
import contextlib
import hashlib
import io
import json
import sys
import time
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
VGP = Path.home() / ".claude/touring/vgp"
VIVO = VGP / "corpus_v0d"
SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
import corpus_v0d as c0
import corpus_v5_inline_neg as v5
import estado_no_instante_v0d as eni
import inventario_erros as inv


def log(*partes) -> None:
    print(time.strftime("%H:%M:%S"), *partes, flush=True)


def sha_tree(raiz: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(
            str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest()
        )
    return h.hexdigest()[:16]


def ler(arq: Path) -> list[dict]:
    return [
        json.loads(ln)
        for ln in arq.read_text(encoding="utf-8").splitlines()
        if ln.strip()
    ]


def iso_ms(t: datetime) -> str:
    return (
        t.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.")
        + f"{t.microsecond // 1000:03d}Z"
    )


def calcular_corte() -> tuple[str, dict]:
    """(corte, de onde veio): o maior ts dos dois arquivos do vivo + 1 minuto."""
    melhor = ("", "")
    for nome in ("entradas.jsonl", "entradas_inline_neg.jsonl"):
        for e in ler(VIVO / nome):
            if e.get("ts", "") > melhor[0]:
                melhor = (e["ts"], f"{nome}:{e.get('id')}")
    t = datetime.fromisoformat(melhor[0].replace("Z", "+00:00")) + timedelta(minutes=1)
    return iso_ms(t), {"maior_ts_do_vivo": melhor[0], "origem": melhor[1]}


def positivos_novos(saida: Path, corte: str, ate: str) -> dict:
    """Roda o V0a (inventario_erros) para o scratchpad — NUNCA para o inventario compartilhado — e filtra a janela."""
    bruto = SP / "inventario_holdout_bruto.jsonl"
    inv.OUT, inv.SINCE = (
        bruto,
        "2026-10-09",
    )  # desde o inicio do dia: o `edited` de cada transcript precisa do passado
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        inv.main()
    (SP / "inventario_holdout.log").write_text(buf.getvalue(), encoding="utf-8")
    linhas = [r for r in ler(bruto) if corte < r.get("first", "") <= ate]
    destino = saida / "erros_inventario_holdout.jsonl"
    destino.write_text(
        "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in linhas),
        encoding="utf-8",
    )
    c0.INVENTARIO = destino
    return {
        "inventario_bruto": len(ler(bruto)),
        "na_janela": len(linhas),
        "por_bucket": dict(Counter(r["bucket"] for r in linhas)),
    }


def ajustar_construtor(corte: str, ate: str) -> None:
    """Recorta por tempo SEM editar o construtor: transcripts truncados em `ate`; universo de negativos na janela."""
    varrer, universo = c0.varrer_transcripts, c0.universo_negativos

    def varrer_ate() -> dict:
        return {
            a: [r for r in rs if r.get("timestamp", "") <= ate]
            for a, rs in varrer().items()
        }

    def universo_janela(indice: dict) -> tuple[list[str], dict]:
        # a limpeza de uma edicao depende so dos registros do PROPRIO arquivo: testar so os arquivos com atividade
        # na janela da o mesmo veredito e poupa O(todas as edicoes). O `indice` completo continua valendo para o
        # conteudo e os overlays (que dependem de edicoes anteriores ao corte).
        sub = {
            a: rs
            for a, rs in indice.items()
            if any(corte < r.get("timestamp", "") <= ate for r in rs)
        }
        todos, cont = universo(sub)
        dentro = [k for k in todos if corte < k.rsplit("@", 1)[1] <= ate]
        return dentro, {
            **cont,
            "arquivos_no_indice": len(indice),
            "arquivos_com_atividade_na_janela": len(sub),
            "candidatos_de_toda_a_historia_desses_arquivos": len(todos),
            "candidatos": len(dentro),
            "corte": corte,
            "ate": ate,
        }

    c0.varrer_transcripts, c0.universo_negativos = varrer_ate, universo_janela


def inline_e_tmp(corte: str, ate: str) -> dict:
    """V5 na janela: exatamente 1 candidato que compila (o `monta` do v5), sem o mesmo tool_use_id duas vezes."""
    todos = v5.universo()
    vistos, janela, dup = set(), [], 0
    for r in sorted(todos, key=lambda r: (r["ts"], r["session"])):
        if not (corte < r["ts"] <= ate):
            continue
        if r["tool_use_id"] in vistos:
            dup += 1
            continue
        vistos.add(r["tool_use_id"])
        janela.append(r)
    dentro, tmp, fora = v5.monta(janela)
    escolhidos = v5._sorteia(dentro, tmp, v5.N_NEG)
    ruins = v5._grava(escolhidos)
    return {
        "universo_total": len(todos),
        "na_janela": len(janela),
        "duplicados_por_tool_use_id": dup,
        "dentro_inline": len(dentro),
        "dentro_tmp": len(tmp),
        "excluidos": dict(fora),
        "gravados": len(escolhidos),
        "por_estrato": dict(Counter(r["extrator"] for r in escolhidos)),
        "snaps_que_nao_compilam": ruins,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--saida", type=Path, default=VGP / "corpus_holdout")
    a = ap.parse_args()
    saida = a.saida
    assert saida.resolve() != VIVO.resolve(), "saida == corpus vivo"
    if saida.exists() and any(saida.iterdir()):
        raise SystemExit(
            f"{saida} ja existe e nao esta vazio — outro nome, nunca sobrescrever"
        )
    vivo_antes = sha_tree(VIVO)
    t0 = time.monotonic()
    corte, origem = calcular_corte()
    ate = iso_ms(datetime.now(timezone.utc))
    log("corte:", corte, origem, "| ate:", ate, "| vivo sha_tree antes:", vivo_antes)
    saida.mkdir(parents=True)
    c0.definir_saida(saida)
    v5.definir_saida(saida)
    assert c0.C == saida and v5.C == saida and c0.C != c0.VIVO
    meta = {
        "corte": corte,
        "corte_origem": origem,
        "ate": ate,
        "saida": str(saida),
        "vivo_sha_tree_antes": vivo_antes,
        "parametros": {
            "SEED": c0.SEED,
            "N_NEG": c0.N_NEG,
            "CAP": c0.CAP,
            "v5_SEED": v5.SEED,
            "v5_N_NEG": v5.N_NEG,
            "v5_N_TMP": v5.N_TMP,
            "jobs": a.jobs,
        },
    }
    meta["positivos_v0a"] = positivos_novos(saida, corte, ate)
    log("positivos (V0a na janela):", meta["positivos_v0a"])
    ajustar_construtor(corte, ate)
    rel = c0.construir(a.jobs, c0.CAP)
    meta["v0d"] = {k: v for k, v in rel.items() if k != "nao_reproduzem"}
    log("V0d construir ok:", json.dumps(meta["v0d"], ensure_ascii=False)[:600])
    meta["v5"] = inline_e_tmp(corte, ate)
    log("V5 ok:", meta["v5"])
    meta["estado_no_instante"] = eni.construir(saida)
    log("estado_no_instante ok:", meta["estado_no_instante"])
    meta["vivo_sha_tree_depois"] = sha_tree(VIVO)
    meta["vivo_intocado"] = meta["vivo_sha_tree_depois"] == vivo_antes
    meta["wall_s"] = round(time.monotonic() - t0, 1)
    (saida / "holdout_meta.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    log("FIM | vivo intocado:", meta["vivo_intocado"], "| wall_s:", meta["wall_s"])
    return 0 if meta["vivo_intocado"] else 3


if __name__ == "__main__":
    raise SystemExit(main())
