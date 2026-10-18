"""Deterministic inventory of the whole ECC catalog for the deepening round H (17/10/2026).

Usage: inventario_ecc.py <ecc_repo> <out_dir>
Read-only on the clone (untrusted data: nothing in it is executed). Writes <out_dir>/inventario-ecc.json,
<out_dir>/INVENTARIO.md and one <out_dir>/lote-<id>.md per reading batch of the round: every skill/agent/command/rule
with its description, size and class, and whether each harness adapter dir is generated from the canonical source.
"""
import collections
import difflib
import json
import re
import sys
from pathlib import Path

CLASSES = [  # first match wins; order matters (a "rust-security" skill is security first)
    ("seguranca", r"secur|secret|vuln|owasp|safety|hipaa|pentest|bounty|threat|sanitiz|permission"),
    ("dominio-nao-codigo", (r"marketing|seo\b|brand|investor|sales|health|medic|financ|legal|video|image|remotion|"
                            r"social|crosspost|article|x402|payment|carrier|taste|content|copywrit|email|newsletter|"
                            r"pitch|startup|ecommerce|shop|recruit|resume|bengali|music|podcast|persona")),
    ("contexto-custo-memoria", r"context|token|cost|budget|compact|memory|learning|instinct|recall|session"),
    ("processo", (r"tdd|plan|prp|review|verif|eval|e2e|test|loop|iterat|retriev|refactor|debug|build|ci\b|release|"
                  r"commit|\bpr\b|onboard|tour|architect|doc|spec|deploy|migrat|benchmark|perf|santa|orchestr|agent")),
]
LANG = (r"\b(rust|python|go|golang|java|kotlin|typescript|javascript|react|angular|vue|django|rails|laravel|spring|"
        r"swift|cpp|c\+\+|csharp|dotnet|php|elixir|scala|flutter|dart|ruby|node|nextjs|svelte|harmonyos)\b")
FRONT = re.compile(r"^---\n.*?\n---", re.DOTALL)
OTHER_DIRS = ("docker/context-profiles", "examples", "src/llm", "schemas", "manifests", "ecc2", "integrations", "plugins", "hooks")
ADAPTERS = (".kiro", ".cursor", ".opencode", ".agents", ".codex", ".codebuddy", ".trae")


def front(text: str) -> dict:
    m = re.match(r"^---\n(.*?)\n---", text, re.DOTALL)
    out = {}
    if m:
        for line in m.group(1).splitlines():
            k, _, v = line.partition(":")
            if v and not k.startswith(" "):
                out[k.strip()] = v.strip().strip('"').strip("'")
    return out


def classify(name: str, desc: str) -> tuple[str, list[str]]:
    hay = f"{name} {desc}".lower()
    langs = sorted({m.group(1) for m in re.finditer(LANG, hay)})
    for cls, pat in CLASSES:
        if re.search(pat, hay):
            return cls, langs
    return ("linguagem" if langs else "outros"), langs


def entry(kind: str, path: Path, root: Path, name: str) -> dict:
    text = path.read_text(encoding="utf-8", errors="replace")
    fm = front(text)
    desc = fm.get("description", "") or next(
        (ln.strip("# ").strip() for ln in text.splitlines()[1:12] if ln.strip() and not ln.startswith("---")), "")
    cls, langs = classify(name, desc)
    heads = [ln.strip("# ").strip() for ln in text.splitlines() if re.match(r"^#{2,3} ", ln)][:12]
    scripts = sorted(set(re.findall(r"(?:scripts|hooks|bin)/[\w./-]+\.(?:js|ts|py|sh|mjs|cjs)", text)))[:8]
    return {"tipo": kind, "nome": name, "caminho": str(path.relative_to(root)), "linhas": text.count("\n") + 1,
            "classe": cls, "linguagens": langs, "descricao": desc[:240], "secoes": heads, "scripts": scripts}


def catalog(root: Path) -> list[dict]:
    cat = [entry("skill", d / "SKILL.md", root, d.name) for d in sorted((root / "skills").iterdir()) if (d / "SKILL.md").is_file()]
    cat += [entry("agent", p, root, p.stem) for p in sorted((root / "agents").glob("*.md"))]
    cat += [entry("command", p, root, p.stem) for p in sorted((root / "commands").glob("*.md"))]
    cat += [entry("rule", p, root, str(p.relative_to(root / "rules"))) for p in sorted((root / "rules").rglob("*.md"))]
    return cat


def ident(path: str) -> str:
    """Matching key of a catalog file: a skill is named by its directory (every skill file is SKILL.md)."""
    p = Path(path)
    return (p.parent.name if p.stem.upper() == "SKILL" else p.stem).lower()


def _norm(text: str) -> str:
    return re.sub(r"\s+", " ", FRONT.sub("", text)).strip()[:4000]


def adapter_origin(root: Path, adapter: str, canon: dict[str, str]) -> dict:
    """Adapter files whose body matches a canonical file of the same name (ratio >= 0.9): generated, not hand-written.

    `com_par_na_fonte` counts NAME matches only (a homonym, not a verified pair): generic names such as README or
    SKILL collide with unrelated files, so `homonimos_ambiguos` counts the files with more than one candidate.
    `iguais_a_fonte` is the robust number: the body is compared with every candidate and the best ratio is kept."""
    by_id = collections.defaultdict(list)
    for k, v in canon.items():
        by_id[ident(k)].append(_norm(v))
    files = [p for p in (root / adapter).rglob("*") if p.is_file() and p.suffix in (".md", ".mdc", ".json", ".yaml", ".yml")]
    same, named, ambiguous, sample, per_file = 0, 0, 0, [], []
    for p in files:
        cands = by_id.get(ident(str(p)), [])
        named += bool(cands)
        ambiguous += len(cands) > 1
        body = _norm(p.read_text(encoding="utf-8", errors="replace"))
        r = max((difflib.SequenceMatcher(None, body, c, autojunk=False).ratio() for c in cands), default=0.0)
        same += r >= 0.9
        sample.append((str(p.relative_to(root)), round(r, 2)))
        per_file.append({"caminho": str(p.relative_to(root)), "tem_par": bool(cands), "candidatos": len(cands),
                         "razao": round(r, 2)})
    return {"arquivos": len(files), "com_par_na_fonte": named, "homonimos_ambiguos": ambiguous,
            "iguais_a_fonte": same, "amostra": sample[:6],
            "por_arquivo": per_file}


