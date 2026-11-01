"""Relevance of billion-context, measured: (A) what Touring already has; (B) Claude Code cache + compaction in the
transcripts of the last 14 days (touring, analise); (C) tool-output shape per project. Read-only; dedup by message.id."""
import collections
import json
import os
import re
import statistics
import subprocess
import time
from pathlib import Path

M = Path("/home/gabrielgadea/projects/touring")
PROJ = Path.home() / ".claude/projects"
DIAS = 14
corte = time.time() - DIAS * 86400

print("== A. sobreposição no Touring (git grep -c por padrão) ==")
for rot, pat in [("lê uso de cache dos transcripts", r"cache_read_input_tokens"), ("hook PreCompact", r"PreCompact"),
                 ("perfil de compressão de saída", r"(?i)compression[_ ]?profile|CompressionProfile"),
                 ("digest --brief (elided_lines)", r"elided_lines"), ("menções ao dsh", r"\bdsh\b")]:
    r = subprocess.run(["git", "-C", str(M), "grep", "-l", "-I", "-E", pat, "--", "crates", "scripts", "client", "docs/*.md"],
                       capture_output=True, text=True, check=False)
    fs = [f for f in r.stdout.splitlines() if f]
    print(f"  {rot:34s} {len(fs):3d} arquivos · ex: {fs[:3]}")
r = subprocess.run(["git", "-C", str(M), "grep", "-n", "-I", "-E", r"elided_lines", "--", "crates"], capture_output=True, text=True, check=False)
brief = sorted({ln.split(":")[0] for ln in r.stdout.splitlines()})
erro = []
for f in brief[:6]:
    t = (M / f).read_text(encoding="utf-8", errors="replace")
    erro.append((f, bool(re.search(r"(?i)(error|fail|panic)[^\n]{0,80}(keep|preserv|retain|verbatim|always)", t))))
print("  o digest preserva linhas de erro explicitamente?", erro)

s = json.loads((Path.home() / ".claude/settings.json").read_text(encoding="utf-8"))
env = s.get("env", {})
seguras = {k: env[k] for k in env if re.match(r"(CLAUDE_AUTOCOMPACT|DISABLE_AUTO_COMPACT|DISABLE_COMPACT|CLAUDE_CODE_.*COMPACT|MAX_THINKING)", k)}
print("\n== B0. settings do CC (só chaves de compactação) ==", seguras, "· ANTHROPIC_BASE_URL definido no settings?", "ANTHROPIC_BASE_URL" in env)

print(f"\n== B. transcripts dos últimos {DIAS} dias (dedup por message.id) ==")
for proj in ("-home-gabrielgadea-projects-touring", "-home-gabrielgadea-projects-analise"):
    base = PROJ / proj
    arquivos = [p for p in base.rglob("*.jsonl") if p.stat().st_mtime >= corte]
    uso = {}  # message.id -> usage (last record wins)
    modelo = collections.Counter()
    compact_por_sessao = collections.Counter()
    sessoes = set()
    tr_bytes, tr_json, tr_total = [], 0, 0
    for p in arquivos:
        sid = p.stem
        sub = "subagents" in p.parts or p.name.startswith("agent-")
        if not sub:
            sessoes.add(sid)
        with p.open(encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if '"compact_boundary"' in line or '"isCompactSummary":true' in line:
                    if not sub and ('"compact_boundary"' in line):
                        compact_por_sessao[sid] += 1
                if '"usage"' not in line and '"tool_result"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                msg = rec.get("message") or {}
                if rec.get("type") == "assistant" and isinstance(msg.get("usage"), dict) and msg.get("id"):
                    uso[msg["id"]] = msg["usage"]
                    modelo[(msg.get("model") or "?").split("-2")[0]] += 0  # registered below once per id
                    uso[msg["id"]]["_model"] = msg.get("model") or "?"
                elif rec.get("type") == "user" and isinstance(msg.get("content"), list):
                    for c in msg["content"]:
                        if isinstance(c, dict) and c.get("type") == "tool_result":
                            cont = c.get("content")
                            txt = cont if isinstance(cont, str) else "".join(x.get("text", "") for x in cont or [] if isinstance(x, dict))
                            n = len(txt.encode("utf-8"))
                            tr_bytes.append(n)
                            tr_total += n
                            if txt.lstrip()[:1] in ("{", "["):
                                tr_json += n
    inp = sum(u.get("input_tokens", 0) for u in uso.values())
    cc = sum(u.get("cache_creation_input_tokens", 0) for u in uso.values())
    cr = sum(u.get("cache_read_input_tokens", 0) for u in uso.values())
    out = sum(u.get("output_tokens", 0) for u in uso.values())
    fam = collections.Counter()
    for u in uso.values():
        m = u["_model"]
        fam["claude" if m.startswith("claude") else ("kimi" if "k3" in m or "kimi" in m else ("minimax" if "minimax" in m.lower() else m[:14]))] += 1
    total_in = inp + cc + cr
    comp = [compact_por_sessao.get(s, 0) for s in sessoes]
    tr_bytes.sort()
    print(f"\n  [{proj.split('-')[-1]}] arquivos={len(arquivos)} sessões principais={len(sessoes)} mensagens únicas={len(uso)} · famílias={dict(fam)}")
    if total_in:
        print(f"    entrada total={total_in / 1e9:.2f} B tokens · ACERTO DE CACHE={100 * cr / total_in:.1f}% · escrita de cache={100 * cc / total_in:.1f}%"
              f" · sem cache={100 * inp / total_in:.2f}% · saída={out / 1e6:.1f} M")
    if comp:
        print(f"    compactações por sessão: média {statistics.mean(comp):.2f} · máx {max(comp)} · sessões com ≥1: {sum(c > 0 for c in comp)}/{len(comp)}")
    if tr_bytes:
        q = lambda f: tr_bytes[min(len(tr_bytes) - 1, int(f * (len(tr_bytes) - 1)))]
        print(f"    saídas de ferramenta: n={len(tr_bytes)} · total={tr_total / 1e6:.0f} MB · mediana={q(0.5)} B · p95={q(0.95) / 1024:.1f} KB"
              f" · máx={tr_bytes[-1] / 1024:.0f} KB · forma JSON={100 * tr_json / max(1, tr_total):.0f}% dos bytes"
              f" · ≥10 KB respondem por {100 * sum(x for x in tr_bytes if x >= 10240) / max(1, tr_total):.0f}% dos bytes")
