#!/usr/bin/env python3
"""Prova dos predicados novos (disposição b) ANTES de editar o scanner.

Mede, sobre o estado atual do repo:
  P-C4: citação de comando conta só em contexto de código (code fence, inline
        `...`, linha de shell $/#/❯) E com "touring" NÃO precedido de "/"
        (elimina o diretório `.../projects/touring` e o path `target/debug/touring`).
  P-C6: ground truth = vars TOURING_* referenciadas em código (.rs/.py/.sh/.toml,
        SEM .json — dados históricos não são reconhecimento vivo), não os.environ.

Saída: contagens velha vs nova por categoria + destino dos 111 itens do delta sob
a régua nova (somem do count → b; ficam → precisam de a/c).
"""
import json
import re
import subprocess
import sys
from pathlib import Path

BUNDLE = Path(__file__).resolve().parent
REPO = BUNDLE.parents[2]

RE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
RE_INLINE = re.compile(r"`[^`\n]+`")
RE_C4 = re.compile(r"(?<![/\w])touring ([a-z][a-z0-9_-]+)\b")  # novo: sem "/" nem letra antes
RE_C6 = re.compile(r"\bTOURING_[A-Z][A-Z0-9_]+\b")
CODE_EXTS = {".rs", ".py", ".sh", ".toml"}


def scope_md() -> dict[str, str]:
    out = {}
    for p in REPO.rglob("*.md"):
        rel = p.relative_to(REPO).as_posix()
        if "/target/" in rel or "/node_modules/" in rel:
            continue
        if rel.startswith(("docs/", "crates/")) or rel in ("ARCHITECTURE.md", "CLAUDE.md"):
            out[rel] = p.read_text(encoding="utf-8", errors="ignore")
    return out


def gt_envvars_codigo() -> set[str]:
    found: set[str] = set()
    for p in REPO.rglob("*"):
        if not p.is_file() or p.suffix not in CODE_EXTS:
            continue
        rel = p.relative_to(REPO).as_posix()
        if "/target/" in rel or "/node_modules/" in rel or "/.git/" in rel:
            continue
        found.update(RE_C6.findall(p.read_text(encoding="utf-8", errors="ignore")))
    return found


def gt_cli() -> set[str]:
    r = subprocess.run(["touring", "--help"], capture_output=True, text=True, cwd=REPO, timeout=30)
    return set(re.findall(r"^\s+([a-z][a-z0-9_-]+)\s", r.stdout, re.MULTILINE))


def _spans_codigo(content: str) -> list[tuple[int, int]]:
    spans = [(m.start(), m.end()) for m in RE_FENCE.finditer(content)]
    masked = RE_FENCE.sub(lambda f: " " * (f.end() - f.start()), content)
    spans += [(m.start(), m.end()) for m in RE_INLINE.finditer(masked)]
    return spans


def c4_citados_novo(contents: dict[str, str]) -> set[str]:
    cited: set[str] = set()
    for content in contents.values():
        spans = _spans_codigo(content)
        for m in RE_C4.finditer(content):
            pos = m.start()
            in_span = any(s <= pos < e for s, e in spans)
            if not in_span:
                # linha de shell fora de fence: primeiro char não-espaço é $/#/❯
                start = content.rfind("\n", 0, pos) + 1
                if content[start:pos].lstrip()[:1] not in ("$", "#", "❯"):
                    continue
            cited.add(m.group(1))
    return cited


def main() -> int:
    contents = scope_md()
    delta = json.loads((BUNDLE / "delta.json").read_text(encoding="utf-8"))["delta"]

    # C6 velho (env) vs novo (código)
    gt_code = gt_envvars_codigo()
    c6_now_old = set(json.loads(Path("/tmp/drift-scan-2026-10-08.json").read_text())
                     ["categories"]["C6-envvars-cited"]["stale"])
    c6_cited = set()
    for c in contents.values():
        c6_cited.update(RE_C6.findall(c))
    c6_now_new = sorted(v for v in c6_cited if v not in gt_code)
    print(f"C6: régua velha (env)={len(c6_now_old)} → régua nova (código, {len(gt_code)} vars)={len(c6_now_new)}")
    d6 = set(delta["C6"])
    print(f"  delta C6 ({len(d6)}): somem sob régua nova={len(d6 - set(c6_now_new))} · ficam={sorted(d6 & set(c6_now_new))}")

    # C4 velho vs novo
    help_cmds = gt_cli()
    c4_now_old = set(json.loads(Path("/tmp/drift-scan-2026-10-08.json").read_text())
                     ["categories"]["C4-cli-cited"]["potential_stale"])
    c4_cited_new = c4_citados_novo(contents)
    c4_now_new = sorted(c for c in c4_cited_new if c not in help_cmds)
    print(f"C4: régua velha={len(c4_now_old)} → régua nova (contexto+path)={len(c4_now_new)}")
    d4 = set(delta["C4"])
    somem = sorted(d4 - set(c4_now_new))
    ficam = sorted(d4 & set(c4_now_new))
    print(f"  delta C4 ({len(d4)}): somem sob régua nova={len(somem)}: {somem}")
    print(f"  delta C4 ficam={len(ficam)}: {ficam}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
