#!/usr/bin/env python3
"""S-7 — replay do A1: B0 × B-D (regra) × B-L × B-J sobre os blocos de injeção.

Plano: docs/plans/2026-10-02-jev-harness-r3/plan.md:114-123
Contrato da saída: docstring de `scripts/r3_results_gate.py` (`results-a1.json`).

## O que cada braço é

* **B0** — nada é descartado (o status quo). A linha de base contra a qual os
  braços se medem.
* **B-D** — regra determinística: descarta bloco com **0 proposições** e não-EXEMPT,
  e duplicata exata na janela de 5 turnos. É a régua do C-A1-1
  (`bytes_saved_rule` ≥ 0,25 · `followed_recall_rule` = 1,0 · `exempt_retained` = 1,0).
* **B-L / B-J** — os mesmos braços do A2 (Laya base zero-shot em CPU; Jev API com
  features redigidas): aqui a decisão é **descartar o bloco ou não** (`a1_block`).

## O texto dos blocos

`a1_blocks.jsonl` guarda metadados e a 1ª linha (`head`, 120 chars) — o TEXTO COMPLETO
não está no dataset congelado, e por isso não pode ser. O replay o **reextrai dos
transcripts** com a MESMA taxonomia de `r3_extract_a1_blocks.py` (importada, não
reimplementada), para que a decisão do braço veja o bloco como o baseline o viu —
a lição da Errata 4.2/E4.5.

## O que NÃO é feito aqui

O **harm-check (C-A1-2)** é julgado por DOOS adjudicadores cross-vendor
(CRITERIOS §3: o pior dos dois decide), e um deles é o avaliador não-Claude que
ainda espera o Gabriel. Então `harm` fica `null` e o gate não fecha o braço B-D —
o pacote de adjudicação é uma entrega separada (`r3_adjudication_a1.py`).
"""

from __future__ import annotations

import argparse
import json
import os
import statistics as st
import sys
import time
from collections import defaultdict
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))

import r3_a1_kpi as kpi
import r3_event_schema as schema
import r3_extract_a1_blocks as ex
import r3_replay_common as common

CORPUS = Path(os.environ.get("R3_CORPUS", "~/.claude/touring/r3-data")).expanduser()
# Cada replay grava no SEU subdiretório. Compartilhar um só diretório é o
# que fez o A1 parecer ter apagado a saída do A2: os dois nombres viviam no mesmo
# lugar, e o `find` de um achava os artefatos do outro. Nenhum replay APAGA nada —
# `mkdir(exist_ok=True)` só cria — mas a separação torna isso impossível por
# construção, e o teste de regressão vigia.
_RAIZ = Path(
    os.environ.get("R3_REPLAY_OUT", "~/.claude/touring/r3-replay")
).expanduser()
SAIDA = _RAIZ / "a1"
QID = "a1_discard"
PREFIXO_QUESTAO = "r3_"
ORCAMENTO_API = 2000
# A1 também é owned pelo Touring (E1.1): `other` é do harness e nenhum braço o descarta.
FAMILIAS_TOURING = set(kpi.TOURING_FAMILIES)


def carregar_blocos() -> list[dict]:
    """Os blocos do dataset congelado, cada um com um `block_id` POSICIONAL.

     O id é a posição na lista do dataset — o dataset é congelado por hash, então a
     posição é ESTÁVEL entre execuções, e não depende de nenhuma contagem durante o
     replay. Isso mata de uma vez a classe de bug em que dois blocos do mesmo turno
    recebiam o mesmo texto (o `_ORDEM` global consumia a ordem numa passagem e a
     seguinte recebia a do vizinho).
    """
    p = CORPUS / "a1_blocks.jsonl"
    blocos = [
        json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()
    ]
    # `block_id` = (session, turno, posição DENTRO do turno) — a mesma chave que o
    # índice de transcripts usa, calculada sobre a MESMA lista na MESMA ordem. Um
    # id global por índice de arquivo não casaria: o índice reseta `seq` a cada
    # turno, e só 7 de 28.688 blocos batiam.
    seq: dict[tuple[str, int], int] = {}
    for b in blocos:
        k = (b["session"], b["turno"])
        b["block_id"] = f"{k[0]}:{k[1]}:{seq.get(k, 0)}"
        seq[k] = seq.get(k, 0) + 1
    return blocos


def _seq_de(b: dict) -> int:
    """A posição do bloco dentro do turno, lida do próprio `block_id`."""
    return int(b["block_id"].rsplit(":", 1)[1])


