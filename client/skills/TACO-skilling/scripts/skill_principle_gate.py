#!/usr/bin/env python3
"""skill_principle_gate.py — a role implies an obligation.

A skill is an AFFIRMATION about how work is done. Two of those affirmations are
structural, and until 2026-08-20 nothing checked them:

  * a skill that **decomposes or dispatches** work must claim it atomically
    (`touring decompose claim`) — `ready` only READS, so two sessions polling it
    receive the SAME subtask and two workers edit the same file;
  * a skill that **audits** must route its verdict through a blind panel
    (`critic-panel` / `worker-critic-pair`) — an auditor grading its own work has
    no independent evidence in it.

The population is MEASURED, never hardcoded: a skill is in scope iff it invokes
the Touring CLI, because those are the skills these principles govern. Roles are
inferred from the skill's own text, so a NEW skill is covered the day it is
written and a regression (deleting the claim line) is caught the same way.

Canonical statement of the principles: `Touring/references/skill-operating-principles.md`.

exit 0 clean · 1 violation · 2 usage error.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

DEFAULT_ROOT = Path.home() / ".claude" / "skills"
REFERENCE = "Touring/references/skill-operating-principles.md"

# In scope iff the skill actually drives the Touring CLI.
IN_SCOPE = re.compile(r"\btouring[- ][a-z][a-z0-9-]{2,}", re.I)

# A role is not a vocabulary match. Measured 2026-08-20: keying "audits" off the
# WORD `cross-audit` anywhere in the body accused a document-drafting skill and a
# process-management skill, because they use the word about documents. Each rule
# below therefore names the SIGNAL that actually implies the obligation:
#
#   P5 is triggered by a MECHANICAL fact (body): if you call the DAG API or hand a
#      unit of work to a worker, `ready` will hand the same one to a second session.
#   P3 is triggered by a PURPOSE fact (description — the only text the runtime
#      reads) PLUS delegated judgment (body): there is nothing to blind in a purely
#      deterministic scorer, whose verdict is already counted by code. That is not
#      an exemption list — it is the principle itself, satisfied structurally.

DESC = re.compile(r"^description:\s*(.+?)(?=^[a-z_]+:|^---)", re.S | re.M)

RULES = [
    {
        "role": "decomposes-or-dispatches-work",
        "principle": "P5 (Wayfinder)",
        "scan": "body",
        "detect": re.compile(
            r"touring decompose (?:create|add|ready)\b"
            r"|dispatch(?:ing|es)?\s+(?:a\s+)?(?:fresh\s+)?subagent"
            r"|spawn\w*\s+(?:pure\s+)?subagents?"
            r"|spawn\w*\s+[\w.-]*\s*(?:agent|engineer)"
            r"|spawnar\s+\w*engineer", re.I),
        "require": re.compile(r"touring decompose claim\b"),
        "remedy": ("cite `touring decompose claim <task> --owner <id>` at the point where "
                   "work is taken — `ready` only READS, so two sessions receive the same subtask"),
    },
    {
        "role": "audits-or-reviews-work",
        "principle": "P3 (the gauntlet)",
        "scan": "description",
        "detect": re.compile(
            r"cross-audit|auditoria cruzada"
            r"|\baudit(?:s|ing)?\s+(?:a\s+)?(?:code|directory|implemented|everything)"
            r"|valida\w*\s+resultados?"
            r"|code review after each", re.I),
        # Nothing to blind unless a judgement is DELEGATED to an agent.
        "gate": re.compile(r"touring-auditor|\bsubagent|\bAgent tool|agente?\s+auditor", re.I),
        "require": re.compile(r"critic-panel|worker-critic-pair"),
        "remedy": ("route the delegated verdict through `critic-panel` (distinct lenses, fresh "
                   "sessions, code-counted quorum) or `worker-critic-pair` — compose the fragment"),
    },
]


def skill_files(root: Path):
    for p in sorted(root.glob("*/SKILL.md")):
        if p.is_file():
            yield p


def audit(root: Path):
    findings, scoped = [], 0
    for path in skill_files(root):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if not IN_SCOPE.search(text):
            continue
        scoped += 1
        dm = DESC.search(text)
        description = dm.group(1) if dm else ""
        for rule in RULES:
            haystack = description if rule["scan"] == "description" else text
            m = rule["detect"].search(haystack)
            if not m or rule["require"].search(text):
                continue
            if rule.get("gate") and not rule["gate"].search(text):
                continue  # the obligation's precondition is absent → not this skill's role
            offset = text.index(m.group(0)) if rule["scan"] == "description" and m.group(0) in text else m.start()
            line = text[:offset].count("\n") + 1
            findings.append({
                "skill": path.parent.name,
                "file": f"{path}:{line}",
                "role": rule["role"],
                "principle": rule["principle"],
                "evidence": m.group(0).strip()[:60],
                "remedy": rule["remedy"],
            })
    return scoped, findings


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=str(DEFAULT_ROOT),
                    help="skills dir (CI: client/skills)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)

    root = Path(args.root).expanduser()
    if not root.is_dir():
        print(f"skill_principle_gate: root inexistente: {root}", file=sys.stderr)
        return 2

    scoped, findings = audit(root)
    if args.json:
        print(json.dumps({"scoped": scoped, "violations": findings,
                          "reference": REFERENCE}, indent=2, ensure_ascii=False))
    elif not args.quiet:
        print(f"skills no escopo (invocam o Touring CLI): {scoped}")
        if not findings:
            print("── todo papel cumpre a obrigação do seu princípio ──")
        for f in findings:
            print(f"\n✗ {f['skill']} — {f['principle']}")
            print(f"    papel:    {f['role']}  (evidência: {f['evidence']!r})")
            print(f"    em:       {f['file']}")
            print(f"    remédio:  {f['remedy']}")
        if findings:
            print(f"\n{len(findings)} violação(ões). Canônico: {REFERENCE}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
