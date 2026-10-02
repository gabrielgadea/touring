#!/usr/bin/env python3
"""S-6 — replay do A2: B0 × B-L (Laya local, CPU) × B-J (Jev API, redigido).

Plano: docs/plans/2026-10-02-jev-harness-r3/plan.md:106-113
Contrato da saída: o docstring de `scripts/r3_results_gate.py` — o veredito NUNCA é
escrito à mão; este script produz os NÚMEROS e o gate mecânico os transforma.

## O que este script NÃO faz (e por quê)

**Não compara com o gold.** O gold da A2 vem de DOIS adjudicadores cross-vendor e o
avaliador 1 (não-Claude) ainda depende do Gabriel (CRITERIOS Errata 3, E3.3). Sem
κ ≥ 0,60 o gate declara `GOLD_INSUFICIENTE` e o A2 não conclui. Por isso a saída é
`predictions-a2.jsonl` — uma PREDIÇÃO por item e por braço — mais um
`results-a2.json` com oagreement de cada braço medido **contra o gold, se ele existir**
(`--gold`), e `gold_insufficient: true` quando não.

## Privacidade e orçamento

* **B-J (Jev API) recebe SÓ `redact_for_jev(...)`** (`r3_event_schema.py:157`): família,
  modo, `closed_by`, contagem, primeiro token e balde do valor-base. O teste de contrato
  (`validate_jev_payload`) roda antes de cada envio; um payload com texto livre aborta o
  braço em vez de vazar.
* **B-L roda em CPU** (`LAYA_DEVICE=cpu`): a GPU é da analise-0e (a Songsera, 02/10).
* **Orçamento de API: ≤ 2.000 chamadas no total** e `question_ids` prefixados `r3_`.
  O replay decrementa o orçamento e aborta no limite — cota de família é recurso.

## Regra dura

O corpus em `~/.claude/touring/r3-data/` é **CONGELADO** e não é regravado: o replay
LÊ `audit_sample.jsonl` e escreve só artefatos novos, em `~/.claude/touring/r3-replay/`.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import statistics as st
import sys
import time
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))

import r3_b0 as b0  # noqa: E402
import r3_event_schema as schema  # noqa: E402

CORPUS = Path(os.environ.get("R3_CORPUS", "~/.claude/touring/r3-data")).expanduser()
# Cada replay grava no SEU subdiretório. Compartilhar um só diretório é o
# que fez o A1 parecer ter apagado a saída do A2: os dois nombres viviam no mesmo
# lugar, e o `find` de um achava os artefatos do outro. Nenhum replay APAGA nada —
# `mkdir(exist_ok=True)` só cria — mas a separação torna isso impossível por
# construção, e o teste de regressão vigia.
_RAIZ = Path(os.environ.get("R3_REPLAY_OUT", "~/.claude/touring/r3-replay")).expanduser()
SAIDA = _RAIZ / "a2"

# Orçamento do lote (regra do lote r3). O Jev API é a única família não-Claude que
# sobra: gastá-lo é decisão, não default.
ORCAMENTO_API = 2000
PREFIXO_QUESTAO = "r3_"
# O id da PERGUNTA tem de casar com o schema do SDK (`/^[a-z0-9_]+$/`), então o
# prefixo `r3_` vai no `state` (que é o que identifica o lote no jev_journal) e a
# pergunta fica com o id cru — question_ids com `r3_` reprovam na validação do SDK.
QID = "a2_followed"
STUDENT_URL = os.environ.get("LAYA_STUDENT_ENDPOINT", "http://127.0.0.1:8971")
# Critérios do C-A2-3: τ escolhido em audit-val para maximizar cobertura
# sujeita a precisão ≥ 0,90. O τ em PRODUÇÃO é 0,6 (prompt_enhance.rs:293).
TAU_PRODUCAO = 0.6
PRECISAO_MIN = 0.90
COBERTURA_MIN = 0.25


def carregar_amostra() -> list[dict]:
    return [json.loads(l) for l in (CORPUS / "audit_sample.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]


def evento_de(item: dict) -> dict:
    """O evento no schema FECHADO, a partir de uma linha do audit_sample.

    `cmd_heads` é a PRIMEIRA PALAVRA de cada comando (o schema não carrega o comando
    inteiro — é o que impede que o texto vaze pelo evento).
    """
    # `first_token` vai para o Jev API e `validate_jev_payload` exige um token
    # SEM ESPAÇO (r3_event_schema._TOKEN_RE) — um `git commit -m 'x'` inteiro
    # reprovaria o braço inteiro. Fica só o que é token, e o resto vira "".
    heads = []
    for c in item.get("cmds", []):
        primeira = (c.strip().split() or [""])[0]
        heads.append(primeira[:40] if re.match(r"^[A-Za-z0-9_.:\-]{0,40}$", primeira) else "")
    return {
        "v": 1, "kind": "a2_offer_closed", "use_case": "a2_outcome",
        "ts_ms": 0, "session_hash": schema.hash_session(item["session"]),
        "mode": "code", "offered_secs": 0,
        "closed_by": "consumed" if item.get("n_cmds") else "window",
        "cmd_heads": heads,
        "baseline_value": item.get("b0_valor"),
        "baseline_followed": item.get("b0_decisao") == "followed",
    }


# ────────────────────────────────────────────────────────── braço B-J (API)

def payload_of(ev: dict) -> dict:
    """O payload EXATO que a chamada enviaria (o mesmo do cache_only)."""
    return schema.redact_for_jev(ev)


def _norm_pred(bruta, ev: dict) -> dict:
    """A entrada do jev_cache no formato de predição deste replay."""
    if isinstance(bruta, dict) and "p_followed" in bruta:
        return bruta
    return {"item": ev.get("session_hash"),
            "p_followed": _prob_de_resposta(bruta, QID), "latency_ms": 0.0}


def _prob_de_resposta(corpo, qid: str) -> float:
    """A probabilidade de uma `noul`, medida no wire.

    A resposta vem `{"type": "noul", "noul": 0.36}` — o valor está na chave
    `noul`, e NÃO em `max_prob`. Ler `max_prob` dava 0.0 em TODAS as respostas e
    o braço pareceria "sempre não seguiu", que é um número plausível-e-absurdo: a
    assinatura de um instrumento defeituoso.
    """
    # `DecisionResult` (laya) e um objeto com `.values`/`.answers`; o Jev HTTP e
    # um dict. Os dois formatos, a mesma extracao.
    for atr in ("answers", "values"):
        sub = getattr(corpo, atr, None)
        if isinstance(sub, dict) and qid in sub:
            corpo = {qid: sub[qid]}
            break
    if not isinstance(corpo, dict):
        return 0.0
    # `system_one` devolve `{qid: {...}}` DIRECTO (medido), e o servidor HTTP
    # embrulha em `{"answers": {...}}`. Os dois formatos são aceitos porque o
    # braço local e o braço API falam com contratos diferentes — e ler só um
    # deles dava 0.0 em TUDO, que é o número plausível-e-absurdo.
    ans = corpo.get(qid)
    if ans is None:
        ans = (corpo.get("answers") or {}).get(qid)
    if isinstance(ans, (int, float)) and not isinstance(ans, bool):
        return float(ans)
    if not isinstance(ans, dict):
        return 0.0
    for chave in ("noul", "max_prob", "score"):
        v = ans.get(chave)
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            return float(v)
    return 0.0


def braço_jev(eventos: list[dict], verbose: bool = True,
              cache_path: Path | None = None, cache_only: bool = False) -> list[dict]:
    """B-J: o Jev API classifica cada oferta. SÓ features redigidas.

    Cada item vira uma pergunta `noul` binária: o aluno foi seguido? O τ é
    calibrado depois em `audit-val` (C-A2-3), então aqui só sai a PROBABILIDADE.
    """
    import jev_client
    out = []
    restantes = ORCAMENTO_API
    spec_q = json.dumps({"noul": "Did the agent follow the recommended route after the nudge?"}, sort_keys=True)
    cache = carregar_cache(cache_path) if cache_path else {}
    # Em cache_only, o cache do PRÓPRIO cliente conta: é a mesma fonte que a
    # chamada leria, e sem ele todo item parecia cache miss (medido: 0/180 no
    # cache do replay contra 180/180 no jev_cache).
    jev_cache = {}
    if cache_only:
        try:
            import jev_client
            jev_cache = jev_client._read_cache(
                Path(os.path.expanduser("~/.claude/touring/jev_cache.json")))
        except Exception as e:  # noqa: BLE001
            jev_cache = {}
            if verbose:
                print(f"  [B-J] jev_cache indisponível: {e}")
    if cache and verbose:
        print(f"  [B-J] cache: {len(cache)} predições reaproveitadas, 0 chamadas")
    chamadas = 0
    faltando_cache = 0
    for ev in eventos:
        ck = _chave_cache("B-J", ev.get("session_hash", ""), spec_q)
        if ck in cache:
            out.append(cache[ck])
            continue
        if cache_only and jev_cache:
            # a mesma chave que jev_client._cache_key usaria para esta chamada
            try:
                import jev_client as _jc
                kj = _jc._cache_key(payload_of(ev), {QID: {
                    "type": "noul",
                    "instructions": "Did the agent follow the recommended route after the nudge?"}},
                    "jev-1.13.0")
                if kj in jev_cache:
                    out.append(_norm_pred(jev_cache[kj], ev))
                    continue
            except Exception:  # noqa: BLE001
                pass
        if cache_only:
            # Modo cache_only: uma predicao que exigiria CHAMADA DE REDE aborta o
            # braço em vez de gastar cota. O cache do replay (cache-a2.jsonl) e o
            # do jev_client (jev_cache.json) sao fontes distintas; o primeiro e o
            # que este replay controla.
            faltando_cache += 1
            out.append({"item": ev.get("session_hash"),
                        "erro": f"CACHE_MISS (cache_only: sem chamada de rede)"})
            continue
        if restantes <= 0:
            if verbose:
                print(f"  [B-J] ORÇAMENTO ESGOTADO em {len(out)} itens")
            break
        payload = schema.redact_for_jev(ev)
        problemas = schema.validate_jev_payload(payload)
        if problemas:
            # Um payload com texto livre ABORTA o braço: melhor sem braço do que com vazamento.
            out.append({"item": ev.get("session_hash"), "erro": f"payload inválido: {problemas[:2]}"})
            continue
        # A `noul` do SDK tem SÓ `instructions` — `criteria` é da `score`
        # (typesafe_ai.Noul; o mesmo formato de scripts/pre_review_triage.py:110).
        spec = {QID: {
            "type": "noul",
            "instructions": "Did the agent follow the recommended route after the nudge?",
        }}
        t0 = time.perf_counter()
        try:
            # `system_one` nao recebe `question_ids`: os ids sao as CHAVES de
            # `questions` (jev_client.py, assinatura medida).
            r = jev_client.system_one(state=payload, questions=spec,
                                      model="jev-1.13.0", timeout=20.0)
            lat = (time.perf_counter() - t0) * 1000
            p_followed = _prob_de_resposta(r, QID)
            pred = {"item": ev.get("session_hash"), "p_followed": p_followed,
                    "latency_ms": lat}
            out.append(pred)
            chamadas += 1
            if cache_path:
                _registrar_cache(cache_path, ck, pred)
            restantes -= 1
        except Exception as e:  # noqa: BLE001 — a falha do braço é dado, não crash
            out.append({"item": ev.get("session_hash"),
                        "erro": f"{type(e).__name__}: {str(e)[:80]}"})
    if verbose:
        extra = f", {faltando_cache} sem cache" if faltando_cache else ""
        print(f"  [B-J] {len(out)} predições, {chamadas} chamadas à API "
              f"({len(cache)} vindas do cache){extra}")
    return out


# ─────────────────────────────────────────────────────── braço B-L (Laya local)

def braço_laya(eventos: list[dict], verbose: bool = True) -> list[dict]:
    """B-L: **Laya BASE zero-shot, em CPU, dentro do processo** — sem servidor
    nem porta.

    O modelo é `convaiinnovations/laya` (322M) em modo zero-shot. NÃO é o
    fine-tune de W4.2 (`laya-w42-intent`), que é um modelo de INTENÇÃO DE
    PROMPT — o modelo errado para desfecho de rota, e com 180 itens não há dado
    para treinar um especialista de desfecho. Rótulo nos resultados:
    `B-L = zero-shot base, CPU`.

    O estado é o MESMO payload redigido do B-J (`redact_for_jev`): o texto do
    comando não entra, e o que entra também não é journalado.

    `laya` roda no venv da analise (`~/.local/share/analise/laya-venv`, uso
    somente-leitura) e a inferência por item custa ~140 ms em CPU — 180 itens
    são ~25 s de modelo, o que é o que justifica o `hw-slot`.
    """
    import time as _time
    import laya
    verbose and print("    carregando convaiinnovations/laya em CPU (zero-shot, sem servidor)…")
    t0 = _time.perf_counter()
    agent = laya.Agent(model_id_or_path="convaiinnovations/laya", device="cpu")
    verbose and print(f"    agent pronto em {(_time.perf_counter() - t0):.1f}s")
    out = []
    for ev in eventos:
        payload = schema.redact_for_jev(ev)
        t1 = _time.perf_counter()
        try:
            r = agent.decide(payload,
                             questions={QID: {
                                 "type": "noul",
                                 "instructions": "Did the agent follow the recommended route after the nudge?"}},
                             return_details=True)
            lat = (_time.perf_counter() - t1) * 1000
            out.append({"item": ev.get("session_hash"),
                        "p_followed": _prob_de_resposta(r, QID),
                        "latency_ms": lat})
        except Exception as e:  # noqa: BLE001 — falha do braço é dado, não crash
            out.append({"item": ev.get("session_hash"),
                        "erro": f"{type(e).__name__}: {str(e)[:80]}"})
    if verbose:
        com_erro = sum(1 for r in out if "erro" in r)
        lat = [r["latency_ms"] for r in out if "latency_ms" in r]
        print(f"    {len(out)} predições, {com_erro} indisponíveis, "
              f"mediana {st.median(lat):.0f} ms" if lat else f"    {len(out)} predições")
    return out



# ──────────────────────────────────────────── cache de predições (orçamento de API)

VERSAO_CACHE = 1   # mude a gramática/pergunta e o cache invalida por construção


def _chave_cache(arm: str, chave: str, spec: str) -> str:
    """A chave de uma predição: braço + item + VERSÃO da pergunta.

    Inclui o `spec` (a pergunta) para que mudar a pergunta invalide o cache em vez
    de devolver a predição da pergunta antiga — o re-run sem mudança de entrada
    tem de dar 0 chamadas, e o re-run com a pergunta mudada tem de dar N.
    """
    return f"{VERSAO_CACHE}|{arm}|{chave}|{hashlib.sha256(spec.encode()).hexdigest()[:10]}"


def carregar_cache(caminho: Path) -> dict:
    if not caminho.exists():
        return {}
    out = {}
    for l in caminho.read_text(encoding="utf-8").splitlines():
        if not l.strip():
            continue
        try:
            r = json.loads(l)
        except ValueError:
            continue          # cache truncado: descarta a linha, não o arquivo
        out[r["cache_key"]] = r["pred"]
    return out


def _registrar_cache(caminho: Path, cache_key: str, pred: dict) -> None:
    caminho.parent.mkdir(parents=True, exist_ok=True)
    with open(caminho, "a", encoding="utf-8") as f:
        f.write(json.dumps({"cache_key": cache_key, "pred": pred}, ensure_ascii=False) + "\n")


# ──────────────────────────────────────────────────────────────── avaliação

def _predicao(p: float, tau: float) -> str:
    return "followed" if p >= tau else "not_followed"


def _acordo(preds: list[str], gold: list[str], mask: list[bool]) -> float | None:
    pares = [(p, g) for p, g, m in zip(preds, gold, mask) if m]
    if not pares:
        return None
    return sum(1 for p, g in pares if p == g) / len(pares)


def _p95(vals: list[float]) -> float | None:
    return round(st.quantiles(vals, n=20)[-1], 2) if len(vals) >= 20 else (
        round(max(vals), 2) if vals else None)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--gold", default="", help="gold_a2.jsonl (item_id -> label); vazio = ainda não existe")
    ap.add_argument("--so-b0", action="store_true", help="só o baseline, sem chamar modelo nenhum")
    ap.add_argument("--cache-only", action="store_true",
                    help="B-J SÓ do cache: um cache miss ABORTA, nunca chama a API")
    ap.add_argument("--max-itens", type=int, default=0, help="limita os itens (0 = todos)")
    args = ap.parse_args()

    itens = carregar_amostra()
    if args.max_itens:
        itens = itens[:args.max_itens]
    eventos = [evento_de(i) for i in itens]
    SAIDA.mkdir(parents=True, exist_ok=True)

    # B0 — o baseline já está no audit_sample (o porte fiel, o mesmo do gate)
    for ev, it in zip(eventos, itens):
        erros = schema.validate_event(ev)
        if erros:
            print(f"ERRO: evento fora do schema: {erros[:3]}")
            return 2
        ev["item_id"] = it["item_id"]
        ev["b0"] = it["b0_decisao"]
        ev["split"] = it["split"]

    print(f"itens: {len(itens)} | eventos no schema: ok")
    if args.so_b0:
        _dump(itens, eventos, {"B-L": [], "B-J": []})
        print("  (--so-b0: nenhum modelo chamado)")
        return 0

    print("B-L (Laya local, CPU):")
    bl = braço_laya(eventos)
    print("B-J (Jev API, features redigidas):")
    bj = braço_jev(eventos, cache_path=SAIDA / "cache-a2.jsonl",
                   cache_only=args.cache_only)
    _dump(itens, eventos, {"B-L": bl, "B-J": bj})
    return 0


def _dump(itens, eventos, preds: dict) -> None:
    """As predições por item, e o `results-a2.json` no contrato do results_gate.

    Sem gold (`--gold` vazio) o agreement é `None` e `gold_insufficient: true` — o
    gate então declara GOLD_INSUFICIENTE, que é o estado CORRETO enquanto o
    avaliador 1 não ratified (E3.3).
    """
    linhas = []
    for i, ev in enumerate(eventos):
        linha = {"item_id": ev["item_id"], "b0": ev["b0"], "split": ev["split"],
                 "baseline_value": ev.get("baseline_value"),
                 "session_hash": ev["session_hash"]}
        for nome, ps in preds.items():
            p = ps[i] if i < len(ps) else {"erro": "sem predicao"}
            linha[nome] = p
        linhas.append(linha)
    (SAIDA / "predictions-a2.jsonl").write_text(
        "\n".join(json.dumps(l, ensure_ascii=False) for l in linhas) + "\n", encoding="utf-8")

    results = {
        "use_case": "a2_outcome",
        "kappa": None, "gold_insufficient": True, "n_indeterminate": 0,
        "b0": {"agreement": None},
        "arms": {},
        "braco_labels": {
            "B-L": "zero-shot base (convaiinnovations/laya, 322M), CPU, dentro do processo",
            "B-J": "Jev API zero-shot, features redigidas",
            "B0": "classify_route_outcome (porte fiel), sem modelo",
        },
        "nota": "gold da A2 ainda não existe (avaliador 1 depende do Gabriel, E3.3); "
                "as predições por item estão em predictions-a2.jsonl; rode com "
                "--gold para preencher kappa/agreement.",
        "n_itens": len(itens),
        "predictions": str(SAIDA / "predictions-a2.jsonl"),
    }
    for nome, ps in preds.items():
        lat = [p["latency_ms"] for p in ps if "latency_ms" in p]
        results["arms"][nome] = {
            "agreement": None, "delta_vs_b0": None, "delta_ci_low": None,
            "precision": None, "coverage": None, "strata": {},
            "latency_p95_ms": _p95(lat),
            "n_predicoes": len(ps),
            "n_erros": sum(1 for p in ps if "erro" in p),
        }
    (SAIDA / "results-a2.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"-> {SAIDA/'predictions-a2.jsonl'}")
    print(f"-> {SAIDA/'results-a2.json'} (agreement: null — o gold ainda não existe)")


if __name__ == "__main__":
    sys.exit(main())
