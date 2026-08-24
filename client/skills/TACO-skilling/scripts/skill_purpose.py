#!/usr/bin/env python3
# #tags: kind:script lang:python purpose:analysis domain:skills
"""Measure PURPOSE overlap between skills — who competes for the same activation.

`skill_overlap.py` measures vocabulary: which commands two skills teach, how much
prose they share. That answers "are these alike" and NOT "do these do the same
thing" — the distinction that blocked the merge decision on 2026-08-19.

Purpose is not what a skill contains; it is **when it should fire**. The only text
the model reads when choosing is `description`, so a description IS a declaration
of purpose, and two skills share purpose exactly when they claim the same
situations.

The method avoids inventing test prompts, which would smuggle in the analyst's
bias about what the skills are for. Every skill already DECLARES its triggers —
"Use when…", "Invoke when…", "Triggers on…", plus any frontmatter `triggers:`.
Those declarations become the queries. For each declared trigger of skill A, all
skills are ranked by BM25 over their descriptions. Then:

* **usurpation** — another skill outranks A on A's OWN declared trigger. The
  strongest available evidence of purpose collision: A staked a claim and lost it.
* **contested** — several skills score within a narrow band at the top, so the
  choice between them is close to arbitrary.
* **self-defence** — A wins its own trigger by a wide margin: a distinct purpose.

BM25 is lexical, so this measures *declared* purpose, not semantics: two skills
that mean the same thing in disjoint words score as distinct. That is a stated
limit, not a hidden one — it makes the finding CONSERVATIVE (it under-reports
collisions, never invents them).
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

# Okapi BM25, standard parameters. k1 damps term-frequency saturation, b controls
# length normalisation — descriptions vary from one line to a full paragraph, so
# b at the usual 0.75 matters here more than in a uniform corpus.
BM25_K1 = 1.5
BM25_B = 0.75

# A rival within this fraction of the winner's score is "contested": the ranking
# separates them by less than the noise of one extra shared word.
CONTESTED_BAND = 0.85

_FRONTMATTER = re.compile(r"\A---\s*\n(.*?)\n---\s*\n", re.DOTALL)
_WORD = re.compile(r"[a-zà-ÿ0-9_]+")

# Trigger clauses skills use to declare when they fire.
_TRIGGER_CLAUSE = re.compile(
    r"(?:use|invoke|activate|apply|reach for)\s+(?:this\s+)?"
    r"(?:skill\s+)?(?:when|whenever|for|if)\s+(.{12,220}?)(?:[.;]|$)",
    re.IGNORECASE | re.DOTALL)
_TRIGGERS_ON = re.compile(r"triggers?\s+on\s+(.{12,220}?)(?:[.;]|$)",
                          re.IGNORECASE | re.DOTALL)

# Words that carry no routing signal — they appear in nearly every description
# and would make every skill look like every other.
_STOP = frozenset("""
a an and are as at be by for from has have in into is it its of on or that the to
with when whenever use uses using this these those which what your you can will
o a os as um uma de do da dos das em no na para por com que se ao aos e ou
skill skills claude code user prompt task
""".split())


def _tokens(text: str) -> list[str]:
    """Lowercase word tokens with stopwords dropped."""
    return [w for w in _WORD.findall(text.lower()) if w not in _STOP and len(w) > 2]


@dataclass
class SkillDoc:
    """A skill reduced to what decides its activation."""

    name: str
    description: str
    declared_triggers: list[str] = field(default_factory=list)
    tokens: list[str] = field(default_factory=list)
    tf: Counter = field(default_factory=Counter)


def _parse_frontmatter(text: str) -> tuple[dict[str, object], str]:
    """Extract flat frontmatter values plus the `triggers:` list.

    Hand-rolled rather than YAML because the corpus mixes `description: >` block
    scalars with plain values, and a strict parser would reject files that the
    Claude Code runtime itself accepts.
    """
    match = _FRONTMATTER.match(text)
    if not match:
        return {}, text
    block = match.group(1)
    meta: dict[str, object] = {}
    triggers: list[str] = []
    current: str | None = None
    buffer: list[str] = []

    for line in block.splitlines():
        list_item = re.match(r"^\s+-\s+(.*)$", line)
        if list_item and current == "triggers":
            triggers.append(list_item.group(1).strip())
            continue
        if line.startswith((" ", "\t")) and current == "description":
            buffer.append(line.strip())
            continue
        key_match = re.match(r"^([a-zA-Z_]+):\s*(.*)$", line)
        if not key_match:
            continue
        if current == "description" and buffer:
            meta["description"] = " ".join(buffer)
            buffer = []
        current = key_match.group(1)
        value = key_match.group(2).strip()
        if current == "description":
            meta["description"] = value.strip("'\"") if value not in (">", "|") else ""
        elif value:
            meta[current] = value.strip("'\"")
    if current == "description" and buffer:
        meta["description"] = " ".join(buffer)
    meta["triggers"] = triggers
    return meta, text[match.end():]


def extract_triggers(description: str, frontmatter_triggers: list[str]) -> list[str]:
    """Every situation the skill claims, as free text.

    Falls back to the description itself when a skill declares no explicit
    trigger — a skill still competes for activation even if it never says
    "use when", and excluding it would silently shrink the contest.
    """
    claims: list[str] = [t for t in frontmatter_triggers if len(t) > 3]
    for pattern in (_TRIGGER_CLAUSE, _TRIGGERS_ON):
        for match in pattern.finditer(description):
            claim = re.sub(r"\s+", " ", match.group(1)).strip(" ,;:-")
            if len(claim) > 10:
                claims.append(claim)
    if not claims and description:
        claims.append(re.sub(r"\s+", " ", description)[:200])
    return claims


class BM25:
    """Okapi BM25 over skill descriptions."""

    def __init__(self, docs: list[SkillDoc]) -> None:
        self.docs = docs
        self.n = len(docs)
        self.avg_len = sum(len(d.tokens) for d in docs) / self.n if self.n else 0.0
        self.df: Counter = Counter()
        for doc in docs:
            for term in set(doc.tokens):
                self.df[term] += 1

    def _idf(self, term: str) -> float:
        """Robertson/Sparck-Jones IDF, floored at 0 so common terms cannot
        contribute negatively and flip a ranking."""
        df = self.df.get(term, 0)
        return max(0.0, math.log((self.n - df + 0.5) / (df + 0.5) + 1.0))

    def score(self, query_tokens: list[str], doc: SkillDoc) -> float:
        """BM25 score of one document against a tokenised query."""
        if not doc.tokens:
            return 0.0
        length_norm = BM25_K1 * (1 - BM25_B + BM25_B * len(doc.tokens) / self.avg_len)
        total = 0.0
        for term in query_tokens:
            freq = doc.tf.get(term, 0)
            if not freq:
                continue
            total += self._idf(term) * (freq * (BM25_K1 + 1)) / (freq + length_norm)
        return total

    def rank(self, query: str) -> list[tuple[str, float]]:
        """All skills ranked for a query, best first, zero-scores dropped."""
        q = _tokens(query)
        scored = [(d.name, self.score(q, d)) for d in self.docs]
        scored = [(n, s) for n, s in scored if s > 0]
        scored.sort(key=lambda x: (-x[1], x[0]))
        return scored


def load_docs(root: Path, only: list[str] | None) -> list[SkillDoc]:
    """Parse every SKILL.md into the activation-relevant projection."""
    docs: list[SkillDoc] = []
    for skill_md in sorted(root.glob("*/SKILL.md")):
        if only and skill_md.parent.name not in only:
            continue
        try:
            text = skill_md.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:
            print(f"warn: {skill_md}: {exc}", file=sys.stderr)
            continue
        meta, _ = _parse_frontmatter(text)
        description = str(meta.get("description") or "")
        doc = SkillDoc(
            name=str(meta.get("name") or skill_md.parent.name),
            description=description,
            declared_triggers=extract_triggers(
                description, list(meta.get("triggers") or [])),
        )
        doc.tokens = _tokens(description)
        doc.tf = Counter(doc.tokens)
        docs.append(doc)
    return docs


def measure(docs: list[SkillDoc]) -> dict[str, object]:
    """Run every declared trigger against the corpus and classify the outcome."""
    bm25 = BM25(docs)
    usurped: list[dict[str, object]] = []
    contested: Counter = Counter()
    defended = 0
    total_claims = 0

    for doc in docs:
        for claim in doc.declared_triggers:
            ranking = bm25.rank(claim)
            if not ranking:
                continue
            total_claims += 1
            winner, top = ranking[0]
            own = next((s for n, s in ranking if n == doc.name), 0.0)

            if winner != doc.name:
                # An owner scoring ZERO on its own declared trigger is not
                # losing a contest — it never entered one. The words of that
                # trigger are absent from its description, and the description
                # is the ONLY text the runtime reads when choosing a skill
                # (confirmed by the skill index injected into context: it
                # carries `name: description` and nothing else). A frontmatter
                # `triggers:` list is inert decoration. Measured 20/08/2026:
                # 57 of 76 declared triggers across the corpus, 75%.
                kind = "inert_trigger" if own == 0.0 else "purpose_collision"
                usurped.append({
                    "owner": doc.name,
                    "trigger": claim[:110],
                    "winner": winner,
                    "winner_score": round(top, 2),
                    "owner_score": round(own, 2),
                    "kind": kind,
                })
            else:
                runner = ranking[1][1] if len(ranking) > 1 else 0.0
                if top > 0 and runner / top >= CONTESTED_BAND:
                    contested[tuple(sorted((doc.name, ranking[1][0])))] += 1
                else:
                    defended += 1

    by_pair: Counter = Counter()
    for row in usurped:
        if row["kind"] == "purpose_collision":
            by_pair[tuple(sorted((str(row["owner"]), str(row["winner"]))))] += 1

    return {
        "skills": len(docs),
        "claims_tested": total_claims,
        "defended": defended,
        "usurped": usurped,
        "collision_pairs": [
            {"a": a, "b": b, "usurpations": n} for (a, b), n in by_pair.most_common()
        ],
        "contested_pairs": [
            {"a": a, "b": b, "near_ties": n} for (a, b), n in contested.most_common()
        ],
    }


def main(argv: list[str] | None = None) -> int:
    """CLI entry point. Always 0 — a measurement, not a gate."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path,
                        default=Path.home() / ".claude" / "skills")
    parser.add_argument("--only", nargs="*")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    docs = load_docs(args.root, args.only)
    if not docs:
        print(f"no skills under {args.root}", file=sys.stderr)
        return 2

    result = measure(docs)
    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0

    print(f"{result['skills']} skills · {result['claims_tested']} gatilhos "
          f"declarados testados · {result['defended']} defendidos com folga\n")

    real = [r for r in result["usurped"] if r["kind"] == "purpose_collision"]
    inert = [r for r in result["usurped"] if r["kind"] == "inert_trigger"]

    print("── COLISÃO DE PROPÓSITO (ambas disputam; a dona perde) ──")
    if not real:
        print("  nenhuma")
    for row in real[:20]:
        print(f"  {row['owner']} perde para {row['winner']}"
              f"  ({row['winner_score']} vs {row['owner_score']})")
        print(f"      gatilho: \"{row['trigger']}\"")

    print(f"\n── GATILHO INERTE ({len(inert)}) — a dona pontua ZERO no que declara ──")
    print("  Não é disputa: as palavras do gatilho não estão na description,")
    print("  e a description é o único texto lido na escolha da skill.")
    for row in inert[:10]:
        print(f"  {row['owner']:<30} \"{row['trigger'][:52]}\"")

    print("\n── PARES EM COLISÃO DE PROPÓSITO ──")
    if not result["collision_pairs"]:
        print("  nenhum")
    for row in result["collision_pairs"][:15]:
        print(f"  {row['a']:<30} ↔ {row['b']:<30} {row['usurpations']} usurpação(ões)")

    print("\n── PARES CONTESTADOS (empate técnico no topo) ──")
    if not result["contested_pairs"]:
        print("  nenhum")
    for row in result["contested_pairs"][:15]:
        print(f"  {row['a']:<30} ↔ {row['b']:<30} {row['near_ties']} quase-empate(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
