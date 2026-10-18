"""V0e — every hook/guard denial in the sessions: which gate, how often, how often the SAME session trips it again,
and whether the next action took the route the denial teaches (Gabriel, 08/10/2026: "precisamos corrigir tudo").

A denial is a tool_result with is_error whose text matches a positive gate predicate (inventario_erros.RESIDUE's
"hook/guard negou", or a python_pre_gate / Edit-hook denial). The gate id is extracted by ordered patterns; what no
pattern names stays "gate não identificado" — visible, never folded. "Rota seguida" = the next tool call of the same
transcript matches the route the gate names (burst → `touring run`; sed -i → Edit; edit-without-read → Read; …).

Usage: python3 v0e_negacoes.py [--since 2026-09-17]   — aggregates only (no command text) are printed.
"""

import json
import re
import sys
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from inventario_erros import ROOTS, SINCE, _text  # noqa: E402

# (gate id, pattern on the denial text, pattern the NEXT call must match to count as "rota seguida")
# Each route is the one the gate's OWN denial text names (read from real denials on 08/10/2026), never a guess.
GATES = [
    # S3 (2ª inspeção da rajada, CODE MODE): NEGAVA até a 30.4.95 e era contado como G1 pelo `RAJADA` abaixo.
    ("S3 rajada (CODE MODE, 2ª inspeção)", r"\[CODE MODE · rajada\]", r"touring run"),
    ("G1 rajada-de-inspeção", r"rajada-de-inspe|inspection burst|RAJADA", r"touring run"),
    ("G7 re-inspeção (3ª leitura)", r"re-inspeção|re-inspe", r"touring run"),
    ("exec-burst (python-inline seriado)", r"exec-burst|chamada seriada do executor", r"touring run"),
    ("G11 orçamento de bypass", r"orçamento de bypass", r"touring run"),
    ("escrita cega inline (sed -i/awk/perl)", r"escrita-cega|sed -i|G9", r"\"name\": \"(Edit|Write)\""),
    ("G3 edit sem Read recente", r"\bG3\b|edit-sem-read|without a Read", r"\"name\": \"Read\""),
    ("OUTER · efeito (1ª mudança do turno)", r"OUTER · efeito", r"\"name\": \"(Edit|Write|MultiEdit)\""),
    ("python_pre_gate (F82 etc.)", r"python_pre_gate", r"\"name\": \"(Edit|Write)\""),
    ("git (block_git / protocolo)", r"block_git|GIT_DESTRUCTIVE|git safety|Git reset discards", r"git "),
    ("rm guard", r"\brm -r?f\b|Force without confirmation|destructive recursive",
     r"\brm (?!-)|\brmdir\b|\"name\": \"AskUserQuestion\""),
    ("write→run (script escrito-e-executado)", r"write→run|escrito-e-executado", r"touring run"),
    ("redundant-exact-call (retry cego)", r"redundant-exact-call|repetição byte-idêntica", r"\"name\": \"(Read|Edit|Write)\"|touring run"),
    ("guard de segurança sem rota (dd, pipe→shell, --force, Windows delete)",
     r"Windows delete command|dd low-level disk|Pipe to shell|Override safety checks", r"(?!x)x"),
    ("process guard (REGRA #19)", r"process-guard|pkill|REGRA #19|daemon-ctl", r"daemon-ctl"),
    ("gate de prosa (CJK)", r"prose[_-]integrity|integridade de prosa|CJK", r"\"name\": \"(Edit|Write)\""),
    ("agent_model_gate", r"agent_model_gate|subagente sem `?model", r"\"model\""),
    ("touring-quality-block-all", r"touring-quality|F2\.[1-6]|F4\.[35]", r"\"name\": \"(Edit|Write)\""),
    # the 41 "não identificado" of 08/10/2026, each by a positive predicate (the denial text names its source):
    ("guard de rm (redação «Recursive deletion: flag»)", r"hook error: Recursive deletion: flag", r"rmdir|\brm [^-]"),
    ("guard de chmod (remove todas as permissões)", r"Removes all permissions from file", r"chmod"),
    ("gate de preservação de sintaxe (parses today)", r"parses today and would not after this edit",
     r"\"name\": \"(Edit|Write)\""),
    # NOT a Touring hook: Claude Code's own permission layer answers unprefixed, at the very start of the result
    ("permissão do Claude Code (não é hook do Touring)",
     r"\A(Force operation|This command requires approval|Local branch deletion|Recursive deletion)", r"(?!)"),
]
COMPILED = [(g, re.compile(p, re.I), re.compile(r, re.I)) for g, p, r in GATES]
WINDOW_S = 300  # a re-trip of the same gate within this window counts as reincidence


# The gate's own announcement: `[G10 exec-burst]`, `[G1 rajada-de-inspeção]`, `[CODE MODE · rajada]`.
TAG = re.compile(r"\[(?:G\d+[a-z]?|CODE MODE)\b[^\]]*\]")


def gate_of(text: str) -> tuple[str, re.Pattern | None]:
    """By the TAG first (the source announces itself), then by free text — a free-text word of one gate inside
    another gate's message ("a rajada desenrolada" in the exec-burst) never steals the denial."""
    tag = TAG.search(text)
    for alvo in ([tag.group(0)] if tag else []) + [text]:
        for gate, pat, route in COMPILED:
            if pat.search(alvo):
                return gate, route
    return "gate não identificado", None


