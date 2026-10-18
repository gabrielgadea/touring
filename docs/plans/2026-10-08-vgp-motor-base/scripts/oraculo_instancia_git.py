"""ORACULO POR INSTANCIA no git — o par C->D. So CONTA; nao constroi corpus, nao sela nada.

PEDIDO (10/10, regra adaptada ao git):
  INSTANTE = commit C; BASE = primeiro pai C^. Arvore do instante = `git ls-tree -r C`;
  conteudo = `git show C:<rel>`.

  recurso   C introduz no diff um src/href local cujo alvo, resolvido relativo ao .html e a raiz,
            NAO existe na arvore de C, E um commit POSTERIOR D na mesma linha de historia corrige
            ESSA referencia (troca o valor por alvo existente em D, OU adiciona o alvo).
            O par C->D e o que prova o erro real: sem D o alvo pode ser artefato de build fora do
            git, e isso NAO e positivo.
            Alvo sob diretorio gitignorado/de build (dist, build, node_modules, _site) -> excluido.
  ancora    C introduz #id/for/aria-* sem o id no documento em C, e D o corrige.
  classe_css C introduz classe sem definicao em NENHUMA folha carregada (link local + <style>) em C,
            e D a corrige. Repos/paginas com Tailwind, utilitario ou CSS-in-JS -> excluido.
  cdn_pin   C introduz URL de CDN sem versao exata. Classe de FORMA (aviso): nao exige D, mas
            reportada A PARTE.

  NEGATIVOS: commits que tocam .html e cuja arvore em C confirma 0 introducoes nas 4 classes.

O que NAO se repete da secao (ii) da proposta: la um DETECTOR de padrao produziu 10 falsos
positivos (o commit REMOVIA `.no-js`, nao corrigia referencia quebrada). Aqui decide o par com o
oraculo — o alvo EXISTIA em D e NAO existia em C.
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent))
from collections import Counter
from pathlib import Path

BASE = Path.home() / ".claude/touring/vgp/oss"
# MEDIDO em 10/10: com 250 commits por repo o custo e inviavel — sao ~1.750 commits, e para cada um
# o codigo abria ate 60 revisoes filhas, cada uma com um `ls-tree -r` INTEIRO. Medido: 2 commits
# por repo rodam em segundos; 250 nao terminam. Duas correcoes:
#   (1) `LIMITE_COMMIT` cai para 40 — o sinal nao precisa de 250, e o teto so pode DIMINUIR se for
#       mais estrito, nunca crescer por amostra;
#   (2) `descobrem-se os filhos` uma vez por commit (a lista de revisoes), em vez de `rev-list`
#       dentro do laco de arquivos.
LIMITE_COMMIT = 40
LIMITE_FILHOS = 40
DIRETORIOS_EXCLUIDOS = ("dist/", "build/", "node_modules/", "_site/", ".next/", "out/")

# O `ls-tree -r` de um rev e o mesmo para todo o resto do script: memoizar troca O(n) chamadas
# git por O(1) por rev. MEDIDO: `git ls-tree -r` no excalidraw leva 0.023s, entao o custo real era o
# numero de chamadas repetidas, nao o tempo de cada uma.
_ARVORE: dict = {}
_FILHOS: dict = {}

ATRIB = re.compile(r"""(\w[\w:-]*)\s*=\s*(["'])([^"']*)\2""")
TAG = re.compile(r"<([A-Za-z][\w:-]*)([^>]*)>")
TAGS_RECURSO = {"script", "img", "link", "iframe", "source", "video", "audio"}
TAGS_REF = {"a", "area", "use", "form", "label"}
ATRS_REF = {"id", "for", "aria-labelledby", "aria-describedby", "aria-controls", "class"}
VERSAO = re.compile(r"^\d+\.\d+\.\d+")
UTILITARIO = re.compile(r"tailwind|unocss|uno\.css|windi\.css|bootstrap|bulma|foundation", re.I)


def git(d: Path, *a: str) -> str:
    r = subprocess.run(["git", "-C", str(d), *a], capture_output=True, text=True, timeout=300)
    return r.stdout or ""


def filhos_de(d: Path, c: str) -> list:
    """As primeiras revisoes posteriores a C na mesma linha. Memoizado por commit.

    O `-n` vai no proprio `git rev-list`: sem ele o git constroi a lista INTEIRA de `c..HEAD` (o
    lodash tem 7.710 commits) e so entao o Python corta. Com `-n`, o git para depois do que foi
    pedido. MEDIDO: era essa a diferenca entre rodar e nao rodar.
    """
    chave = (str(d), c)
    if chave not in _FILHOS:
        out = git(d, "rev-list", "--first-parent", "-n", str(LIMITE_FILHOS), f"{c}..HEAD")
        _FILHOS[chave] = out.split()
    return _FILHOS[chave]


def arvore(d: Path, rev: str) -> set:
    """O conjunto de caminhos que existem NA ARVORE do instante. A prova de existencia.

    Memoizado por (repo, rev): o mesmo rev e consultado varias vezes (o par C->D volta nele), e
    `git ls-tree -r` custa 0.023s mesmo no repo de 74 mil arquivos. Sem o cache, o custo era o
    numero de chamadas repetidas — nao a duracao de cada uma.
    """
    chave = (str(d), rev)
    if chave not in _ARVORE:
        out = git(d, "ls-tree", "-r", "--name-only", rev)
        _ARVORE[chave] = {ln.strip() for ln in out.splitlines() if ln.strip()}
    return _ARVORE[chave]


def existe(d: Path, rev: str, caminho: str) -> bool:
    r = subprocess.run(
        ["git", "-C", str(d), "cat-file", "-e", f"{rev}:{caminho}"],
        capture_output=True,
        timeout=120,
    )
    return r.returncode == 0


def refs(texto: str) -> list:
    """(tag, atributo, valor) de tudo que carrega referencia local. Sem CDN, sem ancora pura."""
    out = []
    for m in TAG.finditer(texto):
        tag = m.group(1).lower()
        for a in ATRIB.finditer(m.group(2) or ""):
            nome, val = a.group(1).lower(), a.group(3).strip()
            if not val:
                continue
            if nome in ATRS_REF or (nome in ("src", "href", "action") and tag in TAGS_RECURSO | TAGS_REF):
                out.append((tag, nome, val))
    return out


def classe_de(valor: str, tag: str, atr: str) -> str:
    if atr == "class":
        return "classe_css"
    if atr in ATRS_REF:
        return "id" if atr == "id" else "ancora"
    if valor.startswith("#"):
        return "ancora"
    if valor.startswith("http") or valor.startswith("//"):
        # MEDIDO em 10/10: a versao antiga (`VERSAO.match(v.rsplit("@",1)[-1])`) classificava como
        # `cdn_pin` QUALQUER URL absoluta sem `@` — sem `@`, o rsplit devolve a URL INTEIRA, que
        # nunca casa com o padrao de versao. Os 329 "positivos" eram 128x istanbul.js.org, 68x
        # Wikipedia, 26x GitHub, NASA, DOI: nenhum CDN. O criterio agora e o HOST, em
        # `scripts/cdn_pin.py` (21 casos em `test_cdn_pin.py`), e este modulo e o montador do
        # corpus importam o MESMO — dois criterios para a mesma classe divergem, e a divergencia
        # aparece como contagem diferente sem ninguem saber qual esta certa.
        from cdn_pin import classe_de_cdn

        return classe_de_cdn(valor) or "recurso"
    return "recurso"


def alvo_de(arq: str, val: str) -> str:
    """O caminho do alvo, relativo a raiz do repo.

    MEDIDO em 10/10 (conferindo os 3 primeiros achados, como manda a regra): `href="/favicon.ico"`
    e ABSOLUTO DO SITE, nao do arquivo. A versao anterior resolvia como `src/favicon.ico` — que
    EXISTE na arvore — entao a referencia nunca esteve quebrada e o par aceito era fiction. Alem disso,
    um absoluto precisa ser procurado em varios lugares plausiveis, porque o servidor pode servir de
    qualquer raiz; se nenhum deles existir no rev, a referencia e de fato ausente.

    Devolve o PRIMEIRO caminho que existe em `rev` entre os candidatos, ou o candidato principal
    (para o teste "existe em C?" responder nao).
    """
    tok = re.split(r"[?#]", val, maxsplit=1)[0]
    if tok.startswith("/"):
        # absoluto do site: tenta a raiz do repo, a pasta do arquivo, e `dist/` (build servido)
        rel = arq.split("/")
        base_dirs = ["/".join(rel[:-1]), "/".join(rel[:-2]), "dist", "dist/" + "/".join(rel[:-1])]
        candidatos = [os.path.normpath(f"{b}/{tok.lstrip('/')}") for b in base_dirs if b]
        return candidatos[0] if candidatos else tok.lstrip("/")
    return os.path.normpath(str(Path(arq).parent / tok)).lstrip("./")


def alvo_existe_em(d: Path, rev: str, arq: str, val: str) -> bool:
    """O alvo existe no rev? Para um absoluto, basta UM dos candidatos plausiveis existir."""
    tok = re.split(r"[?#]", val, maxsplit=1)[0]
    if tok.startswith("/"):
        rel = arq.split("/")
        dirs = [
            "/".join(rel[:-1]),
            "/".join(rel[:-2]),
            "dist",
            "dist/" + "/".join(rel[:-1]),
        ]
        cands = [os.path.normpath(f"{b}/{tok.lstrip('/')}") for b in dirs if b]
    else:
        cands = [os.path.normpath(str(Path(arq).parent / tok)).lstrip("./")]
    return any(c in arvore(d, rev) for c in cands)


def no_build(path: str) -> bool:
    return any(f"/{d}" in f"/{path}" or path.startswith(d) for d in DIRETORIOS_EXCLUIDOS)


def pares_do_repo(d: Path, limite: int) -> tuple:
    """(pares, negativos, exclusoes, exemplos) de UM repo. O par C->D e o que prova o erro."""
    nome = d.name
    commits = [c for c in git(d, "log", "--format=%H", "--", "*.html").split() if c]
    amostra = commits[:limite]
    achados = Counter()
    neg = 0
    excl = Counter()
    exemplos = []
    for c in amostra:
        res = pares_do_commit(d, c, achados, excl, exemplos)
        if res is False:
            neg += 1
    print(f"  {nome:20s} commits_medidos={len(amostra):>4d}  pares={dict(achados)}  negativos={neg}")
    return achados, neg, excl, exemplos


def pares_do_commit(d: Path, c: str, achados: Counter, excl: Counter, exemplos: list):
    """Processa um commit. Devolve False quando NAO houve introducao (=> negativo)."""
    arquivos = [
        a
        for a in git(d, "show", "--name-only", "--format=", c).split("\n")
        if a.strip().endswith(".html")
    ]
    if not arquivos:
        return False
    arv_C = arvore(d, c)
    utilitario = False
    introd = Counter()
    pares_locais = Counter()
    for arq in arquivos:
        txt_pai = git(d, "show", f"{c}~1:{arq}")
        txt_C = git(d, "show", f"{c}:{arq}")
        if not txt_C:
            continue
        if UTILITARIO.search(txt_C):
            utilitario = True
        antes = set(refs(txt_pai))
        for tag, atr, val in refs(txt_C):
            if (tag, atr, val) in antes:
                continue  # nao introduzido por esta edicao
            cl = classe_de(val, tag, atr)
            if cl == "classe_css" and utilitario:
                excl["classe_css:utilitario"] += 1
                continue
            if cl in ("recurso", "cdn_pin") and no_build(val):
                excl[f"{cl}:build"] += 1
                continue
            if cl == "recurso":
                if alvo_existe_em(d, c, arq, val):
                    excl["recurso:existe_em_C"] += 1
                    continue
                pares_locais["recurso"] += 1
                introd["recurso"] += 1
                continue
            introd[cl] += 1
            if cl == "cdn_pin":
                achados["cdn_pin"] += 1  # classe de FORMA: conta sem exigir D
    if not introd:
        return False
    # O FILHO D: quem CORRIGE aquela referencia.
    #
    # MEDIDO em 10/10, conferindo os exemplos a mao: aceitar QUALQUER revisao futura como D produz
    # 85 falsos. Os tres primeiros eram o MESMO commit que cria o HTML e o alvo juntos
    # (`launch-handler`: `3016097 Add Launch Handler API demo (#316)` cria o index.html E
    # images/apple-touch-icon.png) — nao ha quebra nenhuma, o alvo simplesmente ainda nao estava no
    # PAI porque o arquivo acabava de nascer. Alem disso, `https://developer.mozilla.org/...` — uma
    # URL ABSOLUTA — era resolvida contra o repo e contava como recurso local.
    #
    # As tres condicoes que sobram, e que nao tem como passar quem nao quebrou nada:
    #   (1) D e POSTERIOR a C na MESMA linha de historia (--first-parent a partir de C);
    #   (2) D e um commit DISTINTO de C — se o alvo entra em C, nao ha quebra;
    #   (3) a referencia `alvo` NAO EXISTE na arvore de C (a quebra e de C).
    for D in filhos_de(d, c):
        if pares_locais["recurso"] < 1:
            break
        arv_D = arvore(d, D)
        # o que C tocou de verdade: D tem de mexer no MESMO arquivo
        tocados_D = {
            a
            for a in git(d, "show", "--name-only", "--format=", D).split("\n")
            if a.strip().endswith(".html")
        }
        for arq in sorted(set(arquivos) & tocados_D):
            txt_D = git(d, "show", f"{D}:{arq}")
            if not txt_D:
                continue
            for _t2, a2, v2 in refs(txt_C):
                if a2 not in ("src", "href", "action") or no_build(v2):
                    continue
                if v2.startswith(("http", "//", "#", "data:", "mailto:")):
                    continue  # URL absoluta/ancora nao e recurso local
                if alvo_existe_em(d, c, arq, v2):
                    continue  # em C ja existia -> quem quebrou foi outro
                forma = None
                if alvo_existe_em(d, D, arq, v2):
                    forma = "b_adiciona_arquivo"  # D traz o asset no caminho apontado
                elif any(
                    v != v2 and alvo_de(arq, v) == alvo_de(arq, v2) for _t, _a, v in refs(txt_D)
                ):
                    # `v != v2` e ESSENCIAL: sem ele a propria referencia casa com ela mesma e
                    # qualquer reescrita do arquivo vira "correcao". MEDIDO em 10/10 conferindo o
                    # achado: `href="css/style.css"` e identico em C e em D, e mesmo assim contava
                    # como troca — o commit D so removeu `initial-scale=1`.
                    forma = "a_troca_referencia"  # D mexe no mesmo .html e aponta para alvo existente
                if forma:
                    achados["recurso"] += 1
                    achados[forma] += 1
                    pares_locais["recurso"] -= 1
                    if len(exemplos) < 40:
                        exemplos.append(
                            f"[{forma}] {d.name}@{c[:10]} -> {D[:10]} {arq} {v2}"
                        )
                    break
    return True


def main() -> int:
    pares = Counter()
    negs = Counter()
    excl = Counter()
    exemplos = []
    for d in sorted(p for p in BASE.iterdir() if p.is_dir() and (p / ".git").exists()):
        achados, neg, ex, exs = pares_do_repo(d, LIMITE_COMMIT)
        pares += achados
        negs[d.name] = neg
        excl += ex
        exemplos += exs
    print()
    print("=== TETO por classe: pares C->D com oraculo de instancia (git show) ===")
    for k in ("recurso", "ancora", "cdn", "cdn_pin", "classe_css"):
        n = pares.get(k, 0)
        print(f"  {k:12s} {n:>4d}   {'atinge n>=20' if n >= 20 else 'NAO atinge n>=20'}")
    print()
    print("=== `recurso` separado por forma de D (como pedido) ===")
    print(f"  (a) D mexe no MESMO .html e troca a referencia -> alvo existente : {pares.get('a_troca_referencia', 0)}")
    print(f"  (b) D ADICIONA o arquivo exatamente no caminho apontado              : {pares.get('b_adiciona_arquivo', 0)}")
    print(f"\n  negativos disponiveis: {dict(negs)}  total={sum(negs.values())}")
    print(f"  exclusoes por motivo: {dict(excl)}")
    if exemplos:
        print("\n  exemplos (para conferencia manual):")
        for e in exemplos:
            print(f"    {e}")
    return 0


# O guarda importa: `monta_holdout_html_git.py` importa este modulo para REUSAR o oraculo (duas
# copias divergem, e a divergencia apareceria como contagem diferente, sem ninguem saber qual esta
# certa). Sem o `if __name__`, o import roda a contagem inteira e imprime a saida dela no meio da
# montagem do corpus. MEDIDO em 10/10: foi o que aconteceu.
if __name__ == "__main__":
    main()