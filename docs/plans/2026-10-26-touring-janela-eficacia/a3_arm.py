#!/usr/bin/env python3
"""A3 quasi-experiment tooling: daily arm switch and pre-registered analysis (S-12).

Frozen spec: ``A3-preregistro.md`` in this directory (section numbers below refer to
it). Calendar: ``a3-calendario.json``. Subcommands:

    status   which arm is in force now (or "experimento não ativo")
    apply    write the day's level into env.TOURING_HOOK_INJECTION of settings.json
             (dry-run unless --yes; --yes backs the file up first)
    analyze  per-cell efficacy, d_b, bootstrap CI and the §6.4 decision

Conventions the pre-registration leaves open, fixed here and reported to the owner:

* block indexes are 0-based everywhere (``arm_for``, ``blocos_invalidados[].block``);
* the calendar ``day0`` is the local date whose ``switch_hour_local`` o'clock opens day 0;
* a block counts only once its last day has ended (``--as-of``); until then it is
  ``em_curso`` and the decision stays ``pendente``;
* the bootstrap CI is the percentile interval ``[s[int(.025*N)], s[int(.975*N)-1]]``
  of the N sorted resampled estimates.

Outcome classification is NOT reimplemented: ``scripts/measure_tool_trio.py`` is the
instrument of the ponto 0 and is imported as is.

Exit codes: 0 ok, 2 refusal / invalid input, 1 unexpected.
"""
from __future__ import annotations

import argparse
import collections
import datetime as dt
import functools
import json
import os
import random
import shutil
import sys
import tempfile
from dataclasses import dataclass
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

SEED = 20261027  # §6.2
RESAMPLES = 2000  # §6.2
MARGIN_PP = -1.0  # §6.3
MIN_CALLS = 40  # §3: a session counts with at least this many tool calls
MIN_SESSIONS = 10  # §3: a cell is valid with at least this many such sessions
ENV_KEY = "TOURING_HOOK_INJECTION"
FULL_LEVEL = "full"
CALENDAR_PATH = os.path.join(HERE, "a3-calendario.json")
DEFAULT_SETTINGS = "~/.claude/settings.json"
JOURNAL = "~/.claude/touring/injection_suppressed.jsonl"
REQUIRED_KEYS = (
    "day0",
    "switch_hour_local",
    "tz",
    "primary_blocks",
    "extension_blocks",
    "extension_active",
    "arms",
    "blocos_invalidados",
)


def _load_instrument():
    """Import the versioned trio instrument from ``<repo>/scripts``."""
    scripts = os.path.join(REPO, "scripts")
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    import measure_tool_trio

    return measure_tool_trio


m = _load_instrument()


class Refusal(Exception):
    """Invalid input or a refused action: the CLI maps it to exit code 2."""


# --------------------------------------------------------------------------- calendar


def load_calendar(path: str) -> dict:
    """Read and validate ``a3-calendario.json``."""
    try:
        with open(path, encoding="utf-8") as fh:
            cal = json.load(fh)
    except OSError as exc:
        raise Refusal(f"calendário ilegível ({path}): {exc}") from exc
    except ValueError as exc:
        raise Refusal(f"calendário não é JSON válido ({path}): {exc}") from exc
    return validate_calendar(cal)


def _blocks_ok(blocks) -> bool:
    return (
        isinstance(blocks, list)
        and bool(blocks)
        and all(isinstance(b, str) and b and not set(b) - set("AB") for b in blocks)
    )


