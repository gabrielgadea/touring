#!/usr/bin/env python3
"""prose_integrity_gate.py — PreToolUse: foreign-script characters in outgoing text. Shadow first.

Origin: the `analise-ef` session measured CJK/Cyrillic characters inside Portuguese prose (`a terceira返回值`,
`Para节省 espaço`) in 1,22% of the Opus 5.5 text blocks and 4,20% of the MiniMax ones, and four of them had
already been committed into the touring repo. Gabriel approved (04/10/2026) a gate in the executor, shadow
first, with the promotion of `SendMessage` to blocking decided by a pre-registered criterion:
`docs/plans/2026-10-04-integridade-prosa/pre-registro.md` in the touring repo (sha256 7fbb91ef…).

What it reads, per tool:
  SendMessage            `message` (string); characters inside `…` or ``` blocks are quotation (the escape)
  Write                  `content`, minus the characters the file on disk already had
  Edit / MultiEdit       `new_string`, minus the characters its `old_string` had
  NotebookEdit           `new_source`
Only INTRODUCED characters count: editing a file with legitimate CJK test data does not accuse what was there.

Two modes, one per channel:
  `TOURING_PROSE_GATE_MODE` (SendMessage)          `shadow` (default) logs and advises; `block` refuses. Promotion
                                                   stays with the pre-registered criterion (`--criterion`).
  `TOURING_PROSE_GATE_FILE_MODE` (Write, Edit,     `block` (default, Gabriel's order of 06/10/2026, adendo in the
  MultiEdit, NotebookEdit)                         same bundle) refuses; `shadow` logs and advises.
A refusal is exit 2 and names character, codepoint, position and every open route. In files the routes are:
quotation in backticks (as in a message), a line carrying `prose-integrity: allow`, or writing the data from a
script (Bash is not gated). A Write over a file that exists but cannot be read never blocks: what was already
there is unknown. An escape alone is logged too (`action: escaped`), so a route used as a shortcut shows up.
Kill switch: `TOURING_PROSE_GATE_DISABLED=1`. Fail-open: an unreadable payload or an internal error never blocks.

Out of reach, declared: the chat reply is not a tool call (no PreToolUse sees it), and the lexical class
(English or German fragments inside Portuguese) passes any script filter.

CLI besides the hook: `--replay [DIR]` runs the same detector over historical transcripts; `--criterion
LABELS.jsonl` evaluates the pre-registered promotion rule; `--controls` prints the controls.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import re
import sys
import time
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

# Narrower ranges first so a character gets the most specific label. The analise-ef scanner's ranges, Greek
# off, plus cjk_compat and full-width forms (which it does not cover).
SCRIPTS: tuple[tuple[int, int, str], ...] = (
    (0x3000, 0x303F, "cjk_pontuacao"),
    (0x3040, 0x30FF, "kana"),
    (0x2E80, 0x9FFF, "cjk"),
    (0xAC00, 0xD7AF, "hangul"),
    (0x0400, 0x04FF, "cirilico"),
    (0x0530, 0x058F, "armenio"),
    (0x0590, 0x05FF, "hebraico"),
    (0x0600, 0x06FF, "arabe"),
    (0x0900, 0x097F, "devanagari"),
    (0x0E00, 0x0E7F, "thai"),
    (0xF900, 0xFAFF, "cjk_compat"),
    (0xFF00, 0xFFEF, "largura_total"),
)
LOWEST_SIGNAL = min(a for a, _, _ in SCRIPTS)
TOOLS = ("SendMessage", "Write", "Edit", "MultiEdit", "NotebookEdit")
FILE_TOOLS = TOOLS[1:]
PRAGMA = "prose-integrity: allow"
# The pragma counts only after a comment leader: inside a JSON value it "worked" by corrupting the data.
PRAGMA_RE = re.compile(r"(?:#|//|/\*|<!--|--|;).*" + re.escape(PRAGMA))
MAX_OLD_BYTES = 5 * 1024 * 1024
CONTEXT = 40
DEFAULT_LOG = Path.home() / ".claude" / "touring" / "prose_integrity" / "events.jsonl"
PROMOTE_MIN_N = 30
PROMOTE_MAX_LEGIT_UPPER = 0.20


@dataclass(frozen=True)
class Hit:
    pos: int
    char: str
    script: str

    @property
    def codepoint(self) -> str:
        return f"U+{ord(self.char):04X}"


@dataclass
class Verdict:
    tool: str
    hits: list[Hit] = field(default_factory=list)
    escaped: int = 0
    path: str | None = None
    text: str = ""
    old_unknown: bool = False  # the file exists but could not be read: what it already had is unknown


def script_of(ch: str) -> str | None:
    o = ord(ch)
    for a, b, name in SCRIPTS:
        if a <= o <= b:
            return name
    return None


def find(text: str) -> list[Hit]:
    """Every signal character with its position; empty when the text is clean."""
    out = []
    for i, ch in enumerate(text):
        if ord(ch) < LOWEST_SIGNAL:  # the fast path follows the table: a range added below moves it
            continue
        s = script_of(ch)
        if s:
            out.append(Hit(i, ch, s))
    return out


def quoted_spans(text: str) -> list[tuple[int, int]]:
    """[start, end) of every ``` fenced block and every `inline` span — the quotation escape."""
    spans: list[tuple[int, int]] = []
    i, n = 0, len(text)
    while i < n:
        if text.startswith("```", i):
            j = text.find("```", i + 3)
            if j < 0:  # an unclosed fence quotes nothing, like an unclosed tick (review touring-b4, 06/10)
                i += 3
                continue
            spans.append((i, j + 3))
            i = j + 3
        elif text[i] == "`":
            j = text.find("`", i + 1)
            nl = text.find("\n", i + 1)
            if j < 0 or (0 <= nl < j):  # an unclosed or line-crossing backtick quotes nothing
                i += 1
                continue
            spans.append((i, j + 1))
            i = j + 1
        else:
            i += 1
    return spans


