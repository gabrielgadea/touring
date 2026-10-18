"""Sonda 4 — o terreno HTML: onde ha folha de estilo de verdade, o que e CDN, e se a rede responde.

Perguntas que decidem operadores: quantos documentos definem classe numa folha RESOLUVEL
(style inline ou link local), quantos tem ancora para mutar, e qual a forma das URLs de CDN.
"""

import json
import re
from collections import Counter
from html.parser import HTMLParser
from pathlib import Path
from urllib.request import Request, urlopen

ANALISE = Path.home() / "projects/analise"
IGNORAR = {"node_modules", ".git", "dist", ".next", "build", "__pycache__", ".venv", "coverage",
           ".audit-cache", ".hypothesis", ".pytest_cache", ".cache", ".serena"}

STYLE_INLINE = re.compile(r"<style\b", re.I)
LINK_CSS_HREF = re.compile(r"""<link\b[^>]*href\s*=\s*["']([^"']+)["']""", re.I)
CDN_URL = re.compile(
    r"""https?://(?:cdn\.jsdelivr\.net|cdnjs\.cloudflare\.com|unpkg\.com)/[^"'\s>]+"""
)
CLASSE_DEF = re.compile(r"^\s*\.[A-Za-z_][\w-]*", re.M)
REMOTO = ("http://", "https://", "//")


class Coletor(HTMLParser):
    """ids, ancoras, links de folha e classes usadas — o que o oráculo de HTML precisa ver."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.anchors = []
        self.links_css = []
        self.classes = set()
        self.tem_style = False

    def handle_starttag(self, tag, attrs):
        d = dict(attrs)
        if d.get("id"):
            self.ids.add(d["id"])
        if tag == "a" and d.get("href", "").startswith("#"):
            self.anchors.append(d["href"][1:])
        if tag == "label" and d.get("for"):
            self.anchors.append(d["for"])
        if tag == "link":
            self.links_css.append(d)
        if tag == "style":
            self.tem_style = True
        if d.get("class"):
            self.classes.update(d["class"].split())


def camada_css(hrefs):
    local = [h for h in hrefs if not h.startswith(REMOTO)]
    remoto = [h for h in hrefs if h.startswith(REMOTO)]
    return local, remoto


def inspeciona(p):
    """(chave do estrato, contadores) de UM documento — o agregador só soma."""
    t = p.read_text(encoding="utf-8", errors="replace")
    c = Coletor()
    try:
        c.feed(t)
    except Exception:
        return {"chave": "parse_falhou"}
    hrefs = [d.get("href", "") for d in c.links_css if d.get("href")]
    local, remoto = camada_css(hrefs)
    chave = (
        ("inline" if c.tem_style else "")
        + ("|css_local" if local else "")
        + ("|css_remoto" if remoto else "")
        or "sem_css"
    )
    defs = CLASSE_DEF.findall(t) if c.tem_style else []
    return {
        "chave": chave,
        "ids": len(c.ids),
        "ancoras": len(c.anchors),
        "usadas": len(c.classes),
        "defs": len(defs),
        "tamanho": p.stat().st_size,
    }


def varre():
    contagem = Counter()
    urls_cdn = Counter()
    amostra = []
    totais = Counter()
    for p in sorted(ANALISE.rglob("*.html")):
        if any(x in IGNORAR for x in p.parts) or not p.is_file():
            continue
        if p.stat().st_size > 400_000:
            contagem["descartado_grande"] += 1
            continue
        try:
            info = inspeciona(p)
        except OSError:
            contagem["ilegivel"] += 1
            continue
        contagem[info["chave"]] += 1
        totais["docs"] += 1
        if info.get("defs"):
            totais["com_classe_definida"] += 1
            if len(amostra) < 10:
                amostra.append(dict(info, path=str(p)))
        if info.get("ancoras"):
            totais["com_ancora"] += 1
        if "com_classe_definida" or info.get("usadas"):
            totais["com_classe_usada"] += 1
        texto = p.read_text(encoding="utf-8", errors="replace")
        for u in CDN_URL.findall(texto):
            forma = "npm@versao" if re.search(r"/npm/[^/]+@\d", u) else (
                "gh@tag" if "/gh/" in u else "outra")
            urls_cdn[u.split("/")[2] + ":" + forma] += 1
    return {
        "estratos": dict(contagem.most_common(20)),
        "totais": dict(totais),
        "cdn_formas": dict(urls_cdn.most_common(12)),
        "amostra": amostra,
    }


def rede():
    """A rede responde? Timeout NAO e 404 (o jsdelivr pendurou 15 s num arquivo inventado)."""
    testes = {
        "existente_200": "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.production.min.js",
        "arquivo_inexistente_404":
            "https://cdn.jsdelivr.net/npm/react@18.3.1/umd/__vgp_inexistente_9f2a.min.js",
        "versao_inexistente": "https://cdn.jsdelivr.net/npm/react@99.99.99/umd/react.production.min.js",
    }
    out = {}
    for nome, url in testes.items():
        try:
            r = urlopen(Request(url, method="HEAD"), timeout=10)
            out[nome] = {"status": r.status}
        except Exception as e:
            out[nome] = {"status": getattr(e, "code", None), "erro": f"{type(e).__name__}: {e}"}
    return out


resultado = {"html": varre(), "rede": rede()}
Path("/home/gabrielgadea/projects/analise/.vgp-semeado/html_terreno.json").write_text(
    json.dumps(resultado, ensure_ascii=False, indent=1), encoding="utf-8"
)
# Só o resumo vai para a tela: a amostra fica no arquivo (ler o arquivo sob demanda).
h = resultado["html"]
print("docs:", h["totais"])
print("estratos:", h["estratos"])
print("cdn:", h["cdn_formas"])
print("rede:", {k: v.get("status", v.get("erro")) for k, v in resultado["rede"].items()})