def _is_int(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _check_clock(cal: dict) -> None:
    if cal["day0"] is not None:
        try:
            dt.date.fromisoformat(cal["day0"])
        except (TypeError, ValueError) as exc:
            raise Refusal("day0 deve ser null ou 'AAAA-MM-DD'") from exc
    if not _is_int(cal["switch_hour_local"]) or not 0 <= cal["switch_hour_local"] <= 23:
        raise Refusal("switch_hour_local deve ser um inteiro de 0 a 23")
    try:
        ZoneInfo(cal["tz"])
    except (ZoneInfoNotFoundError, ValueError, TypeError) as exc:
        raise Refusal(f"tz desconhecido: {cal['tz']!r}") from exc


def _check_design(cal: dict) -> None:
    arms = cal["arms"]
    arms_ok = isinstance(arms, dict) and set(arms) == {"A", "B"}
    if not arms_ok or not all(isinstance(v, str) and v for v in arms.values()):
        raise Refusal("arms deve mapear exatamente A e B para níveis não vazios")
    for key in ("primary_blocks", "extension_blocks"):
        if not _blocks_ok(cal[key]):
            raise Refusal(f"{key} deve ser uma lista não vazia de padrões com as letras A e B")
    if not isinstance(cal["extension_active"], bool):
        raise Refusal("extension_active deve ser booleano")


def _check_invalidated(cal: dict) -> None:
    total = len(cal["primary_blocks"]) + len(cal["extension_blocks"])
    entries = cal["blocos_invalidados"]
    if not isinstance(entries, list):
        raise Refusal("blocos_invalidados deve ser uma lista")
    for entry in entries:
        ok = (
            isinstance(entry, dict)
            and _is_int(entry.get("block"))
            and 0 <= entry["block"] < total
            and isinstance(entry.get("motivo"), str)
            and entry["motivo"].strip() != ""
        )
        if not ok:
            raise Refusal(
                "cada entrada de blocos_invalidados precisa de {block: índice 0-based, motivo: texto}"
            )


def validate_calendar(cal) -> dict:
    """Raise ``Refusal`` unless ``cal`` has the shape the rest of the script relies on."""
    if not isinstance(cal, dict):
        raise Refusal("calendário deve ser um objeto JSON")
    missing = [k for k in REQUIRED_KEYS if k not in cal]
    if missing:
        raise Refusal(f"calendário sem as chaves: {', '.join(missing)}")
    _check_clock(cal)
    _check_design(cal)
    _check_invalidated(cal)
    return cal


@functools.cache
def _zone(name: str) -> ZoneInfo:
    return ZoneInfo(name)


def sequence(cal: dict) -> list[str]:
    """Block patterns in force: primary, plus the extension once it is active (§3)."""
    blocks = list(cal["primary_blocks"])
    if cal["extension_active"]:
        blocks += cal["extension_blocks"]
    return blocks


def block_spans(cal: dict) -> list[tuple[str, int, int]]:
    """``(pattern, first_day, end_day_exclusive)`` per block of the sequence."""
    spans, first = [], 0
    for pattern in sequence(cal):
        spans.append((pattern, first, first + len(pattern)))
        first += len(pattern)
    return spans


def day_start_utc(cal: dict, k: int) -> dt.datetime:
    """UTC instant at which experiment day ``k`` opens (``k`` may be negative)."""
    day = dt.date.fromisoformat(cal["day0"]) + dt.timedelta(days=k)
    local = dt.datetime.combine(day, dt.time(cal["switch_hour_local"]), tzinfo=_zone(cal["tz"]))
    return local.astimezone(dt.UTC)


def parse_ts(text: str) -> dt.datetime:
    """Aware UTC datetime from an ISO string (a naive one is taken as UTC)."""
    stamp = dt.datetime.fromisoformat(text.strip())
    if stamp.tzinfo is None:
        stamp = stamp.replace(tzinfo=dt.UTC)
    return stamp.astimezone(dt.UTC)


def day_index(stamp: dt.datetime, cal: dict) -> int:
    """Experiment day containing ``stamp`` (negative before day 0, unbounded after)."""
    day = (stamp.astimezone(_zone(cal["tz"])).date() - dt.date.fromisoformat(cal["day0"])).days
    # Compare in UTC: same-zone aware datetimes compare by wall time, not by instant.
    while stamp < day_start_utc(cal, day):
        day -= 1
    while stamp >= day_start_utc(cal, day + 1):
        day += 1
    return day


def arm_for(ts_utc_iso: str, cal: dict) -> tuple[int, str] | None:
    """``(block_index, arm_letter)`` in force at ``ts_utc_iso``; None outside the sequence."""
    if cal.get("day0") is None:
        return None
    day = day_index(parse_ts(ts_utc_iso), cal)
    if day < 0:
        return None
    for index, (pattern, first, end) in enumerate(block_spans(cal)):
        if first <= day < end:
            return index, pattern[day - first]
    return None


def cell_intervals(cal: dict, span: tuple[str, int, int], arm: str):
    """UTC ``[start, end)`` of every day of one block on which ``arm`` is in force."""
    pattern, first, _end = span
    return [
        (day_start_utc(cal, k), day_start_utc(cal, k + 1))
        for k in range(first, first + len(pattern))
        if pattern[k - first] == arm
    ]


# --------------------------------------------------------------------------- sessions


@dataclass
class Sess:
    """One transcript folded into the counters the analysis needs."""

    path: str
    sid: str
    first_ts: str
    last_ts: str
    block: int
    arm: str
    model: str
    calls: int
    erro: int
    vazio: int
    negado: int
    retry: int
    spans_switch: bool

    @property
    def bad(self) -> int:
        return self.erro + self.vazio + self.negado


class _Tap:
    """Records last timestamp and first-assistant model while a transcript streams by."""

    def __init__(self) -> None:
        self.last_ts = ""
        self.model = ""
        self._model_seen = False

    def wrap(self, records):
        for rec in records:
            stamp = rec.get("timestamp")
            if stamp:
                self.last_ts = stamp
            if not self._model_seen and rec.get("type") == "assistant":
                self._model_seen = True
                self.model = (rec.get("message") or {}).get("model") or ""
            yield rec


def first_timestamp(path: str) -> str:
    """Timestamp of the first record that has one ('' when the transcript has none)."""
    with open(path, errors="ignore") as fh:
        for line in fh:
            try:
                rec = json.loads(line)
            except ValueError:
                continue
            if isinstance(rec, dict) and rec.get("timestamp"):
                return rec["timestamp"]
    return ""


def scan_session(path: str, cal: dict) -> Sess | None:
    """Fold one transcript; None when it opens outside the sequence (§3: first record)."""
    first = first_timestamp(path)
    if not first:
        return None
    pos = arm_for(first, cal)
    if pos is None:
        return None
    tap = _Tap()
    stats: dict = {}
    m.measure_session(tap.wrap(m.iter_records(path)), stats)
    last = tap.last_ts or first
    return Sess(
        path=path,
        sid=os.path.splitext(os.path.basename(path))[0],
        first_ts=first,
        last_ts=last,
        block=pos[0],
        arm=pos[1],
        model=tap.model or "(desconhecido)",
        calls=sum(s.calls for s in stats.values()),
        erro=sum(s.outcomes["erro"] for s in stats.values()),
        vazio=sum(s.outcomes["vazio"] for s in stats.values()),
        negado=sum(s.outcomes["negado"] for s in stats.values()),
        retry=sum(s.retry for s in stats.values()),
        spans_switch=day_index(parse_ts(first), cal) != day_index(parse_ts(last), cal),
    )


def scan_corpus(files: list[str], cal: dict) -> tuple[list[Sess], list[dict]]:
    """Fold every transcript; one the instrument cannot measure is listed, never silently dropped."""
    sessions, unmeasured = [], []
    for path in files:
        try:
            sess = scan_session(path, cal)
        except Exception as exc:  # noqa: BLE001 - a malformed transcript must not sink the analysis
            unmeasured.append({"arquivo": os.path.basename(path), "erro": type(exc).__name__})
            continue
        if sess is not None:
            sessions.append(sess)
    return sessions, unmeasured


# ------------------------------------------------------------------------- statistics


def efficacy(calls: int, bad: int) -> float:
    """§4: 100 - erro% - vazio% - negado%, over pooled counts."""
    return 100.0 - 100.0 * bad / calls


def pool(sessions: list[Sess]) -> dict:
    """Pooled counters of a set of sessions (sum of outcomes over sum of calls)."""
    calls = sum(s.calls for s in sessions)
    negado = sum(s.negado for s in sessions)
    bad = sum(s.bad for s in sessions)
    pct = (lambda n: 100.0 * n / calls) if calls else (lambda _n: None)
    return {
        "sessoes": len(sessions),
        "calls": calls,
        "erro": sum(s.erro for s in sessions),
        "vazio": sum(s.vazio for s in sessions),
        "negado": negado,
        "retry": sum(s.retry for s in sessions),
        "eficacia_pct": efficacy(calls, bad) if calls else None,
        "retry_pct": pct(sum(s.retry for s in sessions)),
        "negado_pct": pct(negado),
    }


def group_cells(sessions: list[Sess]):
    """``(eligible, totals)``: sessions with >= MIN_CALLS per cell, and all sessions per cell."""
    eligible: dict[tuple[int, str], list[Sess]] = collections.defaultdict(list)
    totals: collections.Counter = collections.Counter()
    for s in sessions:
        totals[(s.block, s.arm)] += 1
        if s.calls >= MIN_CALLS:
            eligible[(s.block, s.arm)].append(s)
    for group in eligible.values():
        group.sort(key=lambda s: (s.first_ts, s.path))
    return eligible, totals


def _resample_pool(rng: random.Random, cell: list[tuple[int, int]]) -> tuple[int, int]:
    n = len(cell)
    calls = bad = 0
    for _ in range(n):
        c, b = cell[int(rng.random() * n)]
        calls += c
        bad += b
    return calls, bad


def bootstrap_ci(cells, seed: int = SEED, resamples: int = RESAMPLES):
    """§6.2: 95% percentile CI of the mean ``d_b`` by session bootstrap inside each cell.

    ``cells`` is one ``(A_sessions, B_sessions)`` pair per valid block, each session a
    ``(calls, bad)`` tuple. Every resample redraws each cell (A first, then B, blocks
    ascending) and re-pools it. Draws use ``rng.random()`` only, so the stream is the
    plain Mersenne Twister one for a given seed.
    """
    if not cells:
        return None
    rng = random.Random(seed)
    estimates = []
    for _ in range(resamples):
        diffs = []
        for a_cell, b_cell in cells:
            a_calls, a_bad = _resample_pool(rng, a_cell)
            b_calls, b_bad = _resample_pool(rng, b_cell)
            diffs.append(efficacy(b_calls, b_bad) - efficacy(a_calls, a_bad))
        estimates.append(sum(diffs) / len(diffs))
    estimates.sort()
    return estimates[int(0.025 * resamples)], estimates[int(0.975 * resamples) - 1]


def _block_state(index, end_utc, now, invalid, eligible):
    if index in invalid:
        return "invalidado", invalid[index]
    if end_utc > now:
        return "em_curso", "o último dia do bloco ainda não terminou"
    short = [f"{arm}={len(eligible.get((index, arm), []))}" for arm in "AB"
             if len(eligible.get((index, arm), [])) < MIN_SESSIONS]
    if short:
        return "celula_abaixo_do_minimo", (
            f"menos de {MIN_SESSIONS} sessões com >= {MIN_CALLS} calls: {', '.join(short)}"
        )
    return "valido", None


def decide(cal: dict, spans, elapsed: int, valid: int, estimate, ci) -> dict:
    """§6.4. Anything short of the full planned design stays ``pendente`` (fail-closed)."""
    scheduled = len(spans)
    extended = cal["extension_active"]
    if elapsed < scheduled:
        return {
            "resultado": "pendente",
            "motivo": f"{scheduled - elapsed} de {scheduled} blocos ainda em curso ou por vir",
        }
    if valid < scheduled:
        return {
            "resultado": "pendente",
            "motivo": (
                f"{scheduled - valid} de {scheduled} blocos inválidos; §3 manda repeti-los no "
                "fim da sequência, e isso não está mecanizado"
            ),
        }
    if extended:
        adopt = estimate >= MARGIN_PP
        return {
            "resultado": "nao-inferior" if adopt else "inferior",
            "regra": "estimativa-pontual-pos-extensao",
            "acao": "no-advisory vira o default" if adopt else "fica full",
        }
    lower, upper = ci
    if lower > MARGIN_PP:
        return {"resultado": "nao-inferior", "regra": "ic95", "acao": "no-advisory vira o default"}
    if upper < MARGIN_PP:
        return {"resultado": "inferior", "regra": "ic95", "acao": "fica full"}
    return {
        "resultado": "estender",
        "regra": "ic95",
        "acao": "estender com mais 2 blocos ABBA·BAAB (§3): extension_active = true no calendário",
    }


def _block_row(cal: dict, index: int, span, now, invalid, eligible, totals) -> dict:
    """One block of the report: state, the two cells and ``d_pp`` (valid blocks only)."""
    pattern, first, end = span
    end_utc = day_start_utc(cal, end)
    state, reason = _block_state(index, end_utc, now, invalid, eligible)
    cells = {arm: pool(eligible.get((index, arm), [])) for arm in "AB"}
    for arm in "AB":
        cells[arm]["sessoes_total"] = totals.get((index, arm), 0)
    d_pp = None
    if state == "valido":
        d_pp = cells["B"]["eficacia_pct"] - cells["A"]["eficacia_pct"]
    return {
        "block": index,
        "padrao": pattern,
        "inicio_utc": day_start_utc(cal, first).isoformat(),
        "fim_utc": end_utc.isoformat(),
        "estado": state,
        "motivo": reason,
        "celulas": cells,
        "d_pp": d_pp,
    }


def evaluate(eligible, totals, cal: dict, now: dt.datetime, resamples: int = RESAMPLES) -> dict:
    """Per-block cells, ``d_b``, estimate, CI and decision for one set of sessions."""
    spans = block_spans(cal)
    invalid = {e["block"]: e["motivo"] for e in cal["blocos_invalidados"]}
    blocks = [_block_row(cal, i, sp, now, invalid, eligible, totals) for i, sp in enumerate(spans)]
    elapsed = sum(day_start_utc(cal, sp[2]) <= now for sp in spans)
    valid = [blk["block"] for blk in blocks if blk["estado"] == "valido"]
    pairs = [
        ([(s.calls, s.bad) for s in eligible[(b, "A")]], [(s.calls, s.bad) for s in eligible[(b, "B")]])
        for b in valid
    ]
    ci = bootstrap_ci(pairs, resamples=resamples)
    d_values = [blk["d_pp"] for blk in blocks if blk["d_pp"] is not None]
    estimate = sum(d_values) / len(d_values) if d_values else None
    return {
        "blocos": blocks,
        "validos": valid,
        "estimativa": {
            "d_medio_pp": estimate,
            "ic95_pp": list(ci) if ci else None,
            "margem_pp": MARGIN_PP,
            "blocos_validos": len(valid),
            "blocos_agendados": len(spans),
        },
        "decisao": decide(cal, spans, elapsed, len(valid), estimate, ci),
    }


# ------------------------------------------------------------------------ secondaries


def by_model(eligible, valid: list[int]) -> dict:
    """Descriptive per-model split over the valid blocks (not paired by block)."""
    groups: dict[str, dict[str, list[Sess]]] = collections.defaultdict(lambda: {"A": [], "B": []})
    for b in valid:
        for arm in "AB":
            for s in eligible[(b, arm)]:
                groups[s.model][arm].append(s)
    out = {}
    for model, arms in sorted(groups.items()):
        a, b = pool(arms["A"]), pool(arms["B"])
        both = a["eficacia_pct"] is not None and b["eficacia_pct"] is not None
        out[model] = {"A": a, "B": b, "d_pp": b["eficacia_pct"] - a["eficacia_pct"] if both else None}
    return out


def recovery_by_arm(eligible, valid: list[int]) -> dict:
    """§4 secondary: what happens after a deny, per gate, from the instrument's ``Extras``."""
    out = {}
    for arm in "AB":
        ex = m.Extras()
        for b in valid:
            for s in eligible[(b, arm)]:
                m.measure_extras(m.iter_records(s.path), ex)
        gates: dict[str, dict[str, int]] = collections.defaultdict(dict)
        total = recovered = 0
        for (gate, verdict), n in ex.deny_next.items():
            gates[gate][verdict] = n
            total += n
            recovered += n if verdict == "recuperou" else 0
        out[arm] = {
            "negacoes": total,
            "recuperou_pct": 100.0 * recovered / total if total else None,
            "por_gate": dict(sorted(gates.items(), key=lambda kv: -sum(kv[1].values()))),
        }
    return out


def _journal_row(rec) -> dict | None:
    """One normalized journal row, or None for a line that is not a policy record."""
    if not isinstance(rec, dict) or not _is_int(rec.get("ts")):
        return None
    cuts = [c for c in rec.get("cuts") or [] if isinstance(c, dict) and _is_int(c.get("bytes"))]
    return {
        "ts": rec["ts"],
        "level": str(rec.get("level", "")),
        "session": rec.get("session"),
        "cuts": cuts,
        "bytes": sum(c["bytes"] for c in cuts),
    }


def read_journal(path: str | None = None):
    """Rows of ``injection_suppressed.jsonl`` (+ its ``.1`` rotation); None when absent.

    Schema (crates/touring-foundation/src/injection_journal.rs, ``line_for``):
    ``{"ts": unix ms, "level": str, "site": str, "session"?: str,
    "cuts": [{"family": str, "bytes": int}]}``. Used here: ts, level, session and
    cuts[].family / cuts[].bytes.
    """
    base = os.path.expanduser(path or JOURNAL)
    files = [p for p in (base + ".1", base) if os.path.exists(p)]
    if not files:
        return None
    rows = []
    for name in files:
        with open(name, errors="ignore") as fh:
            for line in fh:
                try:
                    row = _journal_row(json.loads(line))
                except ValueError:
                    continue
                if row is not None:
                    rows.append(row)
    return rows


def _millis(stamp: dt.datetime) -> float:
    return stamp.timestamp() * 1000.0


def _dose_cell(cal: dict, index: int, span, arm: str, eligible, rows) -> dict:
    """Journal bytes of one cell: by time window of its days, and by its sessions' ids."""
    windows = [(_millis(lo), _millis(hi)) for lo, hi in cell_intervals(cal, span, arm)]
    ids = {s.sid for s in eligible.get((index, arm), [])}
    in_window = [r for r in rows if any(lo <= r["ts"] < hi for lo, hi in windows)]
    by_level: collections.Counter = collections.Counter()
    for r in in_window:
        by_level[r["level"]] += r["bytes"]
    return {
        "block": index,
        "braco": arm,
        "linhas_janela": len(in_window),
        "bytes_janela": sum(r["bytes"] for r in in_window),
        "bytes_sessoes": sum(r["bytes"] for r in rows if r["session"] in ids),
        "por_nivel": dict(sorted(by_level.items())),
    }


def dose_report(cal: dict, spans, eligible, rows) -> dict:
    """§4 dose: bytes cut by the policy per cell. Undercounts by design (§2, leak 2)."""
    note = "subconta: os cortes dos hooks Python não entram no diário (§2, vazamento 2)"
    if rows is None:
        return {"diario_ausente": True, "nota": note}
    seq_lo, seq_hi = _millis(day_start_utc(cal, 0)), _millis(day_start_utc(cal, spans[-1][2]))
    families: collections.Counter = collections.Counter()
    for r in rows:
        if seq_lo <= r["ts"] < seq_hi:
            for c in r["cuts"]:
                families[str(c.get("family", ""))] += c["bytes"]
    return {
        "diario_ausente": False,
        "campos_usados": ["ts", "level", "session", "cuts[].family", "cuts[].bytes"],
        "nota": note,
        "linhas": len(rows),
        "bytes_total_sequencia": sum(families.values()),
        "por_familia": dict(sorted(families.items(), key=lambda kv: -kv[1])),
        "por_celula": [
            _dose_cell(cal, index, span, arm, eligible, rows)
            for index, span in enumerate(spans)
            for arm in "AB"
        ],
    }


# --------------------------------------------------------------------------- analysis


def build_report(sessions, cal: dict, as_of: dt.datetime, corpus: dict, journal=None) -> dict:
    """Everything the §6.5 report needs, as one JSON-ready dict."""
    spans = block_spans(cal)
    eligible, totals = group_cells(sessions)
    primary = evaluate(eligible, totals, cal, as_of)
    stable = [s for s in sessions if not s.spans_switch]
    s_eligible, s_totals = group_cells(stable)
    sens = evaluate(s_eligible, s_totals, cal, as_of)
    valid = primary["validos"]
    per_arm = {
        arm: pool([s for b in valid for s in eligible[(b, arm)]]) for arm in "AB"
    }
    invalidated = [
        {"block": blk["block"], "estado": blk["estado"], "motivo": blk["motivo"]}
        for blk in primary["blocos"]
        if blk["estado"] in ("invalidado", "celula_abaixo_do_minimo")
    ]
    caveats = [
        "o efeito medido é um limite inferior: avisos colados num deny ainda chegam no braço B (§2, vazamento 1)",
    ]
    if corpus.get("truncado"):
        caveats.append("--sessions cortou o corpus antes de day0: aumente --sessions")
    if corpus.get("nao_medidos"):
        caveats.append(
            f"{len(corpus['nao_medidos'])} transcrito(s) fora da análise: o instrumento falhou ao medi-los (corpus.nao_medidos)"
        )
    if primary["decisao"]["resultado"] == "pendente":
        caveats.append(primary["decisao"]["motivo"])
    return {
        "as_of": as_of.isoformat(),
        "semente": SEED,
        "reamostragens": RESAMPLES,
        "calendario": {
            "day0": cal["day0"],
            "extension_active": cal["extension_active"],
            "sequencia": sequence(cal),
            "niveis": cal["arms"],
        },
        "corpus": corpus,
        "blocos": primary["blocos"],
        "estimativa": primary["estimativa"],
        "decisao": primary["decisao"],
        "por_braco_blocos_validos": per_arm,
        "blocos_invalidados": invalidated,
        "sensibilidade": {
            "descricao": "análise repetida sem as sessões que atravessam uma troca das 04:00",
            "sessoes_excluidas": len(sessions) - len(stable),
            "blocos_validos": sens["validos"],
            "estimativa": sens["estimativa"],
            "decisao": sens["decisao"],
        },
        "secundarios": {
            "por_modelo": by_model(eligible, valid),
            "recuperacao_pos_deny": recovery_by_arm(eligible, valid),
            "dose": dose_report(cal, spans, eligible, journal),
        },
        "ressalvas": caveats,
    }


def _round(obj, digits: int = 4):
    if isinstance(obj, float):
        return round(obj, digits)
    if isinstance(obj, dict):
        return {k: _round(v, digits) for k, v in obj.items()}
    if isinstance(obj, list):
        return [_round(v, digits) for v in obj]
    return obj


def _num(value, digits: int = 2) -> str:
    return "—" if value is None else f"{value:.{digits}f}".replace(".", ",")


def render_text(rep: dict) -> str:
    """Compact pt-BR summary for a terminal (the JSON stays the full record)."""
    corpus = rep["corpus"]
    lines = [
        f"A3 full × no-advisory | as_of {rep['as_of']} | semente {rep['semente']} | {rep['reamostragens']} reamostras",
        (
            f"corpus: {corpus['arquivos']} transcritos, {corpus['sessoes_na_sequencia']} na sequência, "
            f"{corpus['sessoes_validas']} com >= {MIN_CALLS} calls"
        ),
    ]
    for blk in rep["blocos"]:
        a, b = blk["celulas"]["A"], blk["celulas"]["B"]
        lines.append(
            f"bloco {blk['block']} {blk['padrao']} [{blk['estado']}] "
            f"A n={a['sessoes']} efic={_num(a['eficacia_pct'])}% | "
            f"B n={b['sessoes']} efic={_num(b['eficacia_pct'])}% | d={_num(blk['d_pp'])} pp"
        )
        if blk["motivo"]:
            lines.append(f"    motivo: {blk['motivo']}")
    est = rep["estimativa"]
    ci = est["ic95_pp"]
    lines.append(
        f"estimativa: d médio {_num(est['d_medio_pp'])} pp, IC95 "
        f"[{_num(ci[0]) if ci else '—'}; {_num(ci[1]) if ci else '—'}], margem {_num(est['margem_pp'], 1)} pp, "
        f"{est['blocos_validos']}/{est['blocos_agendados']} blocos válidos"
    )
    dec = rep["decisao"]
    lines.append(f"decisão: {dec['resultado']} — {dec.get('acao') or dec.get('motivo')}")
    sens = rep["sensibilidade"]
    lines.append(
        f"sensibilidade (sem {sens['sessoes_excluidas']} sessões que cruzam a troca): "
        f"d médio {_num(sens['estimativa']['d_medio_pp'])} pp, decisão {sens['decisao']['resultado']}"
    )
    for note in rep["ressalvas"]:
        lines.append(f"ressalva: {note}")
    return "\n".join(lines)


def cmd_analyze(args, cal: dict, now: dt.datetime) -> int:
    if cal["day0"] is None:
        raise Refusal("experimento não ativado (day0 = null): nada a analisar")
    if args.sessions <= 0:
        raise Refusal("--sessions deve ser positivo")
    as_of = parse_ts(args.as_of) if args.as_of else now
    dirs = [d for d in args.dirs.split(",") if d]
    files = m.corpus(dirs, args.sessions)
    floor = day_start_utc(cal, 0).timestamp()
    in_window = [f for f in files if os.path.getmtime(f) >= floor]
    truncated = len(files) >= args.sessions and os.path.getmtime(files[-1]) >= floor
    sessions, unmeasured = scan_corpus(in_window, cal)
    corpus = {
        "dirs": dirs,
        "arquivos": len(in_window),
        "truncado": truncated,
        "nao_medidos": unmeasured,
        "sessoes_na_sequencia": len(sessions),
        "sessoes_validas": sum(1 for s in sessions if s.calls >= MIN_CALLS),
    }
    report = _round(build_report(sessions, cal, as_of, corpus, read_journal()))
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=1))
    else:
        print(render_text(report))
    return 0


