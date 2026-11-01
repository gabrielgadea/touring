"""r3 shadow event schema (S-1) — a CLOSED schema: privacy is a test, not a promise (S6).

Three JSONL line kinds. The emitter (Rust, in the daemon) writes `a2_offer_closed` and `a1_block`
to ~/.claude/touring/logs/r3_events.jsonl; the evaluator (Python, a separate process) writes
`shadow_decision` to ~/.claude/touring/logs/r3_shadow.jsonl.

Rules enforced here and re-checked by every consumer:
  * the schema is CLOSED — an unknown key is a violation, so a new field cannot smuggle a prompt;
  * no string longer than MAX_TEXT (80) chars anywhere in a line;
  * a session is only ever referred to by `session_hash` (12 hex of sha256), never the raw id;
  * the Jev API receives ONLY redacted features (JEV_ALLOWED_KEYS, whitespace-free values).
"""
import hashlib
import json
import re

MAX_TEXT = 80
K_WINDOW = 3
HASH_RE = re.compile(r"^[0-9a-f]{12}$")
SHA16_RE = re.compile(r"^[0-9a-f]{16}$")
FORBIDDEN_KEYS = {"prompt", "text", "command", "cmd", "content", "body", "message", "code"}
JEV_ALLOWED_KEYS = {
    "use_case", "kind", "mode", "closed_by", "n_cmds", "first_token",
    "baseline_value_bucket", "hook", "bytes_bucket", "family", "tool_class",
}
_TOKEN_RE = re.compile(r"^[A-Za-z0-9_.:\-]{0,40}$")  # no whitespace ⇒ cannot be free text


def hash_session(raw_session_id: str) -> str:
    """Deterministic join key for the events of one session; never the raw id."""
    return hashlib.sha256(raw_session_id.encode()).hexdigest()[:12]


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _text_violations(value, path):
    out = []
    if isinstance(value, str):
        if len(value) > MAX_TEXT:
            out.append(f"{path}: string of {len(value)} chars exceeds the {MAX_TEXT}-char text cap")
    elif isinstance(value, dict):
        for k, v in value.items():
            out += _text_violations(v, f"{path}.{k}")
    elif isinstance(value, list):
        for i, v in enumerate(value):
            out += _text_violations(v, f"{path}[{i}]")
    return out


# kind -> (required-key -> predicate); keys outside this set are violations (closed schema)
def _a2_fields():
    return {
        "v": lambda x: x == 1,
        "kind": lambda x: x == "a2_offer_closed",
        "use_case": lambda x: x == "a2_outcome",
        "ts_ms": _is_int,
        "session_hash": lambda x: isinstance(x, str) and bool(HASH_RE.match(x)),
        "mode": lambda x: x in {"code", "both", "native"},
        "offered_secs": _is_int,
        "closed_by": lambda x: x in {"consumed", "window", "replaced"},
        "cmd_heads": lambda x: isinstance(x, list) and len(x) <= K_WINDOW and all(isinstance(s, str) for s in x),
        "baseline_value": lambda x: x is None or (isinstance(x, (int, float)) and not isinstance(x, bool)),
        "baseline_followed": lambda x: isinstance(x, bool),
    }


def _a1_fields():
    return {
        "v": lambda x: x == 1,
        "kind": lambda x: x == "a1_block",
        "use_case": lambda x: x == "a1_relevance",
        "ts_ms": _is_int,
        "session_hash": lambda x: isinstance(x, str) and bool(HASH_RE.match(x)),
        "hook": lambda x: isinstance(x, str) and 0 < len(x) <= 40,
        "bytes": lambda x: _is_int(x) and x >= 0,
        "first_line_sha": lambda x: isinstance(x, str) and bool(SHA16_RE.match(x)),
        "first_line_head": lambda x: isinstance(x, str),
    }


def _ref_ok(x):
    return (isinstance(x, dict) and set(x) == {"kind", "ts_ms", "session_hash"}
            and isinstance(x["kind"], str) and _is_int(x["ts_ms"])
            and isinstance(x["session_hash"], str) and bool(HASH_RE.match(x["session_hash"])))