def texto_de(textos: dict, b: dict) -> str:
    """O texto do bloco `b`, casado pela SUA posição dentro do turno.

    O dataset é uma lista na ordem do transcript e o índice também — o `i`-ésimo
    bloco do turno no dataset é o `i`-ésimo no índice. Sem isso, um turno com dois
    blocos dava o MESMO texto aos dois, eles colapsavam num id só, e 262 blocos
    viravam 171 predições sem nenhum erro visível.
    """
    return textos.get((b["session"], b["turno"], _seq_de(b)), "")


def id_de(b: dict, texto: str = "") -> str:
    """A identidade do bloco — a `block_id` posicional da carga.

    A chave `(session, turno)` **não** identifica um bloco: 8.837 de 13.825
    chaves do dataset têm MAIS de um bloco no mesmo turno (medido em 02/10), e
    usar a chave fazia o veredito sobrescrever o anterior — 262 blocos viravam
    171 predições, com `n` errado e sem erro visível.
    """
    return b["block_id"]


def particao_de(blocos: list[dict]) -> dict[tuple[str, int], str]:
    """`(session, turno) -> partição` do split temporal 60/20/20 do S-4.

    Reusa `r3_freeze_dataset.dividir_dataset` para que o replay e o split usem a
    MESMA regra — dois cortes divergentes fariam o replay medir outra população.
    """
    import r3_freeze_dataset as fz

    fora = fz.dividir_dataset(blocos)
    mapa: dict[tuple[str, int], str] = {}
    for part, chaves in fora.items():
        for chave in chaves:
            sess, _, idx = chave.rpartition(":")
            mapa[(sess, int(idx))] = part
    return mapa


def indexar_texto() -> dict[tuple[str, int], str]:
    """`(session, turno) -> texto do bloco`, reextraído dos transcripts.

    O dataset congelado não guarda o texto (só o `head`); o braço precisa do bloco
    inteiro para decidir como o baseline o viu.
    """
    d = Path(ex.PROJ)
    # A CHAVE é a POSIÇÃO: um turno pode ter VÁRIOS blocos injetados (medido:
    # 8.837 de 13.825 chaves), e guardar só o primeiro (`setdefault`) dava o
    # mesmo texto — e o mesmo id — a blocos IRMÃOS, o que fazia 262 blocos
    # virarem 171 predições sem nenhum erro visível.
    textos: dict[tuple[str, int, int], str] = {}
    for tp in sorted(d.glob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True):
        turno = 0
        seq = 0
        try:
            content = tp.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        sess = tp.stem[:8]
        for line in content.splitlines():
            rec = ex._json(line)
            if rec is None:
                continue
            if rec.get("type") == "user":
                turno += 1
                seq = 0
                continue
            att = rec.get("attachment")
            if (
                not isinstance(att, dict)
                or att.get("type") not in ex.R3_ATTACHMENT_TYPES
            ):
                continue
            for text in ex._texts_do_attachment(att):
                textos[(sess, turno, seq)] = text
                seq += 1
    return textos


# ──────────────────────────────────────────── cache de predições (orçamento de API)

VERSAO_CACHE = 1  # mude a gramática/pergunta e o cache invalida por construção


def _chave_cache(arm: str, chave: str, spec: str) -> str:
    """A chave de uma predição (ver `r3_replay_common.chave_cache`), com a VERSAO_CACHE deste replay."""
    return common.chave_cache(VERSAO_CACHE, arm, chave, spec)


carregar_cache = common.carregar_cache
_registrar_cache = common.registrar_cache


# ─────────────────────────────────────────────────────────────── B-D (regra)


def _eh_exempt(bloco: dict, texto: str) -> bool:
    """EXEMPT = família `code-mode-deny` OU marcador da lista congelada.

    A lista vem do dataset (`exempt_markers.json`, hash no manifesto): mudou a
    lista, mudou o braço, e o `--check` do congelamento acusa.
    """
    if bloco.get("family") in ex.EXEMPT_FAMILIES:
        return True
    return any(m in texto for m in ex.EXEMPT_MARKERS)


