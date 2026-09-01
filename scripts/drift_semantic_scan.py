#!/usr/bin/env python3
"""Drift Semântico Scan — 7 categorias de drift pós 14 releases.

Origem: loop 2026-08-26-documentacao-touring, opção (c) autorizada por Gabriel 2026-08-31.
        Recomendação do strategy-2026-08-31-retomar-decisao-drift.md.

7 categorias (lesson-driven — lessons de drift no memory):
  C1: Versões citadas em docs (vs `grep "^version" Cargo.toml`)
  C2: Paths de binário deprecated (vs paths reais via `which touring`, `readlink`)
  C3: Crates fundidos citados (vs `ls crates/`)
  C4: Comandos CLI citados (vs `touring --help`)
  C5: Nomes de crates em exemplos de código (code fences) (vs `ls crates/`)
  C6: Variáveis de ambiente citadas (vs `env | grep TOURING_`)
  C7: Hooks citados (vs `ls ~/.claude/hooks/`)

NOTA VGP: scan_source (crates/touring-hooks-shared/src/ast_grep_signal.rs:96) é o equivalente
Rust-internal para AST-grep scan (42 references, bench P7.5). Aqui usamos Python+regex porque
o target é .md (texto não-AST). tour ast grep --lang markdown não é canônico.

Modos de uso:
  python3 drift_semantic_scan.py           # JSON completo em stdout
  python3 drift_semantic_scan.py --check   # exit 0 se sem drift, exit 1 se drift detectado (CI guard)
  python3 drift_semantic_scan.py --quiet   # progresso em stderr suprimido
  python3 drift_semantic_scan.py --check --quiet   # CI mode silencioso

Saída: stdout JSON (1 chamada) — caller redireciona para
       docs/plans/2026-08-26-documentacao-touring/knowledge/scan-YYYY-MM-DD.json.

SEGURANÇA: zero `shell=True` em subprocess (CMDi CWE-78 — remediado pós gate F2.1).
"""
import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path

ROOT = Path("/home/gabrielgadea/projects/touring")


# ──────────────────────────────────────────────────────────────────────────────
# Coleta de arquivos .md no escopo (sem bash, usa pathlib)
# ──────────────────────────────────────────────────────────────────────────────

def collect_scope_files():
    """Coleta .md em docs/, crates/, ARCHITECTURE.md, CLAUDE.md (sem target/node_modules)."""
    files = set()
    # Single .md files
    for name in ("ARCHITECTURE.md", "CLAUDE.md"):
        p = ROOT / name
        if p.is_file():
            files.add(name)
    # Recursive scan com pathlib.rglob
    for md_path in ROOT.rglob("*.md"):
        rel = md_path.relative_to(ROOT).as_posix()
        if "/target/" in rel or "/node_modules/" in rel:
            continue
        # Tudo em docs/ e crates/ conta
        if rel.startswith("docs/") or rel.startswith("crates/"):
            files.add(rel)
    return sorted(files)


# ──────────────────────────────────────────────────────────────────────────────
# Ground truth (sem bash; usa Path/os/subprocess com lista de args)
# ──────────────────────────────────────────────────────────────────────────────

