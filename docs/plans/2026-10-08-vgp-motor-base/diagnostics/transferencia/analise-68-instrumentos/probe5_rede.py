"""Sonda 5 — o oráculo de CDN fora do sandbox.

Timeout NAO e 404: o jsdelivr pendurou 15 s num arquivo inventado. Este teste mede o que a
rede devolve para (a) o que existe, (b) arquivo inexistente NA VERSAO CERTA, (c) versao
inexistente, e (d) um range em vez de versao exata (o caso do `cdn_pin`).
"""

import json
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

CASOS = {
    "existente": "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.production.min.js",
    "arquivo_inexistente_na_versao_certa":
        "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/__vgp_inexistente_9f2a.min.js",
    "versao_inexistente": "https://cdn.jsdelivr.net/npm/react@99.99.99/umd/react.production.min.js",
    "patch_inexistente": "https://cdn.jsdelivr.net/npm/react@18.3.199/umd/react.production.min.js",
    "unpkg_existente": "https://unpkg.com/react@18.3.1/umd/react.production.min.js",
    "cdnjs_existente": "https://cdnjs.cloudflare.com/ajax/libs/jquery/3.7.1/jquery.min.js",
    "range_em_vez_de_versao": "https://cdn.jsdelivr.net/npm/react@18/umd/react.production.min.js",
}

saida = {}
for nome, url in CASOS.items():
    ini = time.time()
    registro = {"url": url}
    try:
        r = urlopen(Request(url, method="HEAD"), timeout=10)
        registro.update(status=r.status, verdict="existe")
    except HTTPError as e:
        registro.update(status=e.code, verdict="404" if e.code == 404 else "http_erro")
    except (URLError, TimeoutError, OSError) as e:
        registro.update(status=None, verdict="indeterminado", erro=f"{type(e).__name__}: {e}")
    registro["ms"] = round((time.time() - ini) * 1000)
    saida[nome] = registro

Path("/home/gabrielgadea/projects/analise/.vgp-semeado/rede.json").write_text(
    json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8"
)
for nome, r in saida.items():
    print(f"{nome:<38} status={r['status']} verdict={r['verdict']} {r['ms']}ms")