def braço_regra(blocos: list[dict], textos: dict) -> dict:
    """B-D: descarta 0-proposição-não-EXEMPT e duplicata-exata-na-janela.

    Devolve o veredito POR BLOCO (a chave é `(session, turno)`) para que a
    retenção de EXEMPT e o followed-recall sejam auditáveis.
    """
    vistos: dict[str, list[int]] = defaultdict(list)
    veredito: dict[str, dict] = {}
    for b in blocos:
        chave = id_de(b, texto_de(textos, b))
        texto = texto_de(textos, b)
        exempt = _eh_exempt(b, texto)
        # duplicata exata: MESMO texto visto há ≤5 turnos (S-3)
        hist = vistos.get(texto) if texto else None
        dup = bool(hist) and any(
            abs(t - b["turno"]) <= ex.DUPLICATE_WINDOW_TURNS for t in hist
        )
        if texto:
            vistos[texto].append(b["turno"])
        zero_prop = b["propositions"] == 0
        descarta = (not exempt) and (dup or zero_prop)
        veredito[chave] = {
            "descarta": descarta,
            "exempt": exempt,
            "dup": dup,
            "zero_prop": zero_prop,
            **b,
        }
    return veredito


# ────────────────────────────────────────────────── B-L / B-J (modelos)


def _evento_bloco(b: dict, texto: str) -> dict:
    return {
        "v": 1,
        "kind": "a1_block",
        "use_case": "a1_relevance",
        "ts_ms": 0,
        "session_hash": schema.hash_session(b["session"]),
        "hook": b["attachment_type"],
        "family": b["family"],
        "bytes": b["bytes"],
    }


def braço_laya(
    blocos: list[dict],
    textos: dict,
    verbose: bool = True,
    cache_path: Path | None = None,
) -> dict:
    """B-L: Laya BASE zero-shot em CPU, dentro do processo. Zero-shot = sem τ calibrado."""
    import laya

    spec_q = "discard-block"
    cache = carregar_cache(cache_path) if cache_path else {}
    faltando = [b for b in blocos if _chave_cache("B-L", id_de(b), spec_q) not in cache]
    out = {}
    if not faltando:
        if verbose:
            print(f"    B-L: {len(blocos)} predições, TODAS do cache, 0 inferências")
        return {id_de(b): cache[_chave_cache("B-L", id_de(b), spec_q)] for b in blocos}
    agent = laya.Agent(model_id_or_path="convaiinnovations/laya", device="cpu")
    for b in faltando:
        ev = _evento_bloco(b, texto_de(textos, b))
        t0 = time.perf_counter()
        try:
            r = agent.decide(
                schema.redact_for_jev(ev),
                questions={
                    QID: {
                        "type": "noul",
                        "instructions": "Is this injected block useless to the agent? Answer yes only if discarding it changes nothing.",
                    }
                },
                return_details=True,
            )
            pred = {
                "p_discard": _prob(r, QID),
                "latency_ms": (time.perf_counter() - t0) * 1000,
            }
            out[id_de(b)] = pred
            if cache_path:
                _registrar_cache(
                    cache_path, _chave_cache("B-L", id_de(b), spec_q), pred
                )
        except Exception as e:  # noqa: BLE001 — falha do braço é dado, não crash
            out[id_de(b)] = {"erro": f"{type(e).__name__}: {str(e)[:70]}"}
    if verbose:
        lat = [v["latency_ms"] for v in out.values() if "latency_ms" in v]
        print(
            f"    B-L: {len(out)} predições, mediana {st.median(lat):.0f} ms"
            if lat
            else "    B-L: 0"
        )
    return out


def braço_jev(
    blocos: list[dict],
    textos: dict,
    verbose: bool = True,
    cache_path: Path | None = None,
) -> dict:
    """B-J: Jev API, SÓ features redigidas."""
    import jev_client

    out = {}
    restantes = ORCAMENTO_API
    chamadas = 0
    spec_q = "discard-block"
    cache = carregar_cache(cache_path) if cache_path else {}
    if cache and verbose:
        print(f"    B-J: cache {len(cache)} predições")
    for b in blocos:
        ck = _chave_cache("B-J", id_de(b), spec_q)
        if ck in cache:
            out[id_de(b)] = cache[ck]
            continue
        if restantes <= 0:
            break
        ev = _evento_bloco(b, texto_de(textos, b))
        payload = schema.redact_for_jev(ev)
        if schema.validate_jev_payload(payload):
            out[id_de(b)] = {"erro": "payload inválido"}
            continue
        t0 = time.perf_counter()
        try:
            r = jev_client.system_one(
                state=payload,
                questions={
                    QID: {
                        "type": "noul",
                        "instructions": "Is this injected block useless to the agent? Answer yes only if discarding it changes nothing.",
                    }
                },
                model="jev-1.13.0",
                timeout=20.0,
            )
            pred = {
                "p_discard": _prob(r, QID),
                "latency_ms": (time.perf_counter() - t0) * 1000,
            }
            out[id_de(b)] = pred
            if cache_path:
                _registrar_cache(
                    cache_path, _chave_cache("B-J", id_de(b), spec_q), pred
                )
            restantes -= 1
        except Exception as e:  # noqa: BLE001
            out[id_de(b)] = {"erro": f"{type(e).__name__}: {str(e)[:70]}"}
    if verbose:
        print(f"    B-J: {len(out)} predições, {chamadas} chamadas à API")
    return out


