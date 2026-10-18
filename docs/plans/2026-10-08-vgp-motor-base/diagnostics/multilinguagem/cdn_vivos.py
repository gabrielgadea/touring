"""Quantas referências de CDN com versão EXATA nos .html do analise estão quebradas hoje (V0b da web).

Universo: URLs distintas de cdnjs/jsdelivr/unpkg/jquery com versão exata nos .html em disco (fora de node_modules etc.).
HEAD com timeout de 10 s, 8 em paralelo. Baldes por predicado positivo: 2xx/3xx · 404 · outro status · timeout/rede —
timeout NUNCA é "quebrado" (o jsdelivr pendurou 15 s num arquivo inventado). Uso: python3 cdn_vivos.py
"""

from __future__ import annotations

import collections
import concurrent.futures as cf
import re
import urllib.error
import urllib.request
from pathlib import Path

from sonda_cdn_css_tsx import CDN, VERSAO_EXATA, htmls


def status(url: str) -> str:
    try:
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=10) as r:
            return "ok" if r.status < 400 else str(r.status)
    except urllib.error.HTTPError as e:
        return "404" if e.code == 404 else str(e.code)
    except (urllib.error.URLError, TimeoutError, OSError):
        return "timeout/rede"


def main() -> int:
    onde: dict[str, list[str]] = collections.defaultdict(list)
    for p in htmls(Path.home() / "projects/analise"):
        t = p.read_text(errors="replace")
        for m in CDN.finditer(t):
            url = m.group(1)
            if m.group(2) != "fonts.googleapis.com" and VERSAO_EXATA.search(url):
                onde[url].append(str(p))
    if not onde:
        raise SystemExit("0 URLs: universo degenerado")
    with cf.ThreadPoolExecutor(8) as ex:
        res = dict(zip(onde, ex.map(status, onde), strict=True))
    baldes = collections.Counter(res.values())
    print(f"URLs distintas com versão exata: {len(onde)} · referências: {sum(map(len, onde.values()))} · {dict(baldes)}")
    for url, st in sorted(res.items()):
        if st != "ok":
            arq = onde[url]
            print(f"  {st:12} {url[:110]}\n               em {len(arq)} arquivo(s), ex.: {re.sub(r'^.*/projects/', '', arq[0])[:110]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
