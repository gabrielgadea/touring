"""MONTA o estrato held-out de HTML vindo do git: `corpus_holdout_html_git_20261010`.

PEDIDO (coordenadora, 10/10): os 166 negativos confirmados mais os 329 positivos de `cdn_pin`, no
esquema que o juiz le (`entradas.jsonl` + `snap/`), com o `nome_esperado` de `cdn_pin` na convencao
que o vtransv emite, deterministico, ids pelo sha da proveniencia, selo conferivel por
`confere_selo.py`.

DISJUNCAO (medida em `scratch/mede_disjuncao_semeado.py`, antes de montar):
  os 296 paths do `corpus_semeado/html` estao TODOS dentro de `~/projects/analise`; 0 dentro do
  `oss/`; 0 colisao de sha256 de conteudo contra os 5.032 blobs do HEAD dos 7 repos. A unica
  colisao de NOME e `index.html`, que nao identifica documento. Entao o held-out tirado do git nao
  pode ter vazado para o semeado, e a exclusao exigida e de **0 entradas** — o numero e reportado,
  nao presumido.

ORACULO (o mesmo do `scripts/oraculo_instancia_git.py`, que e o que produziu 166 e 329):
  INSTANTE = C; BASE = C^; arvore do instante = `git ls-tree -r C`; conteudo = `git show C:<rel>`.
  `cdn_pin` e classe de FORMA: a URL foi introduzida sem versao exata. Nao exige D.
  NEGATIVO = commit que toca `.html` e em que a arvore de C confirma 0 introducoes.

NENHUM MOTOR RODA AQUI. O script nao chama `touring`, nao chama o juiz, e nao toca nenhum corpus
vivo. O destino e um diretorio NOVO.
"""
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path

# O ORACULO E IMPORTADO, NAO REESCRITO: `filhos_de`, `arvore` e `git` vem do
# `scripts/oraculo_instancia_git.py`, que e o que produziu os 166 e os 329. Duas copias do oraculo
# divergem — e a divergencia apareceria como contagem diferente, sem ninguem saber qual esta certa.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import oraculo_instancia_git as ORA  # noqa: E402
from cdn_pin import classe_de_cdn  # noqa: E402
from confere_selo import sha_tree as selo_sha_tree  # noqa: E402  (a MESMA funcao do conferidor)

# O selo mora FORA do corpus: dentro, o `sha_tree` incluiria o proprio selo (MEDIDO pela
# convalida da coordenadora, e o `confere_selo.py` agora REPROVA selo dentro do corpus).
SELO_DIR = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
SELO_PATH = SELO_DIR / "diagnostics/holdout-html-git-20261010/selo.json"

git = ORA.git
filhos_de = ORA.filhos_de
arvore = ORA.arvore

BASE = Path.home() / ".claude/touring/vgp"
OSS = BASE / "oss"
SAIDA = BASE / "corpus_holdout_html_git_20261010"
LIMITE_COMMIT = ORA.LIMITE_COMMIT
CORTE = "2026-10-10T00:00:00Z"

ATRIB = re.compile(r"""(\w[\w:-]*)\s*=\s*(["'])([^"']*)\2""")
TAG = re.compile(r"<([A-Za-z][\w:-]*)([^>]*)>")
TAGS_RECURSO = {"script", "img", "link", "iframe", "source", "video", "audio"}
VERSAO_EXATA = re.compile(r"^\d+\.\d+\.\d+")
VERSAO_NO_CDN = re.compile(r"/npm/([^/@]+)@([^/\"']+)")


def ts_commit(d: Path, rev: str) -> str:
    return git(d, "show", "-s", "--format=%cI", rev).strip()


def ids_do(repo: str, rev: str, caminho: str, classe: str, nome: str) -> str:
    """Id = sha da PROVENIENCIA (repo|rev|caminho|classe|nome). Sem timestamp, sem ordem de execucao:
    o mesmo item recebe o mesmo id em qualquer corrida, em qualquer maquina."""
    return hashlib.sha256(f"{repo}|{rev}|{caminho}|{classe}|{nome}".encode()).hexdigest()[:12]


