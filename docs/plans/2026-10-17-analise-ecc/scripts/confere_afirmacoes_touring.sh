#!/usr/bin/env bash
# Adversarial check of what the ECC-analysis subagents claimed about TOURING (17/10/2026).
# Read-only. Each block prints the claim id and the raw evidence; the verdict is written by hand in the report.
set -u
T=/home/gabrielgadea/projects/touring
cd "$T" || exit 9

echo "== A5 read_stdin em timeout/vazio/erro devolve objeto vazio? =="
grep -n -E "fn read_stdin|Timeout|Empty|ReadError|Ok\(serde_json::json!\(\{\}\)\)|Ok\(json!\(\{\}\)\)|Value::Object" crates/touring-hook-runtime/src/hook_runtime_ext.rs | sed -n 1,25p

echo "== E1 release.yml: id-token no topo e rust-cache no build =="
grep -n -E "^permissions|id-token|contents:|rust-cache|persist-credentials|^  [a-z_-]+:$|needs:" .github/workflows/release.yml | head -30

echo "== E3 caracteres invisíveis em fontes vivas (ZW/bidi/BOM/Tag) =="
python3 -I - <<'EOF'
import re, pathlib
pat = re.compile("[​-‏‪-‮⁠-⁤⁦-⁩﻿\U000e0000-\U000e007f]")
roots = [pathlib.Path("client"), pathlib.Path("crates"), pathlib.Path("scripts"), pathlib.Path.home() / ".claude/rules",
         pathlib.Path.home() / ".claude/CLAUDE.md", pathlib.Path("CLAUDE.md")]
hits = []
for r in roots:
    files = [r] if r.is_file() else [p for p in r.rglob("*") if p.is_file() and p.suffix in (".py", ".rs", ".sh", ".md", ".toml", ".json", ".yml", ".yaml")]
    for p in files:
        try:
            for n, line in enumerate(p.read_text(encoding="utf-8", errors="ignore").splitlines(), 1):
                for m in pat.finditer(line):
                    hits.append(f"{p}:{n} U+{ord(m.group()):04X}")
        except OSError:
            pass
print(len(hits), "ocorrências;", hits[:8])
EOF

echo "== B gotcha: acertos x prevenidos =="
touring gotcha stats 2>&1 | grep -i -E "hits|prevent" | head -4

echo "== B TierManager: referências =="
touring index find TierManager -j 2>/dev/null | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("defs", len(d.get("definitions", d.get("results", []))), "refs", len(d.get("references", [])))' 2>&1 | head -2
git grep -n "TierManager" -- crates | grep -v "^crates/touring-intelligence/src/rl/memory/tier_manager.rs" | head -5

echo "== B5 memory store varre segredo? =="
git grep -n -i -E "redact|secret|scan_pii|pii" -- crates/touring-cli/src/cli/memory.rs crates/touring-server/src/memory_store.rs 2>/dev/null | head -6

echo "== C context_budget no kpi =="
touring kpi -j 2>/dev/null | python3 -I -c '
import json,sys
d=json.load(sys.stdin); cb=d.get("context_budget",{})
print("status", cb.get("status")); fam=cb.get("by_family") or cb.get("families") or {}
print("chaves", list(cb.keys())[:14]); print("familias", json.dumps(fam, ensure_ascii=False)[:500])'

echo "== D cláusulas do juiz loop_converged x CLAUDE.md =="
sed -n 1,30p ~/.claude/skills/loop-engineering/scripts/loop_converged.py | grep -n -E "^\s*#?\s*[0-9]+[.)]|cláusula|clause" | head -12
grep -n -o -E "Lei L2: [0-9]+ cláusulas[^)]*" ~/.claude/CLAUDE.md | head -2

echo "== D decompose update --status completed: condicional? =="
touring decompose update --help 2>&1 | head -12