# ------------------------------------------------------------------------ status/apply


def level_now(cal: dict, now: dt.datetime):
    """``(block, arm, level)`` in force at ``now`` or None (experiment not active)."""
    pos = arm_for(now.isoformat(), cal)
    return None if pos is None else (pos[0], pos[1], cal["arms"][pos[1]])


def _inactive_reason(cal: dict, now: dt.datetime) -> str:
    if cal["day0"] is None:
        return "day0 não definido"
    if now < day_start_utc(cal, 0):
        return "antes do dia 0"
    return "sequência encerrada"


def cmd_status(_args, cal: dict, now: dt.datetime) -> int:
    cur = level_now(cal, now)
    if cur is None:
        print(f"experimento não ativo ({_inactive_reason(cal, now)})")
        return 0
    block, arm, level = cur
    day = day_index(now, cal)
    pattern, first, end = block_spans(cal)[block]
    nxt = day_start_utc(cal, day + 1).astimezone(_zone(cal["tz"]))
    print(
        f"bloco {block} ({pattern}), dia {day - first + 1}/{end - first} do bloco: "
        f"braço {arm}, nível {level}; próxima troca {nxt.isoformat(timespec='minutes')}"
    )
    return 0


def _unique_backup(real: str, now: dt.datetime) -> str:
    stamp = now.strftime("%Y%m%dT%H%M%S%fZ")
    path, n = f"{real}.bak-{stamp}", 0
    while os.path.exists(path):
        n += 1
        path = f"{real}.bak-{stamp}-{n}"
    return path