def split_quoted(text: str) -> tuple[list[Hit], int]:
    """(hits outside quotation, number of signal characters inside it)."""
    spans = quoted_spans(text)
    hits, escaped = [], 0
    for h in find(text):
        if any(a <= h.pos < b for a, b in spans):
            escaped += 1
        else:
            hits.append(h)
    return hits, escaped


def introduced(new: str, old: str) -> list[Hit]:
    """Signal characters the new text has beyond what the old text had (per character count)."""
    budget = Counter(h.char for h in find(old))
    out = []
    for h in find(new):
        if budget[h.char] > 0:
            budget[h.char] -= 1
        else:
            out.append(h)
    return out


def _read_old(path: str | None) -> str | None:
    """The file's current text; "" when there is no such file; None when it exists but cannot be read."""
    if not path:
        return ""
    try:
        p = Path(path)
        if not p.exists() or p.is_dir():  # a directory holds no text: the Write itself will fail
            return ""
        if p.is_file() and p.stat().st_size <= MAX_OLD_BYTES:
            return p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        pass
    return None


def file_escapes(new: str, hits: list[Hit]) -> tuple[list[Hit], int]:
    """(hits that stay, hits escaped): backtick quotation, or a line that carries the pragma."""
    spans = quoted_spans(new)
    kept, escaped = [], 0
    for h in hits:
        line_start = new.rfind("\n", 0, h.pos) + 1
        line_end = new.find("\n", h.pos)
        line = new[line_start:len(new) if line_end < 0 else line_end]
        if PRAGMA_RE.search(line) or any(a <= h.pos < b for a, b in spans):
            escaped += 1
        else:
            kept.append(h)
    return kept, escaped


def _str(d: dict, key: str) -> str | None:
    v = d.get(key)
    return v if isinstance(v, str) else None


def _send_message(ti: dict, v: Verdict) -> None:
    msg = _str(ti, "message")
    if msg is not None:
        v.text = msg
        v.hits, v.escaped = split_quoted(msg)


def _write(ti: dict, v: Verdict) -> None:
    new = _str(ti, "content")
    if new is not None:
        old = _read_old(v.path)
        v.old_unknown = old is None
        v.text = new
        v.hits, v.escaped = file_escapes(new, introduced(new, old or ""))


