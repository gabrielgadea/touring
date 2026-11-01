"""Held-out DISJUNTO: negativos ANTERIORES ao corte que nenhum corpus contem.

Universo = o do vivo (edicoes .py limpas; execucoes inline/tmp bem-sucedidas com exatamente 1 candidato que compila),
com ts <= corte, MENOS tudo o que o vivo e o holdout temporal ja tem (ids, tool_use_id, sha do conteudo, comando).
Reaproveita corpus_v0d.construir_negativos, corpus_v5_inline_neg.universo/monta/_grava e estado_no_instante_v0d; o que
muda e so a ESCOLHA (semente 20261010, cotas por projeto/estrato, teto por arquivo e por sessao), injetada em tempo de
execucao — nenhum arquivo do bundle e editado e o vivo e so leitura.

  python3 -I construir_disjunto.py --diagnostico      # viabilidade; deixa o universo cacheado
  nice -n 19 python3 -I construir_disjunto.py         # constroi ~/.claude/touring/vgp/corpus_holdout_disjunto
"""

import argparse
import hashlib
import json
import random
import sys
import time
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
SP = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(SP))
import corpus_v0d as c0
import corpus_v5_inline_neg as v5
import estado_no_instante_v0d as eni
import exclusoes_disjunto as ex

VGP, VIVO, TEMPORAL = ex.VGP, ex.VIVO, ex.TEMPORAL
CACHE = SP / "disjunto_universo_filtrados.json"
ESTRATOS = ("c", "heredoc", "touring_run")
STATE: dict = {}


def log(*partes) -> None:
    print(time.strftime("%H:%M:%S"), *partes, flush=True)


def sha_tree(raiz: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in raiz.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(raiz)).encode() + hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


def corte_do_vivo() -> str:
    """Maior ts do vivo + 1 min, LIDO dos arquivos (a mesma conta do holdout temporal)."""
    maior = max(e.get("ts", "") for e in ex.entradas(VIVO))
    t = datetime.fromisoformat(maior.replace("Z", "+00:00")) + timedelta(minutes=1)
    return t.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


