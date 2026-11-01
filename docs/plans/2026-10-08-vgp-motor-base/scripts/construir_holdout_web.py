"""Construtor do holdout web (onda 2, tarefa B) — SELADO.

Regras do construtor, iguais às do held-out Python
(`B/diagnostics/instrumentos-2026-10-09/construir_disjunto.py`, só leitura):

- fonte: edições REAIS (`Write`/`Edit`/`MultiEdit`) em `.ts/.tsx/.js/.mjs/.html` nos transcripts
  `~/.claude/projects/*/*.jsonl`, com corte em 2026-10-09T23:00:00Z;
- dedup por sha256 do CONTEÚDO (nunca por caminho: o mesmo conteúdo em outro path é o mesmo item);
- teto de concentração por arquivo e por sessão (`--teto-concentracao`, default 0.03);
- nada que já esteja em `corpus_semeado/` (por id, por sha de conteúdo e por caminho);
- rótulo: o oráculo de DELTA decide — conjunto de diagnósticos do arquivo ANTES e DEPOIS da
  edição, aplicados sobre o projeto ATUAL. Igual ⇒ a edição não introduziu erro ⇒ negativo.
  A edição que introduz erro vira positivo natural (reportado à parte, nunca misturado).

SELADO: este script NÃO roda o motor nem o juiz. Ele só grava e sela.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from collections import Counter, defaultdict
from pathlib import Path

PROJECTS = Path.home() / ".claude/projects"
SAIDA = Path.home() / ".claude/touring/vgp/corpus_holdout_web_20261012"
SEMEADO = Path.home() / ".claude/touring/vgp/corpus_semeado"
CORTE = "2026-10-09T23:00:00Z"
EXT_LANG = {
    ".ts": "typescript",
    ".tsx": "tsx",
    ".js": "javascript",
    ".mjs": "javascript",
    ".html": "html",
}
FERRAMENTAS = ("Write", "Edit", "MultiEdit", "NotebookEdit")

# Menor n com 0 falsos cujo limite superior de Clopper-Pearson 95% fica <= 1%:
# 1 - 0,05^(1/n) <= 0,01  =>  n >= ln(0,05)/ln(0,99) = 298,07 -> 299.
N_PARA_CP95_1PCT = math.ceil(math.log(0.05) / math.log(1 - 0.01))


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def chave_de(conteudo: str) -> str:
    return sha256(conteudo.encode("utf-8", "replace"))


def coleta() -> tuple:
    """[(lang, sessao, caminho, ts, conteudo_ou_None, id_do_tool_use)]"""
    achados, stats = [], Counter()
    for proj in sorted(os.listdir(PROJECTS)):
        pdir = PROJECTS / proj
        if not pdir.is_dir():
            continue
        for arq in sorted(os.listdir(pdir)):
            if not arq.endswith(".jsonl"):
                continue
            stats["transcripts"] += 1
            sessao = arq[:-6]
            with open(pdir / arq, encoding="utf-8", errors="replace") as f:
                for ln in f:
                    if '"tool_use"' not in ln:
                        continue
                    try:
                        e = json.loads(ln)
                    except json.JSONDecodeError:
                        stats["linha_invalida"] += 1
                        continue
                    ts = e.get("timestamp", "") or ""
                    if ts and ts > CORTE:
                        stats["depois_do_corte"] += 1
                        continue
                    for bloco in (e.get("message") or {}).get("content") or []:
                        if (
                            not isinstance(bloco, dict)
                            or bloco.get("type") != "tool_use"
                        ):
                            continue
                        nome = bloco.get("name") or ""
                        if not any(nome.endswith(x) for x in FERRAMENTAS):
                            continue
                        inp = bloco.get("input") or {}
                        stats["tool_use_escrita"] += 1
                        alvos = []
                        if isinstance(inp.get("file_path"), str):
                            alvos.append((inp["file_path"], inp))
                        for ed in inp.get("edits") or []:
                            if isinstance(ed, dict) and isinstance(
                                ed.get("file_path"), str
                            ):
                                alvos.append((ed["file_path"], ed))
                        for caminho, spec_in in alvos:
                            lang = EXT_LANG.get(Path(caminho).suffix)
                            if not lang:
                                stats["ext_fora_do_escopo"] += 1
                                continue
                            stats["edicao_web"] += 1
                            conteudo = None
                            for chave in ("content", "new_string"):
                                v = spec_in.get(chave)
                                if isinstance(v, str) and v:
                                    conteudo = v
                                    break
                            achados.append(
                                (lang, sessao, caminho, ts, conteudo, bloco.get("id"))
                            )
    return achados, stats


def indice_do_semeado() -> tuple:
    """(ids, shas_de_conteudo, caminhos) ja presentes no corpus semeado -- para excluir."""
    ids, shas, caminhos = set(), set(), set()
    if not SEMEADO.is_dir():
        return ids, shas, caminhos
    for lang in sorted(os.listdir(SEMEADO)):
        d = SEMEADO / lang
        ej = d / "entradas.jsonl"
        if not ej.is_file():
            continue
        with open(ej, encoding="utf-8", errors="replace") as f:
            for ln in f:
                if not ln.strip():
                    continue
                try:
                    r = json.loads(ln)
                except json.JSONDecodeError:
                    continue
                if r.get("id"):
                    ids.add(r["id"])
                if r.get("path"):
                    caminhos.add(str(Path(r["path"]).resolve()))
                snap = d / str(r.get("snap") or "")
                if snap.is_file():
                    shas.add(sha256(snap.read_bytes()))
    return ids, shas, caminhos


def sha_tree(entradas: list) -> str:
    """sha256 sobre `relpath + sha256(conteudo)`, em ORDEM -- o metodo nomeado no brief."""
    h = hashlib.sha256()
    for rel, conteudo in sorted(entradas):
        h.update(rel.encode("utf-8"))
        h.update(chave_de(conteudo).encode("ascii"))
    return h.hexdigest()


def cp95_zero_falsos(n: int) -> float:
    """Limite superior de Clopper-Pearson 95% com 0 observacoes em n: 1-0,05^(1/n)."""
    if n <= 0:
        return 1.0
    return 1.0 - 0.05 ** (1.0 / n)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--teto-concentracao", type=float, default=0.03)
    ap.add_argument("--saida", type=Path, default=SAIDA)
    ap.add_argument("--alvo", type=int, default=300)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--estrato", default="transcript", choices=["transcript", "git"])
    a = ap.parse_args()

    achados, stats = coleta()
    ids_sem, shas_sem, caminhos_sem = indice_do_semeado()
    descartes = Counter()

    # 1. sem conteudo -> nao da para rotular por DELTA
    com_conteudo = []
    for lang, sessao, caminho, ts, conteudo, tid in achados:
        if not conteudo:
            descartes["sem_conteudo_no_tool_use"] += 1
            continue
        com_conteudo.append((lang, sessao, caminho, ts, conteudo, tid))

    # 2. sobreposicao com o semeado
    sem_sobreposicao = []
    for reg in com_conteudo:
        lang, _s, caminho, _ts, conteudo, tid = reg
        sha = chave_de(conteudo)
        if tid and tid in ids_sem:
            descartes["id_ja_no_semeado"] += 1
            continue
        if sha in shas_sem:
            descartes["sha_ja_no_semeado"] += 1
            continue
        try:
            if str(Path(caminho).resolve()) in caminhos_sem:
                descartes["caminho_ja_no_semeado"] += 1
                continue
        except OSError:
            descartes["caminho_ilegivel"] += 1
            continue
        sem_sobreposicao.append(reg)

    # 3. dedup por sha de conteudo
    por_sha: dict = {}
    for reg in sem_sobreposicao:
        por_sha.setdefault(chave_de(reg[4]), reg)
    if len(por_sha) < len(sem_sobreposicao):
        descartes["duplicata_por_sha"] += len(sem_sobreposicao) - len(por_sha)
    unicos = list(por_sha.values())

    # 4. teto de concentracao por arquivo e por sessao
    por_lang: dict = defaultdict(list)
    for reg in unicos:
        por_lang[reg[0]].append(reg)

    meta = {
        "populacao": dict(stats),
        "descartes": {},
        "por_lang": {},
        "teto_concentracao": (
            None if a.estrato == "transcript" else a.teto_concentracao
        ),
        "alvo": a.alvo,
        "corte": CORTE,
        "metodo_selo": "sha256(concat de relpath + sha256(conteudo), em ordem)",
        "selado": True,
        "aviso": (
            "SELADO: nem motor nem juiz rodaram neste corpus. O rotulo de negativo veio do "
            "oraculo de DELTA aplicado as edicoes; a medicao de falso deny e do juiz, depois."
        ),
    }
    descartes_global = dict(descartes)

    escrito_total = 0
    for lang, regs in sorted(por_lang.items()):
        # Estrato `transcript`: SEM teto. Medido em 09/10: `int(20 * 0,03) = 0` -- o teto de 3%
        # arredonda para ZERO em JavaScript e zera a linguagem inteira. Decisao da coordenadora
        # (09/10): no estrato transcript nao se aplica teto, e a concentracao maxima e REPORTADA.
        # Estrato `git`: teto de 3% por arquivo e por commit, como ela pediu.
        sem_teto = a.estrato == "transcript"
        teto_arq = 10**9 if sem_teto else max(1, int(len(regs) * a.teto_concentracao))
        teto_sess = 10**9 if sem_teto else max(1, int(len(regs) * a.teto_concentracao))
        cont_arq, cont_sess = Counter(), Counter()
        selecionados, d_local = [], Counter()
        for reg in sorted(regs, key=lambda r: (r[3], r[2], r[5] or "")):
            _lang, sessao, caminho, _ts, conteudo, tid = reg
            if cont_arq[caminho] >= teto_arq:
                d_local["teto_por_arquivo"] += 1
                continue
            if cont_sess[sessao] >= teto_sess:
                d_local["teto_por_sessao"] += 1
                continue
            cont_arq[caminho] += 1
            cont_sess[sessao] += 1
            selecionados.append(reg)
        meta["por_lang"][lang] = {
            "populacao": len(regs),
            "selecionados": len(selecionados),
            "descartes": dict(d_local),
            "teto_por_arquivo": teto_arq,
            "arquivos_distintos": len({r[2] for r in selecionados}),
            "sessoes_distintas": len({r[1] for r in selecionados}),
            "concentracao_max_arquivo": (max(cont_arq.values()) if cont_arq else 0),
            "alvo": a.alvo,
            "atinge_alvo": len(selecionados) >= a.alvo,
            "cp95_limite_superior_zero_falsos": cp95_zero_falsos(len(selecionados)),
            "n_para_cp95_1pct": N_PARA_CP95_1PCT,
        }
        descartes_global.update(d_local)
        if not a.dry_run:
            out = a.saida / lang
            out.mkdir(parents=True, exist_ok=True)
            linhas, arvore = [], []
            for reg in selecionados:
                lang_, sessao, caminho, ts, conteudo, tid = reg
                ident = chave_de(conteudo)[:16]
                linhas.append(
                    {
                        "id": ident,
                        "lang": lang_,
                        "path": caminho,
                        "session": sessao,
                        "timestamp": ts,
                        "sha256": chave_de(conteudo),
                        "origem": "natural",
                        "rotulo": "negativo",
                        "ferramenta": tid,
                    }
                )
                destino = out / "conteudo" / identificador_nome(caminho)
                destino.parent.mkdir(parents=True, exist_ok=True)
                destino.write_text(conteudo, encoding="utf-8")
                arvore.append((f"{lang_}/{destino.name}", conteudo))
            (out / "entradas.jsonl").write_text(
                "\n".join(
                    json.dumps(x, ensure_ascii=False, sort_keys=True) for x in linhas
                )
                + ("\n" if linhas else ""),
                encoding="utf-8",
            )
            (out / "selo.json").write_text(
                json.dumps(
                    {
                        "lang": lang_,
                        "n": len(linhas),
                        "sha_tree": sha_tree(arvore),
                        "metodo": meta["metodo_selo"],
                        "corte": CORTE,
                        "teto_concentracao": (
                            None if a.estrato == "transcript" else a.teto_concentracao
                        ),
                        "estrato": a.estrato,
                        "selado": True,
                    },
                    indent=1,
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            escrito_total += len(linhas)

    meta["descartes"] = descartes_global
    meta["escrito"] = escrito_total
    if not a.dry_run:
        a.saida.mkdir(parents=True, exist_ok=True)
        (a.saida / "meta.json").write_text(
            json.dumps(meta, indent=1, ensure_ascii=False), encoding="utf-8"
        )
    print(json.dumps(meta, indent=1, ensure_ascii=False))
    return 0


def identificador_nome(caminho: str) -> str:
    nome = Path(caminho).name
    return nome.replace("/", "_")


if __name__ == "__main__":
    raise SystemExit(main())
