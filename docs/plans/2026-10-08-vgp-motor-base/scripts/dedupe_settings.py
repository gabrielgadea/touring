"""Remove repeated hook entries from a Claude Code settings file without losing any distinct hook (D1).

A hook entry is identified by (event, matcher, canonical JSON of the hook). The first occurrence is kept, in
order; later identical ones are dropped, and groups left empty are removed. The proof is an invariant, checked
before writing: the SET of identities is identical before and after — only the multiplicity falls.

Usage: python3 dedupe_settings.py [--file ~/.claude/settings.json] [--apply]
Dry-run by default. --apply writes a backup to ~/.claude/touring/backups/ and replaces the file atomically.
Exit 0 = clean (nothing to drop, or applied with the invariant held); 1 = duplicates found in dry-run; 2 = refusal.
"""

import json
import os
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

BACKUPS = Path.home() / ".claude/touring/backups"


def identities(hooks: dict) -> Counter:
    return Counter((ev, g.get("matcher"), json.dumps(h, sort_keys=True))
                   for ev, groups in hooks.items() for g in groups or [] for h in g.get("hooks") or [])


def dedupe(hooks: dict) -> dict:
    seen, out = set(), {}
    for ev, groups in hooks.items():
        kept_groups = []
        for g in groups or []:
            kept = []
            for h in g.get("hooks") or []:
                key = (ev, g.get("matcher"), json.dumps(h, sort_keys=True))
                if key not in seen:
                    seen.add(key)
                    kept.append(h)
            if kept:
                kept_groups.append({**g, "hooks": kept})
        out[ev] = kept_groups
    return out


def write_atomic(path: Path, text: str) -> None:
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp")
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.replace(tmp, path)


def main() -> int:
    path = Path(sys.argv[sys.argv.index("--file") + 1] if "--file" in sys.argv else Path.home() / ".claude/settings.json")
    cfg = json.loads(path.read_text(encoding="utf-8"))
    before = identities(cfg.get("hooks") or {})
    new_hooks = dedupe(cfg.get("hooks") or {})
    after = identities(new_hooks)
    if set(before) != set(after) or any(n != 1 for n in after.values()):
        print("RECUSA: o invariante falhou (identidade perdida ou repetição restante)")
        return 2
    dropped = sum(before.values()) - sum(after.values())
    top = [(k[0], k[1], json.loads(k[2]).get("command", "")[-40:], n) for k, n in before.most_common(6) if n > 1]
    print(f"{path}: {sum(before.values())} entradas → {sum(after.values())} ({dropped} repetidas), "
          f"{len(after)} identidades distintas preservadas")
    for row in top:
        print(f"   {row}")
    if "--apply" not in sys.argv or dropped == 0:
        return 1 if dropped else 0
    BACKUPS.mkdir(parents=True, exist_ok=True)
    backup = BACKUPS / f"{path.name}.{time.strftime('%Y%m%d-%H%M%S')}"
    backup.write_text(path.read_text(encoding="utf-8"), encoding="utf-8")
    write_atomic(path, json.dumps({**cfg, "hooks": new_hooks}, ensure_ascii=False, indent=2) + "\n")
    print(f"aplicado; backup em {backup}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