def _edit(ti: dict, v: Verdict) -> None:
    new = _str(ti, "new_string")
    if new is not None:
        v.text = new
        v.hits, v.escaped = file_escapes(new, introduced(new, _str(ti, "old_string") or ""))


def _multi_edit(ti: dict, v: Verdict) -> None:
    parts = [(e["new_string"], _str(e, "old_string") or "") for e in ti.get("edits") or []
             if isinstance(e, dict) and isinstance(e.get("new_string"), str)]
    v.text = "\n".join(n for n, _ in parts)
    for n, o in parts:
        kept, esc = file_escapes(n, introduced(n, o))
        v.hits.extend(kept)
        v.escaped += esc


def _notebook_edit(ti: dict, v: Verdict) -> None:
    new = _str(ti, "new_source")
    if new is not None:
        v.text = new
        v.hits, v.escaped = file_escapes(new, find(new))


READERS = {"SendMessage": _send_message, "Write": _write, "Edit": _edit, "MultiEdit": _multi_edit,
           "NotebookEdit": _notebook_edit}


def assess(payload: dict) -> Verdict:
    """What the payload would put out that is signal. Raises nothing on odd shapes: they read as clean."""
    tool = payload.get("tool_name")
    ti = payload.get("tool_input")
    if not isinstance(tool, str) or not isinstance(ti, dict):
        return Verdict(tool=str(tool))
    v = Verdict(tool=tool, path=ti.get("file_path") or ti.get("notebook_path"))
    reader = READERS.get(tool)
    if reader:
        reader(ti, v)
    return v


def _context(text: str, pos: int) -> str:
    return text[max(0, pos - CONTEXT):pos + CONTEXT].replace("\n", " ")


def _describe(v: Verdict, limit: int = 6) -> str:
    shown = ", ".join(f"'{h.char}' {h.codepoint} ({h.script}) na posição {h.pos}" for h in v.hits[:limit])
    more = f" e mais {len(v.hits) - limit}" if len(v.hits) > limit else ""
    return shown + more


FILE_ROUTES = (f"se é proposital, cite-o entre crases na mesma linha (`…`) ou num bloco cercado e fechado "
               f"(```…```), ou marque a linha com um comentário `# {PRAGMA}` (também depois de `//`, `/*`, "
               f"`<!--`, `--` ou `;`; dentro de um valor de dado não vale), ou gere o dado por script no Bash, "
               f"que não passa por este gate")


def advice(v: Verdict) -> str:
    where = f" em {v.path}" if v.path else ""
    base = (f"[prose-integrity] {v.tool}{where}: {len(v.hits)} caractere(s) de alfabeto estranho ao português — "
            f"{_describe(v)}. Se não foi proposital, é corrupção de token: reescreva o trecho.")
    if v.tool == "SendMessage":
        base += " Se é citação proposital (o próprio defeito, um dado), ponha o trecho entre crases."
    else:
        base += f" Rotas: {FILE_ROUTES}."
    if v.old_unknown:
        base += " Não bloqueado: o arquivo existente não pôde ser lido, então o que ele já tinha é desconhecido."
    return base


def refusal(v: Verdict) -> str:
    head = (f"[prose-integrity] {v.tool} recusado: {len(v.hits)} caractere(s) de alfabeto estranho ao português "
            f"fora de citação — {_describe(v)}. Reescreva o trecho; ")
    if v.tool == "SendMessage":
        return head + "se o caractere é o assunto da mensagem, ponha-o entre crases (`…`), que é citação e passa."
    return head + FILE_ROUTES + "."


def effective_mode(tool: str) -> str:
    """`block` or `shadow` for this tool, from its channel's variable (see the module docstring)."""
    if tool == "SendMessage":
        return "block" if os.environ.get("TOURING_PROSE_GATE_MODE") == "block" else "shadow"
    return "shadow" if os.environ.get("TOURING_PROSE_GATE_FILE_MODE") == "shadow" else "block"


