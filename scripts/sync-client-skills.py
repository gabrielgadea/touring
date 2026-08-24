#!/usr/bin/env python3
"""sync-client-skills.py — keep `client/` in step with the skills that actually run.

`client/{skills,rules,agents}` is the versioned copy, inside this repo, of the
Claude Code assets the product ships. What EXECUTES is `~/.claude/{...}`; the repo
copy exists so those assets travel with the code (and the repo is public, so it is
also what anyone cloning reads).

Nothing kept the two in step. The mirror was written by a single bulk copy on
2026-07-25 and then diverged for three weeks, which is how a shell-injection fix
reached the mirror and the instantiations but never the DEPLOYED library that
`touring adw from-template` copies from — the structural guard scanned the two
places that had been fixed. A mirror nobody compares is not a backup; it is a
second, quieter source of truth.

Direction is one-way by design: **live is the source, `client/` is the mirror.**
Work happens in `~/.claude/` because that is what runs; the repo follows.

Scope, deliberately different per area:
  skills — every directory present in `client/skills/`. Inside one, EVERYTHING
           mirrors, new files included: a script added to a mirrored skill is part
           of that skill.
  rules  — only the files already present in `client/rules/`. The mirror is a
  agents   SELECTION of the operator's global assets; auto-adding every live file
           would drag personal, unrelated rules into a public repo.

Usage:
    python3 scripts/sync-client-skills.py --check     # exit 1 if drifted
    python3 scripts/sync-client-skills.py --apply     # live -> mirror
    python3 scripts/sync-client-skills.py --check --json

`--apply` adds and updates. It never deletes: a file present in the mirror and
absent live is reported and left alone unless `--prune` is passed, because the
usual cause is a rename the operator has not finished, not a deletion.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

#: Areas mirrored, and whether new live files are adopted automatically.
#: `skills` discovers scope per DIRECTORY (everything inside one is product);
#: `rules`/`agents` discover it per FILE (the mirror is a curated selection).
AREAS: tuple[tuple[str, str], ...] = (("skills", "dir"), ("rules", "file"), ("agents", "file"))

#: Never mirrored: build artifacts, editor residue, and RUNTIME STATE.
#:
#: `.claude/` is the last one and the one that had already leaked in: indexing a
#: skill directory leaves `.claude/touring/{memory,knowledge,symbols,graph}.db`
#: beside it, and the 2026-07-25 bulk copy swept 31 of them (4.9 MB) into a public
#: repo. They are regenerable per-directory databases, not product — mirroring
#: them publishes one machine's runtime state and makes every reindex look like a
#: source change.
IGNORED_DIRS = {
    "__pycache__", ".pytest_cache", ".ruff_cache", ".mypy_cache", "node_modules", ".claude",
    # `.baseline/` is measured runtime state: `loop_converged.py::clause_orphans`
    # writes the orphan count for whatever scope it was pointed at, so simply
    # RUNNING the gate from inside a mirrored tree deposits a machine-written,
    # machine-specific file there. Observed 2026-08-19 — a `--scope .` smoke test
    # of the judge put one in the skill's own scripts/ dir and the mirror offered
    # to version it.
    ".baseline",
}
IGNORED_SUFFIXES = {".pyc", ".pyo", ".swp", ".db", ".db-wal", ".db-shm", ".sqlite", ".sqlite3"}
IGNORED_NAMES = {".DS_Store"}


def live_root() -> Path:
    return Path.home() / ".claude"


def repo_root() -> Path:
    """The workspace root, derived from this file's location (scripts/…)."""
    return Path(__file__).resolve().parent.parent


def is_noise(path: Path) -> bool:
    """True for generated/runtime files that must never be mirrored.

    The path MUST be relative to its area root. An absolute one carries the root's
    own components — and the live root IS `~/.claude`, whose name is in
    IGNORED_DIRS — so every live file classified as noise, the live-side walk
    contributed nothing, and a new file inside a mirrored skill was never adopted.
    Silently: `client/` still received UPDATES to paths it already knew, so the
    mirror looked healthy. The unit test passed too, because its fake live root
    had no `.claude` in the path — the condition was never exercised where it
    actually holds. Refusing an absolute path makes the misuse unrepresentable
    instead of merely fixing today's three call sites."""
    if path.is_absolute():
        raise ValueError(
            f"is_noise() takes a path relative to its area root, got {path}")
    return (
        bool(IGNORED_DIRS & set(path.parts))
        or path.suffix in IGNORED_SUFFIXES
        or path.name in IGNORED_NAMES
        or ".bak" in path.name
    )