def cotas_proporcionais(mix: Counter, n: int) -> dict[str, int]:
    """Maiores restos: a mistura do VIVO escalada para n, somando exatamente n."""
    total = sum(mix.values())
    base = {k: v * n // total for k, v in mix.items()}
    restos = sorted(mix, key=lambda k: -(mix[k] * n % total))
    for k in restos[: n - sum(base.values())]:
        base[k] += 1
    return base


def projeto(arq: str, memo: dict = {}) -> str:  # noqa: B006 — memo proposital: o git_root custa um subprocesso por diretorio
    d = str(Path(arq).parent)
    if d not in memo:
        memo[d] = Path(str(c0._git_root(arq) or c0.vivos.project_root(Path(arq)) or "")).name
    return memo[d]


# ----------------------------------------------------------------------------------------------- inline / tmp


def pool_inline(corte: str, exc: dict[str, set[str]]) -> tuple[dict[str, list[dict]], dict]:
    """O universo inline/tmp anterior ao corte, sem o que o vivo e o temporal ja contem. {estrato: [registros]}."""
    todos = v5.universo()
    motivos: Counter = Counter()
    vistos, janela = set(), []
    for r in sorted(todos, key=lambda r: (r["ts"], r["session"], r["tool_use_id"])):
        if r["ts"] > corte:
            motivos["depois do corte"] += 1
        elif r["tool_use_id"] in exc["tool_use_ids"]:
            motivos["tool_use_id ja no vivo/temporal"] += 1
        elif r["tool_use_id"] in vistos:
            motivos["tool_use_id duplicado (pai/subagente)"] += 1
        else:
            vistos.add(r["tool_use_id"])
            janela.append(r)
    dentro, tmp, fora = v5.monta(janela)
    pool: dict[str, list[dict]] = {e: [] for e in (*ESTRATOS, "tmp_escrito_e_executado")}
    for r in dentro + tmp:
        if c0.sha(r["programa"]) in exc["shas"]:
            motivos["conteudo ja no vivo/temporal"] += 1
        elif r["command"].strip() in exc["comandos"] or r["command"][:2000].strip() in exc["comandos"]:
            motivos["comando ja no vivo/temporal"] += 1
        else:
            pool[r["extrator"]].append(r)
    return pool, {"universo_total": len(todos), "apos_filtros_de_id": len(janela), "excluidos_monta": dict(fora),
                  "excluidos_por_chave": dict(motivos), "pool_por_estrato": {k: len(v) for k, v in pool.items()}}


def escolher_inline(pool: dict[str, list[dict]], seed: int, quota: dict[str, int], teto: int, filtro=None) -> tuple[list[dict], dict]:
    """Por estrato: ordem embaralhada pela semente; aceita se a sessao ainda cabe sob o teto e o programa e inedito.
    `filtro(registro) -> (ok, motivo)` (opcional, HO13): o estado no instante completo; recusa nomeia o motivo."""
    rng = random.Random(seed)
    escolhidos, stats, shas = [], {}, set()
    por_sessao: Counter = Counter()
    for estrato in (*ESTRATOS, "tmp_escrito_e_executado"):
        cand = sorted(pool[estrato], key=lambda r: (r["ts"], r["tool_use_id"]))
        rng.shuffle(cand)
        aceitos, rej = 0, Counter()
        for r in cand:
            if aceitos >= quota[estrato]:
                break
            h = c0.sha(r["programa"])
            if por_sessao[r["session"]] >= teto:
                rej["teto por sessao"] += 1
            elif h in shas:
                rej["programa repetido na amostra"] += 1
            elif filtro is not None and not (res := filtro(r))[0]:
                rej[res[1]] += 1
            else:
                escolhidos.append(r)
                shas.add(h)
                por_sessao[r["session"]] += 1
                aceitos += 1
        stats[estrato] = {"quota": quota[estrato], "aceitos": aceitos, "pool": len(cand), "rejeitados": dict(rej),
                          "faltou": quota[estrato] - aceitos}
    return escolhidos, stats


# ----------------------------------------------------------------------------------------------- arquivo


def conferir_congelados(caminho: Path | None) -> dict:
    """`--congelados`: cada arquivo tem de ter o sha gravado ANTES de construir; se mudou, aborta (a regra nao muda sob o
    olho de quem constroi). Devolve o que foi conferido, para o meta."""
    if caminho is None:
        return {}
    esperado = json.loads(caminho.read_text(encoding="utf-8"))["arquivos"]
    visto = {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in esperado}
    mudou = {p: (esperado[p], visto[p]) for p in esperado if visto[p] != esperado[p]}
    if mudou:
        raise SystemExit(f"ARQUIVO CONGELADO MUDOU antes de construir: {mudou}")
    return {"arquivo_do_registro": str(caminho), "arquivos": visto}


def resumo_estado_completo(exigido: bool, rej_arquivo: dict, sel_inline: dict, estado: dict) -> dict:
    """As recusas por estado incompleto (candidatos tentados e recusados, por motivo e balde) e a conta do manifesto."""
    inline: Counter = Counter()
    for s in sel_inline.values():
        inline.update({k: v for k, v in s["rejeitados"].items() if k.startswith("estado incompleto")})
    return {"exigido": exigido,
            "recusados_por_estado_incompleto": {"arquivo": {k: v for k, v in rej_arquivo.items() if k.startswith("estado incompleto")}, "inline": dict(inline)},
            "entradas_incompletas_no_corpus": estado["entradas_incompletas"],
            "modulos_por_tipo": estado["modulos_por_tipo"],
            "entradas_com_modulo_que_a_regra_antiga_nao_sobrepunha": estado["entradas_com_modulo_que_a_regra_antiga_nao_sobrepunha"],
            "profundidade_2_medida_nao_exigida": {"entradas_com_lacuna": estado.get("entradas_com_importacao_de_profundidade_2_nao_coberta"),
                                                  "modulos_nao_cobertos": estado.get("modulos_de_profundidade_2_nao_cobertos")}}


def entrada_do_arquivo(arq: str, ts: str) -> dict:
    """O que `estado_no_instante` precisa de uma entrada de arquivo (a mesma raiz que o construtor do vivo grava)."""
    return {"project_root": str(c0._git_root(arq) or c0.vivos.project_root(Path(arq)) or ""), "path": arq, "ts": ts}


def ajustar_arquivo(corte: str, exc: dict[str, set[str]], quotas: dict[str, int], teto: int, exigir_completo: bool = False) -> None:
    """Injeta no construtor do vivo: universo cacheado (<= corte) e a ESCOLHA com exclusoes, cotas e teto por arquivo.
    `exigir_completo` (HO13): so entra o candidato cujo estado no instante e COMPLETO (todo modulo do projeto que ele
    importa reconstroi no instante); o resto fica de fora e contado por motivo."""
    varrer, universo = c0.varrer_transcripts, c0.universo_negativos

    def varrer_guarda() -> dict:
        STATE["indice"] = varrer()
        return STATE["indice"]

    def universo_cache(indice: dict) -> tuple[list[str], dict]:
        if CACHE.is_file():
            d = json.loads(CACHE.read_text(encoding="utf-8"))
        else:
            t0 = time.monotonic()
            todos, cont = universo(indice)
            d = {"filtrados": todos, "contagens": cont, "wall_s": round(time.monotonic() - t0, 1)}
            CACHE.write_text(json.dumps(d), encoding="utf-8")
        ate = [k for k in d["filtrados"] if k.rsplit("@", 1)[1] <= corte]
        STATE["filtrados"] = ate
        return ate, {**d["contagens"], "candidatos_de_toda_a_historia": len(d["filtrados"]), "candidatos_ate_o_corte": len(ate),
                     "corte": corte}

    def sorteia(ids: list[str], seed: int, n: int, reconhece) -> tuple[list[str], list[str], dict]:
        indice, rng = STATE["indice"], random.Random(seed)
        pares = {c0.ident("negativo", "", *k.rsplit("@", 1)): tuple(k.rsplit("@", 1)) for k in STATE["filtrados"]}
        orden = sorted(ids)
        rng.shuffle(orden)
        cont_proj, cont_arq, shas = Counter(), Counter(), set(exc["shas"])
        aceitos: list[str] = []
        rej: Counter = Counter()

        def tenta(eid: str, so_projeto: str | None) -> None:
            arq, ts = pares[eid]
            proj = projeto(arq)
            if eid in exc["ids"]:
                rej["id ja no vivo/temporal"] += 1
            elif so_projeto and proj != so_projeto:
                return
            elif not so_projeto and cont_proj[proj] >= quotas.get(proj, 0):
                rej["cota do projeto cheia (ou projeto sem cota)"] += 1
            elif cont_arq[arq] >= teto:
                rej["teto por arquivo"] += 1
            else:
                texto = c0._conteudo_depois(arq, ts, indice)
                if texto is None:
                    rej["conteudo nao reconstroi"] += 1
                    return
                try:
                    compile(texto, arq, "exec")
                except (SyntaxError, ValueError):
                    rej["nao compila"] += 1
                    return
                h = c0.sha(texto)
                if h in shas:
                    rej["conteudo ja visto (vivo/temporal/amostra)"] += 1
                elif exigir_completo and not (res := eni.estado_completo(entrada_do_arquivo(arq, ts), texto, indice))[0]:
                    rej[res[1]] += 1
                elif not reconhece(eid):
                    rej["reconhece() recusou"] += 1
                else:
                    shas.add(h)
                    aceitos.append(eid)
                    cont_proj[proj] += 1
                    cont_arq[arq] += 1

        for eid in orden:  # fase 1: as cotas por projeto (a mistura do vivo)
            if len(aceitos) >= n:
                break
            tenta(eid, None)
        if len(aceitos) < n:  # fase 2: o que faltou sai do projeto maior (analise), mesmo teto por arquivo
            maior = max(quotas, key=quotas.get)
            for eid in orden:
                if len(aceitos) >= n:
                    break
                if eid not in aceitos:
                    tenta(eid, maior)
        STATE["selecao_arquivo"] = {"aceitos": len(aceitos), "n": n, "por_projeto": dict(cont_proj), "cotas": quotas,
                                    "rejeicoes": dict(rej), "max_por_arquivo": max(cont_arq.values(), default=0),
                                    "arquivos_distintos": len(cont_arq), "candidatos_no_universo": len(ids)}
        return sorted(ids), aceitos, {"podados": sum(rej.values()), "rodadas": 1}

    c0.varrer_transcripts, c0.universo_negativos, c0._sorteia_negativos = varrer_guarda, universo_cache, sorteia


def main() -> int:
    global CACHE
    ap = argparse.ArgumentParser()
    ap.add_argument("--diagnostico", action="store_true")
    ap.add_argument("--saida", type=Path, default=VGP / "corpus_holdout_disjunto")
    ap.add_argument("--excluir-corpus", type=Path, action="append", default=[], metavar="DIR",
                    help="corpus a EXCLUIR alem do vivo e do temporal (held-out anterior); repetivel; so leitura")
    ap.add_argument("--cache", type=Path, default=CACHE,
                    help="onde guardar/ler o universo de candidatos de arquivo (default: ao lado do script)")
    ap.add_argument("--exigir-estado-completo", action="store_true",
                    help="HO13: so entra entrada cujo estado no instante e COMPLETO (todo modulo do projeto importado reconstroi); "
                         "o resto fica fora, contado por motivo")
    ap.add_argument("--congelados", type=Path, metavar="JSON",
                    help="HO13: {'arquivos': {caminho: sha256}} — aborta ANTES de construir se algum arquivo congelado mudou")
    ap.add_argument("--seed", type=int, default=20261010)
    ap.add_argument("--n-arquivo", type=int, default=600)
    ap.add_argument("--n-inline-por-estrato", type=int, default=200)
    ap.add_argument("--n-tmp", type=int, default=100)
    ap.add_argument("--teto", type=float, default=0.03)
    a = ap.parse_args()
    saida, CACHE = a.saida, a.cache
    extras = [d.resolve() for d in a.excluir_corpus]
    assert saida.resolve() not in (VIVO.resolve(), TEMPORAL.resolve(), *extras), "saida == vivo/temporal/corpus excluido"
    assert all(d.is_dir() and (d / "entradas.jsonl").is_file() for d in extras), f"--excluir-corpus sem entradas.jsonl: {extras}"
    t0, vivo_antes, temporal_antes = time.monotonic(), sha_tree(VIVO), sha_tree(TEMPORAL)
    extras_antes = {str(d): sha_tree(d) for d in extras}
    corte = corte_do_vivo()
    meta_t = json.loads((TEMPORAL / "holdout_meta.json").read_text(encoding="utf-8"))
    assert corte == meta_t["corte"], f"corte {corte} difere do holdout temporal {meta_t['corte']}"
    exc = ex.exclusoes(extras)
    teto_arq = int(a.teto * a.n_arquivo)
    teto_sess = int(a.teto * a.n_inline_por_estrato * len(ESTRATOS))
    mix = Counter(Path(e["project_root"]).name for e in ex.ler(VIVO / "entradas.jsonl") if e["rotulo"] == "negativo")
    quotas = cotas_proporcionais(mix, a.n_arquivo)
    log("corte:", corte, "| vivo sha_tree:", vivo_antes, "| exclusoes:", {k: len(v) for k, v in exc.items()})
    log("cotas de projeto (mistura do vivo escalada):", quotas, "| teto por arquivo:", teto_arq, "| teto por sessao:", teto_sess)
    pool, info_i = pool_inline(corte, exc)
    viab = {e: sum(min(n, teto_sess) for n in Counter(r["session"] for r in rs).values()) for e, rs in pool.items()}
    log("pool inline/tmp:", info_i["pool_por_estrato"], "| cabe sob o teto de sessao:", viab, "| excluidos por chave:", info_i["excluidos_por_chave"])
    congelados = conferir_congelados(a.congelados)
    ajustar_arquivo(corte, exc, quotas, teto_arq, a.exigir_estado_completo)
    if a.diagnostico:
        indice = c0.varrer_transcripts()
        filtrados, cont = c0.universo_negativos(indice)
        por_proj = Counter(projeto(k.rsplit("@", 1)[0]) for k in filtrados)
        arqs = Counter(k.rsplit("@", 1)[0] for k in filtrados)
        viab_a = {p: sum(min(n, teto_arq) for k2, n in arqs.items() if projeto(k2) == p) for p in por_proj}
        log("DIAGNOSTICO arquivo: candidatos ate o corte:", len(filtrados), "| por projeto:", dict(por_proj), "| cabe sob o teto de arquivo:", viab_a, "|", cont)
        return 0
    assert not (saida.exists() and any(saida.iterdir())), f"{saida} ja existe e nao esta vazio — outro nome"
    saida.mkdir(parents=True, exist_ok=True)
    c0.definir_saida(saida)
    v5.definir_saida(saida)
    assert c0.C == saida == v5.C and c0.C != c0.VIVO
    c0.SEED, c0.N_NEG = a.seed, a.n_arquivo
    meta = {"corte": corte, "saida": str(saida), "vivo_sha_tree_antes": vivo_antes, "temporal_sha_tree_antes": temporal_antes,
            "parametros": {"seed": a.seed, "n_arquivo": a.n_arquivo, "n_inline_por_estrato": a.n_inline_por_estrato, "n_tmp": a.n_tmp,
                           "teto": a.teto, "teto_por_arquivo": teto_arq, "teto_por_sessao": teto_sess, "cotas_projeto": quotas,
                           "mistura_do_vivo": dict(mix), "transcripts": "todos, sem truncar (so edicoes com ts <= corte entram)"},
            "exclusoes_tamanho": {k: len(v) for k, v in exc.items()}, "pool_inline": info_i}
    neg = c0.construir_negativos()
    (saida / "entradas.jsonl").write_text("".join(json.dumps(e, ensure_ascii=False) + "\n" for e in neg["entradas"]), encoding="utf-8")
    meta["arquivo"] = {k: v for k, v in neg.items() if k != "entradas"} | {"selecao": STATE["selecao_arquivo"]}
    log("ARQUIVO ok:", json.dumps(meta["arquivo"]["selecao"], ensure_ascii=False)[:700])
    quota_i = {e: a.n_inline_por_estrato for e in ESTRATOS} | {"tmp_escrito_e_executado": a.n_tmp}
    filtro = (lambda r: eni.estado_completo({"project_root": str(v5.ANALISE), "path": r["path"], "ts": r["ts"]}, r["programa"], STATE["indice"])) if a.exigir_estado_completo else None
    escolhidos, sel_i = escolher_inline(pool, a.seed, quota_i, teto_sess, filtro)
    meta["inline"] = {"selecao": sel_i, "gravados": len(escolhidos), "snaps_que_nao_compilam": v5._grava(escolhidos)}
    log("INLINE ok:", {e: (s["aceitos"], s["quota"]) for e, s in sel_i.items()})
    meta["estado_no_instante"] = eni.construir(saida)
    log("estado_no_instante ok:", meta["estado_no_instante"])
    meta["estado_completo"] = resumo_estado_completo(a.exigir_estado_completo, meta["arquivo"]["selecao"]["rejeicoes"], sel_i, meta["estado_no_instante"])
    log("estado completo:", json.dumps(meta["estado_completo"], ensure_ascii=False))
    if congelados:
        meta["congelados_antes_de_selar"] = congelados
    meta["vivo_sha_tree_depois"], meta["temporal_sha_tree_depois"] = sha_tree(VIVO), sha_tree(TEMPORAL)
    meta["vivo_e_temporal_intocados"] = meta["vivo_sha_tree_depois"] == vivo_antes and meta["temporal_sha_tree_depois"] == temporal_antes
    if extras:  # so aparece com a flag nova: sem ela o meta e o de antes, chave a chave
        meta["corpus_excluidos_alem_do_vivo_e_temporal"] = {"antes": extras_antes, "depois": {str(d): sha_tree(d) for d in extras}}
        meta["vivo_e_temporal_intocados"] &= meta["corpus_excluidos_alem_do_vivo_e_temporal"]["antes"] == meta["corpus_excluidos_alem_do_vivo_e_temporal"]["depois"]
    meta["wall_s"] = round(time.monotonic() - t0, 1)
    (saida / "holdout_meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
    log("FIM | vivo e temporal intocados:", meta["vivo_e_temporal_intocados"], "| wall_s:", meta["wall_s"])
    if a.exigir_estado_completo and meta["estado_completo"]["entradas_incompletas_no_corpus"]:
        log("ABORTO: entrada incompleta dentro do corpus — o filtro e o manifesto discordam")
        return 4
    return 0 if meta["vivo_e_temporal_intocados"] else 3


if __name__ == "__main__":
    raise SystemExit(main())
