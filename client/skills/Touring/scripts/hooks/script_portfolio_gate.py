#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""script_portfolio_gate.py — PreToolUse (Write): consulta-antes-de-criar para scripts. SOMBRA.

Origem: diagnóstico 27/10/2026 (docs/plans/2026-10-27-portfolio-scripts/) — 41.923 eventos de
criação de script em 2.420 transcripts, 81,2% efêmeros/inline, duplicata exata 1,0% mas recriação
SEMÂNTICA massiva (probe.py reescrito em 14 sessões, mutantes.py em 18). `--harvest` 0×. O gap é
G1: nenhum passo "procurar no portfólio antes de escrever". Este hook É esse passo (D8: o
enforcement mora no executor/hook, não no anúncio).

Dispara quando o Write cria um script (.py/.sh/.js/.mjs/.ts/.rb/.pl) E:
  (a) o destino é efêmero (/tmp, /var/tmp, .cache, scratchpad, tool-results), OU
  (b) o basename é genérico (medido: probe/dump/fetch/build/show/final/ver/verifica/recalc/…).

A consulta, em ordem de custo (fail-open em cada degrau, teto total ~3s):
  1. `~/.claude/scripts-portfolio/MANIFEST.json` (P1) — registro tipado local, <1ms;
  2. `touring tantivy search "<stem>" 3` — índice BM25 (memória companion inclui snippets).

Sombra = nunca bloqueia: emite additionalContext com os 3 candidatos mais próximos e grava
`~/.claude/touring/portfolio_gate/events.jsonl` (um JSON por evento) para medir quantas criações
seriam evitadas (`would_avoid`: candidato com mesmo basename OU mesma finalidade presente).

Kill switch humano: `TOURING_PORTFOLIO_GATE_DISABLED=1`.
CLI: `--stats` resume o events.jsonl (base para o critério de promoção, à la prose gate).
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

SCRIPT_EXTS = {".py", ".sh", ".js", ".mjs", ".ts", ".rb", ".pl", ".bash"}
EPHEMERAL_HINTS = ("/tmp/", "/var/tmp/", "/.cache/", "/scratchpad", "/tool-results/", "/pytest-of-")
# Basenames genéricos medidos como campeões de recriação (mineracao 27/10, >=3 sessões).
GENERIC_BASENAMES = {
    "probe.py", "probe2.py", "dump.py", "fetch.py", "build.py", "show.py", "final.py",
    "ver.py", "verifica.py", "recalc.py", "mutantes.py", "mutar.py", "mutacao.py",
    "extrai.py", "links.py", "run.sh", "rodar.sh", "final.sh", "s1.py", "b1.py",
    "estrutura.py", "ctx.py", "sonda2.py",
}
PORTFOLIO = Path.home() / ".claude" / "scripts-portfolio"
MANIFEST = PORTFOLIO / "MANIFEST.json"
EVENTS = Path.home() / ".claude" / "touring" / "portfolio_gate" / "events.jsonl"
STOPWORDS = {"the", "and", "for", "with", "def", "import", "from", "print", "return"}


def is_ephemeral(path: str) -> bool:
    """Destino efêmero: morre com a sessão (tmpfs, cache, scratchpad)."""
    low = path.lower()
    return low.startswith(("/tmp", "/var/tmp")) or any(h in low for h in EPHEMERAL_HINTS)


def keywords_of(basename: str, head: str) -> list[str]:
    """Palavras-chave da busca: stem do basename + tokens raros do cabeçalho/docstring."""
    stem = Path(basename).stem.replace("_", " ").replace("-", " ")
    toks = [t for t in stem.split() if len(t) > 2 and t not in STOPWORDS]
    for tok in head[:600].split():
        t = "".join(c for c in tok if c.isalnum())
        if len(t) > 5 and t.lower() not in STOPWORDS and not t.isdigit():
            toks.append(t)
        if len(toks) >= 6:
            break
    return toks[:6]


def manifest_candidates(keys: list[str]) -> list[dict]:
    """Candidatos do portfólio tipado (P1) por interseção de propósito/tags/nome."""
    try:
        entries = json.loads(MANIFEST.read_text()).get("scripts", [])
    except (OSError, json.JSONDecodeError):
        return []
    out = []
    for e in entries:
        hay = " ".join(
            str(e.get(k, "")) for k in ("slug", "purpose", "domain", "tags")
        ).lower()
        hits = sum(1 for k in keys if k.lower() in hay)
        if hits:
            out.append({"path": str(PORTFOLIO / e.get("path", "")), "why": e.get("purpose", ""), "hits": hits})
    out.sort(key=lambda c: -c["hits"])
    return out[:3]


