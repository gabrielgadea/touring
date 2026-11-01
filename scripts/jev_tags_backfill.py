#!/usr/bin/env python3
"""jev_tags_backfill.py — S-5.2: prévia do backfill, em modo SOMENTE-LEITURA.

Este script **não grava nada**. Ele existe para responder a uma pergunta antes
de gastar a decisão do Gabriel: *quanto o Jev mudaria, e o que vale a pena pedir?*

O que ele produz é o relatório que o `TagSource::Jev` (S-5.7) teria de justificar:
quais memórias mudariam, com que confiança, e quanto isso vale.

Por que é somente-leitura
-------------------------
**[F] A gravação com marca de origem não é possível hoje.** O plano diz "grava
só conf ≥ τ com `derived_by=jev-1.13.0`, via `touring memory` CLI, nunca SQL
direto", e o teste é `test_reversible_by_derived_by`. Mas:

1. `touring memory tag <KEY> <TAG>` não aceita `--source` — grava `explicit`;
2. `TagSource` (`crates/touring-intelligence/src/rl/memory/tags.rs:506`) é um
   **enum fechado** de 4 variantes: `Backfill · Auto · CodeSync · Explicit`;
3. a reversão que a coluna `source` permite **é** SQL
   (`DELETE … WHERE source='jev'`), que o plano proíbe.

Marcar como `explicit` seria mentira: o doc-comment do enum diz *"stated by the
author/LLM at creation (highest trust)"*, e um Jev não é o autor nem o creation.
Então a gravação espera o `TagSource::Jev`, que é a **mesma edição de `tags.rs`
que o S-5.7 precisa** para `fix`/`report`/`audit`.

O τ, esse sim, existe
--------------------
Lido de `experiments/g2-baselines/results.json` → `threshold_recommended`, com a
curva de evidência ao lado. **[F]** `memory_kind` fecha τ = 0,80 (faixa [0,8, 1,0]:
acc 0,90 com support 0,274); `prompt_intent` e `recall_relevance` são `null` por
decisão — e `null` é lido como **ausência nomeada**, nunca como τ 0.

Alvo
----
**[F] Medido em 01/10:** 12.164 entradas com `kind` de origem `auto` dentro do
vocabulário (7.489 `insight`, 4.396 `lesson`, 257 `reference`…), e **apenas 1**
com dois `kind:auto` conflitantes. `explicit` não é tocado (o autor disse), nem
`code_sync` (o código diz). O valor do backfill está, portanto, em **trocar a
classificação**, não em resolver conflito.

Uso:
    python3 jev_tags_backfill.py --n 300 --preview      # roda o Jev, não grava
    python3 jev_tags_backfill.py --n 50 --dry-run       # só o plano, zero chamadas
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sqlite3
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from jev_client import system_one

REPO = Path(__file__).resolve().parent.parent
MEMORY_DB = Path(os.path.expanduser("~/projects/touring/.claude/touring/memory.db"))
G2_RESULTS = REPO / "docs/plans/2026-09-30-jev-harness-potencializacao/experiments/g2-baselines/results.json"
REPORT_DIR = REPO / "docs/plans/2026-09-30-jev-harness-potencializacao/experiments/s5-backfill"

# O vocabulário de PRODUÇÃO, lido de `Facet::Kind::seed_values()`
# (`tags.rs:251-278`). Delimitado aqui porque o script roda fora do crate.
# **[F] O vocabulário NÃO é copiado aqui.** Era uma tupla duplicada, e a S-5.7
# (fix/report/audit no Rust) a deixou com 22 de 25. Uma única fonte agora:
# `production_kind_values()` lê `Facet::Kind::seed_values()` do Rust.
from jev_tags_sample import KIND_CRITERIA as SAMPLE_KIND_CRITERIA
from jev_tags_sample import production_kind_values

KIND_VALUES = production_kind_values()

# Os critérios do backfill são os da amostra (`jev_tags_sample.KIND_CRITERIA`, na MESMA ordem de
# chaves — a ordem entra no prompt) com cinco redações próprias, listadas abaixo. Assim as 17
# definições idênticas moram num lugar só, como o vocabulário acima.
KIND_CRITERIA = {
    **SAMPLE_KIND_CRITERIA,
    "script": "An executable script or automation step. Not a plan to write one.",
    "lesson": "A learned principle stated in the author's own words, generalizing beyond this incident.",
    "gotcha": "A specific trap that bites: an API quirk, a footgun, an order dependency. Concrete and narrow.",
    "decision": "A choice made between alternatives, with the reason. Not a plan for an unmade choice.",
    "outcome": "The recorded result of a command, run, or experiment, with status. Not interpretation.",
}

# Um único SQL, com a lista de valores como parâmetros vinculados — sem
# interpolação de string. A versão anterior montava o fragmento `IN (...)` com
# f-string, e o quality gate a reprovou em F2.1 (CWE-89, 0.02). A forma
# correta é `?,?,?` com os valores em `params`.
_TARGET_SQL = """
    SELECT e.key, e.value, e.entry_type, e.access_count,
           t.value AS auto_kind, t.source AS auto_source
      FROM memory_entries e
      JOIN memory_tags t ON t.entry_key = e.key AND t.facet = ?
     WHERE e.superseded_by IS NULL
       AND t.source = ?
       AND t.value IN ({placeholders})
     ORDER BY e.access_count DESC, e.key
