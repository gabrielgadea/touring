#!/usr/bin/env python3
"""loop_outer_effect.py — PreToolUse hook: the OUTER's EFFECT half.

Opção C (Gabriel, 03/09/2026). The OUTER phase used to be enforced by a single gate
on `Stop`, and that gate could not work: `Stop` only ever inspects the past, so the
only behaviour it can induce is a retroactive receipt. Measured over 936 real
evaluations (compliance.jsonl, 23/08–03/09/2026):

    55.3% of evaluations ended incomplete       23.4% of sessions never completed
    OUTER was the turn's FIRST action in 11%    median 5 actions taken before it

That last pair is the whole diagnosis: the OUTER was INTERRUPTING work instead of
preceding it — the one thing it exists to do. So the two purposes were split, each
getting the mechanism that can actually serve it:

    ARTIFACT  →  produced by the EXECUTOR (loop_outer_arm spawns diagnose+explore in
                 the background at arm time; costs the model zero context)
    EFFECT    →  induced HERE, on PreToolUse, by blocking the first MUTATING action
                 of the turn and handing the recall over ALREADY ASSEMBLED

This is the `grilling` shape, and the shape of every gate in this house that is
measured to work (the code-mode G1/G3/G10, `pre-edit`): it fires at the decision
point, it blocks the ADVANCE rather than the exit, and what it produces is INPUT to
the work rather than a receipt for it. Thesis ①: affordance changes U(a); persuasion
does not.

WHY `Edit|Write|NotebookEdit` AND NOT `Bash`
    The OUTER's purpose is "know the ground before you change it". Blocking Bash would
    fire during the very investigation the OUTER wants to encourage, and Bash is
    ambiguous (most invocations are read-only). Edit/Write are unambiguously the
    moment the turn stops looking and starts changing. One trigger, no false positive.

CONTRACT
    * fires AT MOST ONCE per armed cycle (`effect_pending` on the marker, set by
      loop_outer_arm and cleared here) — never a second interruption;
    * the reason carries the recall VERBATIM: the nudge delivers the payload, it does
      not order the model to go fetch it (lesson `nudge-entrega-o-programa`, Gabriel
      25/08/2026);
    * absolutely fail-open: any error, any timeout, any missing marker → exit 0 with
      no output, and the edit proceeds;
    * human kill switch: TOURING_WORK_OUTER_DISABLED=1.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import active_marker, save_marker, write_marker  # noqa: E402
from loop_task_signal import assess  # noqa: E402

MUTATING_TOOLS = {"Edit", "Write", "NotebookEdit", "MultiEdit"}
RECALL_TIMEOUT = 6
MAX_REASON_CHARS = 3000


def _payload():
    try:
        return json.load(sys.stdin)
    except Exception:  # noqa: BLE001 — fail-open
        return {}


def _run(cmd, timeout=RECALL_TIMEOUT):
    """Best-effort CLI call; '' on any failure (never raises, never blocks)."""
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return proc.stdout.strip()
    except Exception:  # noqa: BLE001 — fail-open
        return ""


def _recall(topic: str, limit: int = 3) -> list:
    """Top memories for the turn's topic, split by measured outcome.

    Two things about the real CLI, both learned by running it (03/09/2026) after the
    first live firing of this hook reported "no memory matched" for a topic that had
    two memories stored the same hour:

      * `touring memory recall` ALREADY emits JSON — there is no `-j`, and passing one
        makes it a QUERY WORD ("gate outer -j"), which silently matches nothing. A flag
        that degrades into a search term cannot fail loudly, so the hook looked like it
        worked while delivering an empty payload — and an empty payload makes this gate
        hollow, since the payload IS its entire value.
      * the envelope is `{"cases": {"positive": [...], "negative": [...],
        "unobserved": [...]}}`, and it ships its own `guidance`: positives are
        approaches to REUSE, negatives are patterns to AVOID, never guidance. Flattening
        them into one list would invert the meaning of half the payload, so they are
        labelled here.
    """
    raw = _run(["touring", "memory", "recall", topic])
    # None, not [] — absence has two causes and only one of them is "nothing there".
    # With a single empty return the message said "terreno inédito" whether the store
    # was genuinely empty or the daemon had timed out, which is the failure mode this
    # very session diagnosed elsewhere and then committed here (lesson
    # `ausencia-de-sinal-tem-duas-causas`). The caller must be able to tell a silent
    # instrument from a silent subject.
    if not raw:
        return None
    try:
        cases = (json.loads(raw) or {}).get("cases") or {}
    except Exception:  # noqa: BLE001
        return None
    out = []
    for bucket, mark in (("positive", "✓ reusar"), ("negative", "✗ evitar"),
                         ("unobserved", "· sem veredito")):
        kept = 0
        for it in (cases.get(bucket) or []):
            if kept >= limit:
                break
            if not isinstance(it, dict):
                continue
            key = str(it.get("key", "?"))
            # `outcome:*` entries are auto-recorded tool results — half the store, and
            # by the CLI's own documentation they "were the eight most-recalled entries
            # in it, crowding curated lessons out of every result set" (02/08/2026),
            # which is why `recall` hides them behind `--include-outcomes`. They still
            # reach us through `cases`, because a case IS an outcome by construction.
            # Observed live 03/09: 3 of 7 delivered lines were transcript failures with
            # no bearing on the edit at hand. A nudge diluted by noise stops being read.
            if key.startswith("outcome:"):
                continue
            val = str(it.get("value") or "").replace("\n", " ")
            out.append(f"  [{mark}] {key}: {val[:170]}")
            kept += 1
    return out


def _gotchas(file_path: str) -> list:
    """Known pitfalls for the file about to be edited. `gotcha match` is JSON by
    default too — same lesson as `_recall`, same missing `-j`."""
    if not file_path:
        return []  # genuinely nothing to ask about — not a failed lookup
    raw = _run(["touring", "gotcha", "match", file_path])
    if not raw:
        return None  # instrument silent — same three-state contract as _recall
    try:
        items = (json.loads(raw) or {}).get("matches") or []
    except Exception:  # noqa: BLE001
        return None
    return [f"  ⚠ {str(g.get('pattern') or g.get('name') or g)[:120]}" for g in items[:3]]


def _artifact_state(marker: dict) -> str:
    """What the background executor has produced FOR THIS CYCLE.

    Counted against `flow_armed_at`, the same mtime floor the artifact gate uses. The
    first live firing (03/09/2026) reported `explore-ledgers=152` — every ledger the
    project ever accumulated — which reads as "the OUTER is richly done" when in fact
    nothing had been produced for this cycle at all. A status line that cannot come
    back zero is not a status line.
    """
    floor = float(marker.get("flow_armed_at") or marker.get("created_at") or 0)

    def fresh(d: Path, pattern: str) -> int:
        if not d.is_dir():
            return 0
        return sum(1 for f in d.glob(pattern) if f.stat().st_mtime >= floor)

    bundle = marker.get("bundle")
    scope = marker.get("scope") or marker.get("cwd") or ""
    diags = fresh(Path(bundle) / "diagnostics", "*.md") if bundle else 0
    ledgers = fresh(Path(scope) / ".touring-explore", "*.ledger.json") if scope else 0
    if diags or ledgers:
        return f"diagnostics={diags} explore-ledgers={ledgers} (deste ciclo)"
    return ("nada ainda deste ciclo — o executor foi disparado no arm e roda em "
            "background (~30s); não espere por ele")


def build_reason(marker: dict, file_path: str) -> str:
    topic = str(marker.get("topic") or marker.get("task") or "").strip() or "current work"
    lines = [
        f"[OUTER · efeito] Primeira mudança deste turno. O recall já está feito — "
        f"leia, decida, e refaça a mesma edição (este gate não dispara de novo neste ciclo).",
        "",
        f"Tópico armado: {topic}",
    ]
    # Say WHY this fired. A gate that cannot justify itself in the same breath is
    # indistinguishable from noise, and the level is the calibration knob the human
    # tunes (TOURING_OUTER_MIN_LEVEL), so it belongs in the message rather than in a
    # log nobody reads.
    sig = marker.get("task_signal") or {}
    route, vec = sig.get("route") or {}, sig.get("vector") or {}
    if route:
        lines.append(
            f"Por que disparou: {Path(file_path).name} mede {route.get('level')} "
            f"({route.get('mode')}, composite {route.get('composite')}) — blast_radius="
            f"{vec.get('blast_radius')}, {vec.get('files')} consumidor(es), "
            f"{vec.get('symbols')} símbolo(s) públicos. Piso atual: L{sig.get('min_level')} "
            f"(ajuste com TOURING_OUTER_MIN_LEVEL).")
    lines.append(
        f"Artefatos determinísticos já produzidos em background: {_artifact_state(marker)}")
    mem = _recall(topic)
    got = _gotchas(file_path)
    if mem:
        lines += ["", "Memória relevante (touring memory recall, já executado):"] + mem
    if got:
        lines += ["", f"Gotchas registrados para {Path(file_path).name}:"] + got
    # Three states, not two. `None` means the INSTRUMENT was silent (daemon degraded,
    # timeout, unparseable JSON) and the reader must NOT conclude the ground is
    # untrodden; `[]` means the store answered and genuinely holds nothing. Reporting a
    # failed lookup as "terreno inédito" is a confident lie — the same defect shape
    # this session found in three other places today.
    if mem is None or got is None:
        lines += ["", f"⚠ O recall NÃO respondeu (daemon degradado ou timeout de "
                      f"{RECALL_TIMEOUT}s) — isto não é 'terreno inédito', é ausência de "
                      f"medida. Rode `touring memory recall \"{topic}\"` antes de assumir "
                      f"que não há precedente."]
    elif not mem and not got:
        lines += ["", "Nenhuma memória ou gotcha casou este tópico — o store respondeu e "
                      "está vazio para ele. Terreno inédito de fato: vale registrar a "
                      "lição no fim (touring memory store)."]
    lines += [
        "",
        "Isto NÃO pede documento nenhum. O OUTER agora se paga em disco pelo executor; "
        "o que se cobra aqui é só que você tenha visto o acima antes de mudar código.",
    ]
    return "\n".join(lines)[:MAX_REASON_CHARS]


def main() -> int:
    if os.environ.get("TOURING_WORK_OUTER_DISABLED") == "1":
        return 0
    payload = _payload()
    tool = str(payload.get("tool_name") or "")
    if tool not in MUTATING_TOOLS:
        return 0
    cwd = str(payload.get("cwd") or "") or None
    session_id = str(payload.get("session_id") or "") or None
    # No cwd → no action. NEVER fall back to the process environment: that fallback
    # let a test payload re-arm the real project's marker on 23/07/2026 (finding F5).
    if not cwd:
        return 0
    file_path = str((payload.get("tool_input") or {}).get("file_path") or "")
    try:
        path, marker = active_marker(cwd, session_id)
    except Exception:  # noqa: BLE001 — fail-open
        return 0

    # ── Opção D (Gabriel, 03/09/2026): arm on the MEASURED scope of the change ──
    #
    # With no marker, the old design fell back to a regex over the prompt text, and on
    # the session that produced this code that oracle scored 0/2 on the turns that
    # mattered: it armed on a QUESTION about the gate and stayed silent through the
    # turn that wrote ten files. The prompt is a claim about the work; the index knows
    # the work. `loop_task_signal` measures the real blast radius of the file about to
    # change and asks `touring route` — Touring's own CILA classifier, which shipped
    # with no consumers at all — for the level. Calibrated live:
    #
    #     markdown/scratch (blast 0) → L0   not armed
    #     cli/route.rs     (blast 1) → L1   not armed
    #     cli_suggester.rs (blast 19)→ L3   ARMED
    #     verifications/mod.rs (144) → L3   ARMED
    #
    # The gate now costs nothing on a scratch note and fires on a module with real
    # consumers, which is the whole point and is not expressible as a word list.
    if not marker or marker.get("status") != "outer":
        verdict = assess(file_path, cwd)
        if not verdict.get("arm"):
            return 0
        topic = Path(file_path).name or "mudança deste turno"
        try:
            from loop_outer_arm import default_bundle, spawn_outer_artifacts
            bundle = default_bundle(cwd)
            path = write_marker(task="OUTER", scope=cwd, bundle=bundle, cwd=cwd,
                                status="outer", session_id=session_id,
                                flow="work-outer", effect_pending=True,
                                topic=topic, task_signal=verdict)
            _, marker = active_marker(cwd, session_id)
            # The artifact half starts here too — detached, never waited on. Arming
            # moved to this hook, so the executor has to move with it or the OUTER
            # would produce the effect and no record at all.
            spawn_outer_artifacts(cwd, bundle, topic)
        except Exception:  # noqa: BLE001 — fail-open
            return 0
        if not marker:
            return 0

    if not marker.get("effect_pending"):
        return 0  # already delivered for this cycle — never interrupt twice
    try:
        reason = build_reason(marker, file_path)
    except Exception:  # noqa: BLE001 — fail-open
        return 0

    # Clear the sentinel BEFORE emitting: if anything downstream misbehaves, the worst
    # case is one missed nudge, never a turn that cannot edit. Fail-open is the
    # invariant this whole subsystem is held to.
    marker["effect_pending"] = False
    try:
        save_marker(path, marker)
    except Exception:  # noqa: BLE001
        return 0

    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": reason,
    }}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
