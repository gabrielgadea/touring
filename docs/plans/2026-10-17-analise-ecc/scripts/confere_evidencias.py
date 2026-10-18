"""Mechanical check of every `path:line` evidence the round-H readers cited (17/10/2026): does the file exist, is the line
inside it? Catches invented locations (VGP) before any human reading; the manual sample then judges the content.

Usage: confere_evidencias.py <ecc_repo> <achados_dir> [--glob 'H*.json']
Resolution order for a relative path: the ECC clone, ~/projects/touring, ~/.claude, ~. Prints one line per file and
writes the full list to <achados_dir>/../inventario/evidencias.json.
"""
import argparse
import json
import re
from pathlib import Path

HOME = Path.home()
REF = re.compile(r"(?P<path>(?:~/|/)?[\w.@-]+(?:/[\w.@-]+)+\.\w+|[\w.-]+\.(?:md|rs|py|sh|js|ts|json|toml|yml|yaml|mjs|cjs))"
                 r":(?P<a>\d+)(?:[-–](?P<b>\d+))?")


_POR_NOME: dict[str, list[Path]] = {}


def por_nome(name: str, ecc: Path) -> list[Path]:
    """Files named `name` under the ECC clone and the Touring roots (a reader often cites the bare file name)."""
    if not _POR_NOME:
        roots = (ecc, HOME / "projects/touring/crates", HOME / "projects/touring/client", HOME / "projects/touring/scripts",
                 HOME / ".claude/agents", HOME / ".claude/skills", HOME / ".claude/hooks", HOME / ".claude/rules")
        for root in roots:
            for p in root.rglob("*"):
                if p.is_file() and "/target/" not in str(p) and "/.git/" not in str(p):
                    _POR_NOME.setdefault(p.name, []).append(p)
    return _POR_NOME.get(name, [])


def resolve(path: str, ecc: Path) -> tuple[Path | None, str]:
    if path.startswith("~/"):
        p = HOME / path[2:]
        return (p, "") if p.is_file() else (None, "")
    if path.startswith("/"):
        p = Path(path)
        return (p, "") if p.is_file() else (None, "")
    for root in (ecc, HOME / "projects/touring", HOME / ".claude", HOME):
        p = root / path
        if p.is_file():
            return p, ""
    hits = [p for p in por_nome(Path(path).name, ecc) if str(p).endswith(path)]
    if len(hits) == 1:
        return hits[0], "por nome"
    return (hits[0], f"ambíguo ({len(hits)})") if hits else (None, "")


def check(text: str, ecc: Path) -> list[dict]:
    out = []
    for m in REF.finditer(text):
        if "scratchpad/" in m.group("path"):
            out.append({"ref": m.group(0), "ok": True, "motivo": "sonda efêmera do leitor (não durável)"})
            continue
        p, como = resolve(m.group("path"), ecc)
        last = int(m.group("b") or m.group("a"))
        if p is None:
            out.append({"ref": m.group(0), "ok": False, "motivo": "arquivo não encontrado"})
            continue
        n = p.read_text(encoding="utf-8", errors="replace").count("\n") + 1
        out.append({"ref": m.group(0), "ok": last <= n, "motivo": como if last <= n else f"linha {last} > {n} linhas {como}".strip()})
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("ecc", type=Path)
    ap.add_argument("achados", type=Path)
    ap.add_argument("--glob", default="H*.json")
    a = ap.parse_args()
    todos = {}
    for f in sorted(a.achados.glob(a.glob)):
        d = json.loads(f.read_text(encoding="utf-8"))
        res = []
        for i, x in enumerate(d.get("linhas", []), 1):
            for lado in ("ecc", "touring"):
                for r in check(str((x.get(lado) or {}).get("evidencia", "")), a.ecc):
                    res.append({"linha": i, "lado": lado, **r})
        todos[f.name] = res
        ruins = [r for r in res if not r["ok"]]
        sem_ref = sum(1 for x in d.get("linhas", []) if not check(json.dumps(x, ensure_ascii=False), a.ecc))
        print(f"{f.name}: {len(res)} referências, {len(ruins)} inválidas, {sem_ref} linhas sem nenhuma referência caminho:linha")
        for r in ruins[:6]:
            print(f"   linha {r['linha']} {r['lado']}: {r['ref']} — {r['motivo']}")
    (a.achados.parent / "inventario" / "evidencias.json").write_text(json.dumps(todos, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