def mirrored_files(area: str, kind: str, live: Path, mirror: Path) -> list[Path]:
    """Paths relative to the area root that belong in the mirror.

    For a `dir` area this is the union of what the mirror already holds and what
    lives under the same skill directories — that union is what lets a new file
    inside a mirrored skill be adopted while a brand-new skill stays out until
    someone creates its directory in `client/`."""
    rels: set[Path] = set()
    if mirror.is_dir():
        rels |= {
            rel for rel in (f.relative_to(mirror) for f in mirror.rglob("*") if f.is_file())
            if not is_noise(rel)
        }
    if kind == "dir" and mirror.is_dir():
        for scope in (d for d in mirror.iterdir() if d.is_dir()):
            source = live / scope.name
            if not source.is_dir():
                continue
            rels |= {
                rel for rel in (f.relative_to(live) for f in source.rglob("*") if f.is_file())
                if not is_noise(rel)
            }
    return sorted(rels)


#: Written by --apply, describing the MIRROR's own contents. This is what makes the
#: gate checkable on a machine that has no `~/.claude` — CI can prove nobody edited
#: `client/` by hand or slipped a file in beside the sync, which matters because such
#: an edit is silently reverted by the next --apply and its author never learns why.
MANIFEST = "MANIFEST.sha256"


def sha256(path: Path) -> str:
    import hashlib

    return hashlib.sha256(path.read_bytes()).hexdigest()


def mirror_entries(repo_base: Path) -> list[str]:
    """Every mirrored file, as `<sha256>  <area>/<relpath>`, sorted."""
    out: list[str] = []
    for area, _ in AREAS:
        area_dir = repo_base / area
        if not area_dir.is_dir():
            continue
        for f in sorted(area_dir.rglob("*")):
            if f.is_file() and not is_noise(f.relative_to(area_dir)):
                out.append(f"{sha256(f)}  {area}/{f.relative_to(area_dir).as_posix()}")
    return out


def write_manifest(repo_base: Path) -> None:
    (repo_base / MANIFEST).write_text("\n".join(mirror_entries(repo_base)) + "\n", encoding="utf-8")


def check_manifest(repo_base: Path) -> dict:
    """Does the mirror still match the manifest recorded at the last sync?"""
    path = repo_base / MANIFEST
    result = {"manifest_present": path.is_file(), "manifest_mismatch": []}
    if not path.is_file():
        return result
    recorded = {
        line.split("  ", 1)[1]: line.split("  ", 1)[0]
        for line in path.read_text(encoding="utf-8").splitlines()
        if "  " in line
    }
    actual = {
        line.split("  ", 1)[1]: line.split("  ", 1)[0] for line in mirror_entries(repo_base)
    }
    for entry in sorted(set(recorded) | set(actual)):
        if recorded.get(entry) != actual.get(entry):
            kind = ("editado à mão" if entry in recorded and entry in actual
                    else "acrescentado fora do sync" if entry in actual
                    else "removido do espelho")
            result["manifest_mismatch"].append(f"{entry} ({kind})")
    return result


def survey() -> dict:
    """Compare every mirrored path; classify without touching anything."""
    live_base, repo_base = live_root(), repo_root() / "client"
    report = {
        "live_root": str(live_base),
        "mirror_root": str(repo_base),
        "live_present": live_base.is_dir(),
        "drifted": [],       # content differs — the mirror is stale
        "missing": [],       # exists live, never reached the mirror
        "orphaned": [],      # exists in the mirror, gone live (reported, never auto-removed)
        "junk": [],          # runtime state already in the mirror from before it was excluded
        "in_sync": 0,
    }
    # Junk sweep is scoped to the MIRRORED areas only: `client/` also hosts
    # trees this script does not own (client/omarchy is a SOURCE, not a
    # mirror), and a repo-wide sweep would hand --prune permission to delete
    # from the only copy (T6, cross-audit 2026-08-23).
    report["junk"] = sorted(
        f"{area}/{f.relative_to(repo_base / area).as_posix()}"
        for area, _ in AREAS
        if (repo_base / area).is_dir()
        for f in (repo_base / area).rglob("*")
        if f.is_file() and is_noise(f.relative_to(repo_base / area))
    )
    report.update(check_manifest(repo_base))
    if not live_base.is_dir():
        return report

    for area, kind in AREAS:
        live, mirror = live_base / area, repo_base / area
        if not mirror.is_dir():
            continue
        for rel in mirrored_files(area, kind, live, mirror):
            live_file, mirror_file = live / rel, mirror / rel
            entry = f"{area}/{rel.as_posix()}"
            if not live_file.is_file():
                report["orphaned"].append(entry)
            elif not mirror_file.is_file():
                report["missing"].append(entry)
            elif live_file.read_bytes() != mirror_file.read_bytes():
                report["drifted"].append(entry)
            else:
                report["in_sync"] += 1
    return report


