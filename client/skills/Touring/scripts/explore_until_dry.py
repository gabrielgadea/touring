#!/usr/bin/env python3
"""touring explore <topic> --until-dry — F1 of the ADW plan (loop-until-dry exploration).

Multi-lens exploration that keeps sweeping a topic until convergence is MEASURED,
never felt. Implements the CCE v2 (Contrato de Convergência de Exploração) from
docs/plans/2026-07-19-touring-adw-software-factory/plan.md:

  * A persistent LEDGER (JSON) accumulates findings across rounds and sessions;
    every round dedupes against the ENTIRE ledger (dedupe vs seen, not vs kept).
  * A round only counts toward dryness when the FULL automated lens catalog ran
    (lens rotation — a repeat of the same partial sweep never "dries" the topic).
  * Non-automatable lenses (external best-practices) are explicit coverage cells
    that must be VISITED or WAIVED by a human/critic before convergence — the
    lens that never ran in rounds 1-3 of the 2026-07-19 forensics cannot be
    silently skipped again.
  * Truncation is ACCOUNTED: every list a lens takes the head of records how many
    items were elided (the `head -100` failure made measurable).
  * Finding depth is typed: D0 (listing hit), D1 (source/metadata opened),
    D2 (corroborated by 2+ independent lenses — computed, not asserted).
  * OPEN QUESTIONS gate convergence: findings spawn questions (endogenous
    targets); the queue must be empty (answered/waived) to converge.
  * SELF-ECHO suppression (2026-08-29): a memory stored AFTER the campaign
    began that resurfaces via recall is the campaign's own output echoing
    back — recorded VISIBLE (``echo: true``, ``echoes_suppressed`` in the
    verdict) but never wetting the dry-tail, or a self-referential topic
    livelocks ([1,0,0,1,…] measured). Suppression needs POSITIVE temporal
    proof (``stored_at`` from recall vs the ledger's ``created_at``); any
    missing timestamp counts the finding normally.
  * The verdict is epistemically honest: never "complete" — always "no new
    findings UNDER current questions/lenses/depth after K dry rounds".

Exit codes:
  0  converged (dry under contract — see verdict for the conditions)
  1  not converged — continue (report says what is missing)
  3  degraded — daemon down; ledger still updated via grep fallback
  2  usage / path errors

Usage:
  explore_until_dry.py CanonicalName --scope ~/projects/touring
  explore_until_dry.py "adw runner" --rounds 1            # run ONE more round
  explore_until_dry.py topic --question "did we build this before?"
  explore_until_dry.py topic --answer q_ab12cd34 --note "yes — taco-forge, see memory"
  explore_until_dry.py topic --mark-lens external:visited --note "websearch 3 sources"
  explore_until_dry.py topic --status                     # report only, no new round
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
import unicodedata
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib_touring import (  # noqa: E402
    add_common_args, emit_error, emit_kv, emit_result, emit_section, emit_table,
    grep_fallback, mark_degraded, parse_definitions, touring_run,
)

LEDGER_DIRNAME = ".touring-explore"
AUTOMATED_LENSES = ("lexical", "structural", "institutional", "antistaleness",
                    "quality", "portfolio")
MANUAL_LENSES = ("external",)          # requires --mark-lens visited|waived
ALL_LENSES = AUTOMATED_LENSES + MANUAL_LENSES
DEFAULT_DRY_ROUNDS = 2
DEFAULT_MAX_ROUNDS = 12
MEMORY_SCORE_FLOOR = 0.30              # recall entries below this are noise, not findings
TOP_HITS_PER_LENS = 12                 # head taken per lens list — ALWAYS accounted


def det_id(prefix: str, *parts: str) -> str:
    """Deterministic id (REGRA #17): same inputs → same id across sessions."""
    digest = hashlib.sha1("|".join(parts).encode("utf-8")).hexdigest()[:12]
    return f"{prefix}_{digest}"


# === ledger ================================================================

def _slugify(topic: str, fold: bool = True) -> str:
    """Topic → ledger filename stem.

    `"ê".isalnum()` is True in Python, so the pre-2026-08-25 slug carried
    diacritics straight into the filename: `…inteligência…` and
    `…inteligencia…` — the SAME question typed twice, once with the accent —
    opened two ledgers side by side in one scope. Convergence reached in one
    said nothing about the other, and the gate could be satisfied by whichever
    was fresher (observed live, both defects together).

    Folding to the unaccented form makes one question map to one ledger.
    """
    text = topic
    if fold:
        text = "".join(c for c in unicodedata.normalize("NFKD", topic)
                       if not unicodedata.combining(c))
    return "".join(c if c.isalnum() else "-" for c in text.lower()).strip("-")[:48]


def ledger_path_for(topic: str, scope: Path, explicit: str | None) -> Path:
    if explicit:
        return Path(explicit).expanduser().resolve()
    path = scope / LEDGER_DIRNAME / f"{_slugify(topic)}.ledger.json"
    if not path.exists():
        # A ledger written before folding keeps its diacritics in the filename.
        # Adopting it is the non-destructive migration: opening the folded path
        # instead would start a second empty ledger and silently strand every
        # round the first one had already converged.
        legacy = scope / LEDGER_DIRNAME / f"{_slugify(topic, fold=False)}.ledger.json"
        if legacy != path and legacy.exists():
            return legacy
    return path


def load_ledger(path: Path, topic: str, scope: Path) -> dict[str, Any]:
    if path.is_file():
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            if isinstance(data, dict) and data.get("topic") == topic:
                return data
        except (json.JSONDecodeError, OSError) as exc:
            emit_error(f"ledger unreadable ({exc}) — starting fresh at {path}")
    return {
        "version": 2,
        "topic": topic,
        "scope": str(scope),
        # Campaign birth (2026-08-29): the self-echo boundary. A memory stored
        # AFTER this instant that resurfaces via recall is the campaign's own
        # output echoing back — it must not keep the dry-tail wet forever
        # (measured live: [1,0,0,1,...] on a self-referential topic).
        "created_at": time.time(),
        "rounds": [],
        "findings": {},
        "coverage": {lens: {"visits": 0, "max_depth": None, "truncated_total": 0}
                     for lens in ALL_LENSES},
        "questions": [],
        "verdict": None,
    }


def save_ledger(ledger: dict[str, Any], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(ledger, indent=1, ensure_ascii=False, default=str),
                   encoding="utf-8")
    tmp.replace(path)


def ledger_created_at(ledger: dict[str, Any]) -> float | None:
    """Campaign birth epoch — the field on new ledgers, derived from round 1's
    timestamp on legacy ones, ``None`` when neither exists. A ``None`` start
    NEVER suppresses anything: no proof, no echo (Lei L2 — absent signal is
    unknown, not zero)."""
    if isinstance(ledger.get("created_at"), (int, float)):
        return float(ledger["created_at"])
    rounds = ledger.get("rounds") or []
    at = rounds[0].get("at") if rounds and isinstance(rounds[0], dict) else None
    if isinstance(at, str):
        try:
            return datetime.strptime(at, "%Y-%m-%dT%H:%M:%S%z").timestamp()
        except ValueError:
            return None
    return None


def stored_epoch(value: Any) -> float | None:
    """Epoch of a recall entry's ``stored_at`` — INTEGER epoch or the SQLite
    TEXT shape ``YYYY-MM-DD HH:MM:SS`` (UTC). Unparseable → ``None`` (and the
    finding then counts as a normal discovery — never suppressed on doubt)."""
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return (datetime.strptime(value, "%Y-%m-%d %H:%M:%S")
                    .replace(tzinfo=timezone.utc).timestamp())
        except ValueError:
            return None
    return None


# === lens machinery ========================================================

@dataclass
class LensSweep:
    """One lens pass: findings found + coverage bookkeeping."""

    lens: str
    findings: list[dict[str, Any]] = field(default_factory=list)
    truncated: int = 0                 # items elided by TOP_HITS_PER_LENS heads
    degraded: bool = False
    notes: list[str] = field(default_factory=list)

    def add(self, kind: str, key: str, evidence: str, detail: str = "",
            depth: str = "D0") -> None:
        self.findings.append({
            "id": det_id("f", self.lens, kind, key),
            "lens": self.lens, "kind": kind, "key": key,
            "evidence": evidence, "detail": detail[:400], "depth": depth,
        })

    def take_head(self, items: list, label: str) -> list:
        """Head a list, ACCOUNTING the elision (the head -100 lesson)."""
        if len(items) > TOP_HITS_PER_LENS:
            self.truncated += len(items) - TOP_HITS_PER_LENS
            self.notes.append(f"{label}: {len(items) - TOP_HITS_PER_LENS} elided "
                              f"of {len(items)}")
        return items[:TOP_HITS_PER_LENS]


Runner = Callable[..., Any]            # touring_run-compatible (injectable in tests)


def lens_lexical(topic: str, scope: Path, run: Runner, timeout: float) -> LensSweep:
    sweep = LensSweep("lexical")
    res = run(["index", "find", topic, "-j"], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    for d in sweep.take_head(parse_definitions(res.parsed), "index find"):
        key = f"{d.get('file_path', '?')}:{d.get('line_number', d.get('line', '?'))}"
        sweep.add("definition", key, f"touring index find {topic}",
                  str(d.get("symbol_name", d.get("name", ""))))
    res = run(["tantivy", "search", topic, "-j"], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    hits = res.parsed if isinstance(res.parsed, list) else \
        (res.parsed or {}).get("hits", (res.parsed or {}).get("results", [])) \
        if isinstance(res.parsed, dict) else []
    for h in sweep.take_head([h for h in hits if isinstance(h, dict)], "tantivy"):
        key = str(h.get("file_path", h.get("path", h.get("doc", "?"))))
        sweep.add("bm25-hit", key, f"touring tantivy search {topic}",
                  str(h.get("symbol_name", h.get("snippet", "")))[:120])
    return sweep


def lens_structural(topic: str, scope: Path, run: Runner, timeout: float) -> LensSweep:
    sweep = LensSweep("structural")
    res = run(["ast", "find", topic, "-j"], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    for d in sweep.take_head(parse_definitions(res.parsed), "ast find"):
        key = f"{d.get('file_path', '?')}::{d.get('symbol_name', d.get('name', topic))}"
        sweep.add("signature", key, f"touring ast find {topic}",
                  str(d.get("signature", ""))[:150], depth="D1")
    res = run(["wiring", "impact", topic, "--depth", "2", "-j"], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    consumers = (res.parsed or {}).get("consumers", []) if isinstance(res.parsed, dict) else []
    for c in sweep.take_head([c for c in consumers if isinstance(c, dict)], "impact"):
        key = str(c.get("file_path", c.get("module", c.get("symbol", "?"))))
        sweep.add("consumer", key, f"touring wiring impact {topic} --depth 2",
                  str(c.get("symbol", "")))
    return sweep


def lens_institutional(topic: str, scope: Path, run: Runner, timeout: float) -> LensSweep:
    sweep = LensSweep("institutional")
    res = run(["memory", "recall", topic], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    entries = (res.parsed or {}).get("entries", []) if isinstance(res.parsed, dict) else []
    strong = [e for e in entries if isinstance(e, dict)
              and float(e.get("score", 0.0)) >= MEMORY_SCORE_FLOOR]
    for e in sweep.take_head(strong, "memory recall"):
        sweep.add("memory", str(e.get("key", "?")), f"touring memory recall {topic}",
                  str(e.get("value", ""))[:200], depth="D1")
        # The lens reports the FACT (the entry's age); run_round applies the
        # POLICY (echo suppression). Absent stored_at travels as absent.
        if e.get("stored_at") is not None:
            sweep.findings[-1]["stored_at"] = e["stored_at"]
    if not strong and entries:
        sweep.notes.append(f"memory: {len(entries)} entries all below "
                           f"score {MEMORY_SCORE_FLOOR} (noise floor)")
    return sweep


def lens_antistaleness(topic: str, scope: Path, run: Runner, timeout: float) -> LensSweep:
    """VP-Scout Chain 7 / Cadeia 4b: raw grep across the scope, index bypassed."""
    sweep = LensSweep("antistaleness")
    token = max(topic.split(), key=len) if topic.split() else topic
    hits = grep_fallback(token, scope, max_hits=TOP_HITS_PER_LENS * 3)
    for line in sweep.take_head(hits, "grep"):
        key = line.split(":", 2)
        sweep.add("grep-hit", ":".join(key[:2]) if len(key) >= 2 else line,
                  f"rg '\\b{token}\\b' {scope}", (key[2] if len(key) > 2 else "")[:120])
    return sweep


def lens_portfolio(topic: str, scope: Path, run: Runner, timeout: float) -> LensSweep:
    """Prior-art by PURPOSE — what already solves this intent (cross-project).

    The other lenses are keyed by symbol name; this one is keyed by the purpose
    prose artifacts carry (docstrings, `//!` headers, SKILL.md frontmatter). It
    is the lens that answers "has someone already built this?" before anything
    new is written, and it spans every project, not just the current scope.

    Gaps are recorded as findings on purpose: "nothing covers X" is exactly the
    kind of endogenous target that should keep the exploration from drying.
    """
    sweep = LensSweep("portfolio")
    res = run(["portfolio", topic, "-j"], timeout=timeout)
    sweep.degraded |= res.daemon_degraded
    ans = res.parsed if isinstance(res.parsed, dict) else {}
    hits = [h for h in ans.get("prior_art", []) if isinstance(h, dict)]
    for h in sweep.take_head(hits, "portfolio prior-art"):
        entry = h.get("entry", {}) if isinstance(h.get("entry"), dict) else {}
        key = str(entry.get("display_path", entry.get("id", "?")))
        sweep.add("prior-art", key, f"touring portfolio {topic}",
                  str(entry.get("purpose", ""))[:200], depth="D1")
    for gap in ans.get("gaps", [])[:TOP_HITS_PER_LENS]:
        sweep.add("gap", str(gap)[:120], f"touring portfolio {topic}", str(gap))
    corpus = ans.get("corpus_size")
    if corpus == 0:
        sweep.notes.append("portfolio vazio — rode `touring portfolio refresh`")
    elif not hits:
        sweep.notes.append(f"portfolio: 0 candidatos em {corpus} artefatos indexados")
    for lens_ref in ans.get("external", [])[:3]:
        if isinstance(lens_ref, dict):
            sweep.notes.append(
                f"lente externa sugerida: [{lens_ref.get('source')}] "
                f"{lens_ref.get('subject')} — {lens_ref.get('question')}")
    return sweep


def lens_quality(topic: str, scope: Path, run: Runner, timeout: float,
                 ledger: dict[str, Any] | None = None) -> LensSweep:
    """Open ast-meta on the top distinct files other lenses surfaced (D1)."""
    sweep = LensSweep("quality")
    files: list[str] = []
    for f in (ledger or {}).get("findings", {}).values():
        path = str(f.get("key", "")).split(":", 1)[0].split("::", 1)[0]
        if path.endswith((".rs", ".py", ".ts", ".go")) and path not in files:
            files.append(path)
    for path in sweep.take_head(files, "ast meta targets"):
        res = run(["ast", "meta", path, "--depth", "summary", "-j"], timeout=timeout)
        sweep.degraded |= res.daemon_degraded
        meta = res.parsed if isinstance(res.parsed, dict) else {}
        blast = meta.get("blast_radius")
        quality = meta.get("quality_score")
        sweep.add("file-meta", path, f"touring ast meta {path}",
                  f"blast={blast} quality={quality}", depth="D1")
    return sweep


LENS_FNS: dict[str, Callable[..., LensSweep]] = {
    "portfolio": lens_portfolio,
    "lexical": lens_lexical,
    "structural": lens_structural,
    "institutional": lens_institutional,
    "antistaleness": lens_antistaleness,
    "quality": lens_quality,
}


# === round + convergence ===================================================

def run_round(ledger: dict[str, Any], scope: Path, run: Runner,
              timeout: float) -> dict[str, Any]:
    """One full sweep of every automated lens; dedupe against the ENTIRE ledger."""
    topic = ledger["topic"]
    round_no = len(ledger["rounds"]) + 1
    seen = set(ledger["findings"].keys())
    campaign_start = ledger_created_at(ledger)
    new_ids: list[str] = []
    degraded = False
    lens_stats: dict[str, dict[str, int]] = {}
    for lens in AUTOMATED_LENSES:
        fn = LENS_FNS[lens]
        sweep = (fn(topic, scope, run, timeout, ledger=ledger)
                 if lens == "quality" else fn(topic, scope, run, timeout))
        degraded |= sweep.degraded
        fresh = 0
        echoes = 0
        for f in sweep.findings:
            if f["id"] in seen:
                continue
            f["round"] = round_no
            # Self-echo suppression (2026-08-29): a memory stored AFTER the
            # campaign began is the campaign's own output echoing back — it
            # enters the ledger VISIBLE (echo: true) but never wets the
            # dry-tail, or a self-referential topic loops [1,0,0,1,…] forever.
            # Suppression requires POSITIVE temporal proof; any missing
            # timestamp counts the finding normally.
            born = stored_epoch(f.get("stored_at"))
            if campaign_start is not None and born is not None and born > campaign_start:
                f["echo"] = True
                ledger["findings"][f["id"]] = f
                seen.add(f["id"])
                echoes += 1
                continue
            ledger["findings"][f["id"]] = f
            seen.add(f["id"])
            new_ids.append(f["id"])
            fresh += 1
        cov = ledger["coverage"][lens]
        cov["visits"] += 1
        cov["truncated_total"] += sweep.truncated
        depths = [f["depth"] for f in sweep.findings] or ["D0"]
        best = max(depths + ([cov["max_depth"]] if cov["max_depth"] else []))
        cov["max_depth"] = best
        lens_stats[lens] = {"found": len(sweep.findings), "new": fresh,
                            "truncated": sweep.truncated}
        if echoes:
            lens_stats[lens]["echo"] = echoes
    promote_corroborated(ledger)
    entry = {"round": round_no, "new_findings": len(new_ids),
             "lens_stats": lens_stats, "degraded": degraded,
             "at": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
    ledger["rounds"].append(entry)
    return entry


def promote_corroborated(ledger: dict[str, Any]) -> int:
    """D2 is COMPUTED: a key surfaced by 2+ independent lenses is corroborated."""
    by_key: dict[str, set[str]] = {}
    for f in ledger["findings"].values():
        base = str(f["key"]).split(":", 1)[0]
        by_key.setdefault(base, set()).add(f["lens"])
    promoted = 0
    for f in ledger["findings"].values():
        base = str(f["key"]).split(":", 1)[0]
        if len(by_key.get(base, set())) >= 2 and f["depth"] != "D2":
            f["depth"] = "D2"
            promoted += 1
    return promoted


def _unmet_clauses(rounds: list, dry_rounds: int, dry_tail: bool,
                   manual_pending: list[str], open_qs: list[dict],
                   max_rounds: int | None = None) -> list[str]:
    unmet: list[str] = []
    if not dry_tail:
        last = rounds[-1]["new_findings"] if rounds else None
        # "ainda há o que achar" e "o teto me interrompeu" são causas OPOSTAS que produziam a
        # mesma frase — e a segunda se lê como a primeira, levando a encerrar uma exploração
        # que apenas bateu no cap. Medido em 07/08/2026: parada em 12 rodadas soava "incompleto";
        # elevando o teto, as rodadas 13-21 renderam mais 18 achados e SÓ ENTÃO secou.
        if max_rounds is not None and len(rounds) >= max_rounds:
            unmet.append(f"parou no TETO de {max_rounds} rodadas, não por secar "
                         f"(última rodada new={last}) — repita com --max-rounds maior")
        else:
            unmet.append(f"need {dry_rounds} consecutive dry rounds "
                         f"(last round new={last})")
    unmet += [f"lens '{m}' pending — mark visited/waived (--mark-lens)"
              for m in manual_pending]
    unmet += [f"open question {q['id']}: {q['text'][:60]}" for q in open_qs[:5]]
    return unmet


def convergence(ledger: dict[str, Any], dry_rounds: int,
                max_rounds: int | None = None) -> dict[str, Any]:
    """The CCE verdict — honest, conditioned, computed by code (Lei L2).

    ``max_rounds`` só entra no texto do ``unmet``: com ele o veredito distingue
    "não secou" de "bateu no teto", que são causas opostas com a mesma aparência.
    """
    rounds = ledger["rounds"]
    open_qs = [q for q in ledger["questions"] if q["status"] == "open"]
    manual_pending = [lens for lens in MANUAL_LENSES
                      if ledger["coverage"][lens].get("state") not in ("visited", "waived")]
    tail = rounds[-dry_rounds:] if len(rounds) >= dry_rounds else []
    dry_tail = bool(tail) and all(r["new_findings"] == 0 for r in tail)
    converged = dry_tail and not manual_pending and not open_qs
    unmet = _unmet_clauses(rounds, dry_rounds, dry_tail, manual_pending, open_qs, max_rounds)
    verdict = {
        "converged": converged,
        "statement": ("no new findings under current questions/lenses after "
                      f"{dry_rounds} dry rounds — NOT a claim of completeness"
                      if converged else "exploration incomplete"),
        "clauses": {
            "rounds_run": len(rounds),
            "dry_tail": dry_tail,
            "dry_rounds_required": dry_rounds,
            "manual_lenses_pending": manual_pending,
            "open_questions": len(open_qs),
            "degraded_rounds": sum(1 for r in rounds if r.get("degraded")),
            # E4: suppression is DECLARED, never silent — how many findings
            # were recorded as the campaign's own echo and kept off the dry-tail.
            "echoes_suppressed": sum(1 for f in ledger["findings"].values()
                                     if f.get("echo")),
        },
        "unmet": unmet,
        "next_action": (None if converged else
                        (unmet[0] if unmet else "run another round")),
        "at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    }
    ledger["verdict"] = verdict
    return verdict


# === questions + manual lens marks =========================================

def add_question(ledger: dict[str, Any], text: str, origin: str = "cli") -> str:
    qid = det_id("q", text)
    if not any(q["id"] == qid for q in ledger["questions"]):
        ledger["questions"].append({"id": qid, "text": text, "status": "open",
                                    "origin": origin,
                                    "round": len(ledger["rounds"])})
    return qid


def answer_question(ledger: dict[str, Any], qid: str, note: str = "") -> bool:
    for q in ledger["questions"]:
        if q["id"] == qid:
            q["status"] = "answered"
            if note:
                q["answer"] = note
            return True
    return False


def mark_lens(ledger: dict[str, Any], spec: str, note: str = "") -> bool:
    """--mark-lens external:visited|waived — manual cells need explicit closure."""
    lens, _, state = spec.partition(":")
    if lens not in ALL_LENSES or state not in ("visited", "waived"):
        return False
    cell = ledger["coverage"][lens]
    cell["state"] = state
    if note:
        cell["note"] = note
    cell["visits"] = cell.get("visits", 0) + 1
    return True


def ingest_critic(ledger: dict[str, Any], path: Path) -> dict[str, int]:
    """Ingest a fresh-eyes critic report: {findings:[{lens,kind,key,evidence}...],
    questions:[str...]}. Critic findings land as round-less entries; new ones
    reset dryness implicitly (they raise the bar for the next round)."""
    data = json.loads(path.read_text(encoding="utf-8"))
    added_f = added_q = 0
    for f in data.get("findings", []):
        fid = det_id("f", f.get("lens", "critic"), f.get("kind", "item"),
                     str(f.get("key", "")))
        if fid not in ledger["findings"]:
            ledger["findings"][fid] = {**f, "id": fid, "depth": f.get("depth", "D1"),
                                       "round": len(ledger["rounds"]),
                                       "lens": f.get("lens", "critic")}
            added_f += 1
    for q in data.get("questions", []):
        before = len(ledger["questions"])
        add_question(ledger, str(q), origin="critic")
        added_q += len(ledger["questions"]) - before
    if added_f:
        ledger["rounds"].append({"round": len(ledger["rounds"]) + 1,
                                 "new_findings": added_f,
                                 "lens_stats": {"critic": {"found": added_f,
                                                           "new": added_f,
                                                           "truncated": 0}},
                                 "degraded": False, "critic": True,
                                 "at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    return {"findings": added_f, "questions": added_q}


# === reporting =============================================================

def human_report(ledger: dict[str, Any], verdict: dict[str, Any]) -> None:
    emit_section(f"explore --until-dry — {ledger['topic']}")
    emit_kv("scope", ledger["scope"])
    emit_kv("findings", len(ledger["findings"]))
    emit_kv("rounds", len(ledger["rounds"]))
    if ledger["rounds"]:
        curve = " → ".join(str(r["new_findings"]) for r in ledger["rounds"])
        emit_kv("new-per-round", curve)
    emit_section("coverage (lens × depth × truncation)", char="-")
    rows = []
    for lens in ALL_LENSES:
        c = ledger["coverage"][lens]
        state = c.get("state", "auto" if lens in AUTOMATED_LENSES else "PENDING")
        rows.append([lens, c.get("visits", 0), c.get("max_depth") or "-",
                     c.get("truncated_total", 0), state])
    emit_table(rows, headers=["lens", "visits", "depth", "elided", "state"])
    open_qs = [q for q in ledger["questions"] if q["status"] == "open"]
    if open_qs:
        emit_section(f"open questions ({len(open_qs)})", char="-")
        for q in open_qs:
            print(f"  {q['id']}  {q['text'][:90]}")
    emit_section("verdict", char="-")
    emit_kv("converged", verdict["converged"])
    emit_kv("statement", verdict["statement"])
    for u in verdict["unmet"]:
        print(f"  ✗ {u}")


def lens_yield_totals(ledger: dict[str, Any], rounds_before: int) -> dict[str, Any]:
    """Per-lens yield for the payload: lifetime findings + new in the fresh
    rounds (N2, 2026-09-23). The dry-lens-cut hypothesis died on data nobody
    could see — 174 ledgers showed cutting a lens with 2 dry rounds saves 1
    round total — so the yield stops being private to the ledger file and
    travels with every answer."""
    findings: dict[str, int] = {}
    for f in ledger["findings"].values():
        lens = f.get("lens", "?")
        findings[lens] = findings.get(lens, 0) + 1
    new_this_run: dict[str, int] = {}
    for r in ledger["rounds"][rounds_before:]:
        for lens, st in (r.get("lens_stats") or {}).items():
            if isinstance(st, dict):
                new_this_run[lens] = new_this_run.get(lens, 0) + (st.get("new") or 0)
    return {"findings": findings, "new_this_run": new_this_run}


def emit_dry_signal(ledger: dict[str, Any], rounds_before: int) -> None:
    """Emit the ADW loop-node dryness signal (``adw.py`` Law L2 protocol).

    The RUNNER owns loop termination, so the producer — never the agent — must
    state how many findings a round actually added. Three deliberate choices:

    * **stderr**, so ``--json`` stdout stays strict JSON for programmatic callers
      while the ADW specs (which all merge with ``2>&1``) still see it;
    * **last line**, so a ``tail -c <n>`` in the spec's command cannot eat it;
    * **silence when no round ran** (``--status``, or a spent round budget) —
      absence means *unknown*, and ``adw.py`` treats unknown as NOT dry
      (fail-closed). Emitting ``0`` here would let a read-only status call
      masquerade as a dry round and terminate the loop.
    """
    fresh = ledger["rounds"][rounds_before:]
    if not fresh:
        return
    print(f"NEW_FINDINGS={sum(r['new_findings'] for r in fresh)}", file=sys.stderr)


# === main ==================================================================

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("topic", help="Symbol, feature, or concept to explore")
    p.add_argument("--scope", default=".", help="Directory scope (default: cwd)")
    p.add_argument("--ledger", default=None, help="Explicit ledger path")
    p.add_argument("--rounds", type=int, default=1,
                   help="How many NEW rounds to run this invocation (default 1)")
    p.add_argument("--until-dry", action="store_true",
                   help="Keep running rounds until the convergence contract holds "
                        "or --max-rounds is hit")
    p.add_argument("--dry-rounds", type=int, default=DEFAULT_DRY_ROUNDS,
                   help=f"Consecutive dry rounds required (default {DEFAULT_DRY_ROUNDS})")
    p.add_argument("--max-rounds", type=int, default=DEFAULT_MAX_ROUNDS,
                   help=f"Hard cap on total rounds (default {DEFAULT_MAX_ROUNDS})")
    p.add_argument("--status", action="store_true",
                   help="Report current ledger + verdict; run NO new round")
    p.add_argument("--question", action="append", default=[],
                   help="Add an open question (endogenous target); repeatable")
    p.add_argument("--answer", default=None, help="Mark a question id answered")
    p.add_argument("--mark-lens", default=None,
                   help="Close a manual lens cell: '<lens>:visited' or '<lens>:waived'")
    p.add_argument("--note", default="", help="Note for --answer / --mark-lens")
    p.add_argument("--critic-report", default=None,
                   help="Ingest a fresh-eyes critic JSON {findings:[],questions:[]}")
    add_common_args(p)
    return p


def apply_mutations(ledger: dict[str, Any], args) -> tuple[bool, str | None]:
    """Apply --question/--answer/--mark-lens/--critic-report. Returns
    (mutated, error) — error is a message when an argument was invalid."""
    mutated = False
    for qtext in args.question:
        add_question(ledger, qtext)
        mutated = True
    if args.answer:
        if not answer_question(ledger, args.answer, args.note):
            return mutated, f"question not found: {args.answer}"
        mutated = True
    if args.mark_lens:
        if not mark_lens(ledger, args.mark_lens, args.note):
            return mutated, (f"bad --mark-lens '{args.mark_lens}' "
                             f"(lens ∈ {ALL_LENSES}, state ∈ visited|waived)")
        mutated = True
    if args.critic_report:
        stats = ingest_critic(ledger, Path(args.critic_report).expanduser())
        if not args.quiet:
            emit_kv("critic ingested", stats)
        mutated = True
    return mutated, None


def execute_rounds(ledger: dict[str, Any], scope: Path, args,
                   run: Runner) -> tuple[bool, bool]:
    """Run the requested rounds. Returns (mutated, degraded_any)."""
    degraded_any = False
    mutated = False
    budget = (args.max_rounds - len(ledger["rounds"])) if args.until_dry \
        else min(args.rounds, args.max_rounds - len(ledger["rounds"]))
    while budget > 0:
        entry = run_round(ledger, scope, run, args.timeout)
        degraded_any |= entry["degraded"]
        mutated = True
        budget -= 1
        if args.until_dry and convergence(ledger, args.dry_rounds)["converged"]:
            break
    return mutated, degraded_any


def main(argv: list[str] | None = None, run: Runner = touring_run) -> int:
    args = build_parser().parse_args(argv)
    scope = Path(args.scope).expanduser().resolve()
    if not scope.exists():
        emit_error(f"scope not found: {scope}")
        return 2
    lpath = ledger_path_for(args.topic, scope, args.ledger)
    ledger = load_ledger(lpath, args.topic, scope)

    mutated, err = apply_mutations(ledger, args)
    if err:
        emit_error(err)
        return 2

    degraded_any = False
    rounds_before = len(ledger["rounds"])
    if not args.status:
        ran, degraded_any = execute_rounds(ledger, scope, args, run)
        mutated = mutated or ran

    verdict = convergence(ledger, args.dry_rounds, args.max_rounds)
    if mutated:
        save_ledger(ledger, lpath)

    payload = mark_degraded({
        "topic": ledger["topic"], "ledger": str(lpath),
        "findings": len(ledger["findings"]),
        "rounds": [{k: r[k] for k in ("round", "new_findings", "degraded")}
                   for r in ledger["rounds"]],
        "coverage": ledger["coverage"],
        "open_questions": [q for q in ledger["questions"] if q["status"] == "open"],
        "lens_yield": lens_yield_totals(ledger, rounds_before),
        "verdict": verdict,
    }, degraded_any, "daemon degraded during ≥1 lens — grep fallback used")
    if not emit_result(payload, args):
        human_report(ledger, verdict)
    emit_dry_signal(ledger, rounds_before)
    if degraded_any:
        return 3
    return 0 if verdict["converged"] else 1


if __name__ == "__main__":
    sys.exit(main())
