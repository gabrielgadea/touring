#!/usr/bin/env python3
"""loop_outer_arm.py — UserPromptSubmit hook: arm the OUTER-phase marker when a
gated flow is invoked.

Origin (2026-07-23, Gabriel): protocol steps in the loop-engineering OUTER phase
were skipped silently because nothing was armed until step 11 (post-human-gate).
Persuasion (skill instructions, cli-suggest nudges) demonstrably does not change
behavior — affordance and structural gates do (touring-4-pillars cont.¹⁰, thesis
①). This hook is the structural half: the moment the user invokes a gated flow,
a marker with status="outer" is written DETERMINISTICALLY — no orchestrator
discipline involved — and from then on loop_stop_guard.py verifies the flow's
artifact manifest (loop_outer_gate.py) before any turn is allowed to end.

Detection is textual on the submitted prompt (slash command or the
<command-name> tag Claude Code injects for skill invocations). Mapping:

  /loop-engineering , /goal        → flow "strategy-outer"
  /TACO-cross-audit                → flow "cross-audit"

Rules:
  * an existing REAL loop (marker with a DAG task, status "active") is never
    overwritten — arming only applies before a loop exists;
  * an existing outer marker for the same flow is refreshed (updated_at), its
    created_at is preserved so artifact mtime floors stay honest;
  * absolutely fail-open: any error → exit 0, no output, session unaffected.

Emits hookSpecificOutput.additionalContext so the orchestrator SEES the armed
contract (dense + specific, per the injection-density invariant).
"""
from __future__ import annotations

import datetime as _dt
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from loop_marker import active_marker, write_marker  # noqa: E402

# Pattern → flow key. ONLY genuine invocation forms match: a slash command
# anchored at the start of the prompt, or the <command-name> tag Claude Code
# injects for skill invocations. Prose mentions ("o skill loop-engineering
# ficou ótimo", "qual é o /goal disso?") must NEVER arm — the cross-audit of
# 2026-07-23 proved the earlier loose pattern (optional slash, any position)
# armed on both.
def _invocation(name: str) -> re.Pattern[str]:
    return re.compile(
        rf"(?:^\s*/{name}\b|<command-name>\s*/?{name}\s*</command-name>)",
        re.IGNORECASE,
    )


FLOW_PATTERNS = (
    (_invocation("loop-engineering"), "strategy-outer"),
    (_invocation("goal"), "strategy-outer"),
    (_invocation("TACO-cross-audit"), "cross-audit"),
)

# ── the DEFAULT flow (2026-08-02, Gabriel: "o OUTER determinístico já é um
# exemplo de um procedimento que deve ser padrão") ────────────────────────────
# The flows above arm only when a slash command is typed, so ordinary
# engineering work — the overwhelming majority of turns — ran with NO
# enforcement at all. The KPI shows the gap directly: compliance.jsonl holds
# records exclusively for invoked flows (strategy-outer 67/97, cross-audit
# 59/62), and none for the default path, because nothing was ever armed there.
#
# `work-outer` closes it with a deliberately SMALLER contract than
# `strategy-outer`: only the two deterministic OUTER artifacts (diagnostic +
# CCE ledger), no strategy doc — one `touring adw run strategy-loop` satisfies
# it. Cost of a false arm is bounded by `max_continuations` (2) Stop nags, each
# carrying its exact next_action, after which the guard releases anyway.
#
# Detection is a HEURISTIC over prompt text (there is no reliable pre-execution
# signal for "this is L2+ work"), so it is tuned for precision: an imperative
# change-verb AND enough substance that it cannot be a passing question.
# Kill switch: TOURING_WORK_OUTER_DISABLED=1.
DEFAULT_FLOW = "work-outer"
MIN_WORK_WORDS = 5

# Only IMPERATIVE / INFINITIVE forms — the grammatical mood of a command. Open
# stems (`audit\w*`) are wrong: they match the noun buried inside a hyphenated
# name, and `test_prose_never_arms` caught exactly that ("sobre o
# TACO-cross-audit conversamos amanhã" armed on the "audit" fragment). Hence the
# leading `(?<![\w-])`, which refuses a hyphen-joined fragment, while the
# trailing guard still allows the PT-BR enclitic pronoun ("certifique-se").
_WORK_VERB = re.compile(
    r"(?<![\w-])(?:"
    # EN imperative / infinitive. Deliberately NO `fix|audit|debug|build|add|
    # update|complete`: English does not inflect the imperative, so those are
    # spelled identically to the noun and this is a PT-BR-primary environment
    # where they appear as technical nouns ("o fix do audit ficou bom" armed on
    # them — caught by test_default_flow_stays_out_of_conversation). What
    # remains cannot be read as a noun phrase.
    r"implement|refactor|migrate|optimi[sz]e|integrate|rewrite|redesign|harden|"
    r"create|apply|validate|improve|ensure|"
    # PT-BR imperative + infinitive (+ 1st-person plural), e.g. refatore /
    # refatorar / refatoremos
    r"implement(?:e|ar|emos)|refator(?:e|ar|emos)|migr(?:e|ar|emos)|"
    r"otimiz(?:e|ar|emos)|integr(?:e|ar|emos)|reescrev(?:a|er|amos)|"
    r"corrij(?:a|amos)|corrigir|consert(?:e|ar|emos)|depur(?:e|ar|emos)|"
    r"audit(?:e|ar|emos)|padroniz(?:e|ar|emos)|unific(?:e|ar|emos)|"
    r"consolid(?:e|ar|emos)|resolv(?:a|er|amos)|conclu(?:a|ir|amos)|"
    r"garant(?:a|ir|amos)|certifique|revis(?:e|ar|emos)|"
    r"faç(?:a|amos)|fazer|cri(?:e|ar|emos)|adicion(?:e|ar|emos)|"
    r"ajust(?:e|ar|emos)|atualiz(?:e|ar|emos)|apliqu(?:e|emos)|aplicar|"
    r"melhor(?:e|ar|emos)|escrev(?:a|er|amos)|valid(?:e|ar|emos)|"
    r"remov(?:a|er|amos)|deix(?:e|ar|emos)"
    r")(?![\w])",
    re.IGNORECASE,
)