def _log(v: Verdict, payload: dict, mode: str, action: str) -> None:
    path = Path(os.environ.get("TOURING_PROSE_GATE_LOG") or DEFAULT_LOG)
    ev = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "session_id": payload.get("session_id"),
        "cwd": payload.get("cwd"),
        "transcript_path": payload.get("transcript_path"),
        "tool": v.tool,
        "path": v.path,
        "mode": mode,
        "action": action,
        "n_hits": len(v.hits),
        "escaped": v.escaped,
        "scripts": dict(Counter(h.script for h in v.hits)),
        "hits": [{"pos": h.pos, "char": h.char, "codepoint": h.codepoint, "script": h.script,
                  "context": _context(v.text, h.pos)} for h in v.hits[:10]],
        "text_len": len(v.text),
        "text_sha256": hashlib.sha256(v.text.encode("utf-8", "replace")).hexdigest(),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(ev, ensure_ascii=False) + "\n")


def run_hook(raw: str) -> tuple[int, str, str]:
    """(exit code, stdout, stderr) for one PreToolUse payload."""
    if os.environ.get("TOURING_PROSE_GATE_DISABLED") == "1":
        return 0, "", ""
    try:
        payload = json.loads(raw)
    except (ValueError, TypeError):
        return 0, "", ""
    if not isinstance(payload, dict) or payload.get("tool_name") not in TOOLS:
        return 0, "", ""
    v = assess(payload)
    if not v.hits and not v.escaped:
        return 0, "", ""
    mode, action = decide(v)
    try:
        _log(v, payload, mode, action)
    except OSError:
        pass  # the log is evidence, not a precondition: a full disk must not block or crash a session
    return respond(v, action)


def decide(v: Verdict) -> tuple[str, str]:
    """(effective mode, action): `blocked` | `advised` | `escaped`."""
    mode = effective_mode(v.tool)
    if not v.hits:
        return mode, "escaped"
    return mode, "blocked" if mode == "block" and not v.old_unknown else "advised"