def gt_workspace_version() -> str:
    cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8", errors="ignore")
    for line in cargo_toml.split("\n"):
        m = re.match(r'^\s*version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    return "unknown"


def gt_actual_crates() -> list[str]:
    crates_dir = ROOT / "crates"
    if not crates_dir.is_dir():
        return []
    return sorted([p.name for p in crates_dir.iterdir() if p.is_dir()])


def gt_actual_envvars() -> list[str]:
    return sorted([k for k in os.environ if k.startswith("TOURING_")])


def gt_actual_cli_commands() -> set[str]:
    try:
        r = subprocess.run(
            ["touring", "--help"],
            capture_output=True, text=True, cwd=ROOT, timeout=30,
        )
        help_out = r.stdout
    except Exception as e:
        print(f"  WARN: touring --help failed: {e}", file=sys.stderr)
        return set()
    return set(re.findall(r"^\s+([a-z][a-z0-9_-]+)\s", help_out, re.MULTILINE))


def gt_actual_hooks() -> list[str]:
    hooks_dir = Path("/home/gabrielgadea/.claude/hooks")
    if not hooks_dir.is_dir():
        return []
    return sorted([p.name for p in hooks_dir.glob("touring*") if p.is_file()])


# ──────────────────────────────────────────────────────────────────────────────
# Scan por categoria
# ──────────────────────────────────────────────────────────────────────────────

def scan_c1_versions(content_by_file: dict, gt_version: str) -> dict:
    """Versões citadas stale vs versão atual."""
    C1_PATTERN = re.compile(r"\bv?30\.(\d+)\.(\d+)\b")
    cited = set()
    files_with = []
    for f, content in content_by_file.items():
        matches = C1_PATTERN.findall(content)
        if matches:
            distinct = sorted(set(f"30.{a}.{b}" for a, b in matches))
            files_with.append((f, len(matches), distinct))
            for a, b in matches:
                cited.add(f"30.{a}.{b}")
    stale = sorted([v for v in cited if v != gt_version])
    return {
        "gt_current": gt_version,
        "cited_distinct": sorted(cited),
        "stale_versions": stale,
        "stale_count": len(stale),
        "files_with_versions_count": len(files_with),
        "top_files_by_versions_cited": [
            {"file": f, "n_citations": n, "versions": vs}
            for f, n, vs in sorted(files_with, key=lambda x: -x[1])[:15]
        ],
    }


def scan_c2_deprecated_paths(content_by_file: dict) -> dict:
    """Paths de binário deprecated (lesson: daemon-deleted, workspace-frozen)."""
    DEPRECATED = [
        (r"/tmp/touring-daemon-\d+\.sock", "socket legado (substituído por <proj>/.touring/daemon.sock)"),
        (r"target/release/touring-daemon", "binário release direto (hoje vai via ~/.local/bin/touring)"),
        (r"target/debug/touring", "binário debug"),
        (r"\.claude/rust/", "raiz workspace congelada (touring workspace migrou para ~/projects/touring)"),
        (r"\.touring-daemon\.pid", "PID file legado (substituído por /run/user/$UID/touring-daemon.pid)"),
    ]
    result = {}
    for pattern, label in DEPRECATED:
        files_with = [f for f, c in content_by_file.items() if re.search(pattern, c)]
        if files_with:
            result[pattern] = {"label": label, "files_count": len(files_with), "files_sample": files_with[:10]}
    return result


def scan_c3_fused_crates(content_by_file: dict, gt_crates: list[str]) -> dict:
    """Crates fundidos citados (do f1-inventario + lessons)."""
    FUSED = [
        "touring-core", "touring-learning", "touring-ast", "touring-cognitive",
        "touring-index", "touring-cli", "touring-server", "touring-hooks",
        "touring-evolve", "touring-generator", "touring-web",
    ]
    result = {}
    for crate in FUSED:
        files_with = [f for f, c in content_by_file.items() if re.search(rf"\b{re.escape(crate)}\b", c)]
        if files_with:
            result[crate] = files_with
    return {
        "actual_crates_count": len(gt_crates),
        "actual_crates": gt_crates,
        "fused_crates_with_citations": result,
        "fused_crates_count": len(result),
        "total_stale_files": len(set(f for fs in result.values() for f in fs)),
    }


def scan_c4_cli(content_by_file: dict, gt_cli: set[str]) -> dict:
    """Comandos CLI citados vs `touring --help`."""
    C4_PATTERN = re.compile(r"\btouring ([a-z][a-z0-9_-]+)\b")
    cited = set()
    files_with = {}
    for f, content in content_by_file.items():
        cmds = set()
        for m in C4_PATTERN.finditer(content):
            cited.add(m.group(1))
            cmds.add(m.group(1))
        if cmds:
            files_with[f] = sorted(cmds)
    stale = sorted([c for c in cited if c not in gt_cli])
    return {
        "cited_distinct_count": len(cited),
        "actual_commands_count": len(gt_cli),
        "cited_distinct_sample": sorted(cited)[:30],
        "potential_stale_count": len(stale),
        "potential_stale": stale,
        "files_citing_potential_stale_count": sum(
            1 for f, cmds in files_with.items() if any(c in stale for c in cmds)
        ),
    }


def scan_c5_crates_in_code(content_by_file: dict, gt_crates: list[str]) -> dict:
    """Crates em exemplos de código (code fences) — F4/F5 aligned (ARCHITECTURE*.md)."""
    CODE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
    cited = set()
    files_with = {}
    for f, content in content_by_file.items():
        crates = set()
        for block in CODE_FENCE.findall(content):
            for m in re.finditer(r"\b(touring-[a-z][a-z0-9_-]+)\b", block):
                cited.add(m.group(1))
                crates.add(m.group(1))
            for m in re.finditer(r"\b(touring_[a-z][a-z0-9_]+)\b", block):
                cited.add(m.group(1).replace("_", "-"))
                crates.add(m.group(1).replace("_", "-"))
        if crates:
            files_with[f] = sorted(crates)
    stale = sorted([c for c in cited if c not in gt_crates])
    return {
        "cited_distinct_count": len(cited),
        "actual_crates_count": len(gt_crates),
        "cited_distinct_sample": sorted(cited)[:30],
        "stale_count": len(stale),
        "stale": stale,
        "files_with_stale_code_examples_count": sum(
            1 for f, cs in files_with.items() if any(c in stale for c in cs)
        ),
    }


def scan_c6_envvars(content_by_file: dict, gt_envvars: list[str]) -> dict:
    """Env vars citadas vs env real."""
    C6_PATTERN = re.compile(r"\bTOURING_[A-Z][A-Z0-9_]+\b")
    cited = set()
    files_with = {}
    for f, content in content_by_file.items():
        vars_in_file = set()
        for m in C6_PATTERN.finditer(content):
            cited.add(m.group(0))
            vars_in_file.add(m.group(0))
        if vars_in_file:
            files_with[f] = sorted(vars_in_file)
    stale = sorted([v for v in cited if v not in gt_envvars])
    return {
        "cited_distinct_count": len(cited),
        "actual_envvars": sorted(gt_envvars),
        "cited_distinct": sorted(cited),
        "stale_count": len(stale),
        "stale": stale,
        "files_citing_stale_envvars_count": sum(
            1 for f, vs in files_with.items() if any(v in stale for v in vs)
        ),
    }


def scan_c7_hooks(content_by_file: dict, gt_hooks: list[str]) -> dict:
    """Hooks citados vs registry real."""
    C7_PATTERN = re.compile(r"\btouring-hook[a-z_-]*\b")
    files_with = [f for f, c in content_by_file.items() if C7_PATTERN.search(c)]
    return {
        "actual_hooks_count": len(gt_hooks),
        "actual_hooks_sample": gt_hooks,
        "files_citing_hooks_count": len(files_with),
        "files_citing_hooks_sample": files_with[:15],
    }


# ──────────────────────────────────────────────────────────────────────────────
# Main
# ──────────────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(
        description="Drift Semântico Scan — 7 categorias (C1-C7). Exit 0/1 em --check mode.",
    )
    ap.add_argument("--check", action="store_true",
                    help="CI guard mode: exit 0 se nenhum stale, exit 1 se qualquer stale.")
    ap.add_argument("--quiet", action="store_true",
                    help="Suprime progresso em stderr (útil para CI silencioso).")
    args = ap.parse_args()

    def progress(msg):
        if not args.quiet:
            print(msg, file=sys.stderr, flush=True)

    progress("[1/7] Coletando arquivos .md no escopo...")
    files = collect_scope_files()
    content_by_file = {}
    for f in files:
        try:
            content_by_file[f] = (ROOT / f).read_text(encoding="utf-8", errors="ignore")
        except Exception as e:
            progress(f"  WARN: skip {f}: {e}")

    progress("[2/7] Coletando ground truth...")
    gt_version = gt_workspace_version()
    gt_crates = gt_actual_crates()
    gt_envvars = gt_actual_envvars()
    gt_cli_set = gt_actual_cli_commands()
    gt_hooks = gt_actual_hooks()
    if not args.quiet:
        print(f"  version: {gt_version}", file=sys.stderr)
        print(f"  crates: {len(gt_crates)}", file=sys.stderr)
        print(f"  envvars: {len(gt_envvars)}", file=sys.stderr)
        print(f"  cli_commands: {len(gt_cli_set)}", file=sys.stderr)
        print(f"  hooks: {len(gt_hooks)}", file=sys.stderr)

    results = {
        "scan_timestamp_utc": datetime.utcnow().isoformat() + "Z",
        "scope_files_count": len(files),
        "gt": {
            "version": gt_version,
            "crates_count": len(gt_crates),
            "envvars": gt_envvars,
            "cli_commands_count": len(gt_cli_set),
            "hooks": gt_hooks,
        },
        "categories": {},
    }

    progress("[3/7] C1: Versões citadas...")
    results["categories"]["C1-versions"] = scan_c1_versions(content_by_file, gt_version)

    progress("[4/7] C2: Paths deprecated...")
    results["categories"]["C2-deprecated-paths"] = scan_c2_deprecated_paths(content_by_file)

    progress("[5/7] C3: Crates fundidos...")
    c3 = scan_c3_fused_crates(content_by_file, gt_crates)
    results["categories"]["C3-fused-crates-cited"] = c3

    progress("[6/7] C4: Comandos CLI...")
    c4 = scan_c4_cli(content_by_file, gt_cli_set)
    results["categories"]["C4-cli-cited"] = c4

    progress("[7/7] C5/C6/C7: Crates em code/envvars/hooks...")
    c5 = scan_c5_crates_in_code(content_by_file, gt_crates)
    results["categories"]["C5-crates-in-code-examples"] = c5
    c6 = scan_c6_envvars(content_by_file, gt_envvars)
    results["categories"]["C6-envvars-cited"] = c6
    c7 = scan_c7_hooks(content_by_file, gt_hooks)
    results["categories"]["C7-hooks-cited"] = c7

    # Summary — classificação F4/F5 vs W6/W10
    c2_data = results["categories"]["C2-deprecated-paths"]
    c2_files_total = sum(v["files_count"] for v in c2_data.values())
    results["summary"] = {
        "scope_files_count": len(files),
        "drift_f4_f5_aligned": {
            "C1-versions": results["categories"]["C1-versions"]["stale_count"],
            "C3-fused-crates-cited-files": c3["total_stale_files"],
            "C5-crates-in-code-examples": c5["stale_count"],
        },
        "drift_w6_w10_aligned": {
            "C2-deprecated-paths-files": c2_files_total,
            "C4-cli-cited-stale": c4["potential_stale_count"],
            "C6-envvars-cited-stale": c6["stale_count"],
            "C7-hooks-cited-files": c7["files_citing_hooks_count"],
        },
    }

    # Modo --check: exit 0/1, NÃO imprime JSON
    if args.check:
        # Soma sinais stale (C1+C3+C5+C6 — categorias com stale_count numérico)
        total_stale = (
            results["categories"]["C1-versions"]["stale_count"]
            + c3["fused_crates_count"]
            + c5["stale_count"]
            + c6["stale_count"]
        )
        c4_stale = c4["potential_stale_count"]
        print(f"CHECK: {total_stale + c4_stale} stale signals (C1={results['categories']['C1-versions']['stale_count']}, C3={c3['fused_crates_count']}, C4={c4_stale}, C5={c5['stale_count']}, C6={c6['stale_count']})", file=sys.stderr)
        if total_stale + c4_stale > 0:
            sys.exit(1)
        sys.exit(0)

    # Default: imprime JSON completo
    print(json.dumps(results, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
