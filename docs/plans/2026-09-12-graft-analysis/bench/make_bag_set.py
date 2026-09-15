#!/usr/bin/env python3
"""Generate a held-out 'keyword bag' set mechanically (no hand-picked queries).

For each chosen memory/rule file: take its body (frontmatter and the description
excluded, so MEMORY.md's one-line hooks do not compete), split it into thirds, pick
the rarest word (corpus document frequency, length >= 5, not a stopword) from each
third plus the two rarest overall, shuffle them with a fixed seed. Truth = the file.
Appends the set `bag-heldout` to the sets file.
"""
from __future__ import annotations

import json
import random
import re
import sys
from pathlib import Path

HOME = Path.home()
WS = Path("/home/gabrielgadea/projects/touring")
ROOTS = {
    "memory": HOME / ".claude/projects/-home-gabrielgadea-projects-touring/memory",
    "rules": HOME / ".claude/rules",
    "memory-home": HOME / ".claude/projects/-home-gabrielgadea/memory",
}
STOP = set("para como mais sobre entre quando porque depois antes ainda sempre nunca cada toda todos todas isso esse essa este esta which there their would should could about after before other these those where while being".split())


def words(text: str) -> list[str]:
    return re.findall(r"[a-zà-ÿ]{5,}", text.lower())


def body_of(text: str) -> str:
    lines = text.splitlines()
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:])
    return text


def main() -> None:
    sets_path = Path(sys.argv[1])
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 20260913
    set_name = sys.argv[3] if len(sys.argv) > 3 else "bag-heldout"
    corpus = []
    for root in ROOTS.values():
        corpus += [p for p in root.rglob("*.md")]
    corpus += [p for p in WS.rglob("*.md") if "target" not in p.parts and "client" not in p.parts]
    df: dict[str, int] = {}
    for p in corpus:
        for w in set(words(p.read_text(errors="ignore"))):
            df[w] = df.get(w, 0) + 1
    rng = random.Random(seed)
    prior = json.loads(sets_path.read_text())
    used = {t for st in prior for q in st["questions"] for t in q["truth"]}
    questions = []
    for name, root in ROOTS.items():
        files = sorted(p for p in root.glob("*.md") if p.name != "MEMORY.md" and len(body_of(p.read_text(errors="ignore"))) > 1500 and f"@companion/{name}/{p.name}" not in used)
        want = {"memory": 6, "rules": 4, "memory-home": 0}[name] if set_name == "bag-heldout" else {"memory": 5, "rules": 0, "memory-home": 5}[name]
        for p in rng.sample(files, min(want, len(files))):
            body = body_of(p.read_text(errors="ignore"))
            thirds = [body[i * len(body) // 3:(i + 1) * len(body) // 3] for i in range(3)]
            picked: list[str] = []
            for t in thirds:
                cands = sorted({w for w in words(t) if w not in STOP and df.get(w, 0) >= 2}, key=lambda w: (df[w], w))
                for w in cands:
                    if w not in picked:
                        picked.append(w)
                        break
            for w in sorted({w for w in words(body) if w not in STOP and df.get(w, 0) >= 2}, key=lambda w: (df[w], w)):
                if len(picked) >= 5:
                    break
                if w not in picked:
                    picked.append(w)
            rng.shuffle(picked)
            questions.append({"q": " ".join(picked), "truth": [f"@companion/{name}/{p.name}"]})
    sets = [s for s in prior if s["name"] != set_name]
    sets.append({"name": set_name, "filter": None, "questions": questions})
    sets_path.write_text(json.dumps(sets, indent=1, ensure_ascii=False))
    for q in questions:
        print(q["truth"][0].split("/")[-1], "|", q["q"])


if __name__ == "__main__":
    main()