def other_dirs(root: Path) -> dict:
    other = {}
    for d in OTHER_DIRS:
        p = root / d
        if p.is_dir():
            fs = [f for f in p.rglob("*") if f.is_file()]
            other[d] = {"arquivos": len(fs), "subdirs": sorted({str(f.relative_to(p)).split("/")[0] for f in fs})[:40],
                        "linhas": sum(f.read_text(encoding="utf-8", errors="replace").count("\n") for f in fs if f.stat().st_size < 2_000_000)}
    return other


LOTES = {  # reading batches of round H: catalog entries routed to one reader each
    "H1-regras": lambda e: e["tipo"] == "rule",
    "H2a-skills-processo": lambda e: e["tipo"] == "skill" and e["classe"] == "processo",
    "H2b-skills-seguranca-contexto-linguagem": lambda e: e["tipo"] == "skill" and (
        e["classe"] in ("seguranca", "contexto-custo-memoria") or bool(set(e["linguagens"]) & {"rust", "python"})),
    "H3-agents-commands": lambda e: e["tipo"] in ("agent", "command") and e["classe"] != "dominio-nao-codigo",
}


def write_md(out: Path, res: dict) -> None:
    cat, adapters, other = res["catalogo"], res["adaptadores"], res["outros"]
    by = collections.Counter((e["tipo"], e["classe"]) for e in cat)
    md = ["# Inventário do ECC (rodada H, gerado por `scripts/inventario_ecc.py`)", "",
          f"{len(cat)} entradas de catálogo. Lotes de leitura: "
          + ", ".join(f"{k} ({len(v)})" for k, v in res["lotes"].items()) + ".", "",
          "| tipo | classe | n |", "|---|---|---|"] + [f"| {t} | {c} | {n} |" for (t, c), n in sorted(by.items())]
    md += ["", "## Adaptadores de harness: gerados da fonte ou escritos à mão", "",
           ("O par é por NOME (homônimo, não par verificado); «ambíguos» tem mais de um candidato. O número robusto é o "
           "último: corpo comparado com todos os candidatos."), "",
           "| diretório | arquivos | homônimo na fonte | ambíguos | iguais à fonte (≥ 0,9) |", "|---|---|---|---|---|"]
    md += [f"| `{a}` | {v['arquivos']} | {v['com_par_na_fonte']} | {v['homonimos_ambiguos']} | {v['iguais_a_fonte']} |"
           for a, v in adapters.items()]
    md += ["", "## Outros diretórios", "", "| diretório | arquivos | linhas | subdiretórios |", "|---|---|---|---|"]
    md += [f"| `{d}` | {v['arquivos']} | {v['linhas']} | {', '.join(v['subdirs'][:12])} |" for d, v in other.items()]
    md += ["", "## Catálogo completo", "", "| tipo | nome | classe | linguagens | linhas | caminho |", "|---|---|---|---|---|---|"]
    md += [f"| {e['tipo']} | {e['nome']} | {e['classe']} | {', '.join(e['linguagens'])} | {e['linhas']} | `{e['caminho']}` |" for e in cat]
    (out / "INVENTARIO.md").write_text("\n".join(md) + "\n", encoding="utf-8")
    for lote, caminhos in res["lotes"].items():
        rows = [e for e in cat if e["caminho"] in set(caminhos)]
        body = [f"# Lote {lote} ({len(rows)} entradas)", "", "| nome | classe | linhas | caminho | descrição | seções |", "|---|---|---|---|---|---|"]
        body += [f"| {e['nome']} | {e['classe']} | {e['linhas']} | `{e['caminho']}` | {e['descricao'][:160].replace('|', '/')} | "
                 f"{'; '.join(e['secoes'][:6]).replace('|', '/')[:200]} |" for e in rows]
        (out / f"lote-{lote}.md").write_text("\n".join(body) + "\n", encoding="utf-8")


def main() -> int:
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    cat = catalog(root)
    canon = {e["caminho"]: (root / e["caminho"]).read_text(encoding="utf-8", errors="replace") for e in cat}
    res = {"catalogo": cat,
           "adaptadores": {a: adapter_origin(root, a, canon) for a in ADAPTERS if (root / a).is_dir()},
           "outros": other_dirs(root),
           "lotes": {k: [e["caminho"] for e in cat if f(e)] for k, f in LOTES.items()}}
    (out / "inventario-ecc.json").write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    write_md(out, res)
    print(f"catálogo {len(cat)} · lotes {({k: len(v) for k, v in res['lotes'].items()})}")
    print("adaptadores (arquivos, homônimo, ambíguos, iguais):",
          {a: (v["arquivos"], v["com_par_na_fonte"], v["homonimos_ambiguos"], v["iguais_a_fonte"])
           for a, v in res["adaptadores"].items()})
    return 0


if __name__ == "__main__":
    sys.exit(main())
