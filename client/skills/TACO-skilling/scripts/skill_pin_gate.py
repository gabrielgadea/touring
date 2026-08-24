#!/usr/bin/env python3
# #tags: kind:script lang:python purpose:gate domain:skills
"""Pin skills to their sources and fail when a source moved without them.

A skill is a CLAIM about something else — a rule, a CLI surface, a constitution,
a module. Claims rot when what they describe changes, and nothing today notices:
`sync-client-skills.py` mirrors live→client byte for byte, so a skill that has
gone stale is mirrored faithfully as a stale skill.

This gate closes that. Each skill declares its sources in `pins.json` (a sibling
of SKILL.md — deliberately NOT the frontmatter, which the runtime parses for
activation and which no gate should risk breaking). `--check` recomputes every
pin and reports the ones whose source moved after the skill last did.

Three pin kinds, three different notions of "moved":

* `file`   — sha256 of the file's bytes. Any edit trips it.
* `cli`    — a `touring` subcommand the skill teaches. Trips when the command
             STOPS EXISTING, which is the failure that makes a skill actively
             harmful rather than merely dated.
* `symbol` — a Rust path the skill names. Trips when the file is gone.

A tripped pin is not proof the skill is wrong; it is proof that nobody checked.
That is the honest claim, and it is why `--check` prints what changed rather
than guessing what the skill should now say.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

PINS_FILENAME = "pins.json"

# Default for a developer machine. In CI there is no ~/.claude — only the
# versioned mirror `client/` — so every resolution goes through this module-level
# binding, which `--claude-root` rebinds. Hardcoding Path.home() inside the
# resolvers would have made the gate unrunnable exactly where it must run.
CLAUDE_ROOT = Path.home() / ".claude"

_FRONTMATTER = re.compile(r"\A---\s*\n(.*?)\n---\s*\n", re.DOTALL)
_RULE_REF = re.compile(r"(?:~/\.claude/rules/|\brules/)([a-z0-9-]+)\.md")
# A skill's canonical reference is a source like any other — and it is the
# STRONGEST kind, because a skill that defers to a reference ("where the two
# diverge, the reference wins") is asserting something about a file it does not
# own. Missing this pattern meant the most load-bearing dependency in the corpus
# was unpinnable (found 20/08/2026, minutes after creating one).
_SKILL_REF_DOC = re.compile(
    # Case-INSENSITIVE on the filename: reference docs carry names like
    # `TACO-subagent-rule.md`, and a lowercase-only class silently matched
    # nothing — the pin count still looked healthy because other sources filled
    # it in. Verifying WHICH sources were pinned, not how many, caught it.
    r"skills/([A-Za-z][A-Za-z0-9-]+)/references/([A-Za-z0-9_-]+)\.md")
_CRATE_REF = re.compile(r"\b(crates/[a-z0-9-]+/src/[a-z0-9_/]+\.rs)\b")
_CLAUDEMD = re.compile(r"~/\.claude/CLAUDE\.md|\bCLAUDE\.md\b")
_TOURING_CMD = re.compile(
    r"\btouring\s+([a-z][a-z0-9-]{1,30})(?:\s+([a-z][a-z0-9-]{1,30}))?")

# Words that follow `touring ` in prose without being subcommands.
_CMD_STOPWORDS = frozenset({
    "is", "the", "and", "for", "with", "cli", "as", "in", "to", "workspace",
    "daemon", "skill", "when", "if", "or", "of", "that", "this", "was", "are",
    "does", "can", "will", "has", "have", "master", "commands", "command",
})


@dataclass(frozen=True)
class Pin:
    """One declared dependency of a skill on something outside it."""

    kind: str          # file | cli | symbol
    ref: str           # path relative to a root, or a CLI subcommand
    fingerprint: str   # sha256 for files; "present" for cli/symbol

    def to_json(self) -> dict[str, str]:
        return {"kind": self.kind, "ref": self.ref, "fingerprint": self.fingerprint}

    @staticmethod
    def from_json(raw: dict[str, str]) -> "Pin":
        return Pin(raw["kind"], raw["ref"], raw.get("fingerprint", ""))


def sha256_file(path: Path) -> str | None:
    """Hex digest of a file, or None when it does not exist / cannot be read."""
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return None


def resolve_ref(ref: str, repo_root: Path) -> Path:
    """Map a pin ref to a real path. `rules/x.md` and CLAUDE.md live under ~/.claude."""
    if ref.startswith(("rules/", "CLAUDE.md", "skills/")):
        return CLAUDE_ROOT / ref
    return repo_root / ref


def live_touring_commands() -> set[str]:
    """Top-level subcommands the installed `touring` actually exposes.

    An empty set means the binary could not be consulted — the caller MUST treat
    that as "unknown", never as "no commands exist", or every cli pin in the
    corpus trips at once on a machine where the daemon is down.
    """
    try:
        proc = subprocess.run(
            ["touring", "--help"], capture_output=True, text=True, timeout=30,
            check=False)
    except (OSError, subprocess.SubprocessError):
        return set()
    found: set[str] = set()
    for line in (proc.stdout + proc.stderr).splitlines():
        match = re.match(r"^\s{2,}([a-z][a-z0-9-]{1,30})\b", line)
        if match:
            found.add(match.group(1))
    return found


def derive_pins(skill_md: Path, repo_root: Path,
                live_cmds: set[str]) -> list[Pin]:
    """Infer a skill's sources from what its text asserts.

    Derivation is a STARTING POINT, not the contract: it finds what the skill
    happens to mention. A human curating `pins.json` afterwards is expected.
    """
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    pins: list[Pin] = []
    seen: set[tuple[str, str]] = set()

    def add(kind: str, ref: str, fingerprint: str) -> None:
        if (kind, ref) not in seen:
            seen.add((kind, ref))
            pins.append(Pin(kind, ref, fingerprint))

    for match in _RULE_REF.finditer(text):
        ref = f"rules/{match.group(1)}.md"
        digest = sha256_file(resolve_ref(ref, repo_root))
        if digest:
            add("file", ref, digest)

    for match in _SKILL_REF_DOC.finditer(text):
        ref = f"skills/{match.group(1)}/references/{match.group(2)}.md"
        digest = sha256_file(CLAUDE_ROOT / ref)
        if digest:
            add("file", ref, digest)

    if _CLAUDEMD.search(text):
        digest = sha256_file(CLAUDE_ROOT / "CLAUDE.md")
        if digest:
            add("file", "CLAUDE.md", digest)

    for match in _CRATE_REF.finditer(text):
        ref = match.group(1)
        digest = sha256_file(repo_root / ref)
        if digest:
            add("symbol", ref, digest)

    # Only pin CLI commands that exist NOW; a command the skill invents is
    # reported by --check as a broken claim, not silently pinned as real.
    for match in _TOURING_CMD.finditer(text):
        head = match.group(1)
        if head in _CMD_STOPWORDS or (live_cmds and head not in live_cmds):
            continue
        add("cli", head, "present")

    return pins


def load_pins(skill_dir: Path) -> list[Pin] | None:
    """Read `pins.json`, or None when the skill has never been pinned."""
    path = skill_dir / PINS_FILENAME
    if not path.exists():
        return None
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"warn: {path} unreadable ({exc}) — treating as unpinned",
              file=sys.stderr)
        return None
    return [Pin.from_json(p) for p in raw.get("pins", [])]


def write_pins(skill_dir: Path, pins: list[Pin]) -> None:
    """Persist pins next to SKILL.md, sorted so the file is diff-stable."""
    payload = {
        "note": ("Sources this skill makes claims about. Regenerate with "
                 "skill_pin_gate.py --update AFTER reviewing the skill against "
                 "the changed source — never before."),
        "pins": [p.to_json() for p in sorted(pins, key=lambda p: (p.kind, p.ref))],
    }
    (skill_dir / PINS_FILENAME).write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


# A line that DENIES a command does not teach it — it protects against it. The
# corpus carries five such warnings for `touring quality`, and counting them as
# usage inverted the finding completely (20/08/2026): the skills were defending
# against the ghost, not spreading it.
_NEGATION = re.compile(
    r"\bNOT\b|NÃO existe|não existe|inexistente|anti-alucina|⚠|does not exist|no longer",
    re.IGNORECASE)

# `touring <word>` in prose is not always a command invocation.
_PROSE_AFTER_TOURING = frozenset({
    "is", "the", "and", "for", "with", "cli", "as", "in", "to", "workspace",
    "daemon", "skill", "when", "if", "or", "of", "that", "this", "was", "are",
    "does", "can", "will", "has", "have", "master", "commands", "command",
    "tools", "binary", "project", "code", "sem", "desativa", "description",
    "e", "o", "a", "um", "uma", "por", "com", "na", "no", "que", "dos", "das",
    "cada", "antes", "depois", "via", "tem",
})


_SUBCMD_CACHE: dict[str, frozenset[str] | None] = {}


def live_subcommands(head: str) -> frozenset[str] | None:
    """Second-level subcommands `touring <head>` exposes, or None if unknowable.

    The gate verified only the FIRST level until 20/08/2026, so `touring search
    symbols` — a subcommand that does not exist — passed for as long as `touring
    search` existed. The extracting regex had captured the second level all
    along; only the verification ignored it (the same shape as the definer-module
    defect: the datum was there, one site used it).

    None means "could not be parsed" and the caller MUST skip: a command family
    whose help lists no subcommands takes ARGUMENTS, and accusing its arguments
    of being ghosts is exactly the false-accusation this gate must never make.
    """
    if head in _SUBCMD_CACHE:
        return _SUBCMD_CACHE[head]
    result: frozenset[str] | None = None
    try:
        proc = subprocess.run(["touring", head, "--help"], capture_output=True,
                              text=True, timeout=30, check=False)
    except (OSError, subprocess.SubprocessError):
        _SUBCMD_CACHE[head] = None
        return None
    text = proc.stdout + proc.stderr
    found: set[str] = set()
    in_commands = False
    for line in text.splitlines():
        if re.match(r"^(Commands|SUBCOMMANDS|positional arguments):", line.strip()):
            in_commands = True
            continue
        if in_commands:
            if not line.strip():
                if found:
                    break
                continue
            if re.match(r"^\S", line):  # next section header ends the block
                break
            m = re.match(r"^\s{2,}([a-z][a-z0-9-]{1,30})\b", line)
            if m:
                found.add(m.group(1))
        # argparse renders choices inline: `{add,get,ready,claim}`
        for choices in re.findall(r"\{([a-z][a-z0-9,-]{3,})\}", line):
            found.update(c for c in choices.split(",") if c)
    if found:
        result = frozenset(found)
    _SUBCMD_CACHE[head] = result
    return result


def ghost_commands(skill_md: Path, live_cmds: set[str]) -> list[dict[str, object]]:
    """Commands a skill TEACHES that `touring --help` does not expose.

    A skill that documents a command which no longer exists is worse than an
    out-of-date skill — it is a skill that actively sends the reader to an error.
    Returns [] when the live command set is unknown, never a false accusation.
    """
    if not live_cmds:
        return []
    found: dict[str, list[int]] = {}
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    for lineno, line in enumerate(text.splitlines(), start=1):
        if _NEGATION.search(line):
            continue
        for match in _TOURING_CMD.finditer(line):
            head = match.group(1)
            if head in _PROSE_AFTER_TOURING:
                continue
            if head not in live_cmds:
                found.setdefault(head, []).append(lineno)
                continue
            sub = match.group(2)
            if not sub or sub in _PROSE_AFTER_TOURING:
                continue
            subs = live_subcommands(head)
            if subs is None or sub in subs:
                continue  # unknowable, or real → never accuse
            found.setdefault(f"{head} {sub}", []).append(lineno)
    return [{"command": c, "lines": ln} for c, ln in sorted(found.items())]


def check_skill(skill_dir: Path, repo_root: Path,
                live_cmds: set[str]) -> dict[str, object]:
    """Recompute every pin of one skill and report what moved."""
    ghosts = ghost_commands(skill_dir / "SKILL.md", live_cmds)
    pins = load_pins(skill_dir)
    if pins is None:
        return {"skill": skill_dir.name,
                "status": "ghosts" if ghosts else "unpinned",
                "drift": [], "ghosts": ghosts}

    drift: list[dict[str, str]] = []
    unverifiable: list[dict[str, str]] = []
    for pin in pins:
        if pin.kind in ("file", "symbol"):
            resolved = resolve_ref(pin.ref, repo_root)
            current = sha256_file(resolved)
            if current is None:
                # "Absent from THIS checkout" is not "deleted". The global
                # CLAUDE.md is not part of the versioned mirror, so in CI five
                # skills would report drift on every single run — and a gate that
                # always fails is a gate everyone learns to skip. Report it as
                # unverifiable; only a source missing where it SHOULD live blocks.
                unverifiable.append({"kind": pin.kind, "ref": pin.ref,
                                     "what": "source not present in this checkout"})
            elif current != pin.fingerprint:
                drift.append({"kind": pin.kind, "ref": pin.ref,
                              "what": "source changed since the skill was pinned"})
        elif pin.kind == "cli":
            # Unknown command set → cannot judge. Silence beats a false alarm
            # across the whole corpus (the daemon being down is not skill drift).
            if live_cmds and pin.ref not in live_cmds:
                drift.append({"kind": "cli", "ref": pin.ref,
                              "what": "command no longer exists in `touring --help`"})

    return {
        "skill": skill_dir.name,
        "status": "drift" if (drift or ghosts) else "clean",
        "pins": len(pins),
        "drift": drift,
        "ghosts": ghosts,
        "unverifiable": unverifiable,
    }


def iter_skill_dirs(root: Path, only: list[str] | None) -> list[Path]:
    """Every directory under `root` holding a SKILL.md."""
    return [p.parent for p in sorted(root.glob("*/SKILL.md"))
            if not only or p.parent.name in only]


def main(argv: list[str] | None = None) -> int:
    """CLI entry point. exit 0 clean · 1 drift found · 2 usage/environment error."""
    global CLAUDE_ROOT  # rebound by --claude-root; declared first because the
    # argparse defaults below already read it.
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=CLAUDE_ROOT / "skills")
    parser.add_argument("--repo-root", type=Path,
                        default=Path.home() / "projects" / "touring")
    parser.add_argument("--only", nargs="*")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", default=True)
    mode.add_argument("--derive", action="store_true",
                      help="write pins.json for skills that have none")
    mode.add_argument("--update", action="store_true",
                      help="re-pin ALL selected skills to current sources")
    parser.add_argument("--claude-root", type=Path, default=None,
                        help="root holding rules/ and CLAUDE.md (CI: the "
                             "versioned mirror, e.g. client/)")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    if args.claude_root is not None:
        CLAUDE_ROOT = args.claude_root

    if not args.root.is_dir():
        print(f"error: no skills root at {args.root}", file=sys.stderr)
        return 2

    live_cmds = live_touring_commands()
    if not live_cmds:
        print("warn: `touring --help` unavailable — cli pins will not be judged",
              file=sys.stderr)

    skill_dirs = iter_skill_dirs(args.root, args.only)
    if args.derive or args.update:
        written = 0
        for skill_dir in skill_dirs:
            if args.derive and (skill_dir / PINS_FILENAME).exists():
                continue
            pins = derive_pins(skill_dir / "SKILL.md", args.repo_root, live_cmds)
            if pins:
                write_pins(skill_dir, pins)
                written += 1
                print(f"  pinned {skill_dir.name:<34} {len(pins)} sources")
        print(f"\n{written} skills pinned")
        return 0

    results = [check_skill(d, args.repo_root, live_cmds) for d in skill_dirs]
    drifted = [r for r in results if r["status"] == "drift"]
    unpinned = [r for r in results if r["status"] == "unpinned"]

    if args.json:
        print(json.dumps({"results": results,
                          "drift_count": len(drifted),
                          "unpinned_count": len(unpinned)},
                         indent=2, ensure_ascii=False))
        return 1 if drifted else 0

    print(f"verificadas {len(results)} skills\n")
    if drifted:
        print("── SKILLS COM FONTE ALTERADA (revisar) ──")
        for row in drifted:
            print(f"  {row['skill']}")
            for d in row["drift"]:
                print(f"      [{d['kind']}] {d['ref']} — {d['what']}")
    else:
        print("── nenhuma skill pinada divergiu ──")
    unver = sum(len(r.get("unverifiable", [])) for r in results)
    if unver:
        print(f"\n── {unver} pin(s) não verificáveis neste checkout "
              f"(fonte fora do espelho — informativo, não bloqueia) ──")

    ghosted = [r for r in results if r.get("ghosts")]
    if ghosted:
        print("\n── COMANDOS ENSINADOS QUE NÃO EXISTEM (a skill leva a um erro) ──")
        for row in ghosted:
            for g in row["ghosts"]:
                lines = ",".join(str(n) for n in g["lines"][:4])
                print(f"  {row['skill']:<28} touring {g['command']:<22} L{lines}")

    if unpinned:
        print(f"\n── SEM PINS ({len(unpinned)}) ──")
        print("  " + " ".join(r["skill"] for r in unpinned[:20]))
    return 1 if (drifted or ghosted) else 0


if __name__ == "__main__":
    sys.exit(main())
