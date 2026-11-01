"""MEDICAO: quanto HTML versionado existe no acervo `oss/`, e quanto dele serve de universo.

Contexto medido: o estrato dos transcripts NAO alcanca o piso de n>=20 (recurso 3, ancora 1,
cdn 0, cdn_pin 4, classe_css 3 caminhos com base de instancia). Falta UNIVERSO, nao codigo.

Este script mede o candidato a universo: os 7 repos com `.git` real em `~/.claude/touring/vgp/oss/`.
Para cada um, quantos commits tem, quantos tocam `.html`, e — o que decide — se ha PARES
(pai quebrado -> filho corrigido) de referencia local. Sem contar os pares, "commits tocando .html"
e um numero que impressiona e nao prova nada; foi o que enganou a proposta do git na secao (ii) da
PROPOSTA-positivos-html-heldout.md.

Nao constroi nada, nao sela, nao escreve em corpus. So mede.
"""
import subprocess
from collections import Counter
from pathlib import Path

BASE = Path.home() / ".claude/touring/vgp/oss"
LIMITE = 400  # commits .html por repo: o sinal nao e tao raro que 400 nao bastem


def git(d: Path, *args: str) -> str:
    r = subprocess.run(
        ["git", "-C", str(d), *args], capture_output=True, text=True, timeout=300
    )
    return r.stdout or ""


def main() -> int:
    repos = sorted(p for p in BASE.iterdir() if p.is_dir() and (p / ".git").exists())
    print(f"=== {len(repos)} repos com .git em {BASE} ===")
    total_html = 0
    total_commits = 0
    por_repo = {}
    for d in repos:
        todos = [c for c in git(d, "log", "--format=%H").split() if c]
        html = [c for c in git(d, "log", "--format=%H", "--", "*.html").split() if c]
        total_commits += len(todos)
        total_html += len(html)
        por_repo[d.name] = {"commits": len(todos), "commits_html": len(html)}
        print(f"  {d.name:20s} {len(todos):>6d} commits   {len(html):>6d} tocando .html")
    print()
    print(f"  TOTAL: {total_commits} commits, {total_html} tocando .html")
    print()
    print("=== QUE HTML SERVE (tem que ser HTML de PAGINA, com referencia local) ===")
    print("   Um `README.md` ou um fragmento de demo nao tem `src=` local para quebrar.")
    for d in repos:
        # arquivos .html no HEAD, com tamanho e se tem tag que carrega recurso
        arquivos = git(d, "ls-tree", "-r", "--name-only", "HEAD").split("\n")
        htmls = [a for a in arquivos if a.endswith(".html")]
        com_ref = 0
        for a in htmls[:LIMITE]:
            txt = git(d, "show", f"HEAD:{a}")
            if any(
                f"<{t}" in txt and ("src=" in txt or "href=" in txt)
                for t in ("script", "img", "link", "iframe", "source", "video", "audio")
            ):
                com_ref += 1
        por_repo[d.name]["html_no_head"] = len(htmls)
        por_repo[d.name]["html_com_ref"] = com_ref
        print(f"  {d.name:20s} {len(htmls):>5d} .html no HEAD   {com_ref:>5d} com referencia local")
    print()
    print("O proximo degrau NAO e este numero: e contar os PARES (pai quebrado -> filho corrigido)")
    print("com o mesmo oraculo por instancia que vale para o estrato natural. Medir 'commits tocando")
    print(".html' sem contar pares foi o que produziu 10 falsos positivos antes.")
    return 0


main()