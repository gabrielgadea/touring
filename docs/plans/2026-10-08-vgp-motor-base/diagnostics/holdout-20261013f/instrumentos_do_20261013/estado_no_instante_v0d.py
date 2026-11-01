#!/usr/bin/env python3
"""O estado do projeto NO INSTANTE de cada entrada do corpus V0d — a régua sem deriva para o juiz do motor.

Medido em 09/10/2026 (sonda da camada 2, `diagnostics/sonda_camada2_pyright_analise.json`): 29 de 259 negativos
"negados" pelo pyright eram, em boa parte, DERIVA (`PLANO_DE_ENVIO` existia no módulo no dia da edição e saiu depois)
ou defeito JÁ existente no arquivo (o hook só nega o que a edição INTRODUZ: proposto − disco). Julgar contra o
projeto de hoje mede o tempo, não o motor.

Grava, sem tocar `entradas.jsonl` (o juiz da V0d segue intacto), em `<corpus>/estado_no_instante/`:
  antes/<id>.py              negativo: o arquivo 1 ms antes da edição (o `--disk-file` do delta)
  modulos/<id>/<n>.py        os módulos do PROJETO que a entrada importa, no instante dela (`--overlay`)
  manifesto.json             {id: {"disk_file": str|None, "overlays": [{path, file, sha256}], "motivo": str}}

Controle positivo por negativo: reconstruir o DEPOIS com o mesmo código tem de dar o sha do snapshot do corpus;
se não der, o "antes" daquela entrada não é confiável e ela fica sem delta (motivo nomeado, nunca conteúdo errado).
Uso: python3 estado_no_instante_v0d.py [--corpus DIR] | --selftest
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import sys
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


def _arquivos_do_caminho(base: Path, partes: list[str]) -> dict[Path, str]:
    """O que carregar `a.b.c` sob `base` põe no projeto: o `__init__.py` de cada pacote do caminho (o último, se pacote,
    é o módulo em si) e o `c.py` final. Tipo: `ancestral` (pacote no meio do caminho) | `direto` (o alvo)."""
    out: dict[Path, str] = {}
    atual = base
    for i, parte in enumerate(partes):
        atual = atual / parte
        alvo = i == len(partes) - 1
        init = atual / "__init__.py"
        if init.is_file():
            out[init] = "direto" if alvo else "ancestral"
        modulo = atual.with_suffix(".py")
        if alvo and modulo.is_file():
            out[modulo] = "direto"
    return out


def _arquivos_do_import(modulo: str, nivel: int, nomes: tuple[str, ...], arquivo: Path, raiz: Path) -> dict[Path, str]:
    """TODO módulo do projeto que um import carrega, na primeira base em que algo resolve (a mesma ordem de antes: a
    raiz do projeto, depois o diretório do script; relativo: o pacote do arquivo). `from pkg import mod` → o `pkg` E
    o `pkg/mod.py` (ou `pkg/mod/__init__.py`): é o submódulo que o código usa, não o `__init__`."""
    partes = [p for p in modulo.split(".") if p]
    bases = [arquivo.parent.joinpath(*[".."] * (nivel - 1)).resolve()] if nivel else [raiz, arquivo.parent]
    for base in bases:
        achados: dict[Path, str] = {}
        if nivel:  # o pacote corrente roda o próprio __init__ ao importar qualquer irmão
            init = base / "__init__.py"
            if init.is_file():
                achados[init] = "direto" if not partes else "ancestral"
        if partes:
            achados |= _arquivos_do_caminho(base, partes)
        for nome in nomes:
            for f, _ in _arquivos_do_caminho(base.joinpath(*partes), [nome]).items():
                achados[f] = "submodulo"
        if achados:
            return achados
    return {}


def modulos_tipados(entrada: dict, texto: str) -> dict[Path, str]:
    """{arquivo do projeto: tipo} de TODO módulo que a entrada importa — `direto` | `ancestral` | `submodulo`.
    Só entra o que mora sob a raiz do projeto e não é o próprio arquivo."""
    raiz, arquivo = _raiz_e_arquivo(entrada)
    prioridade = {"submodulo": 0, "direto": 1, "ancestral": 2}
    out: dict[Path, str] = {}
    for modulo, nivel, nomes in importacoes(texto):
        for f, tipo in _arquivos_do_import(modulo, nivel, nomes, arquivo, raiz).items():
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


def estado_completo(entrada: dict, texto: str, indice: dict) -> tuple[bool, str]:
    """Cada módulo do projeto que a entrada importa tem de reconstruir NO INSTANTE dela; um que só existe hoje deixa a
    entrada INCOMPLETA (o motor julgaria contra o arquivo de hoje). Motivo = o tipo mais específico que faltou."""
    faltam = {
        tipo
        for mod, tipo in modulos_tipados(entrada, texto).items()
        if corpus_v0d._conteudo_depois(str(mod), entrada["ts"], indice) is None
    }
    for tipo in ("submodulo", "direto", "ancestral"):
        if tipo in faltam:
            return False, MOTIVO_INCOMPLETO[tipo]
    return True, "ok"


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
    e: dict, texto: str, indice: dict, destino: Path
) -> tuple[list[dict], list[str]]:
    """Os módulos importados, no instante da entrada (base git em ts + a dobra da sessão até ts) e os que NÃO
    reconstroem (nomeados): com falta, a entrada está INCOMPLETA — nunca um overlay de hoje no lugar do instante."""
    out, faltas = [], []
    for n, mod in enumerate(modulos_do_projeto(e, texto)):
        conteudo = corpus_v0d._conteudo_depois(str(mod), e["ts"], indice)
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
    """Profundidade 2 — MEDIDA, não exigida: módulos do projeto que um módulo sobreposto importa (no conteúdo DO INSTANTE) e
    que o manifesto não sobrepõe. A regra pré-registrada cobre o que a ENTRADA importa; o que o `pkg/__init__.py` reexporta
    de `pkg/sub.py`, por exemplo, só é medido aqui, para dizer o tamanho do que a regra deixa de fora."""
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


def construir(corpus: Path) -> dict:
    destino = corpus / "estado_no_instante"
    entradas = carregar_entradas(corpus)
    indice = corpus_v0d.varrer_transcripts()
    manifesto, motivos = {}, {}
    por_tipo: dict[str, int] = {}
    a_mais = com_lacuna = n_lacunas = 0
    for e in entradas:
        texto = (corpus / e["snap"]).read_text(errors="ignore")
        motivo_sem = sem_delta(e)
        disk, motivo = (
            (None, motivo_sem) if motivo_sem else estado_negativo(e, indice, destino)
        )
        over, faltas = overlays_e_faltas(e, texto, indice, destino)
        lacunas = importacoes_nao_cobertas(e, over, destino)
        manifesto[e["id"]] = {
            "disk_file": disk,
            "overlays": over,
            "motivo": motivo,
            "completo": not faltas,
            "faltantes": faltas,
            "nao_cobertas_prof2": lacunas,
        }
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