def respond(v: Verdict, action: str) -> tuple[int, str, str]:
    """The hook's (exit, stdout, stderr) for a decided verdict."""
    if action == "escaped":
        return 0, "", ""
    if action == "blocked":
        return 2, "", refusal(v)
    out = {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": advice(v)}}
    return 0, json.dumps(out, ensure_ascii=False), ""


def _dedup_epoch(out: str, raw: str) -> str:
    """S-4p (26/10/2026): dedupe por época via ponte Rust (`touring-hook
    dedup-stdin`) — este processo nunca passa pelo daemon. O aviso repetido na
    mesma época é omitido; um aviso com conteúdo novo passa. Fail-open:
    qualquer falha devolve o texto intacto (a gate é shadow — aviso, não deny).
    """
    try:
        payload = json.loads(raw) if raw.strip() else {}
        if not isinstance(payload, dict):
            return out
        sys.path.insert(0, os.path.expanduser("~/.claude/hooks"))
        import injection_dedup

        return injection_dedup.dedup_output(
            out,
            session_id=str(payload.get("session_id") or ""),
            cwd=str(payload.get("cwd") or ""),
        )
    except Exception:  # noqa: BLE001 — a dedupe nunca custa o aviso
        return out


# ── controls, replay, criterion ──────────────────────────────────────────────

def controls() -> dict[str, bool]:
    """The analise-ef scanner's four controls (C1-C4, verbatim inputs) plus the gate's own two."""
    limpo = find("O relatorio traz uma frase com um caminho de arquivo e nada mais.")
    positivo = find("O relatorio de 03/10 traz 路径 no meio da frase.")
    tipografia = find("dAUC -0,0187 · t = 0,95 · k 0,77 → ★ ─ « » °C ≈ ≤ ≥ — e o Acordo de kappa")
    grego = find("a simboliza tau alto")
    hits, esc = split_quoted("citado `路径` e solto 决")
    return {
        "C1_limpo_nao_acusa": limpo == [],
        "C2_positivo_detecta_cjk": len(positivo) == 2 and all(h.script == "cjk" for h in positivo),
        "C3_negativo_ignora_tipografia": tipografia == [],
        "C4_negativo_ignora_grego": grego == [],
        "C5_crase_e_citacao": [h.char for h in hits] == ["决"] and esc == 2,
        "C6_so_o_introduzido": [h.char for h in introduced("日本語 返", "日本語")] == ["返"],
    }


def _record(line: str, tally: Counter) -> dict | None:
    """One transcript line as an assistant record, or None; an unreadable line is tallied, never swallowed."""
    if not line.strip():
        return None
    try:
        o = json.loads(line)
    except (ValueError, TypeError):
        tally["unreadable_lines"] += 1
        return None
    return o if isinstance(o, dict) and o.get("type") == "assistant" else None


def _tool_uses(f: Path, tally: Counter):
    """(record, tool_use block) for every gated tool call in one transcript."""
    try:
        fh = f.open(encoding="utf-8", errors="replace")
    except OSError:
        return
    with fh:
        for line in fh:
            o = _record(line, tally)
            for b in ((o or {}).get("message") or {}).get("content") or []:
                if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("name") in TOOLS:
                    yield o, b


def _detection(o: dict, b: dict, f: Path) -> dict | None:
    v = assess({"tool_name": b["name"], "tool_input": b.get("input") or {}})
    if v.tool == "Write":  # no old content in a transcript: everything is new
        v.hits = find(v.text)
    if not v.hits:
        return None
    return {"id": b.get("id"), "tool": v.tool, "path": v.path, "model": (o.get("message") or {}).get("model"),
            "session": o.get("sessionId"), "ts": o.get("timestamp"), "file": str(f), "n_hits": len(v.hits),
            "escaped": v.escaped, "scripts": dict(Counter(h.script for h in v.hits)),
            "contexts": [_context(v.text, h.pos) for h in v.hits[:3]]}


def replay(root: Path) -> dict:
    """The detector over every transcript under `root`: tool_use blocks only, as the hook would see them.

    A Write's old file content is not in the transcript, so a historical Write counts everything it wrote.
    Unreadable lines are counted, never swallowed.
    """
    tally: Counter = Counter()
    detections = []
    for f in sorted(Path(root).rglob("*.jsonl")):
        for o, b in _tool_uses(f, tally):
            tally["tool_uses"] += 1
            d = _detection(o, b, f)
            if d:
                detections.append(d)
    return {"tool_uses": tally["tool_uses"], "unreadable_lines": tally["unreadable_lines"],
            "detections": detections}


def wilson_upper(k: int, n: int, z: float = 1.96) -> float:
    """Upper bound of the Wilson score interval for k successes in n trials."""
    if n == 0:
        return 1.0
    p = k / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    margin = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return centre + margin


def criterion(labels: list[str]) -> dict:
    """The pre-registered promotion rule for SendMessage: n >= 30 and Wilson-95 upper(LEGIT+UNCLEAR) <= 0.20."""
    n = len(labels)
    legit = sum(1 for x in labels if x in ("LEGIT", "UNCLEAR"))
    upper = wilson_upper(legit, n)
    return {"n": n, "legit_or_unclear": legit, "corrupt": n - legit, "wilson_upper": round(upper, 4),
            "promote": n >= PROMOTE_MIN_N and upper <= PROMOTE_MAX_LEGIT_UPPER}


def main(argv: list[str]) -> int:
    if argv[1:2] == ["--controls"]:
        c = controls()
        print(json.dumps(c, indent=2))
        return 0 if all(c.values()) else 3
    if argv[1:2] == ["--replay"]:
        if not all(controls().values()):
            print("controles reprovados: o replay não mede o que diz medir", file=sys.stderr)
            return 3
        root = Path(argv[2]) if len(argv) > 2 else Path.home() / ".claude" / "projects"
        print(json.dumps(replay(root), ensure_ascii=False))
        return 0
    if argv[1:2] == ["--criterion"]:
        try:
            text = Path(argv[2]).read_text(encoding="utf-8")
        except (IndexError, OSError) as exc:
            print(f"uso: prose_integrity_gate.py --criterion <labels.jsonl> ({exc})", file=sys.stderr)
            return 2
        labels = [json.loads(l)["label"] for l in text.splitlines()
                  if l.strip() and json.loads(l).get("tool") == "SendMessage"]
        print(json.dumps(criterion(labels)))
        return 0
    try:
        raw = sys.stdin.read()
        rc, out, err = run_hook(raw)
    except Exception:  # noqa: BLE001 — fail-open is the contract: a gate bug must never stop a session
        return 0
    if out:
        print(_dedup_epoch(out, raw))
    if err:
        print(err, file=sys.stderr)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