def is_default_work(prompt: str) -> bool:
    """True when the prompt reads as a substantive engineering request.

    Two conjunctive conditions, both needed: a change-imperative (so "explique
    o que faz X" never arms) and ``MIN_WORK_WORDS`` of substance (so "fix isso"
    stays out of the gate). Errs toward NOT arming — a missed arm costs the
    session nothing, a spurious one costs up to two nags.
    """
    text = prompt or ""
    if os.environ.get("TOURING_WORK_OUTER_DISABLED") == "1":
        return False
    if len(text.split()) < MIN_WORK_WORDS:
        return False
    return bool(_WORK_VERB.search(text))


def detect_flow(prompt: str):
    """Return the flow key the prompt EXPLICITLY invokes, or None.

    Opção D (Gabriel, 03/09/2026) retired the text-heuristic arm. `is_default_work`
    above is kept — it still documents the old contract and its tests still pin the
    behaviour — but `work-outer` is no longer armed from prompt wording. Measured on
    the session that made this change, the heuristic scored 0/2 on the turns that
    mattered:

        "estou achando que a estratégia de escrever um .md…"  (a QUESTION)  → armed
        "Execute D5 e D6 …"       (the turn that wrote ~10 files)           → silent

    Both errors are structural, not tuning: a prompt is a CLAIM about the work, and no
    word list reads intent reliably. `work-outer` is now armed by `loop_outer_effect`
    from the MEASURED blast radius of the file about to change, via `touring route`
    (see loop_task_signal). What stays here is what a human typed on purpose — an
    explicit `/loop-engineering`, `/goal` or `/TACO-cross-audit` is a statement of
    intent, not an inference from it, and those flows are exactly the ones the ledger
    shows working (cross-audit: 3.1% incomplete against 55% for the inferred ones).
    """
    for pattern, flow in FLOW_PATTERNS:
        if pattern.search(prompt or ""):
            return flow
    return None


def default_bundle(cwd: str) -> str:
    """Stable per-DAY OKF bundle for the default flow.

    Per day, never per prompt: a task spanning several turns must accumulate its
    evidence in ONE bundle, otherwise every follow-up prompt would demand a fresh
    diagnostic and the gate would never be satisfiable. Lands under ``docs/plans``
    when the project keeps docs there, else under ``.touring/plans``.
    """
    root = Path(cwd)
    parent = root / "docs" / "plans" if (root / "docs").is_dir() else root / ".touring" / "plans"
    return str(parent / f"{_dt.date.today().isoformat()}-work-outer")


def topic_from_prompt(prompt: str, max_words: int = 12) -> str:
    """A short, stable topic string for the background explore + the effect nudge.

    The ledger is keyed by topic (`.touring-explore/<slug>.ledger.json`), so this must
    be derived deterministically from the prompt and stay short enough that follow-up
    turns on the same subject reuse the SAME ledger instead of spawning a new one.
    """
    words = re.sub(r"[^\w\s-]", " ", prompt or "").split()
    return " ".join(words[:max_words]).strip().lower() or "trabalho do turno"