def _write_atomic(real: str, text: str) -> None:
    """Replace ``real`` with ``text`` via a same-directory temp file, keeping its mode."""
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(real), prefix=".settings-", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as fh:
            fh.write(text)
            fh.flush()
            os.fsync(fh.fileno())
        shutil.copymode(real, tmp)
        os.replace(tmp, real)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


def _load_settings(real: str) -> tuple[str, dict]:
    """``(raw_text, settings)``; ``Refusal`` unless it is a JSON object with an object ``env``."""
    try:
        with open(real, encoding="utf-8", newline="") as fh:
            raw = fh.read()
    except OSError as exc:
        raise Refusal(f"settings ilegível ({real}): {exc}") from exc
    try:
        settings = json.loads(raw)
    except ValueError as exc:
        raise Refusal(f"settings não é JSON válido ({real}): {exc}") from exc
    if not isinstance(settings, dict) or not isinstance(settings.get("env", {}), dict):
        raise Refusal("settings: a raiz e a chave env precisam ser objetos JSON")
    return raw, settings


def _set_level(settings: dict, level: str | None) -> None:
    """Set ``env.TOURING_HOOK_INJECTION`` to ``level``; None removes the key (arm A)."""
    if level is None:
        settings.get("env", {}).pop(ENV_KEY, None)
    else:
        settings.setdefault("env", {})[ENV_KEY] = level