"""


def target_sql() -> tuple[str, list]:
    """O SQL e seus parâmetros. Os `?` são todos VINCULADOS — nenhum valor
    entra na string."""
    placeholders = ",".join("?" * len(KIND_VALUES))
    return (
        _TARGET_SQL.format(placeholders=placeholders),
        ["kind", "auto", *KIND_VALUES],
    )


def load_tau() -> dict:
    """Lê `threshold_recommended` do G2. Ausente = `unmeasured`, nunca 0.8."""
    if not G2_RESULTS.exists():
        return {"status": "unmeasured", "reason": "g2_results_absent", "tau": None}
    data = json.loads(G2_RESULTS.read_text(encoding="utf-8"))
    entry = (data.get("threshold_recommended") or {}).get("memory_kind")
    if not entry or entry.get("tau") is None:
        return {
            "status": "unmeasured",
            "reason": (entry or {}).get("reason", "no threshold for memory_kind"),
            "tau": None,
        }
    return {
        "status": "measured",
        "tau": float(entry["tau"]),
        "reason": entry.get("reason"),
        "calibration_curve": entry.get("calibration_curve"),
        "split_sha256": data.get("split_sha256"),
    }


def load_target(n: int, seed: int) -> list[dict]:
    """As `n` entradas que o backfill considersaria, em modo SOMENTE LEITURA.

    Só `kind:auto` entra. `explicit` é o autor (intocável), `code_sync` é uma
    declaração no código, e `backfill` já foi tentado.
    """
    if not MEMORY_DB.exists():
        raise SystemExit(f"banco ausente: {MEMORY_DB}")
    sql, params = target_sql()
    con = sqlite3.connect(f"file:{MEMORY_DB}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    try:
        rows = con.execute(sql, params).fetchall()
    finally:
        con.close()
    pool = [dict(r) for r in rows]
    # A ordem de consulta já é determinística (access_count DESC, key); o seed
    # só embaralha a PRIMEIRA leva, para que não seja só "as mais acessadas".
    random.Random(seed).shuffle(pool)
    return pool[:n]


def ask(memory: dict, *, client=None) -> dict:
    """Uma chamada por memória. `state` é a chave e o texto — e nada mais,
    sobretudo não o `auto_kind` atual, que é a resposta que queremos medir."""
    state = {
        "memory_key": memory.get("key"),
        "memory_text": (memory.get("value") or "")[:1200],
    }
    question = {
        "kind": {
            "type": "choice",
            "instructions": (
                "Choose the single best value for the `kind` facet of this Touring memory entry. "
                "The vocabulary is a closed set; pick exactly one. "
                "Judge what the text IS, not what its key prefix suggests."
            ),
            "criteria": dict(KIND_CRITERIA),
        }
    }
    started = time.monotonic()
    answers = system_one(state, question, client=client)
    elapsed = (time.monotonic() - started) * 1000
    if answers is None:
        return {"jev_kind": None, "confidence": None, "latency_ms": round(elapsed, 1), "route": "fallback"}
    ans = answers.get("kind", {})
    return {
        "jev_kind": ans.get("choice"),
        "confidence": ans.get("confidence"),
        "latency_ms": round(elapsed, 1),
        "route": "api",
    }


def _is_confident(row: dict, threshold: float | None) -> bool:
    conf = row.get("confidence")
    return isinstance(conf, (int, float)) and threshold is not None and conf >= threshold


def summarize(rows: list[dict], tau: dict) -> dict:
    """O relatório que justifica (ou não) pedir o `TagSource::Jev` ao Gabriel."""
    measured = [r for r in rows if r["jev_kind"] is not None]
    threshold = tau.get("tau")
    above = [r for r in measured if _is_confident(r, threshold)]
    would_change = [r for r in measured if r["jev_kind"] != r["auto_kind"]]
    written = [r for r in above if r["jev_kind"] != r["auto_kind"]]
    lat = sorted(r["latency_ms"] for r in rows)
    transitions = Counter(f"{r['auto_kind']}→{r['jev_kind']}" for r in would_change)

    return {
        "n_requested": len(rows),
        "measured": len(measured),
        "unmeasured": len(rows) - len(measured),
        "tau": threshold,
        "tau_source": tau.get("status"),
        "tau_reason": tau.get("reason"),
        "would_change": len(would_change),
        "would_change_above_tau": len(written),
        "unchanged": len(measured) - len(would_change),
        "agreement": round((len(measured) - len(would_change)) / len(measured), 4) if measured else None,
        "transitions": dict(transitions.most_common(12)),
        "latency_ms": {
            "p50": round(lat[len(lat) // 2], 1) if lat else None,
            "max": round(lat[-1], 1) if lat else None,
        },
        "writes_performed": 0,
        "reversible_by": "TagSource::Jev (S-5.7) — ainda não existe",
    }


def write_one(key: str, kind: str) -> tuple[bool, str]:
    """Grava UMA tag com `source=jev`, via a CLI — nunca SQL.

    **[F] O caminho existe desde o S-5.7 (01/10):** `touring memory tag <KEY>
    "#kind:<valor>" --source jev`, e a reversão é `DELETE FROM
    memory_tags WHERE source='jev'` — provada por ela em 5 verificações, e
    nomeada no próprio `--help` da CLI.

    Por que a CLI e não SQL: o plano proíbe, e com razão — SQL direto pula o
    hook de escrita, e o `TagSource::Jev` ficaria gravado sem nenhum rastro de
    quem pediu. A CLI nomeia a origem e devolve JSON.

    Devolve `(gravou, motivo)`. Motivo é o que o relatório publica, e nunca é
    vazio: um `False` sem motivo é indistinguível de "não havia nada a gravar".
    """
    proc = subprocess.run(
        ["touring", "memory", "tag", key, f"#kind:{kind}", "--source", "jev", "--json"],
        capture_output=True, text=True, timeout=60, check=False,
    )
    return _interpret_write(proc)


def _first_line(text: str) -> str:
    lines = (text or "").strip().splitlines()
    return lines[0][:80] if lines else ""


def _interpret_write(proc) -> tuple[bool, str]:
    """A resposta da CLI em `(gravou, motivo)` — separado de `write_one` porque
    é a parte que decide, e a que concentra os ramos."""
    if proc.returncode != 0:
        detail = _first_line(proc.stderr) or _first_line(proc.stdout)
        return False, f"cli_exit_{proc.returncode}:{detail}"
    try:
        payload = json.loads(proc.stdout or "{}")
    except json.JSONDecodeError as exc:
        return False, f"cli_unparseable:{exc}"
    # A CLI nomeia a fonte que gravou. Se não for `jev`, algo gravou por outro
    # caminho e o relatório precisa dizer, não assumir.
    source = payload.get("source")
    if source != "jev":
        return False, f"unexpected_source:{source}"
    return True, str(payload.get("status", "tagged"))


def run_batch(target: list[dict], tau: dict, *, write: bool) -> list[dict]:
    """Uma chamada por entrada; grava o que muda E está acima de τ.

    Só o que **muda e passa do corte** chega à CLI. O resto — concordância, ou
    mudança abaixo de τ — não vira chamada de escrita, e o relatório diz qual dos
    dois foi. Separado de `main` porque é aqui que mora a política de gravação.
    """
    gravados: list[dict] = []
    for i, memory in enumerate(target, 1):
        result = ask(memory)
        memory.update(result)
        flag = result["jev_kind"] or "SEM RESPOSTA"
        changed = bool(result["jev_kind"]) and result["jev_kind"] != memory["auto_kind"]
        linha = (f"  [{i:3}/{len(target)}] {str(memory['key'])[:44]:44} "
                 f"{memory['auto_kind']!s:11} -> {flag:13} conf={result['confidence']}")
        if write and changed and _is_confident(memory, tau["tau"]):
            ok, motivo = write_one(memory["key"], result["jev_kind"])
            gravados.append({"key": memory["key"], "kind": result["jev_kind"],
                             "confidence": result["confidence"], "written": ok, "reason": motivo})
            print(f"{linha}  {'GRAVOU' if ok else 'FALHOU: ' + motivo}")
        else:
            print(f"{linha}{'  <- MUDA' if changed else ''}")
    return gravados


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--n", type=int, default=300, help="tamanho da leva")
    ap.add_argument("--seed", type=int, default=20261001)
    ap.add_argument("--dry-run", action="store_true", help="só o plano; zero chamadas")
    ap.add_argument("--write", action="store_true",
                    help="GRAVA as tags acima de τ via `touring memory tag --source jev`. "
                         "Reversível por `DELETE FROM memory_tags WHERE source='jev'`.")
    ap.add_argument("--out", type=Path, default=REPORT_DIR / "preview.json")
    args = ap.parse_args(argv)

    tau = load_tau()
    target = load_target(args.n, args.seed)

    if args.dry_run:
        print(f"alvo: {len(target)} entradas com kind:auto dentro dos {len(KIND_VALUES)} valores")
        print(f"tau : {tau['status']} tau={tau['tau']}")
        print("amostra:")
        for row in target[:3]:
            print(f"  · {row['key'][:64]}")
            print(f"    auto_kind={row['auto_kind']!r} acc={row['access_count']}")
        print("\n(dry-run: nenhuma chamada, nenhuma escrita)")
        return 0

    if tau.get("tau") is None:
        print(f"ABORT: τ ausente ({tau.get('reason')}) — sem corte não há o que gravar.", file=sys.stderr)
        return 2

    modo = "ESCRITA (--write)" if args.write else "SOMENTE LEITURA"
    print(f"{'leva' if args.write else 'prévia'} de {len(target)} entradas, τ={tau['tau']} — {modo}\n")
    gravados = run_batch(target, tau, write=args.write)

    summary = summarize(target, tau)
    summary["writes_performed"] = sum(1 for g in gravados if g["written"])
    summary["write_attempts"] = len(gravados)
    summary["reversible_by"] = "DELETE FROM memory_tags WHERE source='jev' (S-5.7)"
    payload = {
        "subtask": "S-5.2",
        "mode": "write" if args.write else "preview_only",
        "writes_performed": summary["writes_performed"],
        "written": gravados,
        "n": args.n,
        "seed": args.seed,
        "db": str(MEMORY_DB),
        "tau_contract": tau,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "summary": summary,
        "rows": target,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print("\n" + json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"\nrelatório: {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
