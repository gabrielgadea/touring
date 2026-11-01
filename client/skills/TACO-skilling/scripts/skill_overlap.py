#!/usr/bin/env python3
# #tags: kind:script lang:python purpose:analysis domain:skills
"""Measure real overlap between Claude Code Agent Skills — by content, not by headings.

Headings are presentation metadata: two skills can carry different titles over
identical prose, or identical titles over unrelated prose. Deciding a merge from
them is deciding from the wrong instrument. This module measures four things that
actually bear on consolidation, each falsifiable by re-running it:

1. **activation collision** — skills whose `description` (the ONLY text the model
   reads when choosing) covers the same trigger space. This is the expensive
   redundancy: every description is loaded in every session.
2. **content near-duplication** — Jaccard over normalised k-shingles, the classic
   near-duplicate signal, computed over prose with code fences and links stripped.
3. **command surface** — which `touring …` / script invocations each skill
   teaches, so "this skill only wraps commands the master already documents" is a
   measurement instead of an impression.
4. **source dependency** — which rules / CLAUDE.md / repo paths a skill asserts
   things about. This is the edge set the co-evolution gate needs: when a source
   changes, the skills pinned to it are the ones at risk of going stale.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

# A shingle of 5 words is the usual floor for prose near-duplication: shorter
# windows match on common phrasing ("use this when the user"), longer ones miss
# paraphrase. Tuned on this corpus; changing it changes every Jaccard below.
SHINGLE_K = 5

# Pair thresholds. Deliberately NOT symmetric with each other: a description
# collision is costlier than prose reuse, because descriptions compete at
# activation time in every single session.
ACTIVATION_ALERT = 0.30
CONTENT_ALERT = 0.25
COMMAND_ALERT = 0.60

_FRONTMATTER = re.compile(r"\A---\s*\n(.*?)\n---\s*\n", re.DOTALL)
_CODE_FENCE = re.compile(r"```.*?```", re.DOTALL)
_INLINE_CODE = re.compile(r"`[^`]*`")
_MD_LINK = re.compile(r"\[([^\]]*)\]\([^)]*\)")
_WORD = re.compile(r"[a-z0-9_]+")

# `touring <subcommand>` and `touring-<binary>`; the subcommand is the unit that
# decides whether two skills teach the same thing.
# Two levels, because the unit that decides "same capability" is `ast meta`,
# not `ast`. Capturing one level collapsed `ast meta` / `ast blast` / `ast tdg`
# into a single token and produced a 1.00 command overlap between two skills
# that share nothing but the word `ast` (observed 2026-08-19).
_TOURING_CMD = re.compile(
    r"\btouring(?:-[a-z]+)?\s+([a-z][a-z0-9-]{1,30})(?:\s+([a-z][a-z0-9-]{1,30}))?")
_SCRIPT_CMD = re.compile(r"\b([a-z_][a-z0-9_]{2,40}\.py)\b")

# Sources a skill can go stale against.
_RULE_REF = re.compile(r"(?:rules/|\.claude/rules/)([a-z0-9-]+)\.md")
_CLAUDEMD_REF = re.compile(r"\bCLAUDE\.md\b")
_CRATE_REF = re.compile(r"\b(crates/[a-z0-9-]+/src/[a-z0-9_/]+\.rs)\b")
_SKILL_REF = re.compile(r"skills/([A-Za-z][A-Za-z0-9-]+)/")


def _strip_frontmatter(text: str) -> tuple[dict[str, str], str]:
    """Split YAML frontmatter from body.

    Only the flat `key: value` pairs are read — enough for `name`/`description`
    and immune to the multi-line `description: >` form that a real YAML parser
    would be needed for (those simply yield an empty value, never a crash).
    """
    match = _FRONTMATTER.match(text)
    if not match:
        return {}, text
    meta: dict[str, str] = {}
    for line in match.group(1).splitlines():
        if line.startswith((" ", "\t", "#")) or ":" not in line:
            continue
        key, _, value = line.partition(":")
        meta[key.strip()] = value.strip().strip("\"'")
    return meta, text[match.end():]


def _normalise_prose(body: str) -> list[str]:
    """Body → lowercase word list, with code and link targets removed.

    Code fences and inline code are stripped because two skills quoting the same
    CLI command are not duplicating prose — that overlap is measured separately
    and deliberately, as `command surface`.
    """
    body = _CODE_FENCE.sub(" ", body)
    body = _INLINE_CODE.sub(" ", body)
    body = _MD_LINK.sub(r"\1", body)
    return _WORD.findall(body.lower())


def _shingles(words: list[str], k: int = SHINGLE_K) -> set[str]:
    """Overlapping k-word windows. Fewer than k words yields a single shingle."""
    if len(words) < k:
        return {" ".join(words)} if words else set()
    return {" ".join(words[i:i + k]) for i in range(len(words) - k + 1)}


def _jaccard(left: set[str], right: set[str]) -> float:
    """|A∩B| / |A∪B|; 0.0 when both are empty (no overlap, not undefined)."""
    if not left and not right:
        return 0.0
    union = left | right
    return len(left & right) / len(union) if union else 0.0


@dataclass
class Skill:
    """One parsed SKILL.md plus everything derived from it."""

    name: str
    path: Path
    description: str
    line_count: int
    desc_shingles: set[str] = field(default_factory=set)
    body_shingles: set[str] = field(default_factory=set)
    commands: set[str] = field(default_factory=set)
    scripts: set[str] = field(default_factory=set)
    rules: set[str] = field(default_factory=set)
    crates: set[str] = field(default_factory=set)
    skill_refs: set[str] = field(default_factory=set)
    cites_claude_md: bool = False

    @property
    def sources(self) -> list[str]:
        """Every external thing this skill asserts something about."""
        out = [f"rule:{r}" for r in sorted(self.rules)]
        out += [f"crate:{c}" for c in sorted(self.crates)]
        if self.cites_claude_md:
            out.append("claude_md")
        return out


def parse_skill(skill_md: Path) -> Skill:
    """Read one SKILL.md into a Skill. Unreadable bytes are replaced, never fatal."""
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    meta, body = _strip_frontmatter(text)
    description = meta.get("description", "")
    words = _normalise_prose(body)
    return Skill(
        name=meta.get("name") or skill_md.parent.name,
        path=skill_md,
        description=description,
        line_count=text.count("\n") + 1,
        # 3-word windows on descriptions: they are one or two sentences, so the
        # 5-word prose window would find almost nothing to compare.
        desc_shingles=_shingles(_WORD.findall(description.lower()), k=3),
        body_shingles=_shingles(words),
        commands={
            f"{m.group(1)} {m.group(2)}" if m.group(2) else m.group(1)
            for m in _TOURING_CMD.finditer(text)
        },
        scripts={m.group(1) for m in _SCRIPT_CMD.finditer(text)},
        rules={m.group(1) for m in _RULE_REF.finditer(text)},
        crates={m.group(1) for m in _CRATE_REF.finditer(text)},
        skill_refs={m.group(1) for m in _SKILL_REF.finditer(text)},
        cites_claude_md=bool(_CLAUDEMD_REF.search(text)),
    )


def load_skills(root: Path, only: list[str] | None = None) -> list[Skill]:
    """Parse every `<root>/*/SKILL.md`, optionally restricted to `only` names."""
    skills: list[Skill] = []
    for skill_md in sorted(root.glob("*/SKILL.md")):
        if only and skill_md.parent.name not in only:
            continue
        try:
            skills.append(parse_skill(skill_md))
        except OSError as exc:
            print(f"warn: unreadable {skill_md}: {exc}", file=sys.stderr)
    return skills


def pair_report(skills: list[Skill]) -> list[dict[str, object]]:
    """All pairs that trip at least one alert threshold, strongest first."""
    rows: list[dict[str, object]] = []
    for i, a in enumerate(skills):
        for b in skills[i + 1:]:
            activation = _jaccard(a.desc_shingles, b.desc_shingles)
            content = _jaccard(a.body_shingles, b.body_shingles)
            cmd = _jaccard(a.commands, b.commands)
            if (activation < ACTIVATION_ALERT and content < CONTENT_ALERT
                    and cmd < COMMAND_ALERT):
                continue
            rows.append({
                "a": a.name, "b": b.name,
                "activation_overlap": round(activation, 3),
                "content_overlap": round(content, 3),
                "command_overlap": round(cmd, 3),
                "shared_commands": sorted(a.commands & b.commands)[:12],
                "a_lines": a.line_count, "b_lines": b.line_count,
            })
    rows.sort(key=lambda r: (r["activation_overlap"], r["content_overlap"]), reverse=True)
    return rows


def subsumption_report(skills: list[Skill], master: str) -> list[dict[str, object]]:
    """Skills whose command surface the master already covers.

    `covered` is asymmetric on purpose: the question is not "are these alike"
    but "does the master already teach everything this one teaches" — the only
    form of overlap that justifies deleting the smaller skill outright.
    """
    # Match on the frontmatter `name` OR the directory name: the master ships as
    # `name: touring` in a directory called `Touring`, and passing the directory
    # name used to yield an empty report that read exactly like "no subsumption
    # found" (observed 2026-08-19 — the silent-failure shape this whole analysis
    # exists to eliminate). An unknown master is a caller error, so it raises.
    hub = next((s for s in skills
                if master in (s.name, s.path.parent.name)), None)
    if hub is None:
        known = ", ".join(sorted(s.name for s in skills)[:8])
        raise KeyError(
            f"master skill {master!r} not found among {len(skills)} skills "
            f"(names look like: {known}…) — subsumption cannot be computed")
    rows: list[dict[str, object]] = []
    for skill in skills:
        if skill.name == master or not skill.commands:
            continue
        missing = skill.commands - hub.commands
        rows.append({
            "skill": skill.name,
            "lines": skill.line_count,
            "commands": len(skill.commands),
            "covered_by_master": round(
                1 - len(missing) / len(skill.commands), 3),
            "unique_to_skill": sorted(missing)[:10],
        })
    rows.sort(key=lambda r: (-r["covered_by_master"], r["lines"]))
    return rows


def source_graph(skills: list[Skill]) -> dict[str, list[str]]:
    """source → skills that depend on it. The co-evolution gate's edge set."""
    graph: dict[str, list[str]] = {}
    for skill in skills:
        for source in skill.sources:
            graph.setdefault(source, []).append(skill.name)
    return {k: sorted(v) for k, v in sorted(graph.items())}


