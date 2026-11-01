from pathlib import Path

p = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/monta_holdout_html_git.py"
)
linhas = p.read_text().splitlines()
i = next(j for j, l in enumerate(linhas) if l.startswith("def cdn_pins_no_texto"))
# a funcao vai ate a proxima linha em branco seguida de def/if
fim = i + 1
while fim < len(linhas) and not (
    linhas[fim].startswith(("def ", "if __name__", "class ")) or linhas[fim] == ""
    and any(x.startswith(("def ", "if __name__")) for x in linhas[fim + 1 : fim + 3])
):
    fim += 1
print("=== substituo ===")
for k in range(i, fim):
    print(f"{k+1:4d}| {linhas[k]}")

NOVO = '''def cdn_pins_no_texto(texto: str) -> list:
    """As URLs de CDN sem versao EXATA que o texto usa.

    A DETECCAO E DO ORACULO, nao minha: `refs()` + `classe_de()` + o `VERSAO` dele. A primeira versao
    deste script exigia `/npm/` no caminho e achou 1 positivo onde o oraculo acha 329 — dois criterios
    para a mesma classe, e o corpus saiu errado. O oraculo e a autoridade: `cdn_pin` e quando o valor
    COMECA COM `http` e a versao depois do `@` nao e exata, sem exigir `/npm/`.

    `//cdn.jsdelivr.net/...` (protocolo-relativo) nao entra, porque `refs()` do oraculo so recolhe
    valores que nao comecam com `//`. Mantenho o criterio dele; e por isso que este numero pode
    divergir do 329, e a divergencia e um dado, nao um defeito a esconder.
    """
    out = []
    for tag, atr, val in ORA.refs(texto):
        if val.startswith("http") and ORA.classe_de(val, tag, atr) == "cdn_pin":
            out.append(val)
    return sorted(set(out))
'''

novas = linhas[:i] + NOVO.splitlines() + linhas[fim:]
p.write_text("\n".join(novas) + "\n", encoding="utf-8")
import ast

ast.parse(p.read_text())
print("\nsubstituido; parse OK")