"""Gabriel 08/10: «commite tudo, preservando o disco» → analise = «só código e docs»; touring = sem crates/, sem o
corpus bruto. Builds the NUL-separated pathspec of UNTRACKED files to add; every exclusion is counted by reason."""
import collections, pathlib, subprocess, sys
REPO = pathlib.Path(sys.argv[1]); MODE = sys.argv[2]; OUT = pathlib.Path(sys.argv[3])
TEXT = {".py", ".rs", ".sh", ".toml", ".yaml", ".yml", ".md", ".txt", ".html", ".css", ".js", ".ts", ".tsx", ".jsx",
        ".json", ".csv", ".sql", ".cfg", ".ini", ".svg", ".j2"}
DIRS = {"scripts", "docs", "packages", ".claude", ".touring", "plans", "generated", "tests", "backend", "frontend"}
JUNK = {"cipher_queue", "scratch", ".scratch", "snapshots", "search_index", "__pycache__", "node_modules", ".cache",
        ".ruff_cache", ".pytest_cache", ".hypothesis"}
st = subprocess.run(["git", "--no-optional-locks", "-C", str(REPO), "status", "--porcelain=v1", "-z", "--untracked-files=all"],
                    capture_output=True, text=True).stdout.split("\0")
keep, why = [], collections.Counter()
def reason(p: pathlib.Path) -> str | None:
    if any(ord(ch) < 32 or ch in '"`' for ch in str(p)) or not str(p).isprintable():
        return "nome com caractere de controle (lixo de fuzz)"
    if MODE == "touring":
        if p.parts[0] == "crates": return "crates/ (Kimi editando; 2º commit)"
        if p.name == "corpus_a2.jsonl": return "corpus bruto de transcript (fica fora)"
    else:
        if p.parts[0] not in DIRS: return f"fora de código/docs ({p.parts[0]})"
        if any(x in JUNK for x in p.parts) or ".claude" in p.parts[1:]: return "runtime/cache/rascunho"
        if p.suffix.lower() not in TEXT: return f"não-texto ({p.suffix or 'sem ext'})"
    f = REPO / p
    if f.is_file() and f.stat().st_size > (1 if MODE == "analise" else 5) * 2**20: return "grande demais"
    return None
for e in filter(None, st):
    if not e.startswith("??"):
        continue
    p = pathlib.Path(e[3:])
    r = reason(p)
    if r: why[r.split(" (")[0] if r.startswith("fora") or r.startswith("não-texto") else r] += 1
    else: keep.append(str(p))
OUT.write_bytes(b"\0".join(s.encode() for s in keep))
tracked = [e for e in filter(None, st) if not e.startswith("??")]
if MODE == "touring":
    tracked = [e for e in tracked if not e[3:].startswith("crates/")]
print(f"{REPO.name}: rastreados com mudança a commitar {len(tracked)} · não rastreados a adicionar {len(keep)} "
      f"({sum((REPO / k).stat().st_size for k in keep) / 2**20:.1f} MB) · excluídos: {dict(why)}")