def _shown(value) -> str:
    return "(ausente)" if value is None else json.dumps(value, ensure_ascii=False)


def cmd_apply(args, cal: dict, now: dt.datetime) -> int:
    cur = level_now(cal, now)
    if cur is None:
        raise Refusal(f"experimento não ativo ({_inactive_reason(cal, now)}): nada a aplicar")
    block, arm, level = cur
    real = os.path.realpath(os.path.expanduser(args.settings))
    raw, settings = _load_settings(real)
    print(f"bloco {block}, braço {arm}, nível {level}")
    return _write_level(real, raw, settings, None if level == FULL_LEVEL else level, args.yes, now)


def _write_level(real: str, raw: str, settings: dict, after: str | None, yes: bool, now: dt.datetime) -> int:
    """Show the diff of ``env.TOURING_HOOK_INJECTION`` and, with ``yes``, write it after a backup."""
    before = settings.get("env", {}).get(ENV_KEY)
    print(f"--- {real}\n+++ {real}\n@@ env.{ENV_KEY} @@\n- {_shown(before)}\n+ {_shown(after)}")
    if before == after:
        print("sem mudança: o settings já está no nível pedido")
        return 0
    if not yes:
        print("dry-run: nada gravado (use --yes para gravar, com backup)")
        return 0
    backup = _unique_backup(real, now)
    shutil.copy2(real, backup)
    _set_level(settings, after)
    text = json.dumps(settings, indent=2, ensure_ascii=False) + ("\n" if raw.endswith("\n") else "")
    _write_atomic(real, text)
    print(f"gravado; backup em {backup}")
    return 0