def cdn_pins_no_texto(texto: str) -> list:
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
def main() -> int:
    if SAIDA.exists():
        print(f"ABORTO: {SAIDA} ja existe — este script so cria diretorio NOVO")
        return 2
    (SAIDA / "snap").mkdir(parents=True)
    entradas = []
    counts = Counter()
    exclusoes = Counter()

    for d in sorted(p for p in OSS.iterdir() if p.is_dir() and (p / ".git").exists()):
        repo = d.name
        commits = [c for c in git(d, "log", "--format=%H", "--", "*.html").split() if c][:LIMITE_COMMIT]
        for c in commits:
            if ts_commit(d, c) > CORTE:
                continue
            arquivos = [
                a
                for a in git(d, "show", "--name-only", "--format=", c).split("\n")
                if a.strip().endswith(".html")
            ]
            if not arquivos:
                continue
            texto_pai = {a: git(d, "show", f"{c}~1:{a}") for a in arquivos}
            pins_introd = Counter()
            for arq in arquivos:
                txt = git(d, "show", f"{c}:{arq}")
                if not txt:
                    continue
                antes = set(cdn_pins_no_texto(texto_pai.get(arq, "")))
                for url in cdn_pins_no_texto(txt):
                    if url in antes:
                        continue  # nao introduzida por este commit
                    pins_introd[url] += 1
            if not pins_introd:
                # NEGATIVO: commit que toca .html e nao introduz nenhuma URL sem versao exata
                counts["negativo"] += 1
                ident = ids_do(repo, c, "|".join(sorted(arquivos)), "negativo", "")
                snap = SAIDA / "snap" / f"{ident}.html"
                snap.write_text(git(d, "show", f"{c}:{sorted(arquivos)[0]}"), encoding="utf-8")
                entradas.append(
                    {
                        "id": ident,
                        "lang": "html",
                        "classe": "",
                        "rotulo": "negativo",
                        "origem": "natural",
                        "nome_esperado": "",
                        "path": f"{repo}/{sorted(arquivos)[0]}",
                        "project_root": repo,
                        "reproduz": "false",
                        "contexto": [],
                        "mensagem": "commit sem URL de CDN sem versao exata",
                        "snap": f"snap/{ident}.html",
                    }
                )
                continue
            #POSITIVO: um item por URL introduzida, e o arquivo fica no INSTANTE
            for arq in arquivos:
                txt = git(d, "show", f"{c}:{arq}")
                if not txt:
                    continue
                antes = set(cdn_pins_no_texto(texto_pai.get(arq, "")))
                for url in cdn_pins_no_texto(txt):
                    if url in antes or pins_introd.get(url, 0) < 1:
                        continue
                    pins_introd[url] -= 1
                    ident = ids_do(repo, c, arq, "cdn_pin", url)
                    (SAIDA / "snap" / f"{ident}.html").write_text(txt, encoding="utf-8")
                    m = VERSAO_NO_CDN.search(url)
                    pkg, ver = (m.group(1), m.group(2).split("/")[0]) if m else ("?", "?")
                    exata = f"{ver}.0" if re.match(r"^\d+\.\d+$", ver) else "?"
                    entries = {
                        "id": ident,
                        "lang": "html",
                        "classe": "cdn_pin",
                        "rotulo": "positivo",
                        "origem": "natural",
                        # a convencao do vtransv, conferida no corpus_semeado/html novo:
                        # a URL COMO ESCRITA no atributo (ex.
                        # https://cdn.jsdelivr.net/npm/gsap@3.13/dist/gsap.min.js)
                        "nome_esperado": url,
                        "path": f"{repo}/{arq}",
                        "project_root": repo,
                        "reproduz": "true",
                        "contexto": [],
                        "mensagem": (
                            f"versao nao exata @{ver} (a exata era @{exata}) -- aviso, nao negacao"
                            if exata != "?"
                            else f"versao nao exata @{ver} -- aviso, nao negacao"
                        ),
                        "snap": f"snap/{ident}.html",
                    }
                    entradas.append(entries)
                    counts["positivo"] += 1
                    counts["cdn_pin"] += 1
                    if len(exemplos) < 6:
                        exemplos.append(f"[cdn_pin] {repo}@{c[:10]} {arq} {url}")

    # o arquivo que o juiz le
    linhas = [json.dumps(e, ensure_ascii=False, sort_keys=True) for e in entradas]
    (SAIDA / "entradas.jsonl").write_text("\n".join(linhas) + "\n", encoding="utf-8")

    arquivos = sorted(q for q in SAIDA.rglob("*") if q.is_file())
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
    SELO_PATH.write_text(json.dumps(selo, indent=1, ensure_ascii=False), encoding="utf-8")

    print(f"=== {SAIDA} ===")
    print(f"  entradas        = {len(entradas)}")
    print(f"  positivos       = {counts['positivo']} (classe {counts['cdn_pin']} = cdn_pin)")
    print(f"  negativos       = {counts['negativo']}")
    print(f"  exclusoes       = {dict(exclusoes)}")
    print(f"  selo (FORA do corpus) = {SELO_PATH}")
    print(f"  entradas_jsonl sha256 = {selo['entradas_jsonl_sha256']}")
    print(f"  sha_tree = {selo['sha_tree']}  arquivos={selo['arquivos']}  bytes={selo['bytes']}")
    print()
    print("=== exemplos (abrir-tres-antes-de-agregar) ===")
    for e in exemplos[:3]:
        print(f"  {e}")
    return 0


exemplos: list = []
raise SystemExit(main())
