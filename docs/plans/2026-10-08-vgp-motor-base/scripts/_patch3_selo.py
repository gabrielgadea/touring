from pathlib import Path

p = Path(
    "/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base"
    "/scripts/monta_holdout_html_git.py"
)
t = p.read_text()

# 1) importa sha_tree do conferidor, em vez de reescrever
velho_import = """from cdn_pin import classe_de_cdn  # noqa: E402"""
novo_import = """from cdn_pin import classe_de_cdn  # noqa: E402
from confere_selo import sha_tree as selo_sha_tree  # noqa: E402  (a MESMA funcao do conferidor)"""
assert velho_import in t, "import nao achado"
t = t.replace(velho_import, novo_import, 1)

# 2) o selo sai do corpus, para o diagnostics, com os campos obrigatorios
i = t.index('    sha = hashlib.sha256((SAIDA / "entradas.jsonl").read_bytes()).hexdigest()')
j = t.index('    (SAIDA / "selo.json").write_text(')
novo_sel = '''    arquivos = sorted(q for q in SAIDA.rglob("*") if q.is_file())
    selo = {
        "estrato": "holdout_html_git",
        "corte": CORTE,
        "regra_c": "classe so e medida com n>=20 E denominador independente do gerador",
        "n_entradas": len(entradas),
        "por_rotulo": {k: v for k, v in counts.items() if k in ("positivo", "negativo")},
        "por_classe": {k: v for k, v in counts.items() if k not in ("positivo", "negativo")},
        "repos": sorted(q.name for q in OSS.iterdir() if q.is_dir() and (q / ".git").exists()),
        "entradas_jsonl_sha256": hashlib.sha256(
            (SAIDA / "entradas.jsonl").read_bytes()
        ).hexdigest(),
        "orcamento": "oraculo por instancia no git: INSTANTE=C, BASE=C^, `git ls-tree -r C`",
        "cdn_pin": (
            "classe de FORMA: URL de CDN (host na lista) com versao ausente, `latest` ou nao "
            "exata X.Y.Z. O criterio e scripts/cdn_pin.py, com 21 casos em test_cdn_pin.py."
        ),
        "disjuncao": (
            "medido: 0 paths do corpus_semeado/html dentro do oss/, 0 colisao de sha256 contra os "
            "blobs do HEAD; exclusao de 0 entradas"
        ),
        # os tres campos que o conferidor exige. `sha_tree` pela MESMA funcao do `confere_selo.py`
        # (importada, nao reescrita): dois metodos de arvore divergem, e a divergencia apareceria
        # como selo «INTACTO» sobre uma arvore diferente da conferida.
        "sha_tree": selo_sha_tree(SAIDA),
        "arquivos": len(arquivos),
        "bytes": sum(q.stat().st_size for q in arquivos),
    }
    SELO_PATH.parent.mkdir(parents=True, exist_ok=True)
'''
fim_escrita = t.index("\n", t.index('    (SAIDA / "selo.json").write_text('))
# remove a linha do write_text antigo e seu argumento (uma linha so)
t = (
    t[:i]
    + novo_sel
    + '    SELO_PATH.write_text(json.dumps(selo, indent=1, ensure_ascii=False), encoding="utf-8")\n'
    + t[fim_escrita + 1 :]
)
p.write_text(t, encoding="utf-8")
import ast

ast.parse(t)
print("selo movido para fora; parse OK")
print("--- o que ficou ---")
for ln in t.splitlines():
    if "SELO_PATH" in ln or "selo_sha_tree" in ln or '"bytes"' in ln or '"arquivos"' in ln:
        print("   ", ln[:110])