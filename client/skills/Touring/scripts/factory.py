#!/usr/bin/env python3
"""factory.py — F4: the factory router. Ticket in → ADW out, deterministically.

`route "<ticket>"` classifies a ticket and recommends an ADW from the library:
deterministic rules first (keyword families + CILA level); the LLM router is
reserved for genuinely ambiguous tickets [video 19:31]. Evidence comes from the
live Touring surfaces: `classify-intent` (CILA), `route` (L0-L4 topology),
`predict-action` (historical prior), `investigate --brief` (codebase evidence).

`start "<ticket>"` routes, instantiates the spec from the library when missing,
and launches `touring adw run` (sync, or detached with --bg). Outcomes feed the
router's RL arm (`touring learning reward factory_router_<adw>`) and the local
stats file backing the router_accuracy KPI.

Usage:
  factory.py route "<ticket>" [--root R] [--json]
  factory.py start "<ticket>" [--root R] [--bg] [--var k=v ...]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

STATS_DIRNAME = ".touring/factory"
PROBE_TIMEOUT_S = 60

# Deterministic keyword families, checked in order — first hit wins. The order
# encodes priority: incidents outrank features; audits outrank exploration.
RULES: list[tuple[str, str, str]] = [
    ("hotfix", r"\b(hotfix|incident|production (is )?down|outage|urgent|emergency)\b",
     "incident language → surgical hotfix, speed over elegance"),
    ("bugfix", r"\b(bug|fix(es|ing)?|broken|fails?|failing|crash(es|ing)?|regress(ion|ed)|error|wrong result)\b",
     "defect language → expert bugfix with verified gate"),
    ("audit", r"\b(audit|review|security|vulnerab|cwe|owasp|quality check)\b",
     "assessment language → deterministic masters + light synthesis"),
    ("chore", r"\b(chore|typo|rename|readme|docs?( update)?|comment|format|bump|cleanup|small task)\b",
     "small mechanical task → single light agent, never the heavy pipeline"),
    ("feature", r"\b(feature|implement|add (a |new )|build|create|support for|endpoint|integrate)\b",
     "construction language → full feature pipeline with conflict-check"),
    ("explore-plan", r"\b(explore|research|investigate|understand|map out|plan(ning)? for|study)\b",
     "discovery language → exploration loop into plan refinement"),
]

ADW_DEFAULT_VARS = {
    "bugfix": {"symptom": "{ticket}", "target": ".", "verify_cmd": None},
    "chore": {"task": "{ticket}", "verify_cmd": None},
    "feature": {"feature": "{ticket}", "target": ".", "write_set": "writes:UNSPECIFIED",
                "verify_cmd": None},
    "hotfix": {"incident": "{ticket}", "verify_cmd": None},
    "audit": {"target": "{ticket}"},
    "explore-plan": {"topic": "{ticket}"},
}


def probe(cmd: list[str], root: Path) -> str:
    try:
        proc = subprocess.run(cmd, cwd=root, capture_output=True, text=True,
                              timeout=PROBE_TIMEOUT_S)
        return (proc.stdout or proc.stderr).strip()
    except Exception as err:  # probes enrich; they never block routing
        return f"probe-error: {err}"


def cila_level(classify_output: str) -> int:
    try:
        return int(json.loads(classify_output).get("metadata", {}).get("cila_level", 0))
    except (json.JSONDecodeError, ValueError, AttributeError):
        match = re.search(r"cila_level\D+(\d+)", classify_output)
        return int(match.group(1)) if match else 0


def deterministic_route(ticket: str) -> tuple[str, str] | None:
    lowered = ticket.lower()
    for adw, pattern, why in RULES:
        if re.search(pattern, lowered):
            return adw, why
    return None


def llm_route(ticket: str) -> tuple[str, str]:
    """Ambiguous tickets only: one light-tier call, forced-choice JSON."""
    names = [r[0] for r in RULES]
    prompt = (f"Route this ticket to exactly one workflow of {names}. "
              f'Ticket: "{ticket}". Reply ONLY with JSON {{"adw": "<name>", "why": "<short>"}}.')
    try:
        proc = subprocess.run(
            ["claude", "-p", prompt, "--output-format", "json", "--model", "haiku"],
            capture_output=True, text=True, timeout=120)
        result = json.loads(json.loads(proc.stdout).get("result", "{}"))
        adw = result.get("adw", "")
        if adw in names:
            return adw, f"llm-router: {result.get('why', 'no rationale')}"
    except Exception:
        pass
    return "explore-plan", "llm-router unavailable/ambiguous → safest default is discovery"


def route_ticket(ticket: str, root: Path) -> dict:
    hit = deterministic_route(ticket)
    router = "deterministic"
    if hit is None:
        router = "llm"
        hit = llm_route(ticket)
    adw, why = hit

    classify_raw = probe(["touring", "classify-intent", ticket], root)
    level = cila_level(classify_raw)
    topology = probe(["touring", "route", ticket], root)
    prior = probe(["touring", "predict-action", ticket], root)
    evidence = probe(["touring", "investigate", "--brief", ticket], root)[:600]

    solo = adw == "chore" or level <= 1
    return {
        "ticket": ticket,
        "adw": adw,
        "router": router,
        "justification": why,
        "cila_level": level,
        "solo_hint": solo,  # L0-L1 stays SOLO — a chore never earns the heavy factory
        "topology": topology.splitlines()[0] if topology else "",
        "prior": prior.splitlines()[-1].strip() if prior else "",
        "evidence_digest": evidence,
        "run_hint": f"touring adw run {adw}",
    }


def load_stats(root: Path) -> dict:
    path = root / STATS_DIRNAME / "stats.json"
    if path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    return {"routed_total": 0, "by_adw": {}, "outcomes": []}


def save_stats(root: Path, stats: dict) -> None:
    path = root / STATS_DIRNAME / "stats.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(stats, ensure_ascii=False, indent=1), encoding="utf-8")


def record_route(root: Path, adw: str) -> None:
    stats = load_stats(root)
    stats["routed_total"] += 1
    stats["by_adw"][adw] = stats["by_adw"].get(adw, 0) + 1
    save_stats(root, stats)


OUTCOME_REWARD = {"completed": 1.0, "waiting_human": 0.7, "failed": 0.2}


def reward_outcome(root: Path, adw: str, status: str) -> None:
    """Close the RL loop: the router's arm learns which ADW pays off."""
    value = OUTCOME_REWARD.get(status, 0.5)
    probe(["touring", "learning", "reward", f"factory_router_{adw}", str(value)], root)
    stats = load_stats(root)
    stats["outcomes"].append({"adw": adw, "status": status, "reward": value})
    save_stats(root, stats)


