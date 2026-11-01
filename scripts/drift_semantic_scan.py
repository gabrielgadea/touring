#!/usr/bin/env python3
"""Drift Semântico Scan — 7 categorias de drift pós 14 releases.

Origem: loop 2026-08-26-documentacao-touring, opção (c) autorizada por Gabriel 2026-08-31.
        Recomendação do strategy-2026-08-31-retomar-decisao-drift.md.

7 categorias (lesson-driven — lessons de drift no memory):
  C1: Versões citadas em docs (vs `grep "^version" Cargo.toml`)
  C2: Paths de binário deprecated (vs paths reais via `which touring`, `readlink`)
  C3: Crates fundidos citados (vs `ls crates/`)
  C4: Comandos CLI citados (vs `touring --help`) — predicado refinado 08/10/2026:
      conta só citação em CONTEXTO DE CÓDIGO (code fence, inline `...`, linha de
      shell $/#/❯) com "touring" NÃO precedido de "/" (elimina o diretório do
      workspace `.../projects/touring` e paths de build; invocação por path de
      build fica fora por construção — majoritariamente em audits datados).
      Antes, prosa portuguesa ("o touring deixa de...") contava como citação:
      18 dos 26 sinais novos do delta 31/08→08/10 eram esse falso positivo.
      10ª reconciliação (01/11/2026): "touring" precedido de "[" é rótulo de log
      (`[touring result-compactor]`, o rodapé do compactador), não subcomando.
  C5: Nomes de crates em exemplos de código (code fences) (vs `ls crates/`) —
      10ª reconciliação (01/11/2026): o nome só conta como crate quando começa
      num limite de token — não é sufixo de um token com hífen (diretório de
      bundle datado `2026-10-26-touring-x`), a forma com sublinhado não é
      componente de caminho (`~/.ssh/touring_signing`), `touring-wt-*` é
      diretório de worktree git, e os diretórios de `tools/mods/` são
      componentes do repo (mods do Claude Code), não crates ausentes.
  C1: 10ª reconciliação (01/11/2026): inteiro com separador de milhar pt-BR
      (`30.973.693 B`: minor e patch com exatamente 3 dígitos) não é versão.
  C6: Variáveis de ambiente citadas — predicado corrigido 08/10/2026: ground
      truth = vars TOURING_* referenciadas no CÓDIGO-FONTE (.rs/.py/.sh/.toml),
      estático e reproduzível. Antes era `os.environ` do processo do scanner —
      o resultado dependia de QUEM roda; uma var viva no código mas não
      exportada na sessão virava falso "stale" (23 dos 30 sinais novos do delta).
      10ª reconciliação (01/11/2026): `.ts` entra no ground truth (os mods de
      `tools/mods/` leem env em TypeScript; a var do mod compact-evict só vive lá).
      CUIDADO: este arquivo é .py fora de docs/, logo entra no ground truth —
      nunca escreva aqui o nome literal de uma var que as docs citam.
  C7: Hooks citados (vs `ls ~/.claude/hooks/`)

Reconciliação do delta 31/08→08/10 (561→669, 111 sinais novos por conjunto):
  docs/plans/2026-10-08-drift-reconciliacao/ (ledger.jsonl + reconciliar.py).

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
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

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


def gt_actual_mods() -> list[str]:
    """Mods do Claude Code versionados no repo (`tools/mods/<nome>/`).

    Componente legítimo que o C5 não pode ler como crate ausente: um mod é citado
    com o nome do diretório em `cp -r tools/mods/<nome> …` e `claude plugin install <nome>@…`.
    """
    mods_dir = ROOT / "tools" / "mods"
    if not mods_dir.is_dir():
        return []
    return sorted([p.name for p in mods_dir.iterdir() if p.is_dir()])


def gt_actual_envvars() -> list[str]:
    """Ground truth ESTÁTICO de env vars: TOURING_* referenciadas no código-fonte.

    Substitui (reconciliação 08/10/2026) o `os.environ` do processo do scanner,
    que tornava a contagem dependente de QUEM roda: uma var viva no código mas
    não exportada na sessão virava falso "stale" (23 dos 30 sinais novos do
    delta 31/08→08/10 eram esse caso). `.json` fica fora: dados históricos
    (ex.: docs/kpi/*.json) não são reconhecimento vivo. `docs/` fica fora:
    artefatos de documentação (inclusive scripts de planos, que citam vars em
    strings) não são código do sistema — medido: o próprio bundle da
    reconciliação contaminou o ground truth com 7 vars (26→19 fantasma).
    """
    # `.ts`: os mods de tools/mods/ leem env em TypeScript (10ª reconciliação, 01/11/2026).
    exts = {".rs", ".py", ".sh", ".toml", ".ts"}
    skip = ("/target/", "/node_modules/", "/.git/", "/.touring-cache/", "/.venv/")
    found: set[str] = set()
    for p in ROOT.rglob("*"):
        if not p.is_file() or p.suffix not in exts:
            continue
        rel = p.relative_to(ROOT).as_posix()
        if rel.startswith("docs/") or any(s in f"/{rel}" for s in skip):
            continue
        try:
            found.update(re.findall(r"\bTOURING_[A-Z][A-Z0-9_]+\b",
                                    p.read_text(encoding="utf-8", errors="ignore")))
        except OSError:
            pass
    return sorted(found)


def gt_actual_cli_commands() -> set[str]:
    try:
        r = subprocess.run(
            ["touring", "--help"],
            capture_output=True, text=True, cwd=ROOT, timeout=30,
        )
        # O binário imprime o help INTEIRO no stderr (stdout vazio, rc=0) —
        # medido 08/10/2026: 0 linhas em stdout, 164 em stderr. Ler só stdout
        # era o defeito que zerava o ground truth desde 31/08 (snapshot da época
        # já registrava cli_commands_count: 0): TODO token citado contava como
        # "potencialmente stale", sem comparação real.
        help_out = r.stdout + "\n" + r.stderr
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
        # Inteiro com separador de milhar pt-BR ("30.973.693 B": 30 milhões) casa o
        # padrão, mas minor e patch com exatamente 3 dígitos cada não é versão.
        matches = [(a, b) for a, b in C1_PATTERN.findall(content)
                   if not (len(a) == 3 and len(b) == 3)]
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


RE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
RE_INLINE_CODE = re.compile(r"`[^`\n]+`")
# "[" também antecede rótulo de log (`[touring result-compactor]`), que não é subcomando.
RE_C4_CMD = re.compile(r"(?<![/\w\[])touring ([a-z][a-z0-9_-]+)\b")
# C5: nome de crate começa num limite de token. `(?<!-)` descarta o sufixo de um token com
# hífen (diretório de bundle datado `2026-10-26-touring-x`); `(?<!/)` na forma com sublinhado
# descarta componente de caminho (`~/.ssh/touring_signing`) — diretórios de crate usam hífen.
RE_C5_HYPHEN = re.compile(r"(?<!-)\b(touring-[a-z][a-z0-9_-]+)\b")
RE_C5_UNDERSCORE = re.compile(r"(?<!/)\b(touring_[a-z][a-z0-9_]+)\b")
# Diretório de worktree git por convenção do projeto (`~/projects/touring-wt-<tag>`).
C5_WORKTREE_PREFIX = "touring-wt-"


def _code_spans(content: str) -> list[tuple[int, int]]:
    """Spans de code fence e código inline `...` (fences mascaradas p/ não duplar)."""
    spans = [(m.start(), m.end()) for m in RE_FENCE.finditer(content)]
    masked = RE_FENCE.sub(lambda f: " " * (f.end() - f.start()), content)
    spans += [(m.start(), m.end()) for m in RE_INLINE_CODE.finditer(masked)]
    return spans


def _in_code_context(content: str, pos: int, spans: list[tuple[int, int]]) -> bool:
    """O match está em fence/inline-code, ou numa linha de shell ($/#/❯)?"""
    if any(s <= pos < e for s, e in spans):
        return True
    line_start = content.rfind("\n", 0, pos) + 1
    return content[line_start:pos].lstrip()[:1] in ("$", "#", "❯")


def scan_c4_cli(content_by_file: dict, gt_cli: set[str]) -> dict:
    """Comandos CLI citados vs `touring --help`.

    Predicado (reconciliação 08/10/2026): uma citação conta quando
      (i) "touring" NÃO é precedido de "/" nem de caractere de palavra — elimina
          o diretório do workspace (".../projects/touring timeout ...") e paths
          de build ("target/debug/touring scan"); invocação por path de build
          fica fora por construção (majoritariamente em audits datados), E
      (ii) o match está em contexto de código (fence, inline `...`, linha de
           shell) — elimina prosa portuguesa ("o touring deixa de ser..."),
           18 dos 26 sinais novos do delta 31/08→08/10.
    """
    cited = set()
    files_with = {}
    for f, content in content_by_file.items():
        spans = _code_spans(content)
        cmds = set()
        for m in RE_C4_CMD.finditer(content):
            if not _in_code_context(content, m.start(), spans):
                continue
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


def _c5_names_in_block(block: str) -> set[str]:
    """Nomes de crate citados num code fence (forma `touring-x` e `touring_x`, normalizada
    para hífen), sem os diretórios de worktree git (`touring-wt-*`)."""
    names = {m.group(1) for m in RE_C5_HYPHEN.finditer(block)}
    names |= {m.group(1).replace("_", "-") for m in RE_C5_UNDERSCORE.finditer(block)}
    return {n for n in names if not n.startswith(C5_WORKTREE_PREFIX)}


def scan_c5_crates_in_code(content_by_file: dict, gt_crates: list[str]) -> dict:
    """Crates em exemplos de código (code fences) — F4/F5 aligned (ARCHITECTURE*.md)."""
    CODE_FENCE = re.compile(r"```[a-zA-Z]*\n(.*?)```", re.DOTALL)
    cited = set()
    files_with = {}
    for f, content in content_by_file.items():
        crates = set()
        for block in CODE_FENCE.findall(content):
            crates |= _c5_names_in_block(block)
        cited |= crates
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
    gt_mods = gt_actual_mods()
    gt_envvars = gt_actual_envvars()
    gt_cli_set = gt_actual_cli_commands()
    gt_hooks = gt_actual_hooks()
    if not args.quiet:
        print(f"  version: {gt_version}", file=sys.stderr)
        print(f"  crates: {len(gt_crates)}", file=sys.stderr)
        print(f"  envvars: {len(gt_envvars)}", file=sys.stderr)
        print(f"  cli_commands: {len(gt_cli_set)}", file=sys.stderr)
        print(f"  hooks: {len(gt_hooks)}", file=sys.stderr)

    results: dict[str, Any] = {
        "scan_timestamp_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "scope_files_count": len(files),
        "gt": {
            "version": gt_version,
            "crates_count": len(gt_crates),
            "envvars": gt_envvars,
            "envvars_ground_truth": "code (.rs/.py/.sh/.toml/.ts) — desde 08/10/2026 (.ts desde 01/11/2026)",
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
    # Mods de tools/mods/ são componentes do repo: entram no ground truth do C5 (não no do C3).
    c5 = scan_c5_crates_in_code(content_by_file, gt_crates + gt_mods)
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
