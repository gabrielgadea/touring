"""`cdn_pin` pelo HOST do CDN, nao pela presenca de `@`.

MEDIDO EM 10/10, em duas etapas, e o numero caiu 329 -> 0 -> (esta):
  (1) o `class_de` do oraculo classificava QUALQUER URL absoluta sem `@` como `cdn_pin`, porque
      `VERSAO.match(v.rsplit("@",1)[-1])` recebe a URL INTEIRA quando nao ha `@`. Os 329 "positivos"
      eram 128x istanbul.js.org, 68x Wikipedia, 26x GitHub, NASA, DOI — nenhum CDN;
  (2) o meu conserto seguinte exigiu `@<versao>` na URL, e ficou ESTREITO DEMAIS: a spec do vtransv
      («range / `latest` / sem versao -> aviso») inclui URL de CDN SEM NENHUMA versao, como
      `https://unpkg.com/react` ou `https://cdn.jsdelivr.net/npm/gsap/dist/gsap.min.js`; e alguns CDNs
      põem a versao no CAMINHO, nao com `@` (cdnjs `/ajax/libs/<lib>/<v>/`,
      `code.jquery.com/jquery-<v>.min.js`).

O criterio CORRETO e o HOST. Um `cdn_pin` e uma URL de CDN cuja versao nao e exata `X.Y.Z` — o que
inclui nao ter versao nenhuma, `latest`, ou um `range`. DOI, Wikipedia, MDN e NASA nunca entram:
o host deles nao esta na lista.

A lista e a da spec (vtransv), nao uma que eu inventasse. Ela vive AQUI, num modulo so, e tanto o
montador quanto o oraculo importam daqui — dois criterios para a mesma classe divergem, e a
divergencia aparece como contagem diferente, sem ninguem saber qual esta certa.
"""
import re

# Hosts de CDN. `cdnjs` e `code.jquery.com` põem a versao no CAMINHO; `unpkg` e `jsdelivr` no `@`;
# `esm.sh`, `skypack`, `googleapis` e `tailwindcss` tem o seu proprio formato.
HOSTS_CDN = (
    "cdn.jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
    "cdnjs.com",
    "code.jquery.com",
    "ajax.googleapis.com",
    "esm.sh",
    "cdn.skypack.dev",
    "skypack.dev",
    "cdn.tailwindcss.com",
    "unpkg.zapcdn.com",
    "maxcdn.bootstrapcdn.com",
    "stackpath.bootstrapcdn.com",
    "cdn.statically.io",
    "fonts.googleapis.com",
    "fonts.gstatic.com",
)

VERSAO_EXATA = re.compile(r"^\d+\.\d+\.\d+$")
VERSAO_NO_SEGMENTO = re.compile(r"(?<![\d.])(\d+\.\d+(?:\.\d+)?)(?![\d.])")
# `code.jquery.com/jquery-3.6.0.min.js`: a versao esta NO NOME DO ARQUIVO, com sufixo depois. O
# padrao de segmento exigia fronteira depois do numero e nao casava. MEDIDO no exercicio de 17 casos
# (16 passam, este reprovava) — `3.6.0.min.js` tem `.` logo depois, e `(?!\d.)` deixa passar.
VERSAO_EM_QUALQUER = re.compile(r"(?<![\d.])(\d+\.\d+(?:\.\d+)?)")
HOSPEDEIRO = re.compile(r"^(?:https?:)?//([^/]+)")


def host_de(valor: str) -> str:
    """O host da URL, minusculo, sem `www.`. String vazia se nao for URL."""
    m = HOSPEDEIRO.match(valor.strip())
    if not m:
        return ""
    h = m.group(1).lower()
    return h[4:] if h.startswith("www.") else h


def e_cdn(valor: str) -> bool:
    """O host e de CDN? E o que separa `cdn_pin` de um link de referencia (DOI, Wikipedia, MDN)."""
    return host_de(valor) in HOSTS_CDN


def versao_de(valor: str) -> str | None:
    """A versao da URL de CDN: depois do `@` (jsdelivr/unpkg), no caminho (cdnjs/jquery), ou None.

    None significa «sem versao» — que, num CDN, e `cdn_pin` por construcao: a URL flutua.
    """
    tok = re.split(r"[?#]", valor, maxsplit=1)[0]
    if "@" in tok.rsplit("/", 1)[-1]:
        v = tok.rsplit("/", 1)[-1].rsplit("@", 1)[-1]
        if v:
            return v
    # versao no caminho OU no NOME do arquivo: `code.jquery.com/jquery-3.6.0.min.js` tem a versao no
# ultimo segmento, e `cdnjs/ajax/libs/jquery/3.6.0/jquery.min.js` tem no anteultimo. Os dois entram.
    for seg in reversed(tok.split("/")):
        achado = VERSAO_EM_QUALQUER.search(seg)
        if achado:
            return achado.group(1)
    return None


def e_cdn_pin(valor: str) -> bool:
    """CDN cuja versao NAO e exata X.Y.Z. `None` (sem versao) conta: a URL flutua."""
    if not e_cdn(valor):
        return False
    v = versao_de(valor)
    if v is None:
        return True
    return not VERSAO_EXATA.match(v)


def classe_de_cdn(valor: str) -> str:
    """`cdn_pin` | `cdn` | "" (nao e CDN). A classe que entra no corpus."""
    if not e_cdn(valor):
        return ""
    return "cdn" if e_cdn_pin(valor) is False else "cdn_pin"