def build_vars(adw: str, ticket: str, overrides: dict[str, str]) -> tuple[dict[str, str], list[str]]:
    """Fill the ADW's canonical vars from the ticket; report what's missing."""
    variables: dict[str, str] = {}
    missing: list[str] = []
    for key, default in ADW_DEFAULT_VARS.get(adw, {}).items():
        if key in overrides:
            variables[key] = overrides[key]
        elif default is None:
            missing.append(key)  # e.g. verify_cmd — a gate must be real, never "true"
        else:
            variables[key] = default.format(ticket=ticket)
    variables.update({k: v for k, v in overrides.items() if k not in variables})
    return variables, missing


def cmd_start(root: Path, ticket: str, overrides: dict[str, str], background: bool) -> int:
    decision = route_ticket(ticket, root)
    adw = decision["adw"]
    record_route(root, adw)

    spec_path = root / ".touring" / "adw" / f"{adw}.toml"
    if not spec_path.is_file():
        probe(["touring", "adw", "from-template", adw], root)
    variables, missing = build_vars(adw, ticket, overrides)
    if missing:
        print(json.dumps({**decision, "started": False,
                          "error": f"missing required --var for `{adw}`: {missing} "
                                   "(a verification gate must be real — refusing a fake gate)"},
                         ensure_ascii=False, indent=1))
        return 2

    cmd = ["touring", "adw", "run", adw]
    for key, value in variables.items():
        cmd += ["--var", f"{key}={value}"]

    if background:
        proc = subprocess.Popen(cmd, cwd=root, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, start_new_session=True)
        print(json.dumps({**decision, "started": True, "background_pid": proc.pid,
                          "vars": variables}, ensure_ascii=False, indent=1))
        return 0

    proc = subprocess.run(cmd, cwd=root)
    status = {0: "completed", 3: "waiting_human"}.get(proc.returncode, "failed")
    reward_outcome(root, adw, status)
    print(json.dumps({**decision, "started": True, "status": status,
                      "vars": variables}, ensure_ascii=False, indent=1))
    return proc.returncode


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="touring factory", description=__doc__)
    sub = parser.add_subparsers(dest="sub", required=True)
    p_route = sub.add_parser("route")
    p_route.add_argument("ticket")
    p_route.add_argument("--root", default=".")
    p_start = sub.add_parser("start")
    p_start.add_argument("ticket")
    p_start.add_argument("--root", default=".")
    p_start.add_argument("--bg", action="store_true")
    p_start.add_argument("--var", action="append", default=[])
    p_stats = sub.add_parser("stats")
    p_stats.add_argument("--root", default=".")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()

    if args.sub == "route":
        decision = route_ticket(args.ticket, root)
        record_route(root, decision["adw"])
        print(json.dumps(decision, ensure_ascii=False, indent=1))
        return 0
    if args.sub == "start":
        overrides = dict(kv.split("=", 1) for kv in (args.var or []))
        return cmd_start(root, args.ticket, overrides, args.bg)
    print(json.dumps(load_stats(root), ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