def spawn_outer_artifacts(cwd: str, bundle: str, topic: str) -> None:
    """Opção C (Gabriel, 03/09/2026): the EXECUTOR pays for the artifact, not the model.

    The OUTER's two deterministic artifacts cost ~30s of wall clock and ZERO reasoning
    (measured 03/09: loop_diagnose 26.4s, explore --until-dry 3.2s). Making the model
    produce them mid-turn cost a context window and, worse, produced them at the WRONG
    MOMENT — first action of the turn in only 11% of cases, median 5 actions already
    taken. Detached and started at arm time, they are simply on disk by the time the
    turn ends, informing nothing less than before and interrupting nothing.

    Deliberately NOT marked `--mark-lens external:waived`: the external lens is the
    one that asks whether an outside source was consulted, and no machine may answer
    that on a human's behalf. A ledger that ends unconverged with `external` pending is
    the HONEST state ("the automatic lenses ran dry; nobody consulted an outside
    source"), and since the Stop gate no longer blocks, that honesty costs nothing.
    """
    log = Path(bundle) / "outer-executor.log"
    script = (
        f"python3 {Path(__file__).resolve().parent.parent / 'loop_diagnose.py'} "
        f"--scope {shlex.quote(cwd)} --bundle {shlex.quote(bundle)} --json ; "
        f"touring explore {shlex.quote(topic)} --scope {shlex.quote(cwd)} "
        f"--until-dry --max-rounds 12"
    )
    try:
        Path(bundle).mkdir(parents=True, exist_ok=True)
        with open(log, "a") as fh:
            subprocess.Popen(
                ["bash", "-lc", script], stdout=fh, stderr=fh, stdin=subprocess.DEVNULL,
                start_new_session=True, cwd=cwd,
            )
    except Exception:  # noqa: BLE001 — fail-open: the artifact is a nice-to-have now,
        pass           # never a precondition for the turn.


def arm(cwd: str, flow: str, session_id=None, topic=None):
    """Write/refresh the outer marker unless a real loop is already active.

    ``session_id`` comes from the hook payload — the authoritative identity —
    so the marker this session writes is the one ITS Stop hook will later find
    (which resolves the same id from the environment), and no concurrent session
    on the same project can see, satisfy or archive it.
    """
    _, existing = active_marker(cwd, session_id)
    if existing and existing.get("status") == "active" and existing.get("task") not in (None, "", "OUTER"):
        return None  # a live loop owns this project — never clobber it
    # Never DOWNGRADE. An explicitly-invoked flow carries a stricter manifest
    # than the default one, so an ordinary work prompt arriving mid-flow must not
    # replace `strategy-outer`/`cross-audit` with `work-outer` and silently drop
    # artifacts the session already owes (here, the strategy doc).
    if (flow == DEFAULT_FLOW and existing
            and existing.get("status") == "outer"
            and existing.get("flow") not in (None, DEFAULT_FLOW)
            and not existing.get("outer_complete")):
        return None
    bundle = (existing or {}).get("bundle")
    if not bundle and flow == DEFAULT_FLOW:
        bundle = default_bundle(cwd)
    # `effect_pending` is the one-shot sentinel loop_outer_effect.py consumes: the
    # EFFECT half fires at most once per armed cycle, on the turn's first mutating
    # action. Re-armed here on every arm, cleared there on delivery.
    return write_marker(task="OUTER", scope=cwd, bundle=bundle,
                        cwd=cwd, status="outer", flow=flow,
                        session_id=session_id,
                        effect_pending=True,
                        topic=topic or (existing or {}).get("topic"))


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    prompt = str(payload.get("prompt") or "")
    cwd = str(payload.get("cwd") or "") or None
    # A payload without cwd is malformed — NEVER fall back to the process
    # environment (CLAUDE_PROJECT_DIR/getcwd): during the 2026-07-23 cross-audit
    # that fallback silently re-armed the REAL project's marker from a test
    # payload (finding F5). No cwd → no arming, fail-safe.
    if not cwd:
        return 0
    session_id = str(payload.get("session_id") or "") or None
    flow = detect_flow(prompt)
    if not flow:
        return 0
    topic = topic_from_prompt(prompt)
    try:
        marker = arm(cwd, flow, session_id, topic=topic)
    except Exception:  # noqa: BLE001 — fail-open
        return 0
    if marker is None:
        return 0
    _, armed = active_marker(cwd, session_id)
    bundle = (armed or {}).get("bundle") or "<plan bundle dir>"
    # The EXECUTOR pays for the artifact, starting now, detached. Nothing is owed by
    # the model and nothing is waited on — by the time the turn ends the diagnostic
    # and the CCE ledger are simply on disk.
    if bundle and bundle != "<plan bundle dir>":
        spawn_outer_artifacts(cwd, bundle, topic)
    context = (
        f"[OUTER · flow '{flow}' armado] Opção C (Gabriel, 03/09/2026): este flow NÃO "
        f"cobra documento nenhum de você. O diagnóstico determinístico e o ledger CCE "
        f"já foram DISPARADOS em background pelo executor (bundle: {bundle}; log: "
        f"{bundle}/outer-executor.log) e estarão em disco sem custo de contexto. "
        f"O Stop hook não bloqueia mais o OUTER — ele apenas registra a avaliação em "
        f"compliance.jsonl, como régua de KPI. "
        f"O que resta é o EFEITO: na PRIMEIRA edição de código deste turno "
        f"(Edit/Write), o hook loop_outer_effect nega uma única vez e entrega o "
        f"recall de memória e os gotchas JÁ MONTADOS — leia e refaça a mesma edição. "
        f"Tópico armado: '{topic}'. Kill switch humano: TOURING_WORK_OUTER_DISABLED=1."
    )
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit", "additionalContext": context}}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