def main(argv: list[str] | None = None) -> int:
    """CLI entry point. Returns 0 always — this is a measurement, not a gate."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path,
                        default=Path.home() / ".claude" / "skills")
    parser.add_argument("--only", nargs="*",
                        help="restrict to these skill directory names")
    parser.add_argument("--master", default="Touring",
                        help="skill treated as the hub for subsumption")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    skills = load_skills(args.root, args.only)
    if not skills:
        print(f"no SKILL.md under {args.root}", file=sys.stderr)
        return 0

    try:
        subsumption = subsumption_report(skills, args.master)
    except KeyError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    result = {
        "skills_analysed": len(skills),
        "pairs": pair_report(skills),
        "subsumption": subsumption,
        "source_graph": source_graph(skills),
        "thresholds": {
            "activation": ACTIVATION_ALERT,
            "content": CONTENT_ALERT,
            "command": COMMAND_ALERT,
            "shingle_k": SHINGLE_K,
        },
    }

    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0

    print(f"analysed {len(skills)} skills under {args.root}\n")
    print("── PARES ACIMA DO LIMIAR (ativação / conteúdo / comandos) ──")
    if not result["pairs"]:
        print("  nenhum")
    for row in result["pairs"][:25]:
        print(f"  {row['a']:<32} ↔ {row['b']:<32} "
              f"ativ={row['activation_overlap']:.2f} "
              f"cont={row['content_overlap']:.2f} "
              f"cmd={row['command_overlap']:.2f}")
        if row["shared_commands"]:
            print(f"       comandos comuns: {' '.join(row['shared_commands'])}")

    print(f"\n── SUBSUNÇÃO PELO MASTER ({args.master}) ──")
    for row in result["subsumption"][:25]:
        flag = "  ← 100% coberta" if row["covered_by_master"] >= 1.0 else ""
        print(f"  {row['skill']:<34} {row['lines']:>4}L  "
              f"cobertura={row['covered_by_master']:.0%}{flag}")
        if row["unique_to_skill"]:
            print(f"       só nela: {' '.join(row['unique_to_skill'])}")

    print("\n── GRAFO DE FONTES (o que pinar) ──")
    for source, dependents in result["source_graph"].items():
        if len(dependents) >= 2:
            print(f"  {source:<38} → {len(dependents)} skills: "
                  f"{' '.join(dependents[:6])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
