from pathlib import Path

p = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/oraculo_instancia_git.py"
)
t = p.read_text()

velho = '''    if valor.startswith("http"):
        return "cdn_pin" if not VERSAO.match(valor.rsplit("@", 1)[-1]) else "cdn"
    return "recurso"'''
novo = '''    if valor.startswith("http") or valor.startswith("//"):
        # MEDIDO em 10/10: a versao antiga (`VERSAO.match(v.rsplit("@",1)[-1])`) classificava como
        # `cdn_pin` QUALQUER URL absoluta sem `@` — sem `@`, o rsplit devolve a URL INTEIRA, que
        # nunca casa com o padrao de versao. Os 329 "positivos" eram 128x istanbul.js.org, 68x
        # Wikipedia, 26x GitHub, NASA, DOI: nenhum CDN. O criterio agora e o HOST, em
        # `scripts/cdn_pin.py` (21 casos em `test_cdn_pin.py`), e este modulo e o montador do
        # corpus importam o MESMO — dois criterios para a mesma classe divergem, e a divergencia
        # aparece como contagem diferente sem ninguem saber qual esta certa.
        from cdn_pin import classe_de_cdn

        return classe_de_cdn(valor) or "recurso"
    return "recurso"'''
assert velho in t, "classe_de nao achado"
t = t.replace(velho, novo, 1)

# o import relativo ao diretorio do script, para o `from cdn_pin import` funcionar
if "sys.path.insert" not in t:
    t = t.replace(
        "import subprocess\n",
        "import subprocess\nimport sys\n\nsys.path.insert(0, str(__import__(\"pathlib\").Path(__file__).resolve().parent))\n",
        1,
    )
p.write_text(t, encoding="utf-8")
import ast

ast.parse(t)
print("classe_de do oraculo corrigido; parse OK")