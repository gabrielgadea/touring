#!/usr/bin/env python3
"""Mineracao dos transcripts do Claude Code: onde e por que scripts sao criados.

Varre ~/.claude/projects/**/*.jsonl uma unica vez (stream, pre-filtro em bytes),
extrai eventos de criacao de script (Write para extensoes de script, heredocs no
Bash, python -c inline, touring run --lang/--harvest) e emite UM JSON agregado.

Saida: stdout = agregado compacto; o detalhe completo vai para
detail_path (mesmo diretorio deste script).
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

PROJECTS = Path.home() / ".claude" / "projects"
DETAIL_PATH = Path(__file__).resolve().parent / "transcript_miner_detail.json"

SCRIPT_EXTS = {".py", ".sh", ".js", ".mjs", ".ts", ".rb", ".pl", ".bash"}
HEREDOC_WRITE = re.compile(r"(?:cat|tee)\s*>{1,2}\s*([^\s<|;]+)\s*<<", re.S)
INLINE_PY = re.compile(r"python3?\s+-\s*<<|python3?\s+-c\s", re.S)

EPHEMERAL_PREFIXES = ("/tmp", "/var/tmp", "/dev/shm")
EPHEMERAL_PARTS = ("/.cache/", "/scratchpad", "/tool-results/", "/pytest-of-", "/touring-run-")
PLAN_PART = "/docs/plans/"

PURPOSE_RULES = [
    ("teste/verificacao", ("test", "assert", "verif", "check_", "_check", "pytest", "gate")),
    ("benchmark/medicao", ("bench", "measure", "timing", "perf", "p95", "latency")),
    ("auditoria/cross-audit", ("audit", "cross_audit", "crossaudit", "review")),
    ("analise/relatorio", ("analy", "report", "diagnos", "census", "survey", "inventory", "miner")),
    ("migracao/rewrite", ("migrat", "rewrite", "refactor", "backfill", "port_")),
    ("sync/propagacao", ("sync", "propagat", "mirror", "deploy", "release")),
    ("corpus/dataset", ("corpus", "dataset", "seed", "fixture", "generat")),
    ("juiz/julgamento", ("judge", "juiz", "score", "rubric", "eval")),
    ("scrape/fetch", ("fetch", "scrape", "crawl", "download")),
    ("validacao-plano", ("validat", "plan_", "converge", "loop_")),
]


def classify_location(path: str) -> str:
    """Rotula o destino do script: efemero vs persistente (e qual tipo)."""
    low = path.lower()
    if low.startswith(EPHEMERAL_PREFIXES) or any(p in low for p in EPHEMERAL_PARTS):
        return "efemero"
    if PLAN_PART in low:
        return "persistente:docs/plans"
    if "/scripts/" in low:
        return "persistente:scripts/"
    if "/.claude/skills/" in low:
        return "persistente:skill"
    if "/.local/bin/" in low:
        return "persistente:bin"
    if "/.claude/hooks/" in low:
        return "persistente:hook"
    return "persistente:outro"


def classify_purpose(basename: str, head: str) -> str:
    """Classifica a finalidade por palavras-chave do nome + 400 primeiros chars."""
    text = (basename + " " + head[:400]).lower()
    for label, keys in PURPOSE_RULES:
        if any(k in text for k in keys):
            return label
    return "outro"


def norm_hash(content: str) -> str:
    """Hash do conteudo normalizado (sem whitespace/comentarios) p/ near-dup."""
    lines = []
    for ln in content.splitlines():
        s = ln.strip()
        if not s or s.startswith("#") or s.startswith("//"):
            continue
        lines.append(re.sub(r"\s+", "", s))
    return hashlib.sha1("\n".join(lines).encode()).hexdigest()[:16]


def iter_tool_uses(obj: dict) -> list[dict]:
    """Extrai blocos tool_use de uma linha de transcript."""
    msg = obj.get("message")
    if not isinstance(msg, dict):
        return []
    content = msg.get("content")
    if not isinstance(content, list):
        return []
    return [c for c in content if isinstance(c, dict) and c.get("type") == "tool_use"]


def main() -> None:
    events: list[dict] = []
    files = sorted(PROJECTS.rglob("*.jsonl"))
    errors = 0
    for fp in files:
        project = fp.parent.name
        session = fp.stem
        try:
            with fp.open("rb") as fh:
                for raw in fh:
                    if b'"tool_use"' not in raw:
                        continue
                    if b'"Write"' not in raw and b'"Bash"' not in raw:
                        continue
                    try:
                        obj = json.loads(raw)
                    except (json.JSONDecodeError, UnicodeDecodeError):
                        errors += 1
                        continue
                    ts = str(obj.get("timestamp", ""))[:7]  # YYYY-MM
                    for tu in iter_tool_uses(obj):
                        name = tu.get("name")
                        inp = tu.get("input") or {}
                        if name == "Write":
                            path = str(inp.get("file_path", ""))
                            if Path(path).suffix.lower() not in SCRIPT_EXTS:
                                continue
                            content = str(inp.get("content", ""))
                            events.append({
                                "kind": "write", "path": path,
                                "basename": Path(path).name,
                                "loc": classify_location(path),
                                "purpose": classify_purpose(Path(path).name, content),
                                "bytes": len(content), "hash": norm_hash(content),
                                "session": session, "project": project, "month": ts,
                            })
                        elif name == "Bash":
                            cmd = str(inp.get("command", ""))
                            m = HEREDOC_WRITE.search(cmd)
                            if m:
                                path = m.group(1)
                                events.append({
                                    "kind": "heredoc", "path": path,
                                    "basename": Path(path).name or path,
                                    "loc": classify_location(path),
                                    "purpose": classify_purpose(Path(path).name, cmd),
                                    "bytes": len(cmd), "hash": norm_hash(cmd),
                                    "session": session, "project": project, "month": ts,
                                })
                            elif INLINE_PY.search(cmd) and len(cmd) > 400:
                                events.append({
                                    "kind": "inline_python", "path": "(inline)",
                                    "basename": "(inline)",
                                    "loc": "inline-sem-arquivo",
                                    "purpose": classify_purpose("", cmd),
                                    "bytes": len(cmd), "hash": norm_hash(cmd),
                                    "session": session, "project": project, "month": ts,
                                })
        except OSError:
            errors += 1

    # --- agregacao ---
    by_loc: Counter = Counter(e["loc"] for e in events)
    by_purpose: Counter = Counter(e["purpose"] for e in events)
    by_month: Counter = Counter(e["month"] for e in events)
    by_project: Counter = Counter(e["project"] for e in events)
    by_kind: Counter = Counter(e["kind"] for e in events)

    # duplicatas exatas (mesmo hash normalizado) em paths/sessoes distintas
    hash_groups: dict[str, list[dict]] = defaultdict(list)
    for e in events:
        hash_groups[e["hash"]].append(e)
    exact_dups = {h: g for h, g in hash_groups.items()
                  if len({x["path"] for x in g}) > 1 or len({x["session"] for x in g}) > 1}

    # mesmo basename reescrito em sessoes diferentes (recriacao provavel)
    base_groups: dict[str, set[str]] = defaultdict(set)
    for e in events:
        if e["basename"] not in ("(inline)",):
            base_groups[e["basename"]].add(e["session"])
    rewritten = sorted(
        ((b, len(s)) for b, s in base_groups.items() if len(s) >= 3),
        key=lambda x: -x[1],
    )

    detail = {
        "events_total": len(events),
        "exact_dup_groups": {
            h: [{"path": x["path"], "session": x["session"], "month": x["month"],
                 "purpose": x["purpose"]} for x in g]
            for h, g in sorted(exact_dups.items(), key=lambda kv: -len(kv[1]))[:200]
        },
        "top_rewritten_basenames": rewritten[:100],
        "top_basenames": Counter(e["basename"] for e in events).most_common(60),
    }
    DETAIL_PATH.write_text(json.dumps(detail, ensure_ascii=False, indent=1))

    dup_event_count = sum(len(g) for g in exact_dups.values())
    ephemeral = sum(v for k, v in by_loc.items() if k in ("efemero", "inline-sem-arquivo"))
    out = {
        "transcripts_varridos": len(files),
        "erros_parse": errors,
        "eventos_criacao_script": len(events),
        "por_tipo": dict(by_kind),
        "por_local": dict(by_loc.most_common()),
        "pct_efemero_ou_inline": round(100 * ephemeral / max(1, len(events)), 1),
        "por_finalidade": dict(by_purpose.most_common()),
        "por_mes": dict(sorted(by_month.items())),
        "top_projetos": dict(by_project.most_common(10)),
        "grupos_duplicata_exata": len(exact_dups),
        "eventos_em_grupos_dup": dup_event_count,
        "pct_eventos_duplicados": round(100 * dup_event_count / max(1, len(events)), 1),
        "top_basenames_reescritos_3plus_sessoes": rewritten[:25],
        "detalhe_em": str(DETAIL_PATH),
    }
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    sys.exit(main())
