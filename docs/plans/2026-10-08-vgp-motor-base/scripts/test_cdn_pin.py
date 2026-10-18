#!/usr/bin/env python3
"""Os 17 casos que definem `cdn_pin`. Roda sem corpus, sem motor, sem rede.

Nao e teste de cobertura: e o CONTRATO da classe, escrito como casos. Cada linha e um caso que
alguem citados ou que ja quebrou o numero:

  - `unpkg.com/react` e `cdn.jsdelivr.net/npm/gsap/dist/gsap.min.js` — a spec do vtransf inclui CDN
    SEM NENHUMA versao ("sem versao -> aviso"). Foi a correcao da coordenadora: meu criterio
    anterior exigia `@<versao>` e ficava estreito demais, com zero;
  - `code.jquery.com/jquery-3.6.0.min.js` — versao no NOME do arquivo, com sufixo depois. Este caso
    REPROVAVA com o padrao de fronteira e e o que introduiu `VERSAO_EM_QUALQUER`;
  - DOI, Wikipedia, MDN, istanbul.js.org, NASA, GitHub — NUNCA entram. Foi a contaminacao que
    transformou 329 URLs de referencia em "positivos de cdn_pin".

    python3 scripts/test_cdn_pin.py    # exit 0 quando os 17 passam
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cdn_pin import classe_de_cdn  # noqa: E402

CASOS = [
    # (url, classe esperada, por que)
    ("https://unpkg.com/react", "cdn_pin", "CDN sem versao nenhuma"),
    ("https://cdn.jsdelivr.net/npm/gsap/dist/gsap.min.js", "cdn_pin", "CDN sem versao"),
    ("https://cdn.jsdelivr.net/npm/gsap@3.13/dist/gsap.min.js", "cdn_pin", "versao nao exata"),
    ("https://cdn.jsdelivr.net/npm/gsap@latest/dist/gsap.min.js", "cdn_pin", "latest flutua"),
    ("https://cdn.jsdelivr.net/npm/gsap@3.13.0/dist/gsap.min.js", "cdn", "versao exata"),
    ("https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/css/bootstrap.min.css", "cdn", "versao exata"),
    ("https://cdnjs.cloudflare.com/ajax/libs/jquery/3.6.0/jquery.min.js", "cdn", "versao no caminho"),
    ("https://cdnjs.cloudflare.com/ajax/libs/jquery/3.6/jquery.min.js", "cdn_pin", "versao parcial"),
    ("https://code.jquery.com/jquery-3.6.0.min.js", "cdn", "versao no NOME do arquivo"),
    ("https://code.jquery.com/jquery-3.5.min.js", "cdn_pin", "versao parcial no nome"),
    ("https://esm.sh/react@18.2.0", "cdn", "versao exata"),
    ("https://esm.sh/react", "cdn_pin", "sem versao"),
    ("https://cdn.skypack.dev/react", "cdn_pin", "skypack sem versao"),
    ("https://fonts.googleapis.com/css2?family=Roboto", "cdn_pin", "fonts sem versao"),
    ("https://dx.doi.org/10.1098/rspa.2016.0404", "", "DOI nao e CDN"),
    ("https://en.wikipedia.org/wiki/Main_Page", "", "Wikipedia nao e CDN"),
    ("https://developer.mozilla.org/en-US/docs/Web/HTML", "", "MDN nao e CDN"),
    ("https://istanbul.js.org/instrument", "", "istanbul nao e CDN"),
    ("https://eclipse.gsfc.nasa.gov/SEhelp/uncertainty2004.html", "", "NASA nao e CDN"),
    ("https://github.com/foo/bar", "", "GitHub nao e CDN"),
    ("https://cldr.unicode.org", "", "CLDR nao e CDN"),
]


def main() -> int:
    falhas = 0
    for url, esperado, por_que in CASOS:
        obtido = classe_de_cdn(url)
        ok = obtido == esperado
        falhas += not ok
        print(
            f"  {'ok  ' if ok else 'FALHA'} {obtido or '(nao-cdn)':9s} "
            f"esperado {esperado or '(nao-cdn)':9s} | {url[:58]}"
        )
        if not ok:
            print(f"         por que: {por_que}")
    total = len(CASOS)
    print()
    print(f"{total - falhas}/{total} casos do contrato de `cdn_pin`")
    return 0 if falhas == 0 else 1


def test_contrato_cdn_pin():
    """O contrato, como pytest o ve.

    MEDIDO em 10/10 (achado da coordenadora): sem esta funcao, `pytest scripts/test_cdn_pin.py`
    responde «no tests ran» com **exit 5** — porque o arquivo so tinha `main()`, que o pytest nao
    coleta. E exit 5 e o codigo que muitos CI tratam como "nada a fazer": o contrato passaria SEM RODAR,
    que e pior do que um CI vermelho, porque parece verde.

    A funcao chama `main()` de verdade (que imprime os 21 casos e devolve != 0 se algum falhar), e
    o assert transforma o resultado em veredito do pytest. `main()` fica sem efeito quando importado.
    """
    assert main() == 0, f"o contrato de cdn_pin reprovou: rode `python3 scripts/test_cdn_pin.py`"


if __name__ == "__main__":
    sys.exit(main())