def apply(report: dict, prune: bool) -> dict:
    """Copy live -> mirror for everything drifted or missing."""
    live_base, repo_base = live_root(), repo_root() / "client"
    done = {"updated": [], "added": [], "removed": []}
    for entry in report["drifted"] + report["missing"]:
        rel = Path(entry)
        source, target = live_base / rel, repo_base / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        existed = target.is_file()
        shutil.copy2(source, target)
        done["updated" if existed else "added"].append(entry)
    if prune:
        # One meaning: leave the mirror holding exactly what belongs in it — both
        # what vanished from the live side and the runtime state that never belonged.
        for entry in report["orphaned"] + report["junk"]:
            (repo_base / entry).unlink(missing_ok=True)
            done["removed"].append(entry)
        # Same scope rule as the junk sweep: only the mirrored areas may have
        # their empty directories collapsed — never client/omarchy.
        for area, _ in AREAS:
            base = repo_base / area
            if not base.is_dir():
                continue
            for d in sorted(base.rglob("*"), key=lambda p: -len(p.parts)):
                if d.is_dir() and not any(d.iterdir()):
                    d.rmdir()
    write_manifest(repo_base)  # the manifest always describes the mirror as it now stands
    return done


def render(report: dict, done: dict | None) -> None:
    mismatch = report.get("manifest_mismatch") or []
    if not report["live_present"]:
        # CI mode: no live side to compare against, but the manifest still proves
        # nobody hand-edited the mirror — an edit the next --apply would silently undo.
        if not report.get("manifest_present"):
            print(f"⚠  SEM MANIFESTO  {report['mirror_root']}/{MANIFEST} ausente — "
                  f"rode --apply na máquina do operador para gerá-lo")
        elif mismatch:
            print(f"❌ ESPELHO ALTERADO FORA DO SYNC  {len(mismatch)} arquivo(s)")
            for entry in mismatch:
                print(f"   {entry}")
            print("\n   o espelho é gerado, não editado: altere ~/.claude/... e rode --apply")
        else:
            print(f"✅ MANIFESTO OK  espelho íntegro ({len(mirror_entries(repo_root() / 'client'))} arquivos)")
            print(f"   (lado vivo ausente aqui — a comparação completa roda na máquina do operador)")
        return
    drift = len(report["drifted"]) + len(report["missing"])
    if done is not None:
        # After --apply the findings are HISTORY, so they are labelled by what was
        # done to them. Reprinting "desatualizado" for a file just corrected reads
        # as if the sync had failed.
        print(f"✅ SYNCED  +{len(done['added'])} novos  ~{len(done['updated'])} atualizados"
              f"  -{len(done['removed'])} removidos  ({report['in_sync']} já iguais)")
        for label, key in (("adicionado", "added"), ("atualizado", "updated"),
                           ("removido", "removed")):
            for entry in done[key]:
                print(f"   {label:12} {entry}")
        if report["orphaned"] and not done["removed"]:
            for entry in report["orphaned"]:
                print(f"   {'só no espelho':12} {entry}  (mantido; use --prune para remover)")
        return

    if drift == 0:
        print(f"✅ CLEAN  {report['in_sync']} arquivos espelhados, nenhum divergente")
    else:
        print(f"❌ DRIFT  {drift} arquivo(s) fora de sincronia ({report['in_sync']} iguais)")
    for label, key in (("desatualizado no espelho", "drifted"), ("ausente no espelho", "missing")):
        for entry in report[key]:
            print(f"   {label:24} {entry}")
    for entry in report["orphaned"]:
        print(f"   {'só no espelho':24} {entry}  (use --prune para remover)")
    if report["junk"]:
        size = sum((repo_root() / "client" / e).stat().st_size for e in report["junk"]) / 1024 / 1024
        print(f"\n   ⚠ {len(report['junk'])} arquivo(s) de runtime no espelho ({size:.1f} MB) — "
              f"estado de máquina, não produto")
        for entry in report["junk"][:5]:
            print(f"   {'runtime':24} {entry}")
        if len(report["junk"]) > 5:
            print(f"   {'':24} … +{len(report['junk']) - 5}")
        print("     remover: python3 scripts/sync-client-skills.py --apply --prune")
    for entry in mismatch:
        print(f"   {'alterado fora do sync':24} {entry}")
    if drift or mismatch:
        print("\n   corrigir: python3 scripts/sync-client-skills.py --apply")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="exit 1 se houver divergência")
    mode.add_argument("--apply", action="store_true", help="copia live -> client/")
    parser.add_argument("--prune", action="store_true",
                        help="com --apply, remove do espelho o que sumiu do lado vivo")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    report = survey()
    done = apply(report, args.prune) if args.apply and report["live_present"] else None

    if args.json:
        print(json.dumps({**report, "applied": done}, ensure_ascii=False, indent=1))
    else:
        render(report, done)

    if args.apply:
        return 0
    mismatch = bool(report.get("manifest_mismatch"))
    if not report["live_present"]:
        # No live side: the full comparison is impossible, so it does NOT block —
        # but the manifest check is real here and does.
        return 1 if mismatch else 0
    return 1 if (report["drifted"] or report["missing"] or mismatch) else 0


if __name__ == "__main__":
    sys.exit(main())