def tantivy_candidates(query: str) -> list[dict]:
    """Candidatos do índice BM25 (scripts de planos + snippets companion)."""
    try:
        p = subprocess.run(  # noqa: S603 — argv fixo, sem shell
            ["touring", "tantivy", "search", query, "3"],
            capture_output=True, text=True, timeout=2.5,
        )
        rows = json.loads(p.stdout) if p.returncode == 0 else []
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        return []
    out = []
    for r in rows if isinstance(rows, list) else []:
        fp = str(r.get("file_path", ""))
        if fp and Path(fp).suffix.lower() in SCRIPT_EXTS:
            out.append({"path": fp, "why": str(r.get("symbol_name", ""))[:80], "hits": 1})
    return out[:3]


def emit(payload: dict) -> None:
    """Resposta PreToolUse canônica; sombra nunca sai != 0."""
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "additionalContext": payload["context"],
    }}))


def log_event(event: dict) -> None:
    """Um JSON por linha; falha de I/O nunca derruba o hook."""
    try:
        EVENTS.parent.mkdir(parents=True, exist_ok=True)
        with EVENTS.open("a") as fh:
            fh.write(json.dumps(event, ensure_ascii=False) + "\n")
    except OSError:
        pass


def stats() -> int:
    """Resumo do events.jsonl: volume, % com candidato, % would_avoid (critério de promoção)."""
    try:
        rows = [json.loads(l) for l in EVENTS.read_text().splitlines() if l.strip()]
    except (OSError, json.JSONDecodeError):
        print("sem eventos")
        return 0
    n = len(rows)
    com_cand = sum(1 for r in rows if r.get("n_candidates"))
    evit = sum(1 for r in rows if r.get("would_avoid"))
    print(json.dumps({
        "eventos": n,
        "com_candidato": com_cand,
        "would_avoid": evit,
        "pct_evitaveis": round(100 * evit / n, 1) if n else 0.0,
    }, ensure_ascii=False))
    return 0


def injection_allowed() -> bool:
    """Interruptor de injeção de contexto (26/10/2026) — client/hooks/injection_policy.py.

    Fail-open (padrão evidence_guard.py): política ausente ou ilegível NUNCA cala o hook.
    """
    mod_path = Path.home() / ".claude/hooks/injection_policy.py"
    if not mod_path.exists():
        return True
    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location("injection_policy", mod_path)
        if spec is None or spec.loader is None:
            return True
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return bool(mod.allows("plugin-hook"))
    except Exception:  # noqa: BLE001 — fail-open por contrato do interruptor
        return True


def main() -> int:
    """Hook PreToolUse (Write): consulta o portfólio e avisa — nunca bloqueia."""
    if os.environ.get("TOURING_PORTFOLIO_GATE_DISABLED"):
        return 0
    if "--stats" in sys.argv:
        return stats()
    if not injection_allowed():
        return 0
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, EOFError):
        return 0  # fail-open: payload ilegível nunca bloqueia
    ti = payload.get("tool_input") or {}
    path = str(ti.get("file_path", ""))
    if Path(path).suffix.lower() not in SCRIPT_EXTS:
        return 0
    basename = Path(path).name
    ephemeral = is_ephemeral(path)
    generic = basename in GENERIC_BASENAMES
    if not (ephemeral or generic):
        return 0

    keys = keywords_of(basename, str(ti.get("content", "")))
    candidates = manifest_candidates(keys) or tantivy_candidates(" ".join(keys))
    same_name = any(Path(c["path"]).name == basename for c in candidates)
    would_avoid = bool(candidates) and (same_name or any(c.get("hits", 0) >= 2 for c in candidates))
    log_event({
        "ts": int(time.time()), "file": path, "basename": basename,
        "ephemeral": ephemeral, "generic_name": generic,
        "n_candidates": len(candidates), "would_avoid": would_avoid,
        "top": [c["path"][-80:] for c in candidates],
    })
    if not candidates:
        return 0  # sem candidato, silêncio (o portfólio ainda está sendo povoado)

    lista = " · ".join(f"{c['path']} ({c['why']})" for c in candidates)
    destino = "destino EFÊMERO (morre com a sessão)" if ephemeral else "basename genérico recriado entre sessões"
    emit({"context": (
        f"[PORTFOLIO · consulta-antes-de-criar] {basename}: {destino}. "
        f"O portfólio/índice já tem trabalho próximo: {lista}. "
        f"Antes de recriar: leia o candidato e estenda; se o script for reutilizável, grave em "
        f"~/.claude/scripts-portfolio/ (contrato no MANIFEST.json) ou `touring run --harvest <slug>` "
        f"(medido 27/10: 81% dos scripts nascem efêmeros; --harvest nunca foi usado)."
    )})
    return 0


if __name__ == "__main__":
    sys.exit(main())
