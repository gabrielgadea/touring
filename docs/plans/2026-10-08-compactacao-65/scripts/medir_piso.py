"""The fixed floor every analise session pays before its first word (item 8 of the compaction plan).

Counts: always-loaded instruction files (CLAUDE.md files + rules WITHOUT `paths:` frontmatter +
MEMORY.md), agent descriptions (what the agent listing re-injects), and plugin MCP servers disabled
for the project (their authenticate/complete_authentication stubs leave the deferred-tool list).

Usage: python3 medir_piso.py [--json out.json]
"""

import json
import re
import sys
from pathlib import Path

HOME = Path.home()
ANALISE = HOME / "projects/analise"
MEMORY = HOME / ".claude/projects/-home-gabrielgadea-projects-analise/memory/MEMORY.md"


def always_loaded_rules(root: Path) -> list[Path]:
    out = []
    for f in sorted(root.rglob("*.md")):
        head = f.read_text(errors="replace")[:600]
        if not re.search(r"^---\s*\n(?:.*\n)*?paths:", head, re.M):
            out.append(f)
    return out


def description_chars(root: Path) -> list[tuple[int, str]]:
    rows = []
    for f in root.glob("**/*.md"):
        text = f.read_text(errors="replace")
        m = re.match(r"---\n(.*?)\n---", text, re.S)
        if not m:
            continue
        d = re.search(r"^description:\s*(.*?)(?=^\w[\w-]*:|\Z)", m.group(1), re.S | re.M)
        if d:
            rows.append((len(d.group(1)), f.name))
    return rows


def main():
    files = [HOME / ".claude/CLAUDE.md", ANALISE / "CLAUDE.md", ANALISE / ".claude/CLAUDE.md", MEMORY]
    rules = always_loaded_rules(HOME / ".claude/rules") + always_loaded_rules(ANALISE / ".claude/rules")
    instr = {str(f): len(f.read_text(errors="replace")) for f in files + rules if f.exists()}
    agents = description_chars(HOME / ".claude/agents") + description_chars(ANALISE / ".claude/agents")
    cfg = json.loads((HOME / ".claude.json").read_text())
    disabled = (cfg.get("projects") or {}).get(str(ANALISE), {}).get("disabledMcpServers") or []
    out = {
        "instruction_chars": sum(instr.values()), "instruction_files": len(instr),
        "rules_always_loaded": [str(r).replace(str(HOME), "~") for r in rules],
        "agent_description_chars": sum(n for n, _ in agents), "agents": len(agents),
        "agent_description_top": sorted(agents, reverse=True)[:6],
        "plugin_mcp_disabled": sorted(s for s in disabled if s.startswith("plugin:")),
    }
    plugin_disabled = [s for s in disabled if s.startswith("plugin:")]
    print(f"instruções sempre carregadas: {sum(instr.values()):,} chars em {len(instr)} arquivos "
          f"({len(rules)} regras sem paths:)")
    print(f"descrições de agentes: {sum(n for n, _ in agents):,} chars em {len(agents)} agentes; "
          f"maiores {sorted(agents, reverse=True)[:3]}")
    print(f"MCP de plugin desabilitados no analise: {len(plugin_disabled)}")
    if "--json" in sys.argv:
        Path(sys.argv[sys.argv.index("--json") + 1]).write_text(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
