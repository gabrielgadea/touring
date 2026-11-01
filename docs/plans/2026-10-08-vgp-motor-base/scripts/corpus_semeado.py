#!/usr/bin/env python3
"""Corpus SEMEADO do VGP para TS, TSX, JS e HTML (VL0 §7).

Nao ha erro natural dessas linguagens nos transcripts (0 edicoes TS com erro na janela), entao
o recall delas se mede num corpus semeado: uma mutacao por item, num arquivo real, cada uma
confirmada por um ORACULO INDEPENDENTE DO MOTOR que acusa o mutado E NAO o original. Item sem
oraculo conclusivo sai do corpus, nunca rotulado por suposicao.

Esquema (o mesmo que `scripts/juiz_motor_vgp.py` ja le):
  <dir>/entradas.jsonl | <dir>/snap/<id>.<ext> | <dir>/estado_no_instante/manifesto.json

O motor so nega o que a mutacao introduz: cada positivo carrega no manifesto o `disk_file` do
ORIGINAL, que e o delta.

Uso:
  python3 corpus_semeado.py --selftest
  python3 corpus_semeado.py --lang typescript --n 20 --negativos 200
  python3 corpus_semeado.py --confere-determinismo
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from html.parser import HTMLParser
from pathlib import Path
from typing import Self
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

# --------------------------------------------------------------------------- contrato
SEMENTE = "20261009"
SAIDA_PADRAO = Path.home() / ".claude/touring/vgp/corpus_semeado"
LINGUAGENS = ("typescript", "tsx", "javascript", "html")
EXT = {"typescript": "ts", "tsx": "tsx", "javascript": "js", "html": "html"}
SUFIXO = {"typescript": ".ts", "tsx": ".tsx", "javascript": ".js", "html": ".html"}

ANALISE = Path.home() / "projects/analise"
FRONTEND = ANALISE / "frontend"
PORTAL = Path.home() / "projects/konverter/portal"
TSC = FRONTEND / "node_modules/.bin/tsc"

IGNORAR = {
    "node_modules",
    ".git",
    "dist",
    ".next",
    "build",
    "__pycache__",
    ".venv",
    "coverage",
    ".audit-cache",
    ".hypothesis",
    ".pytest_cache",
    ".serena",
    "venv",
    # Medido em 09/10: sem estes, a varredura de HTML entrava em `.cache/` (extracoes de
    # processo, `target/doc/` do rustdoc) e na PRÓPRIA sombra de trabalho -- medido: `.vgp-semeado/
    # corpus/html/snap/*.orig.html` aparecia como fonte do corpus. Corpus que se realimenta nao e
    # deterministico: o sha de `entradas.jsonl` passaria a depender do que a rodada anterior deixou.
    ".cache",
    ".vgp-semeado",
    ".vgp-sombra-frontend",
    ".audit",
    "target",
    "site-packages",
    ".next-static",
    # Onda 2, tarefa C (medido em 09/10): `~/projects` tem 9 diretorios `touring-wt-*` que sao o
    # MESMO repo em worktrees distintos (mesmo sha256 nos mesmos arquivos). Sem esta lista,
    # `varre` conta o mesmo arquivo ate 9 vezes e o "teto da fonte" sai inflado.
    "touring-wt-f21",
    "touring-wt-g1",
    "touring-wt-l2",
    "touring-wt-q50",
    "touring-wt-semeado",
    "touring-wt-vgp-attr",
    "touring-wt-vl0",
    "touring-wt-vrs",
    "touring-wt-wiring",
    # codigo de terceiro: semear aqui mediria a biblioteca, nao o motor
    "third_party",
    # instrumentacao de hook: nao e codigo de aplicacao. Medido em 09/10 (onda 2): o `tsc` nao
    # produz diagnostico util nesses arquivos e o oraculo do DELTA reprovava os 4 operadores de js
    # com "o mutado nao introduziu diagnostico novo (base: 0)".
    ".claude",
    ".gemini",
    # artefato de cobertura (medido em 09/10: `relatoria/rdp/htmlcov/coverage_html_cb_*.js` e o
    # bundle do relatorio de teste do pytest/coverage -- gerado, nao codigo do projeto)
    "htmlcov",
    "lcov-report",
}
ALVO_POR_LANG = 20
NEGATIVOS_POR_LANG = 200
TAMANHO_MAX_KB = 400
TIMEOUT_REDE_S = 10

DESLIGADOS: set[str] = set()  # so o mutante do selftest mexe nisto

# Linguagens cujo oraculo e o DELTA de diagnosticos, e nao "base limpa + erro no arquivo-alvo".
# `javascript` entra porque a linha de base de `packages/` NAO e limpa (medido: 136 arquivos com
# erro); recusar o oraculo por causa disso deixava a linguagem com zero positivo.
LINGUAGENS_DELTA = frozenset({"javascript"})

# Quantas MUTACOES um mesmo arquivo pode receber, por linguagem (tarefa C da onda 2, 09/10).
# `javascript` tem 11 arquivos limpos na base: com 1 mutacao por arquivo o teto seria 11 por
# classe. Cada variacao usa simbolo distinto e o oraculo do DELTA exige diagnostico distinto --
# entao sao evidencias separadas, e a concentracao por arquivo e reportada no log.
MUTACOES_POR_ARQUIVO = {"javascript": 3}


# --------------------------------------------------------------------------- utilidades
def sha12(texto: str) -> str:
    return hashlib.sha256(texto.encode("utf-8")).hexdigest()[:12]


def nome_marcado(prefixo: str, identificador: str) -> str:
    return f"{prefixo}{identificador}"


def amostra(lista: list, n: int) -> list:
    """Amostra DETERMINISTICA: evenly spaced sobre a lista ja ordenada."""
    if len(lista) <= n:
        return list(lista)
    return [lista[int(i * len(lista) / n)] for i in range(n)]


def escreve_jsonl(caminho: Path, linhas: list) -> None:
    caminho.parent.mkdir(parents=True, exist_ok=True)
    caminho.write_text(
        "\n".join(json.dumps(x, ensure_ascii=False, sort_keys=True) for x in linhas)
        + "\n",
        encoding="utf-8",
    )


# Arquivos que a varredura NAO pode semear (onda 2, tarefa C -- medido em 09/10):
#  - `*.min.js`: bundle minificado. 44 dos 63 negativos da 1a rodada eram minificados do lunr
#    (biblioteca de terceiro) -- e um negativo de arquivo de terceiro nao mede o motor.
#  - configuracao de build: `tailwind.config.js`, `postcss.config.js`, `eslint.config.js`. A
#    coordenadora proibiu, e o motivo e tecnico: 36 dos 80 positivos da 1a rodada caíram em
#    `tailwind.config.js` (12 mutacoes x 3 variacoes). Corpus de motor medindo configuracao
#    de tailwind nao mede o motor sobre codigo.
ARQ_NAO_SEMEAVEIS = (
    ".min.js",
    "tailwind.config.js",
    "postcss.config.js",
    # as variantes `.mjs` do mesmo arquivo: medido em 09/10, `eslint.config.mjs` escapava do
    # filtro acima porque o nome nao termina em `.js` -- e config de build nao se semeia
    "tailwind.config.mjs",
    "postcss.config.mjs",
    "eslint.config.mjs",
    "eslint.config.js",
)


def varre(raiz: Path, sufixos: tuple | str) -> list:
    """Arquivos sob `raiz`, com os diretorios de IGNORAR fora.

    O filtro olha as partes RELATIVAS a `raiz`, nunca as absolutas. Medido em 09/10: comparar
    `p.parts` inteiro faz o filtro casar o `.cache` do PRÓPRIO caminho da sombra
    (`~/.cache/vgp-semeado/proj-XXX/frontend`), que e onde a varredura acontece -- e então
    `varre` devolve 0 para qualquer linguagem. Era o que zerava o determinismo de typescript."""
    base = raiz.resolve()
    achados = []
    for s in (sufixos,) if isinstance(sufixos, str) else sufixos:
        for p in base.rglob(f"*{s}"):
            if any(x in IGNORAR for x in p.relative_to(base).parts) or not p.is_file():
                continue
            if p.name.endswith(ARQ_NAO_SEMEAVEIS):
                continue
            if p.stat().st_size <= TAMANHO_MAX_KB * 1024:
                achados.append(p)
    return sorted(set(achados))


def le(p: Path) -> str:
    return p.read_text(encoding="utf-8", errors="replace")


def tag(nome: str, atributos: str) -> str:
    """Monta tag por concatenacao: o fonte nao carrega literal de tag de script."""
    return "<" + nome + atributos + ">"


# --------------------------------------------------------------------------- oraculo: tsc
LINHA_TSC = re.compile(
    r"^(?P<arq>.+?)\((?P<lin>\d+),(?P<col>\d+)\): error (?P<cod>TS\d+): (?P<msg>.*)$"
)


def parse_tsc(stdout: str) -> dict:
    """{arquivo: [achado]} -- o que o compilador REALMENTE apontou, por arquivo."""
    por_arq: dict = {}
    for linha in stdout.splitlines():
        m = LINHA_TSC.match(linha.strip())
        if m:
            por_arq.setdefault(m.group("arq"), []).append(
                {
                    "cod": m.group("cod"),
                    "msg": m.group("msg"),
                    "lin": int(m.group("lin")),
                }
            )
    return por_arq


def sombra_de(raiz: Path, destino: Path, apenas: list | None = None) -> Path:
    """Copia por hardlink: instantanea e sem custo. Sobrescrever = remove + write, NUNCA
    escrever no inode -- e essa a garantia de que o arquivo-fonte fica intacto.

    O destino PRECISA estar no mesmo dispositivo: medido, /tmp e device 58 e a home e 31, e
    `cp -al` entre eles morre com "Invalid cross-device link".

    `apenas` = lista de caminhos (relativos a `raiz`) a espelhar. Medido em 09/10 (onda 2): sem
    isso, espelhar `~/projects` INTEIRO falha -- `cp -al` devolve "Operation not permitted" em
    binarios de vector store (o link cruzado exige permissao que o sandbox nega), e o custo
    aparente e de 25 GB para uma fonte que tem 154 `.js`. Espelhar so o que a varredura
    encontra resolve os dois, e o que nao esta na sombra simplesmente nao existe para o `tsc` --
    o que e o comportamento correto: a linguagem nao tem fonte, e isso e medido, nao escondido."""
    if destino.exists():
        shutil.rmtree(destino)
    destino.mkdir(parents=True, exist_ok=True)
    rels = apenas if apenas is not None else None
    if rels is None:
        subprocess.run(["cp", "-al", str(raiz), str(destino)], check=True)
        espelha_os_tipos(raiz, destino)
        return destino
    for rel in rels:
        src = raiz / rel
        if not src.is_file():
            continue
        dst = destino / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(src, dst)
        except OSError:
            # link cruzado indisponivel neste ponto: copia real, ainda sem tocar no original
            shutil.copy2(src, dst)
    espelha_os_tipos(raiz, destino)
    return destino


# Diretorios de node_modules que a sombra PRECISA ter. Medido em 09/10 (onda 2): sem os tipos,
# o `tsc` responde `TS2688: Cannot find type definition file for 'node'` e ABORTA sem analisar
# nenhum arquivo -- o que o gerador lia como "base limpa". Ou seja: a sombra sem `node_modules` e
# pior que useless, ela e um oraculo que produz zero sem avisar.
PASTAS_DE_TIPO = ("node_modules/@types", "node_modules/vite", "node_modules/typescript")


def espelha_os_tipos(raiz: Path, destino: Path) -> list:
    """Espelha `node_modules/@types` na sombra. Devolve o que FALHOU (vazio = tudo certo).

    Idempotente: quando a sombra ja veio com `node_modules` inteiro (copia completa), o destino
    existe e o `cp -al` recusaria -- e com `check=False` o erro sumiria, o `tsc` ficaria sem tipos
    (TS2688) e o oraculo produziria zero sem avisar. Por isso o `exists()` antes, e o retorno do
    erro em vez do silencio."""
    problemas = []
    for rel in PASTAS_DE_TIPO:
        fonte = raiz / rel
        if not fonte.is_dir():
            problemas.append(f"{rel}: fonte ausente")
            continue
        alvo = destino / rel
        if alvo.exists():
            continue  # ja veio na copia completa
        alvo.parent.mkdir(parents=True, exist_ok=True)
        r = subprocess.run(
            ["cp", "-al", str(fonte), str(alvo)],
            check=False,
            capture_output=True,
            text=True,
        )
        if r.returncode != 0:
            problemas.append(f"{rel}: {r.stderr.strip()[:120]}")
    return problemas


def dir_sombra(base: Path) -> Path:
    """Onde a sombra nasce: mesmo dispositivo da fonte (home), nunca /tmp -- e, dentro do
    sandbox, `VGP_SEMEADO_TMP` aponta para a area gravavel (a home e somente leitura la)."""
    raiz = Path(
        os.environ.get("VGP_SEMEADO_TMP", str(Path.home() / ".cache" / "vgp-semeado"))
    )
    raiz.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix="proj-", dir=str(raiz))) / base.name


def escreve_isolado(destino: Path, texto: str) -> None:
    """Escreve SEM tocar no inode original.

    Toda sombra e feita por hardlink: os arquivos compartilham o inode com o arquivo REAL. Uma
    escrita in-place (`open(p, "w")`) alteraria o projeto de origem. Por isso o `unlink` ANTES:
    o nome some do diretorio (o original, com seu inode, continua intocado no outro caminho) e
    a escrita cria um inode novo. Medido e exigido pela coordenadora (onda 2, 09/10).
    """
    destino.unlink(missing_ok=True)
    destino.write_text(texto, encoding="utf-8")


def roda_tsc(raiz: Path, timeout: int = 900) -> tuple:
    """(diagnosticos por arquivo, saida bruta) -- a saida bruta e o que a guarda le.

    Medido em 09/10: `tsc` sem `tsconfig` responde `TS5057` e nao devolve nenhum diagnostico --
    o que, lido sozinho, e indistinguivel de "base limpa"."""
    r = subprocess.run(
        [str(TSC), "--noEmit", "-p", "."],
        cwd=raiz,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return parse_tsc(r.stdout), r.stdout


def acusou_o_tsc(achados: list, cod: str, nome: str) -> bool:
    return any(a["cod"] == cod and nome in a["msg"] for a in achados)


# --------------------------------------------------------------------------- oraculo por DELTA
# Para `javascript` a linha de base do projeto NAO e limpa (medido: 136 arquivos com erro em
# `packages/`), e recusar o oraculo por causa disso deixa a linguagem com zero positivo. O oracular
# passa a ser o DELTA: o mutado tem EXATAMENTE UM diagnostico a mais que o original, com o codigo
# esperado, na linha da mutacao, nomeando o nome esperado -- e o resto do conjunto e IDENTICO ao do
# original. Isso cumpre "acusa o mutado e nao o original" mesmo com a base suja.
#
# O mutante do selftest troca `DELTA_EXIGE_LINHA_E_NOME` por uma checagem laxa que aceita QUALQUER
# diagnostico novo: um oraculo assim tem de REPROVAR o selftest, senao nao discrimina nada.
DELTA_EXIGE_LINHA_E_NOME = True

# O MUTANTE do oraculo: quando False, `confere_delta` aceita QUALQUER diagnostico novo -- de
# qualquer codigo, em qualquer linha, nomeando qualquer nome. E o oraculo que o brief proibe
# explicitamente, e o selftest tem de provar que ele e defeituoso (ele aceita um diagnostico que
# NAO e o esperado, que e exatamente como um corpus falso entra).
DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO = True


class DeltaLaxo:
    """Context manager que DESLIGA a exigencia de diagnostico especifico (o mutante do selftest).

    Sem `global`: a flag e lida de `globals()` dentro de `confere_delta`, e um `global` solto
    esconderia o estado do resto do processo. O contexto deixa a reversao impossivel de esquecer."""

    def __init__(self, ligado: bool) -> None:
        self.ligado = ligado
        self.anterior = DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO

    def __enter__(self) -> Self:
        globals()["DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO"] = self.ligado
        return self

    def __exit__(self, *_exc) -> bool:
        globals()["DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO"] = self.anterior
        return False


def assinatura_diag(achados: list) -> Counter:
    """Contagem de (cod, lin, msg) -- comparavel entre duas execucoes do tsc."""
    return Counter((a["cod"], a["lin"], a["msg"]) for a in achados)


def delta_diagnosticos(base: list, mutado: list) -> tuple:
    """(novos, perdidos) -- multiconjuntos: o mesmo diagnostico duas vezes conta duas."""
    c_base, c_mut = assinatura_diag(base), assinatura_diag(mutado)
    novos, perdidos = c_mut - c_base, c_base - c_mut
    return novos, perdidos


def confere_delta(
    base: list, mutado: list, cod: str, nome: str, lin_esperada: int
) -> str:
    """(vazio = CONFIRMA). O DELTA tem de ser de EXATAMENTE um diagnostico, e ser o esperado."""
    novos, perdidos = delta_diagnosticos(base, mutado)
    if not novos:
        return f"o mutado nao introduziu diagnostico novo (base: {len(base)})"
    if perdidos:
        return (
            f"o mutado alterou {sum(perdidos.values())} diagnostico(s) que ja existiam: "
            f"{list(perdidos)[:2]}"
        )
    if sum(novos.values()) != 1:
        return (
            f"o mutado introduziu {sum(novos.values())} diagnosticos novos "
            f"(o oraculo exige exatamente 1): {list(novos)[:3]}"
        )
    (d_cod, d_lin, d_msg), n = next(iter(novos.items()))
    if n != 1:
        return f"o diagnostico novo aparece {n}x no mutado"
    if not DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO:
        return (
            ""  # o mutante: aceita qualquer diagnostico novo (e por isso e defeituoso)
        )
    if d_cod != cod:
        return f"o diagnostico novo e {d_cod}, esperava {cod} (msg: {d_msg})"
    if DELTA_EXIGE_LINHA_E_NOME:
        if nome not in d_msg:
            return f"o diagnostico {d_cod} nao nomeia {nome!r} (msg: {d_msg})"
        if d_lin != lin_esperada:
            return (
                f"o diagnostico {d_cod} caiu na linha {d_lin}, "
                f"a mutacao esta na {lin_esperada} (msg: {d_msg})"
            )
    return ""


def linha_da_mutacao(original: str, linha_codigo: str) -> int:
    """1-based: a linha do PRIMEIRO token de CODIGO da mutacao, no arquivo mutado.

    Medido em 09/10 (o selftest reprovava `r_js_kwarg` com "o diagnostico caiu na linha 175, a
    mutacao esta na 174"). O truque e nao contar: o proprio arquivo mutado sabe a resposta.
    `muta_e_confere` monta `original + "\\n" + linha_codigo`; aqui montamos o mesmo texto e
    procuramos a primeira linha real de codigo a partir do fim do original.
    """
    partes = (original + "\n" + linha_codigo).split("\n")
    # quantas linhas o original ocupa (a ultima pode ser vazia, se ele termina em \\n)
    n_original = len(original.split("\n"))
    for i in range(n_original, len(partes)):
        if not partes[i].strip():
            continue
        if partes[i].strip().startswith(("/*", "*", "//")):
            continue
        return i + 1
    return n_original + 1


# --------------------------------------------------------------------------- operadores de codigo
# Cada receita devolve (linha_anexada, codigo_do_tsc_esperado, nome_esperado). Todas foram
# provadas uma a uma contra o tsc do projeto antes de entrarem aqui: 1 erro, o esperado, nenhum outro.


def r_import(_rel: str, ident: str) -> tuple:
    nome = nome_marcado("vgpAusente", ident)
    modulo = nome_marcado("__vgpModAusente", ident)
    return (f"import {{ {nome} }} from './{modulo}';", "TS2307", modulo)


def r_nome(_rel: str, ident: str) -> tuple:
    nome = nome_marcado("vgpAusente", ident)
    return (f"{nome};", "TS2304", nome)


def r_atributo(_rel: str, ident: str) -> tuple:
    prop = nome_marcado("vgpPropAusente", ident)
    return (f"export const _vgpProbe{ident} = new Date().{prop};", "TS2339", prop)


def r_kwarg(_rel: str, ident: str) -> tuple:
    prop = nome_marcado("vgpPropAusente", ident)
    chave = nome_marcado("vgpCampo", ident)
    return (
        f"export const _vgpProbe{ident}: {{ {chave}: number }} = {{ {chave}: 1, {prop}: 2 }};",
        "TS2353",
        prop,
    )


def r_componente(_rel: str, ident: str) -> tuple:
    comp = nome_marcado("VgpCompAusente", ident)
    return (f"export const _vgpProbe{ident} = () => <{comp} />;", "TS2304", comp)


def r_prop(_rel: str, ident: str) -> tuple:
    prop = nome_marcado("vgpPropAusente", ident)
    return (
        f"export const _vgpProbe{ident} = () => <div {prop}={{1}} />;",
        "TS2322",
        prop,
    )


# --- receitas de JS: as de TS nao servem (medido pelo selftest em 09/10, 3 de 4 reprovaram)
# Motivo medido, nao suposto: `export` e anotacao de tipo sao de TS. Numa `.js` CommonJS o
# `export` (ou o `import`) converte o arquivo em modulo ES, o que produz um TS2580 NOVO no mesmo
# arquivo -- e o oraculo do delta, que exige exatamente 1 diagnostico novo, recusa (e deve recusar).
# A anotacao de tipo produz TS8010 "Type annotations can only be used in TypeScript files".
# Aqui: `require` em vez de `import`, sem `export`, e o tipo vem de JSDoc (que e JS).


def r_js_import(_rel: str, ident: str) -> tuple:
    modulo = nome_marcado("__vgpModAusente", ident)
    return (f"require('./{modulo}');", "TS2307", modulo)


def r_js_nome(_rel: str, ident: str) -> tuple:
    nome = nome_marcado("vgpAusente", ident)
    return (f"{nome};", "TS2304", nome)


def r_js_atributo(_rel: str, ident: str) -> tuple:
    prop = nome_marcado("vgpPropAusente", ident)
    return (f"new Date().{prop};", "TS2339", prop)


def r_js_kwarg(_rel: str, ident: str) -> tuple:
    """Campo a mais num literal de objeto cujo tipo o JSDoc declara.

    Medido em 09/10 (o selftest reprovou esta receita em 5 de 5 arquivos, e a sonda por receita
    mostrou por que): em JSDoc, `{vgpCampo: number}` e um NOME DE TIPO, nao um record inline --
    o `tsc` fecha a expressao no primeiro `}` e acusa `TS1005: "}' expected"` mais um `TS2552`
    ("cannot find name 'vgpCampo'"), ou seja DOIS diagnosticos novos, e o oraculo do delta recusa.
    O record inline exige as chaves INTERNAS: `{{ vgpCampo: number }}`."""
    prop = nome_marcado("vgpPropAusente", ident)
    chave = nome_marcado("vgpCampo", ident)
    return (
        (
            "/** @type {{ " + chave + ": number }} */\n"
            "const _vgpProbe" + ident + " = { " + chave + ": 1, " + prop + ": 2 };"
        ),
        "TS2353",
        prop,
    )


OPERADORES = {
    "typescript": {
        "import": r_import,
        "nome": r_nome,
        "atributo": r_atributo,
        "kwarg": r_kwarg,
    },
    "tsx": {
        "import": r_import,
        "nome": r_nome,
        "atributo": r_atributo,
        "kwarg": r_kwarg,
        "componente": r_componente,
        "prop": r_prop,
    },
    "javascript": {
        "import": r_js_import,
        "nome": r_js_nome,
        "atributo": r_js_atributo,
        "kwarg": r_js_kwarg,
    },
}

# --------------------------------------------------------------------------- HTML: coleta
ATRIBUTO = re.compile(
    r"""(?P<tag>[A-Za-z][\w:-]*)(?P<antes>[^>]*?)(?P<aspas>["'])(?P<val>[^"']*)["']"""
)
CLASSE_DEF = re.compile(r"(?<![\w-])\.(-?[_A-Za-z][\w-]*)")
CLASSES_USADAS = re.compile(r"""\bclass\s*=\s*(["'])([^"']*)\1""")
CDN_URL = re.compile(
    r"""https?://(?:cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com)/[^"'\s>]+"""
)
VERSAO_NO_CDN = re.compile(r"/npm/([^/@]+)@(\d+\.\d+\.\d+[0-9A-Za-z.-]*)(/|$)")
UTILITARIO = re.compile(
    r"tailwind|unocss|uno\.css|windi\.css|bootstrap|bulma|foundation", re.I
)
TAGS_COM_RECURSO = ("script", "img", "link", "iframe", "source", "video", "audio")


class Documento(HTMLParser):
    """ids, ancoras e folhas -- o que os cinco operadores de HTML precisam ver."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: set = set()
        self.ancoras: list = []
        self.folhas: list = []
        self.estilos = 0

    def handle_starttag(self, tag, attrs):
        self._colhe(tag, attrs)

    def handle_startendtag(self, tag, attrs):
        self._colhe(tag, attrs)

    @staticmethod
    def _ancora_de(tag: str, d: dict) -> str:
        if tag == "a" and d.get("href", "").startswith("#"):
            return d["href"][1:]
        if tag == "label" and d.get("for"):
            return d["for"]
        return ""

    def _colhe(self, tag: str, attrs) -> None:
        d = {k: (v or "") for k, v in attrs}
        if d.get("id"):
            self.ids.add(d["id"])
        alvo = self._ancora_de(tag, d)
        if len(alvo) > 1:
            self.ancoras.append(alvo)
        if tag == "link":
            self.folhas.append(d)
        if tag == "style":
            self.estilos += 1


def monta_doc(texto: str):
    d = Documento()
    try:
        d.feed(texto)
    except Exception:
        return None  # documento que o parser nao fecha: fora do corpus
    return d


def ids_do(texto: str) -> set:
    d = monta_doc(texto)
    return d.ids if d else set()


def folha_resolvivel(doc: Documento, texto: str) -> bool:
    """Descarte do oraculo de classe: TODA folha tem de ser resolvivel, e nada de framework
    utilitario -- la classe e gancho de JS, nao definicao."""
    if UTILITARIO.search(texto):
        return False
    return not any(
        "stylesheet" in d.get("rel", "")
        and d.get("href", "").startswith(("http://", "https://", "//"))
        for d in doc.folhas
    )


# --------------------------------------------------------------------------- oraculo: CDN
def carrega_cache(destino: Path) -> dict:
    p = destino / "cdn_cache.json"
    return json.loads(p.read_text(encoding="utf-8")) if p.is_file() else {}


def guarda_cache(destino: Path, cache: dict) -> None:
    destino.mkdir(parents=True, exist_ok=True)
    (destino / "cdn_cache.json").write_text(
        json.dumps(cache, indent=1, sort_keys=True), encoding="utf-8"
    )


def cabecalho(url: str, cache: dict) -> dict:
    """HEAD com cache por URL. TIMEOUT NAO E 404: devolve `indeterminado` e o item SAI."""
    if url in cache:
        return cache[url]
    reg: dict = {"url": url}
    try:
        r = urlopen(Request(url, method="HEAD"), timeout=TIMEOUT_REDE_S)
        reg.update(status=r.status, verdict="existe")
    except HTTPError as e:
        reg.update(status=e.code, verdict="404" if e.code == 404 else "http_erro")
    except (URLError, TimeoutError, OSError) as e:
        reg.update(
            status=None, verdict="indeterminado", erro=f"{type(e).__name__}: {e}"
        )
    cache[url] = reg
    return reg


# --------------------------------------------------------------------------- emissao
def entrada(
    ident: str,
    rotulo: str,
    classe: str,
    nome: str,
    lang: str,
    caminho: Path,
    project_root: Path,
    snap_rel: str,
    mensagem: str,
    reproduz: str = "true",
) -> dict:
    return {
        "id": ident,
        "rotulo": rotulo,
        "reproduz": reproduz,
        "classe": classe,
        "nome_esperado": nome,
        "origem": "semeado",
        "lang": lang,
        "path": str(caminho),
        "project_root": str(project_root),
        "snap": snap_rel,
        "mensagem": mensagem,
        "contexto": [],
    }


def grava_item(
    saida_lang: Path, ident: str, conteudo: str, original: str, ext: str
) -> tuple:
    """snap/<id>.<ext> e o MUTADO; snap/<id>.orig.<ext> e o original que vai no manifesto."""
    (saida_lang / "snap").mkdir(parents=True, exist_ok=True)
    (saida_lang / f"snap/{ident}.{ext}").write_text(conteudo, encoding="utf-8")
    (saida_lang / f"snap/{ident}.orig.{ext}").write_text(original, encoding="utf-8")
    return f"snap/{ident}.{ext}", f"snap/{ident}.orig.{ext}"


def persistir(
    saida_lang: Path, entradas: list, manifesto: dict, negativos: list | None = None
) -> None:
    """Grava o indice DEPOIS DE CADA CLASSE, nao so na saida.

    Medido em 09/10: uma geracao interrompida no meio deixou 44 snaps em disco e
    `entradas.jsonl` inexistente — uma pilha sem indice, inservivel para o juiz. Estado
    escrito so no fim e estado que nao sobrevive a ninguem."""
    todas = sorted(entradas + (negativos or []), key=lambda e: e["id"])
    if not todas:
        return
    escreve_jsonl(saida_lang / "entradas.jsonl", todas)
    (saida_lang / "estado_no_instante").mkdir(parents=True, exist_ok=True)
    (saida_lang / "estado_no_instante" / "manifesto.json").write_text(
        json.dumps(manifesto, indent=1, sort_keys=True), encoding="utf-8"
    )


# --------------------------------------------------------------------------- TS/TSX/JS
def _confere_por_delta(
    rel: str,
    original: str,
    linha: str,
    cod: str,
    nome: str,
    base: dict,
    erros: dict,
) -> tuple:
    """Ramo DELTA: a linha de base pode estar suja; o que tem de ser EXATO e o delta."""
    fora = {a: v for a, v in erros.items() if a != rel and a not in base}
    if fora:
        return False, f"o mutado produziu erro TAMBEM fora de {rel}: {fora}", None, None
    motivo = confere_delta(
        base.get(rel, []),
        erros.get(rel, []),
        cod,
        nome,
        linha_da_mutacao(original, linha),
    )
    if motivo:
        return False, f"{rel}: {motivo}", None, None
    return True, f"delta={cod} em {rel}: exatamente 1 diagnostico novo", None, None


def _confere_legado(rel: str, cod: str, nome: str, base: dict, erros: dict) -> tuple:
    """Ramo legado: base limpa, e o erro tem de ser SO no arquivo-alvo."""
    if {a: v for a, v in erros.items() if a != rel}:
        return (
            False,
            f"o mutado produziu erro TAMBEM fora de {rel}: {erros}",
            None,
            None,
        )
    if acusou_o_tsc(erros.get(rel, []), cod, nome):
        if acusou_o_tsc(base.get(rel, []), cod, nome):
            return False, f"{cod}/{nome} JA estava na linha de base", None, None
        return True, f"{cod} em {rel}: {[a['msg'] for a in erros[rel]]}", None, None
    return (
        False,
        f"{rel} sem {cod} sobre {nome!r} (ocorreu: {erros.get(rel) or 'nenhum'})",
        None,
        None,
    )


def muta_e_confere(
    sombra: Path,
    rel: str,
    original: str,
    receita,
    ident: str,
    base: dict,
    por_delta: bool = False,
) -> tuple:
    """Aplica UMA mutacao, roda o tsc, restaura. Devolve (entrou?, motivo, mutado, nome).

    `por_delta=True` usa o oraculo do DELTA (util para `javascript`, cuja linha de base e suja):
    o diagnostico novo tem de ser exatamente um, o esperado, na linha da mutacao. Com
    `por_delta=False` o oraculo legado exige base limpa e erro SO no arquivo-alvo."""
    linha, cod, nome = receita(rel, ident)
    mutado = original + "\n" + linha + "\n"
    caminho = sombra / rel
    escreve_isolado(caminho, mutado)
    try:
        erros, _saida = roda_tsc(sombra)
    finally:
        escreve_isolado(caminho, original)  # SEMPRE restaura, mesmo se o tsc explodir

    if por_delta:
        entrou, motivo, _m, _n = _confere_por_delta(
            rel, original, linha, cod, nome, base, erros
        )
    else:
        entrou, motivo, _m, _n = _confere_legado(rel, cod, nome, base, erros)
    return entrou, motivo, (mutado if entrou else None), nome


def um_item_de_classe(
    classe: str,
    receita,
    lang: str,
    sombra: Path,
    base_real: Path,
    saida_lang: Path,
    rels: list,
    n: int,
    erros_base: dict,
    contadores: Counter,
    log,
    por_delta: bool = False,
    por_arquivo: int = 1,
    variacoes: list | None = None,
) -> list:
    """N itens de UMA classe. Sai do corpus o que o oraculo nao confirmar.

    `por_arquivo` = quantas MUTACOES o mesmo arquivo pode receber, cada uma com simbolo distinto.
    Medido em 09/10 (tarefa C da onda 2): com 11 arquivos `.js` limpos na base e 1 mutacao por
    arquivo, o teto era 11 por classe -- e a fonte permite mais. A concentracao por arquivo e
    reportada, nunca oculta: um corpus de 20 itens em 5 arquivos e outro de 20 em 20 arquivos
    medem coisas diferentes."""
    obtidos = []
    por_rel: Counter = Counter()
    # (arquivo, variacao): a variacao entra no id, entao dois itens do mesmo arquivo tem ids
    # distintos e snaps distintos -- e o oraculo ainda exige delta distinto em cada.
    combinacoes = [
        (rel, v)
        for rel in amostra(rels, max(n * 4, n))
        for v in (variacoes or [0])
        if por_rel[rel] < por_arquivo
    ]
    for rel, variacao in combinacoes:
        if len(obtidos) >= n:
            break
        ident = sha12(f"{lang}|{classe}|{rel}|v{variacao}")
        original = le(sombra / rel)
        receita_v = receita
        if variacoes:
            receita_v = receita_variada(receita, variacao)
        entrou, motivo, mutado, nome = muta_e_confere(
            sombra, rel, original, receita_v, ident, erros_base, por_delta=por_delta
        )
        if not entrou:
            contadores[f"{classe}:oráculo"] += 1
            log(f"    descartado {classe} {rel} v{variacao}: {motivo}")
            continue
        por_rel[rel] += 1
        snap_rel, snap_orig = grava_item(saida_lang, ident, mutado, original, EXT[lang])
        obtidos.append(
            (
                entrada(
                    ident,
                    "positivo",
                    classe,
                    nome,
                    lang,
                    base_real / rel,
                    base_real,
                    snap_rel,
                    motivo,
                ),
                {"disk_file": snap_orig, "overlays": []},
            )
        )
    contadores[f"{classe}:n"] = len(obtidos)
    if por_rel:
        topo = por_rel.most_common(1)[0]
        log(
            f"    {classe}: {len(obtidos)}/{n} em {len(por_rel)} arquivo(s); "
            f"concentracao maxima {topo[1]} em {topo[0]}"
        )
    return obtidos


def receita_variada(receita, variacao: int):
    """A MESMA receita com OUTRO simbolo: o delta tem de ser distinto, e nao a mesma linha.

    Sem isto, duas variacoes no mesmo arquivo dariam o mesmo `nome_esperado` e o mesmo
    diagnostico -- e o juiz contaria duas evidences de uma coisa so."""

    def variante(_rel: str, ident: str) -> tuple:
        linha, cod, nome = receita(_rel, ident)
        marca = f"v{variacao}"
        return (
            linha.replace(nome, f"{nome}{marca}"),
            cod,
            f"{nome}{marca}",
        )

    return variante


def _oraculo_recusado(erros_base: dict) -> dict:
    """Base suja com oraculo legado: recusar e melhor do que semear item nao confirmado."""
    return {
        "entradas": [],
        "manifesto": {},
        "descartes": {"projeto_ja_erra": len(erros_base)},
        "por_classe": {},
        "n": 0,
        "erros_base": erros_base,
        "aviso": f"o projeto ja erra em {len(erros_base)} arquivo(s) -- oraculo recusado",
    }


def confere_que_a_base_foi_verificada(
    erros_base: dict, rels: list, lang: str, saida_tsc: str
) -> None:
    """A base "limpa" so vale se o `tsc` REALMENTE viu os arquivos.

    Medido em 09/10 (onda 2): sem o `tsconfig.json` na sombra o `tsc` responde `TS5057: Cannot find
    a tsconfig.json`, devolve ZERO arquivos, e o gerador le isso como "linha de base limpa" -- o
    selftest passou a dizer 26/36 com `ocorreu: nenhum`. Um oraculo que nao olhou nada nao e
    oraculo: ausencia de sinal tem DUAS causas (a base esta limpa, ou o compilador nao rodou), e
    esta e a segunda. Erro alto, nunca um verde."""
    if not rels:
        raise RuntimeError(
            f"[{lang}] a varredura nao achou NENHUM arquivo .{EXT[lang]} na base -- base vazia "
            "ou filtro errado; isto NAO e 'base limpa'"
        )
    if any(c in saida_tsc for c in ("TS5057", "TS5083", "TS2688", "TS18003")):
        raise RuntimeError(
            f"[{lang}] o tsc ABORTOU sem analisar (tsconfig ou tipos faltando na sombra): a base "
            f"nao foi verificada. Saida: {saida_tsc.strip()[:200]}"
        )


def gera_codigo(
    lang: str, base: Path, sombra: Path, saida_lang: Path, n: int, log
) -> dict:
    """O tsc do PROJETO como oraculo: uma mutacao por item, em arquivo que roda hoje.

    `javascript` usa o oraculo do DELTA: a linha de base de `packages/` NAO e limpa (medido: 136
    arquivos com erro), e recusar o oraculo por isso zerava a linguagem. O DELTA exige exatamente um
    diagnostico novo, o esperado -- e isso acusa o mutado sem acusar o original."""
    erros_base, saida_tsc = roda_tsc(sombra)
    rels = [str(p.relative_to(sombra)) for p in varre(sombra, (SUFIXO[lang],))]
    confere_que_a_base_foi_verificada(erros_base, rels, lang, saida_tsc)
    por_delta = lang in LINGUAGENS_DELTA
    if erros_base and not por_delta:
        return _oraculo_recusado(erros_base)
    estado_base = "limpa" if not erros_base else f"suja em {len(erros_base)} arq"
    tipo_oraculo = "DELTA" if por_delta else "legado"
    log(
        f"[{lang}] linha de base {estado_base} (oraculo {tipo_oraculo}); "
        f"{len(rels)} arquivos .{EXT[lang]}"
    )
    itens, contadores = [], Counter()
    for classe, receita in sorted(OPERADORES[lang].items()):
        if classe in DESLIGADOS:
            contadores[f"{classe}:desligado_no_mutante"] += 1
            continue
        itens += um_item_de_classe(
            classe,
            receita,
            lang,
            sombra,
            base,
            saida_lang,
            rels,
            n,
            erros_base,
            contadores,
            log,
            por_delta=por_delta,
            por_arquivo=MUTACOES_POR_ARQUIVO.get(lang, 1),
            variacoes=(
                list(range(MUTACOES_POR_ARQUIVO.get(lang, 1)))
                if MUTACOES_POR_ARQUIVO.get(lang, 1) > 1
                else None
            ),
        )
        persistir(
            saida_lang,
            [e for e, _m in itens],
            {e["id"]: m for e, m in itens},
        )
    return {
        "entradas": [e for e, _m in itens],
        "manifesto": {e["id"]: m for e, m in itens},
        "descartes": dict(contadores),
        "por_classe": {k: v for k, v in contadores.items() if k.endswith(":n")},
        "n": len(itens),
        "erros_base": erros_base,
        "oraculo": "delta" if por_delta else "legado",
    }


def negativos_codigo(
    lang: str, base: Path, sombra: Path, saida_lang: Path, n: int, erros_base: dict
) -> dict:
    """Arquivos ORIGINAIS limpos pelo MESMO oraculo: o tsc nao acusa erro nenhum neles.

    Teto real medido em 09/10 para `javascript`: 8 arquivos `.js` de FONTE no projeto todo (fora
    de `dist/`, `target/`, `.cache/`, `node_modules/`), dos quais 3 tem linha de base limpa. O
    numero de negativo e o teto, declarado -- nunca preenchido por suposicao."""
    entradas = []
    for p in amostra(varre(sombra, (SUFIXO[lang],)), n * 3):
        if len(entradas) >= n:
            break
        rel = str(p.relative_to(sombra))
        if erros_base.get(rel):
            continue  # pre-existente: nao e negativo do motor
        ident = sha12(f"{lang}|negativo|{rel}")
        texto = le(p)
        snap_rel, _o = grava_item(saida_lang, ident, texto, texto, EXT[lang])
        entradas.append(
            entrada(
                ident,
                "negativo",
                "",
                "",
                lang,
                base / rel,
                base,
                snap_rel,
                "linha de base do projeto: 0 erros",
                reproduz="false",
            )
        )
    return {"entradas": entradas, "n": len(entradas), "teto": n}


# --------------------------------------------------------------------------- operadores de HTML
def recurso_local_existente(texto: str, doc_dir: Path) -> tuple | None:
    for tagname, _antes, _aspas, val in ATRIBUTO.findall(texto):
        if tagname.lower() not in TAGS_COM_RECURSO:
            continue
        if not val or val.startswith(
            ("#", "data:", "mailto:", "http://", "https://", "//")
        ):
            continue
        if (doc_dir / val.split("?")[0].split("#")[0]).resolve().is_file():
            return tagname, val
    return None


def m_recurso(texto: str, doc_dir: Path, ident: str) -> tuple | None:
    achado = recurso_local_existente(texto, doc_dir)
    if not achado:
        return None
    val = achado[1]
    # O TOKEN e a referencia sem query e sem fragmento; o SUFIXO e o que vem DEPOIS. MEDIDO em
    # 10/10: a versao anterior faceava `rpartition(".")` sobre o valor COMPLETO, entao uma query
    # caia dentro de `ext` e disparava `len(ext) > 5` -- a entrada era perdida sem que ninguem
    # soubesse por que. O grupo de captura devolve o suFIXO inteiro, com `?` ou `#`.
    partes = re.split(r"([?#].*)\Z", val, maxsplit=1)
    token = partes[0]
    sufixo = partes[1] if len(partes) > 1 else ""
    base, ponto, ext = token.rpartition(".")
    if not ponto or "/" in ext or len(ext) > 5:
        return None
    nome = nome_marcado("vgpAusente", ident)
    # MEDIDO em 10/10 (correcao da coordenadora, confirmada por medicao): `nome_esperado` era SO
    # o marcador, e o juiz compara por IGUALDADE -- entao o recall de `recurso` saia 0 POR
    # CONSTRUCAO do corpus. `cdn` ja usava a URL como escrita, `ancora` o id e `classe_css` a
    # classe; a convencao UNICA e "o token referenciado como esta no codigo".
    #
    # O valor e o atributo COMO ESCRITO (prefixo de caminho incluido), sem query e sem fragmento:
    # e o que o operador ve no `src=`/`href=` e o que o juiz casa por igualdade. Query e fragmento
    # sao parametros, nao o token -- includi-los quebraria o casamento de novo.
    #
    # A substituicao ancora no VALOR COMPLETO e repõe o sufixo: e o que mantem a query no documento
    # e o que garante que o trocado seja o atributo, nao uma coincidencia do token solto. A
    # unica mudanca no documento e o marcador -- que e o que a classe promete.
    escrito = f"{base}.{nome}.{ext}"
    return (
        texto.replace(val, escrito + sufixo, 1),
        escrito,
        "recurso",
        f"recurso local inexistente: {escrito} (o original {val} existe no disco)",
    )


def m_ancora(texto: str, ident: str, doc: Documento) -> tuple | None:
    """Aponta o `href="#alvo"` (ou o `for=` de um `<label>`) para um id que NAO existe.

    MEDIDO em 09/10 (dom-examples/color-manipulation.html): a versao anterior usava
    `texto.replace(f'"{alvo}"', ..., 1)`, que substitui a PRIMEIRA ocorrencia do nome entre aspas --
    e essa e o `id="original"` do elemento, nao o `href="#original"`. O resultado era o oposto do
    pretendido: o documento mutado deixava de APONTAR para a ancora, e `p_ancora` (que exige
    "aponta para o alvo E o alvo nao existe") reprovava o operador. Agora so o atributo CITADO
    (`href=` de `<a>`, `for=` de `<label>`) e trocado, e o `id` original fica no lugar -- que e o
    que mantem o resto do documento igual e faz o DELTA ser de um atributo so.
    """
    nome = nome_marcado("vgpAncoraAusente", ident)
    for alvo in doc.ancoras:
        if alvo not in doc.ids:
            continue
        # grupo 1 = `<a ... href="#` (o `#` FICA no grupo), grupo 2 = a aspa de abertura.
        # A aspa de fechamento e a ultima coisa casada, e volta no lugar. MEDIDO em 09/10: duas
        # versao erradas -- uma montava `href="vgp"` (sem `#`, e para de ser ancora) e outra
        # montava `href=vgp"` (sem a aspa de abertura). O par (1)+(2) ja carrega a abertura e o
        # `#`, entao a reconstrucao e `g(1) + nome + g(2)` -- nada de aspa extra.
        citedores = (
            re.compile(
                rf'(<a\b[^>]*?\shref\s*=\s*(["\'])#){re.escape(alvo)}(["\'])',
                re.IGNORECASE,
            ),
            re.compile(
                rf'(<label\b[^>]*?\sfor\s*=\s*(["\'])){re.escape(alvo)}(["\'])',
                re.IGNORECASE,
            ),
        )
        for rx in citedores:
            achado = rx.search(texto)
            if achado:
                return (
                    texto[: achado.start()]
                    + achado.group(1)
                    + nome
                    + achado.group(3)
                    + texto[achado.end() :],
                    nome,
                    "ancora",
                    f"id {nome!r} ausente no documento (o original #{alvo} existe)",
                )
    return None


def m_cdn(texto: str, cache: dict, ident: str, pin: bool):
    """`cdn`: versao exata trocada por uma que da 404. `cdn_pin`: versao rebaixada a range."""
    for url in CDN_URL.findall(texto):
        achado = VERSAO_NO_CDN.search(url)
        if not achado:
            continue
        ver = achado.group(2)
        partes = ver.split(".")
        nova = ".".join(partes[:2] + ["99"]) if not pin else partes[0] + "." + partes[1]
        novo_url = url.replace(f"@{ver}", f"@{nova}", 1)
        if novo_url == url:
            continue
        r = cabecalho(novo_url, cache)
        if pin:
            if r.get("verdict") != "existe":
                return None  # o pin tem de RESOLVER: so o rebaixamento e o defeito
            return (
                texto.replace(url, novo_url, 1),
                novo_url,
                "cdn_pin",
                f"versao nao exata @{nova} (a exata era @{ver}); resolve "
                f"(HTTP {r.get('status')}) -- aviso, nao negacao",
            )
        antes = cabecalho(url, cache)
        if r.get("verdict") != "404" or antes.get("verdict") != "existe":
            return None  # timeout NAO e 404; e o original tem de resolver
        return (
            texto.replace(url, novo_url, 1),
            novo_url,
            "cdn",
            f"HTTP 404 em {novo_url} (o original @{ver} devolve HTTP {antes.get('status')})",
        )
    return None


def m_classe_css(texto: str, ident: str):
    definidas = {m.group(1) for m in CLASSE_DEF.finditer(texto)}
    achado = CLASSES_USADAS.search(texto)
    if not definidas or not achado:
        return None
    valor = achado.group(2)
    trocada = next((c for c in valor.split() if c in definidas), "")
    if not trocada:
        return None
    nova = trocada + "q"
    if f".{nova}" in texto:
        return None
    mutado = texto[: achado.start()] + texto[achado.start() :].replace(
        valor, valor.replace(trocada, nova), 1
    )
    return (
        mutado,
        nova,
        "classe_css",
        f"classe {nova!r} nao e definida em nenhuma folha carregada "
        f"(a original {trocada!r} e, no style inline)",
    )


def refs_locais(texto: str) -> list:
    return [
        v
        for _t, _a, _q, v in ATRIBUTO.findall(texto)
        if v and not v.startswith(("http", "//", "#", "data:", "mailto:"))
    ]


def classes_do_doc(texto: str) -> set:
    achados = CLASSES_USADAS.findall(texto)
    return {c for grupo in achados for c in grupo[1].split() if c}


def ancoras_do(texto: str) -> set:
    d = monta_doc(texto)
    return set(d.ancoras) if d else set()


def cdns_do(texto: str) -> set:
    return set(CDN_URL.findall(texto))


def p_recurso(nome, ctx) -> str:
    """O documento USA um recurso local com esse nome e ele NAO existe no disco.

    MEDIDO em 10/10: comparava o valor do atributo INTEIRO. Com query ou fragmento
    (`src="a/um.png?v=2"`), `(dir / v).exists()` e False mesmo quando `a/um.png` existe -- e o
    oraculo acusaria o ORIGINAL tambem, que e o sinal de que ele nao e oraculo. Agora compara o
    TOKEN (sem query e sem fragmento), a mesma coisa que `recurso_local_existente` existence-testa.
    """
    for v in refs_locais(ctx["mutado"]):
        token = re.split(r"[?#]", v, maxsplit=1)[0]
        if nome in token and not (ctx["dir"] / token).exists():
            return ""
    return "nenhum recurso local com esse nome esta quebrado no mutado"


def p_ancora(nome, ctx) -> str:
    """O documento APONTA para #nome e o id nome nao existe nele."""
    if nome not in ancoras_do(ctx["mutado"]):
        return "o documento mutado nem aponta para essa ancora"
    return "o id existe no documento mutado" if nome in ids_do(ctx["mutado"]) else ""


def p_classe_css(nome, ctx) -> str:
    """O documento USA a classe e nenhuma folha carregada a define."""
    if nome not in classes_do_doc(ctx["mutado"]):
        return "o documento mutado nem usa essa classe"
    return "a classe esta definida" if f".{nome}" in ctx["mutado"] else ""


def p_cdn(nome, ctx) -> str:
    """O documento usa a URL e ela devolve 404."""
    if nome not in cdns_do(ctx["mutado"]):
        return "o documento mutado nem usa essa URL"
    r = cabecalho(nome, ctx["cache"])
    return "" if r.get("verdict") == "404" else f"o CDN nao confirmou 404: {r}"


def p_cdn_pin(nome, ctx) -> str:
    """A URL resolve, mas nao tem versao exata."""
    if nome not in cdns_do(ctx["mutado"]):
        return "o documento mutado nem usa essa URL"
    if cabecalho(nome, ctx["cache"]).get("verdict") != "existe":
        return "a URL do pin nao resolve"
    achado = VERSAO_NO_CDN.search(nome)
    return "a versao ainda e exata" if achado else ""


PREDICADOS = {
    "recurso": p_recurso,
    "ancora": p_ancora,
    "classe_css": p_classe_css,
    "cdn": p_cdn,
    "cdn_pin": p_cdn_pin,
}


def oraculo_html(classe: str, nome: str, ctx: dict) -> str:
    """(vazio = CONFIRMA). Cada ramo le o DOCUMENTO: e o que separa o mutado do original."""
    pred = PREDICADOS.get(classe)
    return "classe sem oraculo: " + classe if pred is None else pred(nome, ctx)


def opera_html(
    texto: str,
    caminho: Path,
    ident: str,
    cache: dict,
    doc: Documento,
    saturadas: set | None = None,
):
    """Os cinco operadores, por ordem; devolve o 1o que o oraculo CONFIRMAR.

    `saturadas` = classes que ja bateram o alvo. Medido em 09/10: sem isso, `ancora` fechava em
    10/20 porque quase todo documento com ancora mutavel tem TAMBEM recurso local, `recurso` e
    testado primeiro, e o documento inteiro era descartado por `recurso:saturado` -- levando
    junto a ancora, a classe_css e o cdn que o mesmo documento poderia ter provado. Uma fonte pode
    servir mais de uma classe, desde que cada operador tenha a SUA mutacao e o SEU oraculo."""
    doc_dir = caminho.parent
    candidatos = (
        ("recurso", lambda: m_recurso(texto, doc_dir, ident)),
        ("ancora", lambda: m_ancora(texto, ident, doc)),
        (
            "classe_css",
            lambda: (
                m_classe_css(texto, ident) if folha_resolvivel(doc, texto) else None
            ),
        ),
        ("cdn", lambda: m_cdn(texto, cache, ident, pin=False)),
        ("cdn_pin", lambda: m_cdn(texto, cache, ident, pin=True)),
    )
    ctx = {"dir": doc_dir, "cache": cache, "mutado": ""}
    for classe, fabrica in candidatos:
        if classe in DESLIGADOS or (saturadas and classe in saturadas):
            continue
        m = fabrica()
        if not m:
            continue
        mutado, nome, classe_real, msg = m
        ctx["mutado"] = mutado
        if oraculo_html(classe_real, nome, ctx):
            continue
        return mutado, nome, classe_real, msg
    return None


def recurso_quebrado(texto: str, doc_dir: Path) -> str:
    """O mesmo criterio de `recurso_local_existente`: SO tags que carregam recurso.

    Medido em 09/10: sem o filtro de tag, esta funcao acusava `<!DOCTYPE html lang=...>` e
    `<html lang="pt-BR">` como recurso inexistente -- 200 de 200 documentos -- porque o regex de
    atributos casa qualquer tag com atributo entre aspas. O filtro de negativo ficava assim
    sempre vazio, e com ele o falso deny de HTML, que e o que o negativo existe para medir."""
    for tagname, _antes, _aspas, val in ATRIBUTO.findall(texto):
        if tagname.lower() not in TAGS_COM_RECURSO:
            continue
        if not val or val.startswith(("http", "//", "#", "data:", "mailto:")):
            continue
        if not (doc_dir / val.split("?")[0].split("#")[0]).resolve().is_file():
            return val
    return ""


def ancora_quebrada(doc: Documento) -> str:
    return next((a for a in doc.ancoras if a not in doc.ids), "")


def gera_html(saida_lang: Path, n: int, cache: dict, log) -> dict:
    itens, descartes, por_classe = [], Counter(), Counter()
    docs = varre(ANALISE, SUFIXO["html"])
    log(f"[html] {len(docs)} documentos candidatos")
    for caminho in docs:
        texto = le(caminho)
        doc = monta_doc(texto)
        if doc is None:
            descartes["parse_falhou"] += 1
            continue
        achado = opera_html(
            texto,
            caminho,
            sha12(f"html|{caminho}"),
            cache,
            doc,
            saturadas={c for c, k in por_classe.items() if k >= n},
        )
        if not achado:
            descartes["sem_mutacao_confirmada"] += 1
            continue
        mutado, nome, classe, msg = achado
        if por_classe[classe] >= n:
            descartes[f"{classe}:saturado"] += 1
            continue
        ident = sha12(f"html|{classe}|{caminho}|{nome}")
        snap_rel, snap_orig = grava_item(saida_lang, ident, mutado, texto, EXT["html"])
        itens.append(
            (
                entrada(
                    ident,
                    "positivo",
                    classe,
                    nome,
                    "html",
                    caminho,
                    ANALISE,
                    snap_rel,
                    msg,
                ),
                {"disk_file": snap_orig, "overlays": []},
            )
        )
        por_classe[classe] += 1
        if len(itens) % 25 == 0:
            persistir(
                saida_lang,
                [e for e, _m in itens],
                {e["id"]: m for e, m in itens},
            )
    log(f"[html] classes: {dict(por_classe)}")
    return {
        "entradas": [e for e, _m in itens],
        "manifesto": {e["id"]: m for e, m in itens},
        "descartes": dict(descartes),
        "por_classe": dict(por_classe),
        "n": len(itens),
    }


def negativos_html(saida_lang: Path, n: int, log) -> dict:
    """Documentos que passam nos oraculos de recurso, ancora e classe: a regua e o mesmo codigo."""
    entradas, descartes = [], Counter()
    for caminho in varre(ANALISE, SUFIXO["html"]):
        if len(entradas) >= n:
            break
        texto = le(caminho)
        doc = monta_doc(texto)
        if doc is None:
            descartes["parse_falhou"] += 1
            continue
        if not folha_resolvivel(doc, texto):
            descartes["folha_nao_resolvivel"] += 1
            continue
        if recurso_quebrado(texto, caminho.parent):
            descartes["recurso_quebrado"] += 1
            continue
        if ancora_quebrada(doc):
            descartes["ancora_quebrada"] += 1
            continue
        ident = sha12(f"html|negativo|{caminho}")
        snap_rel, _o = grava_item(saida_lang, ident, texto, texto, EXT["html"])
        entradas.append(
            entrada(
                ident,
                "negativo",
                "",
                "",
                "html",
                caminho,
                ANALISE,
                snap_rel,
                "recurso, ancora, classe e folha conferidos",
                reproduz="false",
            )
        )
    log(f"[html] negativos: {len(entradas)} - descartes {dict(descartes)}")
    return {"entradas": entradas, "n": len(entradas), "descartes": dict(descartes)}


# --------------------------------------------------------------------------- selftest
FIXTURE = (
    tag("html", "")
    + tag("head", "")
    + tag("style", "")
    + ".cartao { color: red }"
    + tag("/style", "")
    + tag("a", ' href="#alvo"')
    + "i"
    + tag("/a", "")
    + tag("div", ' id="alvo" class="cartao"')
    + "x"
    + tag("/div", "")
    + tag("script", ' src="./existe.js"')
    + tag("/script", "")
    + tag("/body", "")
    + tag("/html", "")
)
FIXTURE_CDN = FIXTURE.replace(
    ' src="./existe.js"',
    ' src="https://cdn.jsdelivr.net/npm/react@18.3.1/umd/react.production.min.js"',
)


def fabrica_html(classe: str, texto: str, ident: str, dir_ex: Path, cache: dict, doc):
    if classe == "recurso":
        return lambda: m_recurso(texto, dir_ex, ident)
    if classe == "ancora":
        return lambda: m_ancora(texto, ident, doc if doc is not None else Documento())
    if classe == "classe_css":
        return lambda: m_classe_css(texto, ident)
    if classe in ("cdn", "cdn_pin"):
        return lambda: m_cdn(texto, cache, ident, pin=(classe == "cdn_pin"))
    raise KeyError(classe)


def checa_operador_html(classe: str, cache: dict, dir_ex: Path) -> tuple:
    """Um operador de HTML: o oraculo acusa o MUTADO e NAO o original."""
    (dir_ex / "existe.js").write_text("// stub\n", encoding="utf-8")
    texto = FIXTURE_CDN if classe in ("cdn", "cdn_pin") else FIXTURE
    ident = "aa11bb22cc33"
    m = fabrica_html(classe, texto, ident, dir_ex, cache, monta_doc(texto))()
    if not m:
        return False, "a receita nao produziu mutacao no fixture"
    mutado, nome, classe_real, _msg = m
    if classe_real != classe:
        return False, f"a receita produziu {classe_real}, esperava {classe}"
    ctx = {"dir": dir_ex, "cache": cache, "mutado": mutado}
    if oraculo_html(classe, nome, ctx):
        return False, "o oraculo NAO acusou o mutado"
    ctx["mutado"] = texto
    if oraculo_html(classe, nome, ctx) == "":
        return False, "o oraculo acusou o ORIGINAL -- nao e oraculo, sao dois lados"
    return True, f"acusou o mutado e nao o original ({classe})"


def arquivo_de_teste(sombra: Path) -> str:
    """Um .tsx real do projeto: a mutacao tem de acontecer em codigo que roda hoje."""
    cands = sorted(
        p
        for p in sombra.rglob("*.tsx")
        if "node_modules" not in p.parts and p.stat().st_size > 400
    )
    return str(cands[0].relative_to(sombra))


def chave_codigo(classe: str) -> str:
    return f"ts/tsx: {classe} - o tsc acusa o mutado, e o original estava limpo"


def chave_html(classe: str) -> str:
    return f"html: {classe} - o oraculo acusa o mutado e nao o original"


# O juiz casa `nome_esperado` por IGUALDADE com o token que o motor le do codigo. Uma entrada de
# `recurso` com SO o marcador em `nome_esperado` nunca casa -- e o recall da classe sai 0 sem que
# o motor tenha errado. MEDIDO em 10/10 em `corpus_semeado/html`: 20/20 com so o marcador, contra
# 0/20 em `cdn` (que usa a URL como escrita). Esta e a checagem de CONTRATO que trava isso.
CONVENCAO_RECURSO = "html: recurso - nome_esperado e o token referenciado como escrito, nao o marcador sozinho"


def checagens_codigo(
    sombra: Path,
    rel: str,
    original: str,
    erros_base: dict,
    log,
    apenas: set | None = None,
) -> dict:
    checks = {}
    for classe, receita in sorted(OPERADORES["tsx"].items()):
        if apenas is not None and classe not in apenas:
            continue
        chave = chave_codigo(classe)
        if classe in DESLIGADOS:
            checks[chave] = False  # o mutante: sem operador, a checagem nao pode passar
            continue
        ident = sha12(f"selftest|{classe}")
        entrou, motivo, _m, _n = muta_e_confere(
            sombra, rel, original, receita, ident, erros_base
        )
        checks[chave] = entrou
        if not entrou:
            log(f"    FALHOU {classe}: {motivo}")
    return checks


def _controles_delta(n_lin: int) -> dict:
    """Dois mutados que o oraculo tem de RECUSAR, mais o mutante do proprio oraculo.

    O mutante e o teste que o brief exige: um oraculo que aceita QUALQUER diagnostico novo tem de
    reprovar. Aqui ele e provado defeituoso de duas maneiras -- aceita o codigo errado e aceita o
    codigo na linha errada -- e o oraculo verdadeiro reprova os dois. Nenhum teste normal pegaria
    isso: so um oraculo mutado, exercitado contra um mutado errado, pega."""
    nome, cod = "vgpAusente", "TS2304"
    base_limpa: list = []
    cod_errado = [{"cod": "TS2339", "lin": n_lin, "msg": f"algo sobre {nome}"}]
    linha_errada = [{"cod": cod, "lin": n_lin - 3, "msg": f"Cannot find name {nome}"}]
    certo = [{"cod": cod, "lin": n_lin, "msg": f"Cannot find name {nome}"}]

    vero_cod = bool(confere_delta(base_limpa, cod_errado, cod, nome, n_lin))
    vero_lin = bool(confere_delta(base_limpa, linha_errada, cod, nome, n_lin))
    aceito = not confere_delta(base_limpa, certo, cod, nome, n_lin)
    lax_cod = lax_lin = "contexto nao entrou: o mutante nem chegou a ser exercitado"
    with DeltaLaxo(False):
        lax_cod = confere_delta(base_limpa, cod_errado, cod, nome, n_lin)
        lax_lin = confere_delta(base_limpa, linha_errada, cod, nome, n_lin)
    return {
        "delta: controle positivo - o oraculo aceita o diagnostico esperado": aceito,
        "delta: o oraculo REPROVA codigo errado": vero_cod,
        "delta: o oraculo REPROVA linha errada": vero_lin,
        "mutante do oraculo delta: o laxo ACEITA codigo errado (reprovado)": lax_cod
        == "",
        "mutante do oraculo delta: o laxo ACEITA linha errada (reprovado)": lax_lin
        == "",
    }


def checagens_delta(sombra_js: Path, log) -> dict:
    """O oraculo do DELTA na linguagem js de verdade (base suja: 2 `tsc` por operador)."""
    checks: dict = {}
    erros_base, _saida_js = roda_tsc(sombra_js)
    rels = [
        str(p.relative_to(sombra_js)) for p in varre(sombra_js, (SUFIXO["javascript"],))
    ]
    log(f"[delta] js: base com {len(erros_base)} arquivo(s) com erro, {len(rels)} .js")
    if not rels:
        return {"delta: ha .js para mutar": False}
    rel = rels[0]
    original = le(sombra_js / rel)
    for classe, receita in sorted(OPERADORES["javascript"].items()):
        chave = f"delta/js: {classe} - exatamente 1 diagnostico novo, o esperado"
        if classe in DESLIGADOS:
            checks[chave] = False
            continue
        ident = sha12(f"delta|{classe}")
        entrou, motivo, _mutado, _n = muta_e_confere(
            sombra_js, rel, original, receita, ident, erros_base, por_delta=True
        )
        checks[chave] = entrou
        if not entrou:
            log(f"    FALHOU delta/{classe}: {motivo}")
    checks.update(_controles_delta(linha_da_mutacao(original, "")))
    return checks


def confere_convencao_recurso(cache: dict, tmp: Path, log) -> bool:
    """`nome_esperado` de `recurso` e o token REFERENCIADO, nao o marcador sozinho.

    MEDIDO em 10/10 (`scratch/mapeia_nome_esperado.py`): `recurso` tinha 20/20 com SO o marcador em
    `nome_esperado`, enquanto `cdn` usava a URL como escrita (0/20 so marcador) e `classe_css` a
    classe. O juiz compara por IGUALDADE, entao o recall de `recurso` saia 0 POR CONSTRUCAO do
    corpus -- o corpus era a causa, nao o motor.

    A checagem expoe TRES condicoes, e o MUTANTE (voltar ao marcador) reprova a primeira:
      1. `nome_esperado` NAO pode ser so o marcador;
      2. tem de aparecer no atributo do documento mutado (a referencia como esta, com o prefixo
         de caminho), sem query e sem fragmento;
      3. o recurso tem de estar quebrado no mutado e existir no disco -- o par dos dois lados,
         que e o que a classe promete.
    """
    (tmp / "existe.js").write_text("// stub\n", encoding="utf-8")
    texto = FIXTURE
    ident = "aa11bb22"
    m = fabrica_html("recurso", texto, ident, tmp, cache, monta_doc(texto))()
    if not m:
        log("    FALHOU convencao/recurso: a receita nao produziu mutacao no fixture")
        return False
    mutado, nome, _classe, _motivo = m
    marcador = nome_marcado("vgpAusente", ident)
    if nome == marcador:
        log(
            f"    FALHOU convencao/recurso: nome_esperado e SO o marcador ({nome}) -- "
            "o juiz compara por igualdade e isso nunca casa"
        )
        return False
    if "?" in nome or "#" in nome:
        log(
            f"    FALHOU convencao/recurso: nome_esperado carrega query ou fragmento: {nome}"
        )
        return False
    if nome not in mutado:
        log(f"    FALHOU convencao/recurso: {nome} nao esta no atributo do mutado")
        return False
    if oraculo_html("recurso", nome, {"dir": tmp, "cache": cache, "mutado": mutado}):
        log(f"    FALHOU convencao/recurso: o oraculo nao acusou o token {nome}")
        return False
    if (
        oraculo_html("recurso", nome, {"dir": tmp, "cache": cache, "mutado": texto})
        == ""
    ):
        log("    FALHOU convencao/recurso: o oraculo acusou o ORIGINAL")
        return False
    return confere_convencao_recurso_com_query(cache, tmp, log)


def confere_convencao_recurso_com_query(cache: dict, tmp: Path, log) -> bool:
    """O caso que o `FIXTURE` nao cobre: referencia COM query e fragmento.

    O `FIXTURE` tem `src="./existe.js"` -- prefixo curto, sem query. A convencao pedida e sobre a
    referencia COMPLETA (o prefixo de caminho entra; a query e o fragmento saem), entao um caso sem
    query e um caso com prefixo curto nao provam o que precisa ser provado.

    MEDIDO em 10/10 sobre um `.png` real de disco: `(dir / "recurso.png?v=2").exists()` e False e
    `(dir / "recurso.png").exists()` e True. E por isso que o TOKEN -- e nao o valor -- vai para
    `nome_esperado`, e e o token que o oraculo existence-testa.
    """
    sub = tmp / "shared"
    sub.mkdir(exist_ok=True)
    (sub / "recurso.png").write_text("stub\n", encoding="utf-8")
    ident = "cc33dd44"
    texto = (
        tag("html", "")
        + tag("body", "")
        + tag("img", ' src="shared/recurso.png?v=2#grade"')
        + tag("/body", "")
        + tag("/html", "")
    )
    m = fabrica_html("recurso", texto, ident, tmp, cache, monta_doc(texto))()
    if not m:
        log("    FALHOU convencao/recurso+query: a receita nao produziu mutacao")
        return False
    mutado, nome, _classe, _motivo = m
    marcador = nome_marcado("vgpAusente", ident)
    if nome == marcador or not nome.startswith("shared/"):
        log(
            "    FALHOU convencao/recurso+query: nome_esperado nao e o token referenciado como "
            f"escrito (veio {nome}, marcador {marcador})"
        )
        return False
    if "?" in nome or "#" in nome:
        log(f"    FALHOU convencao/recurso+query: query ou fragmento vazou para {nome}")
        return False
    if f'src="{nome}?v=2#grade"' not in mutado:
        log(
            f'    FALHOU convencao/recurso+query: o atributo mutado nao tem src="{nome}?v=2#grade"'
        )
        return False
    ctx = {"dir": tmp, "cache": cache, "mutado": mutado}
    if oraculo_html("recurso", nome, ctx):
        log(f"    FALHOU convencao/recurso+query: o oraculo nao acusou o token {nome}")
        return False
    if oraculo_html("recurso", nome, {**ctx, "mutado": texto}) == "":
        log(
            "    FALHOU convencao/recurso+query: o oraculo acusou o ORIGINAL (a query mascara o disco)"
        )
        return False
    return True


def checagens_html(cache: dict, tmp: Path, log, apenas: set | None = None) -> dict:
    checks = {}
    for classe in ("recurso", "ancora", "classe_css", "cdn", "cdn_pin"):
        if apenas is not None and classe not in apenas:
            continue
        chave = chave_html(classe)
        if classe in DESLIGADOS:
            checks[chave] = False
            continue
        ok, msg = checa_operador_html(classe, cache, tmp)
        checks[chave] = ok
        if not ok:
            log(f"    FALHOU {classe}: {msg}")
    # A convencao do `nome_esperado` e uma checagem de CONTRATO do corpus, nao do operador: ela
    # vale mesmo com `recurso` desligado no mutante (o mutante tem de reprovar o operador, e nao
    # esta checagem). Por isso fica fora do laco de `DESLIGADOS`.
    checks[CONVENCAO_RECURSO] = confere_convencao_recurso(cache, tmp, log)
    return checks


def checagem_mutantes(
    sombra: Path, rel: str, original: str, erros_base: dict, cache: dict, tmp: Path, log
) -> list:
    """O mutante que DESLIGA cada operador tem de reprovar o selftest -- senao ele nao discrimina.
    O mutante passa pelo MESMO filtro da geracao (da nao contorna-lo chamando a receita crua)."""
    saida = []
    for classe in sorted(OPERADORES["tsx"]) + [
        "recurso",
        "ancora",
        "classe_css",
        "cdn",
        "cdn_pin",
    ]:
        DESLIGADOS.clear()
        DESLIGADOS.add(classe)
        if classe in OPERADORES["tsx"]:
            res = checagens_codigo(
                sombra, rel, original, erros_base, log, apenas={classe}
            )
            chave = chave_codigo(classe)
        else:
            res = checagens_html(cache, tmp, log, apenas={classe})
            chave = chave_html(classe)
        saida.append((classe, res.get(chave) is False))
    DESLIGADOS.clear()
    return saida


def checagem_mutantes_delta(sombra_js: Path, log) -> list:
    """Um mutante por operador no caminho do DELTA: desligar a classe tem de reprovar a checagem.

    Passa pelo MESMO filtro da geracao (via `checagens_delta`), nao pela receita crua -- o contorno
    que a analise-68 ja registrou como defeito proprio."""
    saida = []
    for classe in sorted(OPERADORES["javascript"]):
        DESLIGADOS.clear()
        DESLIGADOS.add(classe)
        res = checagens_delta(sombra_js, log)
        # Os controles do oraculo nao dependem da classe: sao do `confere_delta`, nao do operador.
        for k, v in res.items():
            if k.startswith("delta/js: ") and classe in k:
                saida.append((classe, v is False))
                break
        else:
            saida.append((classe, False))
            log(f"    mutante delta/{classe}: a checagem nem apareceu")
    DESLIGADOS.clear()
    return saida


def prova_do_hardlink(log) -> bool:
    """A sombra por hardlink NUNCA pode alterar o arquivo real. Medido, nao presumido.

    Exigido pela coordenadora (onda 2, 09/10): "garanta no codigo que todo arquivo mutado e
    desligado antes de ser escrito". Este teste faz a mutacao de verdade e compara o sha256 do
    ORIGINAL antes e depois. O MUTANTE (escrita in-place, sem `unlink`) tem de reprovar -- senao
    a garantia nao esta provada, so afirmada."""
    import hashlib
    import os as _os
    import tempfile as _tf

    raiz = Path(
        _tf.mkdtemp(
            prefix="vgp-hardlink-",
            dir=_os.environ.get(
                "VGP_SEMEADO_TMP", str(Path.home() / ".cache" / "vgp-semeado")
            ),
        )
    )
    (raiz / "a").mkdir()
    real = raiz / "a" / "x.ts"
    original_de_teste = "const original = 1;\n"
    real.write_text(original_de_teste, encoding="utf-8")
    sombra = raiz / "b"
    # `cp -al`, e NAO `shutil.copytree`: MEDIDO em 09/10 que o copytree faz COPIA REAL (inode
    # diferente), e um teste de "a escrita altera o original" sobre arquivos que NAO compartilham
    # inode prova nada -- passaria mesmo com o `escreve_isolado` removido. O teste so vale se a
    # premissa (inode compartilhado) for VERIFICADA, e e verificada logo abaixo.
    subprocess.run(["cp", "-al", str(raiz / "a"), str(sombra)], check=True)
    espelho = sombra / "x.ts"
    mesmo_inode = espelho.stat().st_ino == real.stat().st_ino
    sha_antes = hashlib.sha256(real.read_bytes()).hexdigest()

    # O caminho BOM: escrita in-place. Numa sombra de VERDADE (inode compartilhado) isso altera o
    # arquivo real -- e por isso o mutante tem de ser REPROVADO por este teste.
    with open(espelho, "w", encoding="utf-8") as f:
        f.write("const MUTADO = 2;\n")
    sha_depois_do_mutante = hashlib.sha256(real.read_bytes()).hexdigest()
    mutante_altera_original = sha_antes != sha_depois_do_mutante
    # restaura o ORIGINAL de verdade: o mutante alterou o inode compartilhado, entao
    # reescrever o espelho nao traria o original de volta.
    real.write_text(original_de_teste, encoding="utf-8")
    espelho.unlink(missing_ok=True)
    subprocess.run(["cp", "-al", str(real), str(espelho)], check=True)

    # o caminho CORRETO: `escreve_isolado` desliga antes de escrever
    escreve_isolado(espelho, "const MUTADO = 3;\n")
    sha_depois = hashlib.sha256(real.read_bytes()).hexdigest()
    preserva = sha_antes == sha_depois
    inode_diferente = espelho.stat().st_ino != real.stat().st_ino
    log(
        f"    hardlink: inode compartilhado={mesmo_inode} | mutante(in-place) alterou o "
        f"original={mutante_altera_original} (tem de ser True, senao o mutante nao discrimina) | "
        f"escreve_isolado preservou={preserva} | inode novo={inode_diferente}"
    )
    # passa quando: a premissa (inode compartilhado) vale, o MUTANTE altera o original -- e por
    # isso tem de ser rejeitado --, e o caminho CORRETO nao altera nada.
    return bool(
        mesmo_inode and mutante_altera_original and preserva and inode_diferente
    )


def selftest(log) -> int:
    cache = carrega_cache(SAIDA_PADRAO)
    tmp = Path(tempfile.mkdtemp(prefix="vgp-selftest-"))
    sombra = prepara_sombra("typescript")[1]
    ini = time.time()
    erros_base, saida_tsc = roda_tsc(sombra)
    # A guarda que faltava (medido: sem ela, `tsc` sem tsconfig = "base limpa" = 26/36).
    rels_st = [str(p.relative_to(sombra)) for p in varre(sombra, (SUFIXO["tsx"],))]
    confere_que_a_base_foi_verificada(erros_base, rels_st, "selftest", saida_tsc)
    checks: dict = {}
    checks["controle positivo: a linha de base do projeto tem 0 erros"] = not erros_base
    checks[
        "a mutacao na sombra NAO altera o arquivo real (mutante in-place reprovado)"
    ] = prova_do_hardlink(log)
    log(
        f"selftest: linha de base {'limpa' if not erros_base else 'SUJA'} "
        f"({len(erros_base)} arquivos com erro)"
    )
    rel = arquivo_de_teste(sombra)
    original = le(sombra / rel)
    checks.update(checagens_codigo(sombra, rel, original, erros_base, log))
    checks.update(checagens_html(cache, tmp, log))
    # O oraculo do DELTA, na linguagem que tem a base suja: e o unico caminho que ainda gera
    # positivo para ela. A sombra e a de `packages/`, separada da do TS.
    _base_js, sombra_js = prepara_sombra("javascript")
    checks.update(checagens_delta(sombra_js, log))
    guarda_cache(SAIDA_PADRAO, cache)
    mutantes = checagem_mutantes(sombra, rel, original, erros_base, cache, tmp, log)
    mutantes += checagem_mutantes_delta(sombra_js, log)
    for k, v in checks.items():
        print(f"  {'ok  ' if v else 'FALHA'} {k}")
    for classe, pego in mutantes:
        print(
            f"  {'ok  ' if pego else 'FALHA'} mutante que desliga {classe}: "
            f"{'reprovado aqui' if pego else 'PASSOU - o selftest nao discrimina'}"
        )
    total = len(checks) + len(mutantes)
    passou = sum(checks.values()) + sum(1 for _c, p in mutantes if p)
    print(
        f"selftest: {passou}/{total} em {time.time() - ini:.0f}s - sombra em {sombra} "
        f"(NAO removida: exclusao recursiva e decisao do humano)"
    )
    return 0 if passou == total else 1


# --------------------------------------------------------------------------- main
TSCONFIG_JS = json.dumps(
    {
        "compilerOptions": {
            "target": "ES2022",
            "module": "ESNext",
            "moduleResolution": "bundler",
            "allowJs": True,
            "checkJs": True,
            "noEmit": True,
            "skipLibCheck": True,
            "allowImportingTsExtensions": True,
            "strict": False,
        },
        "include": ["**/*.js", "**/*.mjs"],
    },
    indent=1,
)


BASE_JS = Path.home() / "projects"


def base_da(lang: str) -> Path:
    """De onde vem cada linguagem.

    `javascript` (medido em 09/10, onda 2, tarefa C): a varredura de `~/projects` inteiro, com
    dedup por sha e sem codigo de terceiro, achou **96** `.js` de fonte em 4 projetos --
    `analise` 76, `konverter` 14, `simulador-urna-rs-2026` 2, `touring` 4 -- dos quais **11**
    passam o `tsc --checkJs` sem diagnostico.

    Duas armadilhas que inflavam a contagem e foram removidas: (1) `touring-wt-*` sao o MESMO repo
    em 5 diretorios (mesmo sha256) -- deduplicar por caminho contaria 5x; (2) `third_party/
    tree-sitter-*/grammar.js` (77 KB, MIT, de terceiros) e codigo proibido para semear.

    A base do JS e `~/projects`: a sombra recebe o `tsconfig` de `checkJs` na raiz."""
    if lang == "javascript":
        return BASE_JS
    return FRONTEND if (FRONTEND / "tsconfig.json").is_file() else PORTAL


def prepara_sombra(lang: str) -> tuple:
    """(base real, sombra pronta para o tsc).

    Para `javascript` a base e `~/projects` INTEIRO, e espelhar tudo por hardlink falha (medido:
    "Operation not permitted" em binarios de vector store). A sombra espelha apenas os arquivos
    que a varredura de fonte encontra -- o `tsc` ve o mesmo conjunto de arquivos que o motor vera,
    e o que nao esta na sombra e, por construcao, fonte inexistente.

    O `tsconfig.json` da base e REESCRITO na sombra, nunca espelhado: para `javascript` ele vira o
    `TSCONFIG_JS` (checkJs), e para as demais ele e copiado do original. Sem isso o `tsc` responde
    `TS5057: Cannot find a tsconfig.json` e devolve ZERO arquivos -- que e a assinatura de uma
    base "limpa" que nao foi verificada (medido em 09/10, onda 2: o selftest caiu para 26/36)."""
    base = base_da(lang)
    apenas = None
    if lang == "javascript":
        apenas = [str(p.relative_to(base)) for p in varre(base, (".js", ".mjs"))]
    sombra = sombra_de(base, dir_sombra(base), apenas=apenas)
    tsconfig_origem = base / "tsconfig.json"
    if lang == "javascript":
        texto = TSCONFIG_JS
    elif tsconfig_origem.is_file():
        texto = tsconfig_origem.read_text(encoding="utf-8", errors="replace")
    else:
        texto = ""
    # `escreve_isolado` desliga antes de gravar: escrever no inode do espelho hardlinkado
    # alteraria o ARQUIVO REAL do projeto (medido e exigido pela coordenadora, onda 2).
    escreve_isolado(sombra / "tsconfig.json", texto)
    return base, sombra


def gera(saida: Path, langs: tuple, n: int, n_neg: int, cache: dict, log) -> dict:
    relatorio: dict = {"semente": SEMENTE, "langs": {}}
    for lang in langs:
        saida_lang = saida / lang
        if lang == "html":
            pos = gera_html(saida_lang, n, cache, log)
            neg = negativos_html(saida_lang, n_neg, log)
        else:
            base, sombra = prepara_sombra(lang)
            pos = gera_codigo(lang, base, sombra, saida_lang, n, log)
            neg = negativos_codigo(
                lang, base, sombra, saida_lang, n_neg, pos.get("erros_base", {})
            )
        persistir(
            saida_lang,
            pos.get("entradas", []),
            pos.get("manifesto", {}),
            neg.get("entradas", []),
        )
        relatorio["langs"][lang] = {
            "positivos": pos.get("n", 0),
            "negativos": neg.get("n", 0),
            "por_classe": pos.get("por_classe", {}),
            "descartes": pos.get("descartes", {}),
            "aviso": pos.get("aviso", ""),
        }
        log(f"[{lang}] positivos={pos.get('n', 0)} negativos={neg.get('n', 0)}")
    return relatorio


def sha_por_lang(saida: Path, langs: tuple) -> dict:
    return {
        lang: hashlib.sha256(
            (saida / lang / "entradas.jsonl").read_bytes()
        ).hexdigest()[:16]
        for lang in langs
        if (saida / lang / "entradas.jsonl").is_file()
    }


def tmp_gravavel(prefixo: str) -> Path:
    """Temp do sandbox: `VGP_SEMEADO_TMP` quando dentro dele, `~/.cache` fora (nunca /tmp,
    que alem de outro dispositivo e escrita-negada no sandbox)."""
    raiz = Path(
        os.environ.get("VGP_SEMEADO_TMP", str(Path.home() / ".cache" / "vgp-semeado"))
    )
    raiz.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix=prefixo, dir=str(raiz)))


def main() -> int:
    ap = argparse.ArgumentParser(description="Corpus semeado do VGP (TS/TSX/JS/HTML)")
    ap.add_argument("--lang", default="todas", choices=[*LINGUAGENS, "todas"])
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--confere-determinismo", action="store_true")
    ap.add_argument("--n", type=int, default=ALVO_POR_LANG)
    ap.add_argument("--negativos", type=int, default=NEGATIVOS_POR_LANG)
    ap.add_argument("--saida", type=Path, default=SAIDA_PADRAO)
    ap.add_argument("--relatorio", type=Path)
    a = ap.parse_args()

    def log(msg):
        print(msg, flush=True)

    if a.selftest:
        return selftest(log)
    langs = LINGUAGENS if a.lang == "todas" else (a.lang,)
    ini = time.time()
    cache = carrega_cache(a.saida)
    if a.confere_determinismo:
        # Duas geracoes INDEPENDENTES, cada uma em seu diretorio: o veredito nao depende de um
        # corpus pre-existente (e o cache do CDN entra nas duas, senao a rede decide o sha).
        um_dir = tmp_gravavel("vgp-dete1-")
        dois_dir = tmp_gravavel("vgp-dete2-")
        gera(um_dir, langs, a.n, a.negativos, dict(cache), log)
        guarda_cache(um_dir, cache)
        gera(dois_dir, langs, a.n, a.negativos, carrega_cache(um_dir), log)
        um, dois = sha_por_lang(um_dir, langs), sha_por_lang(dois_dir, langs)
        iguais = um == dois and bool(um)
        log(f"determinismo 1a: {um}")
        log(f"determinismo 2a: {dois}")
        log(
            "determinismo: "
            + ("IDENTICO" if iguais else "DIVERGE (a semente nao fechou)")
        )
        return 0 if iguais else 1
    relatorio = gera(a.saida, langs, a.n, a.negativos, cache, log)
    guarda_cache(a.saida, cache)
    relatorio["tempo_s"] = round(time.time() - ini, 1)
    if a.relatorio:
        a.relatorio.parent.mkdir(parents=True, exist_ok=True)
        a.relatorio.write_text(
            json.dumps(relatorio, indent=1, ensure_ascii=False), encoding="utf-8"
        )
    print(json.dumps(relatorio, indent=1, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
