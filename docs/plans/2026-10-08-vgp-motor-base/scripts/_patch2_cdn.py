from pathlib import Path

p = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/monta_holdout_html_git.py"
)
linhas = p.read_text().splitlines()
i = next(j for j, l in enumerate(linhas) if l.startswith("def cdn_pins_no_texto"))
fim = i + 1
while fim < len(linhas) and not linhas[fim].startswith(("def ", "if __name__")):
    fim += 1
print("=== substituo ===")
for k in range(i, fim):
    print(f"{k+1:4d}| {linhas[k][:110]}")

NOVO = '''def cdn_pins_no_texto(texto: str) -> list:
    """As URLs de CDN cuja versao nao e exata — o que a classe `cdn_pin` significa.

    O criterio e o do modulo `cdn_pin.py`: pelo HOST do CDN, nao pela presenca de `@`. Tres defeitos
    medidos nesta onda, e o numero caiu 329 -> 0 -> (agora pelo criterio certo):
      1. o `classe_de` do oraculo classificava QUALQUER URL absoluta sem `@` como `cdn_pin` (o
         `rsplit` devolve a URL inteira, que nunca casa com o padrao de versao): os 329 "positivos"
         eram 128x istanbul.js.org, 68x Wikipedia, 26x GitHub, NASA, DOI — nenhum CDN;
      2. o meu conserto seguinte exigiu `@<versao>` e ficou ESTREITO DEMAIS — a spec do vtransv
         inclui CDN SEM NENHUMA versao (`https://unpkg.com/react`), e alguns CDNs poem a versao no
         CAMINHO ou no NOME do arquivo (cdnjs, code.jquery.com);
      3. o contrato vive em `scripts/test_cdn_pin.py`, 21 casos, exit 0.

    DOI, Wikipedia, MDN, istanbul e NASA nunca entram: o host deles nao esta na lista de CDN.
    """
    out = []
    for _tag, _atr, val in ORA.refs(texto):
        if classe_de_cdn(val) == "cdn_pin":
            out.append(val)
    return sorted(set(out))
'''
novas = linhas[:i] + NOVO.splitlines() + linhas[fim:]
p.write_text("\n".join(novas) + "\n", encoding="utf-8")
import ast

ast.parse(p.read_text())
print("\nsubstituido; parse OK")