def _prob(corpo, qid: str) -> float:
    """A probabilidade, nos DOIS formatos de resposta (DecisionResult do laya e
    o dict do Jev). Ver a nota longa no `r3_replay_a2._prob_de_resposta`: ler o
    lugar errado dava 0.0 em tudo, um número plausível-e-absurdo."""
    corpo = common.corpo_da_resposta(corpo, qid)
    if corpo is None:
        return 0.0
    ans = corpo.get(qid) if qid in corpo else (corpo.get("answers") or {}).get(qid)
    return common.valor_da_resposta(ans)


# ──────────────────────────────────────────────────────────────── métricas


def metricas(blocos, part, veredito=None, preds=None, tau=0.6, textos_ref=None) -> dict:
    """Bytes do universo Touring, em `test`, e o que o braço economiza/retém.

    O denominador é o bytes Touring na partição `test` (E1.1) — `other` é do
    harness e nenhum gate do Touring pode descartá-lo.
    """
    universo = [
        b
        for b in blocos
        if b.get("is_touring_family") and part.get((b["session"], b["turno"])) == "test"
    ]
    total = sum(b["bytes"] for b in universo)
    if veredito is None or textos_ref is None:
        return {"bytes_totais": total, "n_blocos": len(universo)}

    def _v(b):
        return veredito[id_de(b)]

    mantidos = [b for b in universo if not _v(b)["descarta"]]
    exempt = [b for b in universo if _v(b)["exempt"]]
    exempt_descartados = [b for b in exempt if _v(b)["descarta"]]
    out = {
        "bytes_totais": total,
        "bytes_mantidos": sum(b["bytes"] for b in mantidos),
        "bytes_saved": round(1 - sum(b["bytes"] for b in mantidos) / total, 4)
        if total
        else 0.0,
        "n_blocos": len(universo),
        "n_exempt": len(exempt),
        "exempt_retained": round(1 - len(exempt_descartados) / len(exempt), 4)
        if exempt
        else 1.0,
    }
    if preds:
        acertos = tot = 0
        for b in universo:
            chave = id_de(b)
            p = preds.get(chave, {})
            if "p_discard" not in p:
                continue
            decisao_modelo = p["p_discard"] >= tau
            decisao_regra = veredito[chave]["descarta"]
            tot += 1
            acertos += 1 if decisao_modelo == decisao_regra else 0
        out["agreement_com_bd"] = round(acertos / tot, 4) if tot else None
        out["n_comparavel"] = tot
    return out