# A denial is announced by its source, at the start of a line: a hook error prefix, a Touring gate tag, or a
# Claude Code permission phrase. Loose words ("block", "guard", "denied") inside ordinary output — CEG logs, pytest
# assertions, WebSearch errors — were counted as denials in the first pass (08/10/2026); they are not.
HOOK_DENIAL = re.compile(r"^(PreToolUse|PostToolUse|PostToolBatch|Stop|UserPromptSubmit):?\S*\s+hook (error|blocked)"
                         r"|^\[G\d+[a-z]? |python_pre_gate: |^(Force operation|Force without confirmation|"
                         r"Recursive deletion|Local branch deletion|This command requires approval)", re.M)


def is_denial(_tool: str, text: str) -> bool:
    """The tool does not change the predicate: every source announces itself the same way, in the text."""
    return bool(HOOK_DENIAL.search(text))


class Stats:
    def __init__(self):
        self.count = Counter()
        self.sessions = defaultdict(set)
        self.tools = defaultdict(Counter)
        self.followed = Counter()
        self.judged = Counter()
        self.retrip = Counter()


def _ts(rec: dict) -> float:
    try:
        return datetime.fromisoformat(rec.get("timestamp", "").replace("Z", "+00:00")).timestamp()
    except ValueError:
        return 0.0


def scan(f: Path, st: Stats) -> None:
    names, pending, last_trip = {}, [], {}
    for line in f.open(encoding="utf-8", errors="replace"):
        if '"tool_use"' not in line and '"tool_result"' not in line:
            continue
        rec = json.loads(line)
        if rec.get("timestamp", "") < SINCE:
            continue
        for c in (rec.get("message") or {}).get("content") or []:
            if isinstance(c, dict):
                _event(c, rec, f.stem, names, pending, last_trip, st)


def _event(c, rec, sid, names, pending, last_trip, st) -> None:
    if c.get("type") == "tool_use":
        names[c.get("id")] = c.get("name")
        blob = json.dumps({"name": c.get("name"), **(c.get("input") or {})}, ensure_ascii=False)
        for gate, route in pending:
            st.judged[gate] += 1
            st.followed[gate] += bool(route and route.search(blob))
        pending.clear()
    elif c.get("type") == "tool_result" and c.get("is_error"):
        tool = names.get(c.get("tool_use_id"), "?")
        text = _text(c)
        if is_denial(tool, text):
            _denial(tool, text, rec, sid, pending, last_trip, st)


def _denial(tool, text, rec, sid, pending, last_trip, st) -> None:
    gate, route = gate_of(text)
    st.count[gate] += 1
    st.sessions[gate].add(sid)
    st.tools[gate][tool] += 1
    now = _ts(rec)
    st.retrip[gate] += bool(now and now - last_trip.get(gate, -1e18) <= WINDOW_S)
    last_trip[gate] = now
    pending.append((gate, route))


def main() -> int:
    st, since_ts = Stats(), datetime.fromisoformat(SINCE).timestamp()
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime >= since_ts:
                scan(f, st)
    total = sum(st.count.values())
    print(f"negações de hook/guard: {total} em {len({s for v in st.sessions.values() for s in v})} transcripts")
    print(f"{'gate':42} {'n':>5} {'sessões':>8} {'reincide≤5min':>14} {'rota seguida':>13}  ferramentas")
    for gate, n in st.count.most_common():
        judged = st.judged[gate] or 1
        print(f"{gate:42} {n:5} {len(st.sessions[gate]):8} {st.retrip[gate] / n:13.0%} "
              f"{st.followed[gate] / judged:12.0%}  {dict(st.tools[gate].most_common(3))}")
    return 0


# Real denial texts (09/10/2026), one per gate that a free-text pattern used to steal: the classification is by the
# gate's OWN announcement (its tag) first. Before, "[G10 exec-burst] … a rajada desenrolada" matched G1's `RAJADA`
# (re.I, first in GATES) — measured by the sister session analise-25: 87 exec-burst denials credited to G1.
SELFTEST = [
    ("PreToolUse:Bash hook error: [G10 exec-burst] 5ª chamada seriada do executor `python-inline` (0 `touring run` "
     "na janela de 600s) — a rajada desenrolada JÁ é o programa", "exec-burst (python-inline seriado)"),
    ("PreToolUse:Bash hook error: [G10 write→run] 2º script escrito-e-executado na janela de 600s",
     "write→run (script escrito-e-executado)"),
    ("PreToolUse:Bash hook error: [G1 rajada-de-inspeção] 3ª inspeção `grep` em 300s", "G1 rajada-de-inspeção"),
    ("PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `grep` em 300s — rode o lote",
     "S3 rajada (CODE MODE, 2ª inspeção)"),
    ("PreToolUse:Bash hook error: [G11 orçamento de bypass] dois bypasses seguidos", "G11 orçamento de bypass"),
    ("Force without confirmation is not allowed", "rm guard"),
]


def selftest() -> int:
    falhas = [(t[:60], esperado, gate_of(t)[0]) for t, esperado in SELFTEST if gate_of(t)[0] != esperado]
    for t, esperado, obtido in falhas:
        print(f"  FALHA  {t!r}: esperado {esperado!r}, obtido {obtido!r}")
    print(f"selftest: {len(SELFTEST) - len(falhas)}/{len(SELFTEST)}")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(selftest() if "--selftest" in sys.argv else main())
