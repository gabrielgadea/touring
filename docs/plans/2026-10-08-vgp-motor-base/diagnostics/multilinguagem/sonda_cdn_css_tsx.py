"""Sonda para a ampliação web do VGP: CDN (versão existe? arquivo existe? latência), classes CSS e volume de TSX.

(1) CDN: HEAD/GET em URLs reais e com versão inventada nos hosts da regra de artifacts — o 404 da inventada é o
controle positivo de que a conferência distingue. (2) Varre os .html em disco (fora de node_modules/.venv/target) de
analise e touring: URLs de CDN por host, versão exata × range/latest, e classes de `class="…"` sem definição em <style>
do próprio arquivo. (3) Conta .tsx por projeto. Uso: python3 sonda_cdn_css_tsx.py
"""

from __future__ import annotations

import collections
import re
import time
import urllib.error
import urllib.request
from pathlib import Path

URLS = {
    "jsdelivr real": "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.production.min.js",
    "jsdelivr versão inventada": "https://cdn.jsdelivr.net/npm/react@18.3.99/umd/react.production.min.js",
    "jsdelivr arquivo inventado": "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.inventado.js",
    "unpkg real": "https://unpkg.com/react@18.3.1/umd/react.production.min.js",
    "unpkg versão inventada": "https://unpkg.com/react@18.3.99/umd/react.production.min.js",
    "cdnjs real": "https://cdnjs.cloudflare.com/ajax/libs/d3/7.9.0/d3.min.js",
    "cdnjs versão inventada": "https://cdnjs.cloudflare.com/ajax/libs/d3/7.9.99/d3.min.js",
    "jsdelivr data API (lista versões)": "https://data.jsdelivr.com/v1/packages/npm/react",
    "cdnjs API (lista versões)": "https://api.cdnjs.com/libraries/d3?fields=versions",
}
CDN = re.compile(
    r"""(?:src|href)\s*=\s*["'](https?://(cdnjs\.cloudflare\.com|cdn\.jsdelivr\.net|unpkg\.com|cdn\.tailwindcss\.com|"""
    r"""code\.jquery\.com|esm\.sh|cdn\.skypack\.dev|fonts\.googleapis\.com)/[^"']+)["']"""
)
VERSAO_EXATA = re.compile(r"@\d+\.\d+\.\d+|/\d+\.\d+\.\d+/|-\d+\.\d+\.\d+(\.min)?\.js")
CLASSE = re.compile(r"""\bclass\s*=\s*["']([^"'{}$<>]+)["']""")
STYLE = re.compile(r"<style[^>]*>(.*?)</style>", re.S | re.I)
SELETOR_CLASSE = re.compile(r"\.(-?[_a-zA-Z][\w-]*)")
PULAR = ("node_modules", ".venv", "/target/", "site-packages", "/.git/")


def sondar_url(nome: str, url: str) -> None:
    req = urllib.request.Request(url, method="GET" if "API" in nome else "HEAD")
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            st = r.status
    except urllib.error.HTTPError as e:
        st = e.code
    except (urllib.error.URLError, TimeoutError) as e:
        st = f"erro {type(e).__name__}"
    print(f"  {nome:38} {st!s:>6}  {1000 * (time.perf_counter() - t0):6.0f} ms")


def htmls(raiz: Path) -> list[Path]:
    return [p for p in raiz.rglob("*.htm*") if p.is_file() and not any(x in str(p) for x in PULAR)]


def varrer_html(raiz: Path) -> None:
    arqs = htmls(raiz)
    hosts: collections.Counter = collections.Counter()
    exata = nao_exata = 0
    exemplos_nao_exata: list[str] = []
    com_class = classes_total = sem_def_local = 0
    for p in arqs:
        try:
            t = p.read_text(errors="replace")
        except OSError:
            continue
        for m in CDN.finditer(t):
            hosts[m.group(2)] += 1
            if m.group(2) == "fonts.googleapis.com":
                continue
            if VERSAO_EXATA.search(m.group(1)):
                exata += 1
            else:
                nao_exata += 1
                exemplos_nao_exata.append(m.group(1)[:90])
        usadas = {c for m in CLASSE.finditer(t) for c in m.group(1).split()}
        if usadas:
            com_class += 1
            definidas = {c for s in STYLE.findall(t) for c in SELETOR_CLASSE.findall(s)}
            classes_total += len(usadas)
            sem_def_local += len(usadas - definidas)
    print(f"\n## {raiz}: {len(arqs)} .html")
    print(f"  URLs de CDN por host: {dict(hosts.most_common())}")
    print(f"  versão exata {exata} · sem versão exata (range/latest/ausente) {nao_exata}")
    for e in exemplos_nao_exata[:4]:
        print(f"    ex.: {e}")
    print(f"  arquivos com class= {com_class} · classes distintas usadas (soma por arquivo) {classes_total}"
          f" · sem definição no <style> do próprio arquivo {sem_def_local}")


def main() -> int:
    print("## CDN — status e latência (HEAD; API por GET)")
    for nome, url in URLS.items():
        sondar_url(nome, url)
    for raiz in (Path.home() / "projects/analise", Path.home() / "projects/touring"):
        varrer_html(raiz)
    print("\n## .tsx por projeto")
    for proj in ("analise", "touring", "konverter"):
        r = Path.home() / "projects" / proj
        n = sum(1 for p in r.rglob("*.tsx") if not any(x in str(p) for x in PULAR))
        print(f"  {proj:10} {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
