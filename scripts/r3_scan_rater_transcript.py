"""E7.2 — mechanical blindness/vendor check of the rater-1 subagent transcript (Errata 7).

The rater is VALID only if, in its transcript: (a) no tool call names a forbidden path token, (b) no tool call reaches
a home path outside the three allowed files, (c) every assistant turn ran on a MiniMax model. No assistant turn at all
is no signal (exit 2), never a pass. Exit 0 valid · 1 invalid · 2 no signal.
"""
import argparse
import json
import re
import sys
from pathlib import Path

FORBIDDEN = ("key.jsonl", "withheld.json", "adj_claude.jsonl", "a2_offers", "audit_sample", "predictions-a2",
             "results-a2", "r3-replay", "r3_b0", "r3_replay_a2", "r3_arms", "r3_results_gate", "RESULTS.md",
             "CRITERIOS-ACEITACAO", "frozen-verified")
ALLOWED_FILES = ("adj_pack/rater_view/INSTRUCOES.md", "adj_pack/rater_view/pack_blind.jsonl", "adj_out/adj_minimax.jsonl")
ALLOWED_DIRS = ("adj_pack/rater_view", "adj_out")   # a directory the allowed files live in (cd / ls of THAT dir only)
HOME_PATH = re.compile(r"(?:/home/[A-Za-z0-9_.-]+|~)/[^\s'\"`;|&<>)(\\]*")   # `\` is shell syntax (line continuation), never path


def _calls(entry):
    msg = entry.get("message") if isinstance(entry.get("message"), dict) else {}
    content = msg.get("content")
    for b in content if isinstance(content, list) else []:
        if isinstance(b, dict) and b.get("type") == "tool_use":
            yield b


def _allowed(path: str) -> bool:
    p = path.rstrip("/")
    return any(p.endswith(f) for f in ALLOWED_FILES) or any(p.endswith(d) for d in ALLOWED_DIRS)


def scan(path) -> dict:
    models, violations, n_calls, n_turns = set(), [], 0, 0
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        msg = e.get("message") if isinstance(e.get("message"), dict) else {}
        if e.get("type") == "assistant":
            n_turns += 1
            models.add(str(msg.get("model", "")))
        for call in _calls(e):
            n_calls += 1
            text = json.dumps(call.get("input", {}), ensure_ascii=False)
            for tok in FORBIDDEN:
                if tok in text:
                    violations.append({"kind": "forbidden_token", "token": tok, "tool": call.get("name")})
            for hp in HOME_PATH.findall(text):
                if not _allowed(hp):
                    violations.append({"kind": "out_of_scope_path", "path": hp[:120], "tool": call.get("name")})
    bad_models = sorted(m for m in models if "minimax" not in m.lower())
    if bad_models:
        violations.append({"kind": "not_minimax", "models": bad_models})
    if not n_turns:
        return {"valid": None, "reason": "no assistant turn in the transcript: no signal", "n_turns": 0,
                "n_tool_calls": n_calls, "models": sorted(models), "violations": violations}
    return {"valid": not violations, "n_turns": n_turns, "n_tool_calls": n_calls, "models": sorted(models),
            "violations": violations}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("transcript")
    out = scan(ap.parse_args(argv).transcript)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return 2 if out["valid"] is None else 0 if out["valid"] else 1


if __name__ == "__main__":
    sys.exit(main())