def _decision_ok(x):
    if not isinstance(x, dict) or not x or not set(x) <= {"followed", "would_drop", "p"}:
        return False
    if "p" in x and not (isinstance(x["p"], (int, float)) and 0.0 <= x["p"] <= 1.0):
        return False
    return all(isinstance(x[k], bool) for k in ("followed", "would_drop") if k in x)


def _sd_fields():
    ref, decision = _ref_ok, _decision_ok
    return {
        "v": lambda x: x == 1,
        "kind": lambda x: x == "shadow_decision",
        "use_case": lambda x: x in {"a2_outcome", "a1_relevance"},
        "ts_ms": _is_int,
        "arm": lambda x: x in {"B0", "B-D", "B-L", "B-J"},
        "event_ref": ref,
        "decision": decision,
        "baseline_agrees": lambda x: x is None or isinstance(x, bool),
        "latency_ms": lambda x: _is_int(x) and x >= 0,
        "model": lambda x: isinstance(x, str) and len(x) <= 40,
    }


SCHEMAS = {"a2_offer_closed": _a2_fields, "a1_block": _a1_fields, "shadow_decision": _sd_fields}


def validate_event(event) -> list:
    """Return the list of violations (empty ⇒ valid)."""
    if not isinstance(event, dict):
        return ["event is not an object"]
    kind = event.get("kind")
    if kind not in SCHEMAS:
        return [f"unknown kind {kind!r}"]
    fields = SCHEMAS[kind]()
    out = [f"{k}: missing required field" for k in fields if k not in event]
    out += [f"{k}: unknown key (the schema is closed)" for k in event if k not in fields]
    for k, ok in fields.items():
        if k in event and not ok(event[k]):
            out.append(f"{k}: invalid value {str(event[k])[:40]!r}")
    return out + _text_violations(event, "$")


def scan_privacy(line: str) -> list:
    """Privacy scan of ONE raw JSONL line, independent of the schema (a second, dumber net)."""
    try:
        obj = json.loads(line)
    except ValueError:
        return ["line is not JSON"]
    out = _text_violations(obj, "$")

    def keys(o, path="$"):
        if isinstance(o, dict):
            for k, v in o.items():
                if k.lower() in FORBIDDEN_KEYS:
                    yield f"{path}.{k}: forbidden key (free text lives nowhere in a shadow line)"
                yield from keys(v, f"{path}.{k}")
        elif isinstance(o, list):
            for i, v in enumerate(o):
                yield from keys(v, f"{path}[{i}]")

    return out + list(keys(obj))


def _bucket(n):
    return "0" if n == 0 else "1-1k" if n < 1000 else "1k-4k" if n < 4000 else "4k+"


def redact_for_jev(event: dict) -> dict:
    """The ONLY thing the Jev API may see for an event: derived, whitespace-free features."""
    if event["kind"] == "a2_offer_closed":
        heads = event.get("cmd_heads") or []
        first = heads[0].split()[0][:40] if heads and heads[0].split() else ""
        bv = event.get("baseline_value")
        return {
            "use_case": event["use_case"], "kind": event["kind"], "mode": event["mode"],
            "closed_by": event["closed_by"], "n_cmds": len(heads), "first_token": first,
            "baseline_value_bucket": "none" if bv is None else "zero" if bv == 0 else "half" if bv < 1 else "full",
        }
    if event["kind"] == "a1_block":
        return {"use_case": event["use_case"], "kind": event["kind"], "hook": event["hook"],
                "bytes_bucket": _bucket(event["bytes"])}
    raise ValueError(f"nothing to redact for kind {event['kind']!r}")


def validate_jev_payload(payload: dict) -> list:
    out = [f"{k}: key not allowed in a Jev payload" for k in payload if k not in JEV_ALLOWED_KEYS]
    for k, v in payload.items():
        if isinstance(v, str) and not _TOKEN_RE.match(v):
            out.append(f"{k}: value is not a whitespace-free token (free text cannot leave the machine)")
    return out