def denominadores(blocos: list[dict], part: dict, m_bd: dict) -> dict:
    """D-6 (b): o corte do braço sobre os DOIS denominadores, na partição `test`.

    E1.1 é a definição do dataset (as 6 famílias); a taxonomia S-4 conta todo bloco
    que a régua atribui ao Touring (`kpi.is_touring_by_taxonomy`). Os bytes cortados
    são os mesmos — o braço só decide sobre E1.1 —, o que muda é sobre quanto se divide:
    um bloco fora de E1.1 e dentro da taxonomia é um bloco que ninguém cortou.
    """
    test = [b for b in blocos if part.get((b["session"], b["turno"])) == "test"]
    e1 = m_bd["bytes_totais"]
    cortado = e1 - m_bd.get("bytes_mantidos", e1)
    s4 = sum(
        b["bytes"]
        for b in test
        if kpi.is_touring_by_taxonomy(b.get("family", kpi.OTHER))
    )
    return {
        "e1_1": {"bytes": e1, "bytes_saved": round(cortado / e1, 4) if e1 else 0.0},
        "taxonomia_s4": {
            "bytes": s4,
            "bytes_saved": round(cortado / s4, 4) if s4 else 0.0,
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--so-regra", action="store_true", help="só B0 e B-D, sem modelo")
    ap.add_argument("--max-blocos", type=int, default=0)
    args = ap.parse_args()

    blocos = carregar_blocos()
    part = particao_de(blocos)
    textos = indexar_texto()
    n_test = sum(1 for b in blocos if part.get((b["session"], b["turno"])) == "test")
    print(
        f"blocos: {len(blocos)} | transcripts indexados: {len(textos)} | em `test`: {n_test}"
    )
    faltando = sum(1 for b in blocos if not texto_de(textos, b))
    print(
        f"  sem texto reextraído: {faltando} ({100.0 * faltando / max(1, len(blocos)):.1f}%)"
    )

    universo = [b for b in blocos if b.get("is_touring_family")]
    # A partição `test` é o DENOMINADOR (E1.1 + o gate mede sobre ela): os braços
    # são pontuados só nos blocos de `test`. Filtrar por partição — e não por
    # posição na lista — é o que evita `n_comparavel = 0` com agreement `None`,
    # que o gate leria como "o braço não respondeu" em vez de "sem medida".
    test_blocks = [
        b for b in universo if part.get((b["session"], b["turno"])) == "test"
    ]
    veredito = braço_regra(universo, textos)
    m_bd = metricas(universo, part, veredito, textos_ref=textos)
    print("\nB-D (regra, partição test):")
    print(f"  bytes_saved      = {m_bd['bytes_saved']:.4f}  (piso C-A1-1 = 0,25)")
    print(f"  exempt_retained  = {m_bd['exempt_retained']:.4f}  (piso = 1,00)")
    print(
        f"  bytes/turno (universo Touring) = {m_bd['bytes_totais'] / max(1, n_test):.0f}"
    )
    # D-6 (b): o MESMO corte, declarado também sobre o denominador da taxonomia S-4.
    den = denominadores(blocos, part, m_bd)
    m_bd["denominadores"] = den
    print(
        f"  denominador E1.1 (6 famílias, dataset congelado) = {den['e1_1']['bytes']} B "
        f"→ bytes_saved {den['e1_1']['bytes_saved']:.4f}"
    )
    print(
        f"  denominador taxonomia S-4 (declarado, D-6)       = {den['taxonomia_s4']['bytes']} B "
        f"→ bytes_saved {den['taxonomia_s4']['bytes_saved']:.4f}"
    )

    arms = {"B-D": {**m_bd, "harm": None}}
    if not args.so_regra:
        amostra = test_blocks[: args.max_blocos] if args.max_blocos else test_blocks
        print(f"\n  amostra dos braços: {len(amostra)} blocos de `test`")
        print(f"\nB-L (Laya base zero-shot, CPU) em {len(amostra)} blocos:")
        bl = braço_laya(amostra, textos, cache_path=SAIDA / "cache-a1.jsonl")
        print(f"B-J (Jev API, redigido) em {len(amostra)} blocos:")
        bj = braço_jev(amostra, textos, cache_path=SAIDA / "cache-a1.jsonl")
        for nome, pr in (("B-L", bl), ("B-J", bj)):
            m = metricas(amostra, part, veredito, pr, textos_ref=textos)
            lat = [v["latency_ms"] for v in pr.values() if "latency_ms" in v]
            m["latency_p95_ms"] = (
                round(st.quantiles(lat, n=20)[-1], 2) if len(lat) >= 20 else None
            )
            m["n_predicoes"] = len(pr)
            m["n_erros"] = sum(1 for v in pr.values() if "erro" in v)
            arms[nome] = m
            print(
                f"  {nome}: agreement_com_bd={m['agreement_com_bd']} "
                f"(n={m['n_comparavel']}) bytes_saved={m['bytes_saved']:.4f}"
            )

    results = {
        "use_case": "a1_relevance",
        "arms": arms,
        "harm_gold_insufficient": True,
        "nota": "C-A1-2 (harm) é julgado por DOIS adjudicadores cross-vendor e um "
        "deles depende do Gabriel: harm fica null e o gate não fecha o B-D. "
        "O pacote de adjudicação é entregue à parte (r3_adjudication_a1.py).",
        "braco_labels": {
            "B0": "nada descartado (status quo)",
            "B-D": "regra determinística: 0 proposições não-EXEMPT + duplicata na janela de 5 turnos",
            "B-L": "Laya base zero-shot (convaiinnovations/laya, 322M), CPU",
            "B-J": "Jev API zero-shot, features redigidas",
        },
        "universe": "famílias do Touring (E1.1); `other` é do harness e ninguém o descarta",
        "split": "temporal 60/20/20 por sessão (mesma regra do S-4)",
    }
    SAIDA.mkdir(parents=True, exist_ok=True)
    (SAIDA / "results-a1.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    (SAIDA / "bd-verdict.jsonl").write_text(
        "\n".join(
            json.dumps({"block_id": k, **v}, ensure_ascii=False)
            for k, v in veredito.items()
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"\n-> {SAIDA / 'results-a1.json'}")
    print(f"-> {SAIDA / 'bd-verdict.jsonl'} (veredito da regra por bloco, auditável)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
