"""Evidence for the four pending decisions of the ECC analysis (17/10/2026). Read-only.

D1 tag signing · D2 git hook bypass · D3 quality-lock lanes · D4 /home/<user> in versioned files.
"""
import collections
import json
import re
import subprocess
import time
from pathlib import Path

HOME = Path.home()
T = HOME / "projects/touring"


def sh(*cmd, cwd=T):
    r = subprocess.run(list(cmd), cwd=cwd, capture_output=True, text=True, check=False, timeout=60)
    return (r.stdout or r.stderr).strip()


print("== D1 assinatura ==")
for k in ("user.signingkey", "gpg.format", "commit.gpgsign", "tag.gpgSign"):
    print(f"  {k} = {sh('git', 'config', '--get', k) or '(vazio)'}")
print("  chaves gpg secretas:", len([l for l in sh("gpg", "--list-secret-keys", "--with-colons").splitlines() if l.startswith("sec")]))
print("  chaves ssh .pub:", len(list((HOME / ".ssh").glob("*.pub"))))
sig = collections.Counter(sh("git", "log", "--format=%G?", "-300").split())
print("  últimos 300 commits por estado de assinatura:", dict(sig))
print("  tags:", sh("git", "tag", "-l"), "· remoto:", sh("git", "remote", "get-url", "origin"))
print("  visibilidade:", sh("gh", "repo", "view", "--json", "visibility,nameWithOwner", "-q", ".nameWithOwner + \" \" + .visibility"))
print("  releases no GitHub:", sh("gh", "release", "list", "--limit", "5") or "(nenhuma)")
print("  propagate-release cria tag?", bool(re.search(r"git tag", (T / "scripts/propagate-release.sh").read_text(encoding="utf-8"))))

print("== D2 hooks de git ==")
print("  core.hooksPath global =", sh("git", "config", "--global", "--get", "core.hooksPath") or "(vazio)")
for p in sorted((HOME / "projects").iterdir()):
    if not (p / ".git").exists() or p.name.startswith("touring-wt-"):
        continue
    gd = Path(sh("git", "rev-parse", "--absolute-git-dir", cwd=p))
    ativos = [h.name for h in (gd / "hooks").glob("*") if h.is_file() and not h.name.endswith(".sample")] if gd.is_dir() else []
    precommit = (p / ".pre-commit-config.yaml").exists() or (p / ".husky").exists() or (p / "lefthook.yml").exists()
    hp = sh("git", "config", "--get", "core.hooksPath", cwd=p)
    if ativos or precommit or hp:
        print(f"  {p.name}: hooks={ativos} framework={precommit} hooksPath={hp or '-'}")
corte = time.time() - 30 * 86400
nv = collections.Counter()
for proj in (HOME / ".claude/projects").iterdir():
    for f in proj.rglob("*.jsonl"):
        if f.stat().st_mtime < corte:
            continue
        with f.open(encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if "--no-verify" not in line and "hooksPath" not in line:
                    continue
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if isinstance(c, dict) and c.get("type") == "tool_use" and c.get("name") == "Bash":
                        cmd = str((c.get("input") or {}).get("command", ""))
                        if re.search(r"git\b[^\n|;&]*\s--no-verify\b", cmd):
                            nv[("no-verify", proj.name.split("-")[-1])] += 1
                        if re.search(r"git\s+-c\s+core\.hooksPath", cmd):
                            nv[("hooksPath", proj.name.split("-")[-1])] += 1
print("  usos em comandos Bash (30 dias):", dict(nv) or "nenhum")

print("== D3 lock do touring-quality ==")
rows = [json.loads(l) for l in (HOME / ".claude/touring/logs/quality-runs.jsonl").open(encoding="utf-8", errors="replace") if l.startswith("{")]
for dia in ("2026-10-15", "2026-10-16", "2026-10-17"):
    d = [r for r in rows if r["ts"].startswith(dia)]
    arq = [r for r in d if r["target"].startswith("/tmp/tq_cache/tree/") or Path(r["target"]).suffix]
    dirs = [r for r in d if r not in arq]
    held = sum(r["secs"] for r in dirs)
    alvos = collections.Counter(r["target"] for r in dirs)
    print(f"  {dia}: arquivo n={len(arq)} soma={sum(r['secs'] for r in arq):.1f}s máx={max([r['secs'] for r in arq] or [0]):.2f}s"
          f" · diretório n={len(dirs)} soma={held:.0f}s ({100 * held / 86400:.1f}% do dia) alvos={alvos.most_common(3)}")
log = Path("/tmp/hook_invocations.log")
if log.exists():
    t = log.read_text(encoding="utf-8", errors="replace")
    print("  hook_invocations.log: invoked=", t.count("touring-quality-block-all.sh invoked"), "busy=", t.count("quality lock busy"),
          "empty=", t.count("empty score JSON"))
print("  quem pede score de diretório do analise:",
      sh("bash", "-c", "grep -rln --include=*.py --include=*.sh -E 'touring-quality(-score)? score' ~/.claude/skills/loop-engineering/scripts ~/.claude/hooks ~/projects/analise/.claude 2>/dev/null | head -8"))

print("== D4 /home/<user> em arquivos versionados ==")
ls = sh("git", "grep", "-l", "-I", "/home/gabrielgadea").splitlines()
cls = collections.Counter()
for f in ls:
    parts = f.split("/")
    k = "/".join(parts[:2]) if parts[0] in ("docs", "crates", "client", "scripts") else parts[0]
    cls[k] += 1
print("  arquivos:", len(ls), "· por área:", cls.most_common(12))
oc = sh("bash", "-c", "git grep -I -o /home/gabrielgadea | wc -l")
print("  ocorrências:", oc)
codigo = [f for f in ls if f.endswith((".rs", ".py", ".sh", ".toml", ".ts", ".js")) and not f.startswith("docs/")]
print("  em código fora de docs/:", len(codigo), codigo[:10])
kpi = [f for f in ls if f.startswith("docs/kpi/")]
if kpi:
    amostra = (T / kpi[0]).read_text(encoding="utf-8", errors="replace")
    chaves = re.findall(r'"([a-z_]+)":', amostra)[:20]
    print("  docs/kpi amostra:", kpi[0], "· chaves:", chaves)
    print("  projetos nomeados nos nomes de arquivo:", sorted({re.sub(r'^\d{4}-\d{2}-\d{2}--', '', Path(f).stem) for f in kpi})[:10])
print("  gitignore cobre docs/kpi?", bool(re.search(r"docs/kpi", (T / ".gitignore").read_text(encoding="utf-8"))))
print("  histórico: commits que tocaram docs/kpi:", sh("bash", "-c", "git log --oneline -- docs/kpi | wc -l"))