def cmd_reset(args, cal: dict, now: dt.datetime) -> int:
    """End of the experiment: remove the key (level full). Refuses while the calendar says
    a block is running, so an extension written into the calendar keeps the arms in force."""
    cur = level_now(cal, now)
    if cur is not None:
        block, arm, level = cur
        raise Refusal(f"experimento ativo (bloco {block}, braço {arm}, nível {level}): reset recusado")
    real = os.path.realpath(os.path.expanduser(args.settings))
    raw, settings = _load_settings(real)
    print(f"experimento não ativo ({_inactive_reason(cal, now)}): volta ao nível {FULL_LEVEL}")
    return _write_level(real, raw, settings, None, args.yes, now)


# ------------------------------------------------------------------------------- CLI


def build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--calendar", default=CALENDAR_PATH, help="caminho do a3-calendario.json")
    ap = argparse.ArgumentParser(prog="a3_arm.py", description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("status", parents=[common], help="braço em vigor agora")
    ap_apply = sub.add_parser("apply", parents=[common], help="grava o nível do dia no settings.json")
    ap_apply.add_argument("--yes", action="store_true", help="grava de verdade (default: dry-run)")
    ap_apply.add_argument("--settings", default=DEFAULT_SETTINGS)
    ap_reset = sub.add_parser("reset", parents=[common], help="fim do experimento: remove a chave (nível full)")
    ap_reset.add_argument("--yes", action="store_true", help="grava de verdade (default: dry-run)")
    ap_reset.add_argument("--settings", default=DEFAULT_SETTINGS)
    ap_an = sub.add_parser("analyze", parents=[common], help="análise pré-registrada (§6)")
    ap_an.add_argument("--dirs", default=",".join(m.DEFAULT_DIRS), help="projetos em ~/.claude/projects, separados por vírgula")
    ap_an.add_argument("--sessions", type=int, default=2000, help="máximo de transcritos mais recentes lidos")
    ap_an.add_argument("--as-of", default="", help="instante de referência ISO UTC (default: agora)")
    ap_an.add_argument("--json", action="store_true", help="relatório completo em JSON")
    return ap


COMMANDS = {"status": cmd_status, "apply": cmd_apply, "reset": cmd_reset, "analyze": cmd_analyze}


def main(argv=None, now: dt.datetime | None = None) -> int:
    args = build_parser().parse_args(argv)
    now = now or dt.datetime.now(dt.UTC)
    try:
        return COMMANDS[args.cmd](args, load_calendar(args.calendar), now)
    except Refusal as exc:
        print(f"recusado: {exc}", file=sys.stderr)
        return 2
    except Exception as exc:  # noqa: BLE001 - the CLI contract is "exit 1, never a traceback"
        print(f"erro inesperado: {exc!r}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
