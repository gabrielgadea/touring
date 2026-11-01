#!/usr/bin/env python3
"""O estado do projeto NO INSTANTE de cada entrada do corpus V0d — a régua sem deriva para o juiz do motor.

Medido em 09/10/2026 (sonda da camada 2, `diagnostics/sonda_camada2_pyright_analise.json`): 29 de 259 negativos
"negados" pelo pyright eram, em boa parte, DERIVA (`PLANO_DE_ENVIO` existia no módulo no dia da edição e saiu depois)
ou defeito JÁ existente no arquivo (o hook só nega o que a edição INTRODUZ: proposto − disco). Julgar contra o
projeto de hoje mede o tempo, não o motor.

Grava, sem tocar `entradas.jsonl` (o juiz da V0d segue intacto), em `<corpus>/estado_no_instante/`:
  antes/<id>.py              negativo: o arquivo 1 ms antes da edição (o `--disk-file` do delta)
  modulos/<id>/<n>.py        TODO módulo do PROJETO alcançável pelos imports da entrada, em qualquer profundidade (o FECHO
                             transitivo, 10/10/2026), no instante dela (`--overlay`)
  manifesto.json             {id: {"disk_file": str|None, "overlays": [{path, file, sha256}], "motivo": str,
                                   "completo": bool, "faltantes": [path], "fecho": {...}, "nao_cobertas_prof2": [path]}}

O fecho segue os imports lendo o conteúdo DO INSTANTE de cada módulo (o motor segue os imports dentro dos módulos
sobrepostos: um `pkg/__init__.py` que reexporta `from .nucleo import f` fazia o motor ler o `nucleo.py` DE HOJE). Um módulo
do fecho que não reconstrói no instante deixa a entrada INCOMPLETA; `nao_cobertas_prof2` é o verificador independente
(tem de ser vazio num fecho completo).

Controle positivo por negativo: reconstruir o DEPOIS com o mesmo código tem de dar o sha do snapshot do corpus;
se não der, o "antes" daquela entrada não é confiável e ela fica sem delta (motivo nomeado, nunca conteúdo errado).
Uso: python3 estado_no_instante_v0d.py [--corpus DIR] | --selftest
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import math
import sys
from collections import Counter
from functools import lru_cache
from datetime import datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import corpus_v0d  # noqa: E402 — o construtor do corpus é a fonte única da reconstrução

CORPUS = Path.home() / ".claude/touring/vgp/corpus_v0d"


def instante_anterior(ts: str) -> str:
    """`ts` − 1 ms no MESMO formato ISO-Z com milissegundos (o construtor compara timestamps como string)."""
    t = datetime.fromisoformat(ts.replace("Z", "+00:00")) - timedelta(milliseconds=1)
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


def sha(texto: str) -> str:
    return hashlib.sha256(texto.encode()).hexdigest()


def nomes_importados(texto: str) -> list[tuple[str, int]]:
    """(módulo, nível) de cada import do programa; nível > 0 = relativo."""
    try:
        arvore = ast.parse(texto)
    except SyntaxError:
        return []
    out = []
    for no in ast.walk(arvore):
        if isinstance(no, ast.Import):
            out += [(a.name, 0) for a in no.names]
        elif isinstance(no, ast.ImportFrom):
            out.append((no.module or "", no.level))
    return out


def candidatos(modulo: str, nivel: int, arquivo: Path, raiz: Path) -> list[Path]:
    """Onde o módulo mora: relativo ao pacote do arquivo, ou à raiz do projeto e ao diretório do script."""
    partes = [p for p in modulo.split(".") if p]
    if nivel:
        bases = [arquivo.parent.joinpath(*[".."] * (nivel - 1)).resolve()]
    else:
        bases = [raiz, arquivo.parent]
    out = []
    for b in bases:
        alvo = b.joinpath(*partes) if partes else b
        out += [alvo.with_suffix(".py"), alvo / "__init__.py"]
    return out


def _raiz_e_arquivo(entrada: dict) -> tuple[Path, Path]:
    raiz = Path(entrada["project_root"]).resolve()
    arquivo = (
        Path(entrada["path"])
        if not entrada["path"].startswith("<")
        else raiz / "__inline__.py"
    )
    return raiz, arquivo


def modulos_do_projeto_legado(entrada: dict, texto: str) -> list[Path]:
    """A regra ANTIGA (até 09/10/2026): por import, só o PRIMEIRO candidato — `from pkg import mod` devolvia o
    `pkg/__init__.py` e nunca o `pkg/mod.py`. Fica para medir quantas entradas a regra nova completa (e como mutante)."""
    raiz, arquivo = _raiz_e_arquivo(entrada)
    achados: dict[str, Path] = {}
    for modulo, nivel in nomes_importados(texto):
        for c in candidatos(modulo, nivel, arquivo, raiz):
            if (
                c.is_file()
                and raiz in c.resolve().parents
                and c.resolve() != arquivo.resolve()
            ):
                achados[str(c.resolve())] = c.resolve()
                break
    return sorted(achados.values())


def importacoes(texto: str) -> list[tuple[str, int, tuple[str, ...]]]:
    """(módulo, nível, nomes importados) de cada import; `import a.b` → nomes vazios; `*` não é nome de módulo."""
    try:
        arvore = ast.parse(texto)
    except (SyntaxError, ValueError):
        return []
    out = []
    for no in ast.walk(arvore):
        if isinstance(no, ast.Import):
            out += [(a.name, 0, ()) for a in no.names]
        elif isinstance(no, ast.ImportFrom):
            out.append((no.module or "", no.level, tuple(a.name for a in no.names if a.name != "*")))
    return out


@lru_cache(maxsize=32)
def _fontes_locais(raiz: str) -> tuple[Path, ...]:
    """Raízes locais usuais de um monorepo, se existirem na árvore atual."""
    r = Path(raiz)
    fontes = [r / "src", r / "python"]
    for pacote in sorted(r.glob("packages/*")):
        fontes += [pacote / "src", pacote / "python"]
    return tuple(dict.fromkeys(p.resolve() for p in fontes if p.is_dir()))


@lru_cache(maxsize=512)
def _modulos_versionados(raiz: str, ts: str) -> tuple[frozenset[Path], tuple[Path, ...]]:
    """Arquivos e raízes de código que existiam no Git em `ts`, inclusive os já apagados hoje."""
    if not ts:
        return frozenset(), ()
    root = corpus_v0d._git_root(str(Path(raiz) / "__inline__.py"))
    if root is None:
        return frozenset(), ()
    rev = corpus_v0d._git(root, "rev-list", "-1", f"--before={ts}", "HEAD")
    if rev.returncode or not rev.stdout.strip():
        return frozenset(), ()
    tree = corpus_v0d._git(root, "ls-tree", "-r", "--name-only", rev.stdout.strip())
    if tree.returncode:
        return frozenset(), ()
    arquivos: set[Path] = set()
    fontes: set[Path] = set()
    for rel in tree.stdout.splitlines():
        if not rel.endswith(".py"):
            continue
        partes = Path(rel).parts
        arquivos.add((root / rel).resolve())
        if partes[0] in ("src", "python"):
            fontes.add((root / partes[0]).resolve())
        if len(partes) >= 3 and partes[0] == "packages" and partes[2] in ("src", "python"):
            fontes.add((root.joinpath(*partes[:3])).resolve())
    return frozenset(arquivos), tuple(sorted(fontes))


def _contexto_local(raiz: Path, ts: str) -> tuple[frozenset[Path], tuple[Path, ...]]:
    historicos, fontes_git = _modulos_versionados(str(raiz), ts)
    fontes = tuple(dict.fromkeys((*_fontes_locais(str(raiz)), *fontes_git)))
    return historicos, fontes


def _arquivos_do_caminho(
    base: Path, partes: list[str], historicos: frozenset[Path] = frozenset()
) -> dict[Path, str]:
    """O que carregar `a.b.c` sob `base` põe no projeto: o `__init__.py` de cada pacote do caminho (o último, se pacote,
    é o módulo em si) e o `c.py` final. Tipo: `ancestral` (pacote no meio do caminho) | `direto` (o alvo)."""
    out: dict[Path, str] = {}
    atual = base
    for i, parte in enumerate(partes):
        atual = atual / parte
        alvo = i == len(partes) - 1
        init = atual / "__init__.py"
        if init.is_file() or init in historicos:
            out[init] = "direto" if alvo else "ancestral"
        modulo = atual.with_suffix(".py")
        if alvo and (modulo.is_file() or modulo in historicos):
            out[modulo] = "direto"
    return out


def _arquivos_do_import(
    modulo: str,
    nivel: int,
    nomes: tuple[str, ...],
    arquivo: Path,
    raiz: Path,
    historicos: frozenset[Path] = frozenset(),
    fontes: tuple[Path, ...] = (),
) -> dict[Path, str]:
    """TODO módulo local que um import carrega, na primeira base em que algo resolve. Além da raiz e do
    diretório do script, cobre `src`, `python` e `packages/*/{src,python}`; o conjunto do Git em `ts` permite
    resolver módulos já apagados. `from pkg import mod` inclui o pacote e o submódulo."""
    partes = [p for p in modulo.split(".") if p]
    bases = (
        [arquivo.parent.joinpath(*[".."] * (nivel - 1)).resolve()]
        if nivel
        else [raiz, arquivo.parent, *fontes]
    )
    for base in bases:
        achados: dict[Path, str] = {}
        if nivel:  # o pacote corrente roda o próprio __init__ ao importar qualquer irmão
            init = base / "__init__.py"
            if init.is_file() or init in historicos:
                achados[init] = "direto" if not partes else "ancestral"
        if partes:
            achados |= _arquivos_do_caminho(base, partes, historicos)
        for nome in nomes:
            for f, _ in _arquivos_do_caminho(base.joinpath(*partes), [nome], historicos).items():
                achados[f] = "submodulo"
        if achados:
            return achados
    return {}


def modulos_tipados(entrada: dict, texto: str) -> dict[Path, str]:
    """{arquivo do projeto: tipo} de TODO módulo que a entrada importa — `direto` | `ancestral` | `submodulo`.
    Só entra o que mora sob a raiz do projeto e não é o próprio arquivo."""
    raiz, arquivo = _raiz_e_arquivo(entrada)
    historicos, fontes = _contexto_local(raiz, entrada["ts"])
    prioridade = {"submodulo": 0, "direto": 1, "ancestral": 2}
    out: dict[Path, str] = {}
    for modulo, nivel, nomes in importacoes(texto):
        for f, tipo in _arquivos_do_import(
            modulo, nivel, nomes, arquivo, raiz, historicos, fontes
        ).items():
            f = f.resolve()
            if raiz not in f.parents or f == arquivo.resolve():
                continue
            if f not in out or prioridade[tipo] < prioridade[out[f]]:
                out[f] = tipo
    return dict(sorted(out.items()))


def modulos_do_projeto(entrada: dict, texto: str) -> list[Path]:
    """Os módulos do projeto que a entrada importa, TODOS (a regra completa de 10/10/2026)."""
    return list(modulos_tipados(entrada, texto))


MOTIVO_INCOMPLETO = {
    "submodulo": "estado incompleto: submódulo importado não reconstrói no instante",
    "direto": "estado incompleto: módulo importado não reconstrói no instante",
    "ancestral": "estado incompleto: __init__ de pacote ancestral não reconstrói no instante",
}


MOTIVO_INCOMPLETO_FECHO = {
    "submodulo": "estado incompleto: submódulo do fecho transitivo (profundidade ≥ 2) não reconstrói no instante",
    "direto": "estado incompleto: módulo do fecho transitivo (profundidade ≥ 2) não reconstrói no instante",
    "ancestral": "estado incompleto: __init__ de pacote ancestral do fecho transitivo (profundidade ≥ 2) não reconstrói no instante",
}
TIPOS_POR_ESPECIFICIDADE = ("submodulo", "direto", "ancestral")


def fecho_transitivo(entrada: dict, texto: str, indice: dict) -> list[dict]:
    """O FECHO (HO13f, 10/10/2026): todo módulo do projeto ALCANÇÁVEL pelos imports da entrada, em qualquer profundidade.

    Busca em largura: o nível 1 é `modulos_tipados(entrada, texto)`; cada módulo é lido NO INSTANTE da entrada
    (`corpus_v0d._conteudo_depois`) e os imports DESSE conteúdo dão o nível seguinte — nunca o arquivo de hoje. Um módulo
    aparece uma vez (a menor profundidade; o ciclo termina), a própria entrada nunca entra (o `--content-file` já é ela) e
    um módulo irreconstrutível fica no fecho com `conteudo=None` (não se expande: o que ele importa é desconhecido).
    Devolve [{path, tipo, profundidade, conteudo}] em ordem estável (profundidade, caminho)."""
    _, arquivo = _raiz_e_arquivo(entrada)
    proprio = arquivo.resolve()
    achados: dict[Path, dict] = {}
    fronteira = [(texto, entrada, 1)]
    while fronteira:
        proxima = []
        for fonte, quem, profundidade in fronteira:
            for mod, tipo in modulos_tipados(quem, fonte).items():
                if mod == proprio or mod in achados:
                    continue
                conteudo = corpus_v0d._conteudo_depois(str(mod), entrada["ts"], indice)
                achados[mod] = {"path": mod, "tipo": tipo, "profundidade": profundidade, "conteudo": conteudo}
                if conteudo is not None:
                    proxima.append((conteudo, {**entrada, "path": str(mod)}, profundidade + 1))
        fronteira = proxima
    return sorted(achados.values(), key=lambda m: (m["profundidade"], str(m["path"])))


def avaliar(fecho: list[dict], profundidade_maxima: int | None = None) -> tuple[bool, str]:
    """(completo, motivo) do fecho. `profundidade_maxima=1` reproduz a regra de ANTES (só o que a entrada importa) — é como o
    viés do fecho é medido; sem limite vale o fecho inteiro. Falta de profundidade 1 mantém o motivo antigo (as contagens por
    motivo seguem comparáveis); falta mais funda recebe o motivo próprio do fecho. Motivo = o tipo mais específico que faltou."""
    faltam = [m for m in fecho if m["conteudo"] is None and (profundidade_maxima is None or m["profundidade"] <= profundidade_maxima)]
    for motivos, rasa in ((MOTIVO_INCOMPLETO, True), (MOTIVO_INCOMPLETO_FECHO, False)):
        tipos = {m["tipo"] for m in faltam if (m["profundidade"] == 1) == rasa}
        for tipo in TIPOS_POR_ESPECIFICIDADE:
            if tipo in tipos:
                return False, motivos[tipo]
    return True, "ok"


def estado_completo(entrada: dict, texto: str, indice: dict) -> tuple[bool, str]:
    """Todo módulo do FECHO transitivo tem de reconstruir NO INSTANTE da entrada; um que só existe hoje deixa a entrada
    INCOMPLETA (o motor julgaria contra o arquivo de hoje, seja o que a entrada importa ou o que um módulo dela reexporta)."""
    return avaliar(fecho_transitivo(entrada, texto, indice))


def estado_negativo(e: dict, indice: dict, destino: Path) -> tuple[str | None, str]:
    """O `--disk-file` do negativo, com o controle positivo do DEPOIS."""
    depois = corpus_v0d._conteudo_depois(e["path"], e["ts"], indice)
    if depois is None or sha(depois) != e["snap_sha256"]:
        return (
            None,
            "controle positivo falhou: o DEPOIS reconstruído hoje não é o snapshot do corpus",
        )
    antes = corpus_v0d._conteudo_depois(e["path"], instante_anterior(e["ts"]), indice)
    # O DEPOIS reconstruiu (controle acima) e 1 ms antes não há base git nem criação na sessão: o arquivo NASCEU
    # nesta edição — o disco anterior é um arquivo vazio, estado conhecido (e o delta vira a régua estrita).
    motivo = (
        "ok" if antes is not None else "o arquivo nasceu nesta edição (antes = vazio)"
    )
    arq = destino / "antes" / f"{e['id']}.py"
    arq.parent.mkdir(parents=True, exist_ok=True)
    arq.write_text(antes or "")
    return str(arq.relative_to(destino.parent)), motivo


def overlays_e_faltas(
    e: dict, texto: str, indice: dict, destino: Path, fecho: list[dict] | None = None
) -> tuple[list[dict], list[str]]:
    """Os módulos do FECHO transitivo, no instante da entrada (base git em ts + a dobra da sessão até ts) e os que NÃO
    reconstroem (nomeados): com falta, a entrada está INCOMPLETA — nunca um overlay de hoje no lugar do instante."""
    out, faltas = [], []
    for n, m in enumerate(fecho if fecho is not None else fecho_transitivo(e, texto, indice)):
        mod, conteudo = m["path"], m["conteudo"]
        if conteudo is None:
            faltas.append(str(mod))
            continue
        arq = destino / "modulos" / e["id"] / f"{n}.py"
        arq.parent.mkdir(parents=True, exist_ok=True)
        arq.write_text(conteudo)
        out.append(
            {
                "path": str(mod),
                "file": str(arq.relative_to(destino.parent)),
                "sha256": sha(conteudo),
            }
        )
    return out, faltas


def importacoes_nao_cobertas(e: dict, over: list[dict], destino: Path) -> list[str]:
    """VERIFICADOR INDEPENDENTE do fecho: módulos do projeto que um módulo sobreposto importa (no conteúdo DO INSTANTE) e que o
    manifesto não sobrepõe. Num fecho completo é VAZIO — lê os overlays gravados e reaplica `modulos_tipados`, sem repetir a
    busca em largura que os produziu. (Até o 20261013 era a MEDIDA da profundidade 2, que a regra de então não exigia: 91 das
    1.153 entradas e 717 módulos ficaram de fora; o fecho transitivo a fecha.)"""
    cobertos = {str(Path(o["path"]).resolve()) for o in over}
    if not e["path"].startswith("<"):
        cobertos.add(str(Path(e["path"]).resolve()))  # um módulo que importa de volta a própria entrada não é lacuna
    faltam: set[str] = set()
    for o in over:
        texto = (destino.parent / o["file"]).read_text(errors="ignore")
        sub = {"project_root": e["project_root"], "path": o["path"], "ts": e["ts"], "id": e["id"]}
        faltam |= {str(m) for m in modulos_tipados(sub, texto)} - cobertos
    return sorted(faltam)


def overlays(e: dict, texto: str, indice: dict, destino: Path) -> list[dict]:
    """`overlays_e_faltas` sem as faltas (assinatura e comportamento de antes: o que não reconstrói é pulado)."""
    return overlays_e_faltas(e, texto, indice, destino)[0]


def sem_delta(e: dict) -> str | None:
    """Positivo, inline e /tmp escrito-e-executado não têm "antes": o programa inteiro é a edição."""
    if e["rotulo"] != "negativo":
        return "positivo: sem delta"
    if e["path"].startswith("<"):
        return "inline: o programa inteiro é novo (sem delta)"
    if e.get("origem") == "tmp":
        return "tmp: escrito e executado no mesmo comando (sem delta)"
    return None


def carregar_entradas(corpus: Path) -> list[dict]:
    """O corpus da V0d + os negativos inline/tmp da V5 (`entradas_inline_neg.jsonl`), que também precisam dos módulos
    do projeto NO INSTANTE — medido em 09/10: 4 dos 9 falsos deny inline eram deriva de módulo."""
    arquivos = [corpus / "entradas.jsonl", corpus / "entradas_inline_neg.jsonl"]
    return [
        json.loads(x)
        for a in arquivos
        if a.is_file()
        for x in a.read_text().splitlines()
        if x.strip()
    ]


def distribuicao(valores: list[int]) -> dict:
    """Resumo de uma amostra de inteiros (posto mais próximo, sem interpolar): n, soma, min, mediana, média, p90, p99, max."""
    if not valores:
        return {"n": 0, "soma": 0, "min": 0, "mediana": 0, "media": 0.0, "p90": 0, "p99": 0, "max": 0}
    ordem = sorted(valores)

    def posto(p: float) -> int:
        return ordem[min(len(ordem) - 1, max(0, math.ceil(p * len(ordem)) - 1))]

    return {"n": len(ordem), "soma": sum(ordem), "min": ordem[0], "mediana": posto(0.5), "media": round(sum(ordem) / len(ordem), 2),
            "p90": posto(0.9), "p99": posto(0.99), "max": ordem[-1]}


def construir(corpus: Path) -> dict:
    destino = corpus / "estado_no_instante"
    entradas = carregar_entradas(corpus)
    indice = corpus_v0d.varrer_transcripts()
    manifesto, motivos = {}, {}
    por_tipo: dict[str, int] = {}
    a_mais = com_lacuna = n_lacunas = 0
    tamanhos: list[int] = []
    prof_max: Counter = Counter()
    for e in entradas:
        texto = (corpus / e["snap"]).read_text(errors="ignore")
        motivo_sem = sem_delta(e)
        disk, motivo = (
            (None, motivo_sem) if motivo_sem else estado_negativo(e, indice, destino)
        )
        fecho = fecho_transitivo(e, texto, indice)
        over, faltas = overlays_e_faltas(e, texto, indice, destino, fecho)
        lacunas = importacoes_nao_cobertas(e, over, destino)
        por_prof = Counter(m["profundidade"] for m in fecho)
        manifesto[e["id"]] = {
            "disk_file": disk,
            "overlays": over,
            "motivo": motivo,
            "completo": not faltas,
            "faltantes": faltas,
            "fecho": {
                "tamanho": len(fecho),
                "profundidade_max": max(por_prof, default=0),
                "por_profundidade": {str(d): n for d, n in sorted(por_prof.items())},
            },
            "nao_cobertas_prof2": lacunas,
        }
        tamanhos.append(len(fecho))
        prof_max[max(por_prof, default=0)] += 1
        com_lacuna += bool(lacunas)
        n_lacunas += len(lacunas)
        motivos[motivo] = motivos.get(motivo, 0) + 1
        for tipo in modulos_tipados(e, texto).values():
            por_tipo[tipo] = por_tipo.get(tipo, 0) + 1
        a_mais += len(modulos_do_projeto(e, texto)) > len(modulos_do_projeto_legado(e, texto))
    (destino / "manifesto.json").write_text(
        json.dumps(manifesto, ensure_ascii=False, indent=1)
    )
    n_over = sum(len(m["overlays"]) for m in manifesto.values())
    return {
        "entradas": len(manifesto),
        "motivos": motivos,
        "overlays": n_over,
        "entradas_incompletas": sum(not m["completo"] for m in manifesto.values()),
        "modulos_por_tipo": por_tipo,
        "entradas_com_modulo_que_a_regra_antiga_nao_sobrepunha": a_mais,
        "entradas_com_importacao_de_profundidade_2_nao_coberta": com_lacuna,
        "modulos_de_profundidade_2_nao_cobertos": n_lacunas,
        "fecho": {
            "tamanho_por_entrada": distribuicao(tamanhos),
            "entradas_por_profundidade_maxima": {str(d): n for d, n in sorted(prof_max.items())},
        },
    }


def selftest() -> int:
    casos = {
        "ms preservado": instante_anterior("2026-09-17T16:01:54.271Z")
        == "2026-09-17T16:01:54.270Z",
        "vira o segundo": instante_anterior("2026-09-17T16:01:54.000Z")
        == "2026-09-17T16:01:53.999Z",
        "ordem de string": instante_anterior("2026-09-17T16:01:54.271Z")
        < "2026-09-17T16:01:54.271Z",
        "imports": nomes_importados("import a.b\nfrom .c import d\nfrom e import f\n")
        == [("a.b", 0), ("c", 1), ("e", 0)],
        "sintaxe ruim não levanta": nomes_importados("def (") == [],
        "positivo sem delta": sem_delta({"rotulo": "positivo", "path": "/p/a.py"})
        is not None,
        "inline sem delta": sem_delta({"rotulo": "negativo", "path": "<stdin>"})
        is not None,
        "tmp sem delta": sem_delta(
            {"rotulo": "negativo", "path": "/tmp/x.py", "origem": "tmp"}
        )
        is not None,
        "negativo de arquivo TEM delta": sem_delta(
            {"rotulo": "negativo", "path": "/p/a.py", "origem": "atribuido"}
        )
        is None,
    }
    for nome, ok in casos.items():
        print(f"  {'OK ' if ok else 'FALHA'} {nome}")
    return 0 if all(casos.values()) else 1


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Estado do projeto no instante de cada entrada do corpus V0d"
    )
    ap.add_argument("--corpus", type=Path, default=CORPUS)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    print(json.dumps(construir(a.corpus), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
