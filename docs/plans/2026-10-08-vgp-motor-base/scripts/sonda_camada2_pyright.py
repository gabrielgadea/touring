#!/usr/bin/env python3
"""Sonda da camada 2 (V3): o pyright-langserver PERSISTENTE como verificador, medido no corpus V0d.

Por que langserver e não a CLI: o conteúdo proposto entra em memória (`textDocument/didOpen`), sem tocar o disco;
os imports resolvem contra o projeto real; e o custo de subir o processo é pago uma vez — a CLI paga ~1 s a cada
chamada, acima do orçamento da V3 (p95 ≤ 1 s por edição).

**Casamento por versão (09/10/2026, medido)**: o pyright 1.1.408 não anuncia diagnóstico pull (`diagnosticProvider`
nulo), só push; e o `didClose` publica uma LIMPEZA (`version` nula, 0 diagnósticos) ~50 ms depois. A 1ª versão desta
sonda abria tudo como versão 1 e aceitava a 1ª `publishDiagnostics` da uri — reabrir a mesma uri (o "antes" do delta,
ou a entrada seguinte no mesmo arquivo) podia receber a limpeza da abertura anterior como resultado. Agora cada
abertura tem versão única (`versionSupport`) e só a publicação daquela versão conta.

Por entrada grava os achados de NEGAÇÃO com o ESCOPO do alvo — `proprio` (nome do próprio programa), `projeto` (módulo
ou classe sob a raiz do projeto), `externo` (stdlib/terceiros/builtin) — e a classe do erro do positivo pela mensagem
do runtime: `simbolo` (atributo de módulo, import, kwarg, nome, membro de classe que não é dado) ou `forma` (atributo
de um VALOR builtin: `'list' object has no attribute 'get'` — forma do dado, fora do alcance estático e do VGP).
`politicas()` simula quem NEGA (o resto avisa) sobre esses registros.

Uso: python3 sonda_camada2_pyright.py --projeto ~/projects/analise [--regua estrita|instante] [--limite N]
                                       [--json saida.json] | --selftest
"""

from __future__ import annotations

import argparse
import builtins
import json
import math
import re
import subprocess
import sys
import time
import warnings
from dataclasses import dataclass, field
from pathlib import Path

CORPUS = Path.home() / ".claude/touring/vgp/corpus_v0d"
NOME = re.compile(r'"([^"]+)"')
CLASSE_CONCRETA = re.compile(r'Cannot access attribute "[^"]+" for class "([^"]+)"')
# Classe concreta = nome simples. `dict[Unknown, Unknown]`, `X | None`, `Unknown` são inferência incompleta
# (medido nesta própria sonda: um dict heterogêneo virou "attribute append unknown") — avisam, não negam.
NOME_SIMPLES = re.compile(r"^[A-Za-z_][A-Za-z0-9_.]*$")
NEGA_MODULO = ("is not a known attribute of module", "is unknown import symbol")
DE_MODULO = re.compile(r'of module "([^"]+)"')
FROM_IMPORT = re.compile(r"^\s*from\s+(\.*[\w.]*)\s+import\b")
# Atributo de um VALOR builtin = forma do dado (o runtime diz o tipo do valor, não o símbolo que faltou).
FORMA = re.compile(
    r"'(str|list|dict|int|float|tuple|bytes|set|frozenset|bool|NoneType)' object has no attribute"
)
SIMBOLO = re.compile(
    r"module '[^']+' has no attribute|cannot import name|No module named|is not defined"
    r"|unexpected keyword argument|object has no attribute|type object '[^']+' has no attribute"
)
IMPORT_ESTRELA = re.compile(r"^\s*from\s+\S+\s+import\s+\*", re.M)
DUNDER = re.compile(r"^__\w+__$")
BUILTIN_CLASSES = {n for n in dir(builtins) if isinstance(getattr(builtins, n), type)}


class Lsp:
    """Cliente JSON-RPC mínimo sobre stdio (framing Content-Length), com versão única por abertura."""

    def __init__(self, cmd: list[str], raiz: Path, python: Path) -> None:
        self.p = subprocess.Popen(
            cmd,
            cwd=raiz,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.n = 0
        self.versao = 0
        self.raiz, self.python = raiz, python

    def enviar(self, obj: dict) -> None:
        corpo = json.dumps(obj).encode()
        assert self.p.stdin is not None
        self.p.stdin.write(f"Content-Length: {len(corpo)}\r\n\r\n".encode() + corpo)
        self.p.stdin.flush()

    def ler(self) -> dict:
        assert self.p.stdout is not None
        tamanho = 0
        while linha := self.p.stdout.readline().strip():
            if linha.lower().startswith(b"content-length:"):
                tamanho = int(linha.split(b":")[1])
        return json.loads(self.p.stdout.read(tamanho))

    def pedir(self, metodo: str, params: dict) -> int:
        self.n += 1
        self.enviar(
            {"jsonrpc": "2.0", "id": self.n, "method": metodo, "params": params}
        )
        return self.n

    def avisar(self, metodo: str, params: dict) -> None:
        self.enviar({"jsonrpc": "2.0", "method": metodo, "params": params})

    def abrir(self, uri: str, texto: str) -> int:
        """didOpen com versão ÚNICA — a única publicação que conta é a desta versão."""
        self.versao += 1
        self.avisar(
            "textDocument/didOpen",
            {
                "textDocument": {
                    "uri": uri,
                    "languageId": "python",
                    "version": self.versao,
                    "text": texto,
                }
            },
        )
        return self.versao

    def fechar(self, uri: str) -> None:
        self.avisar("textDocument/didClose", {"textDocument": {"uri": uri}})

    def responder_servidor(self, msg: dict) -> None:
        """O servidor pede configuração/registro; responder é obrigatório ou ele espera para sempre."""
        if msg.get("method") == "workspace/configuration":
            itens = msg["params"]["items"]
            conf = {
                "python": {"pythonPath": str(self.python)},
                "python.analysis": {
                    "typeCheckingMode": "standard",
                    "diagnosticMode": "openFilesOnly",
                    "autoSearchPaths": True,
                    "extraPaths": [str(self.raiz)],
                },
            }
            self.enviar(
                {
                    "jsonrpc": "2.0",
                    "id": msg["id"],
                    "result": [conf.get(i.get("section", ""), None) for i in itens],
                }
            )
        else:
            self.enviar({"jsonrpc": "2.0", "id": msg["id"], "result": None})

    def esperar(self, pred) -> dict:
        while True:
            msg = self.ler()
            if "method" in msg and "id" in msg:
                self.responder_servidor(msg)
            elif pred(msg):
                return msg


def iniciar(raiz: Path) -> Lsp:
    lsp = Lsp(
        [str(raiz / ".venv/bin/pyright-langserver"), "--stdio"],
        raiz,
        raiz / ".venv/bin/python",
    )
    rid = lsp.pedir(
        "initialize",
        {
            "processId": None,
            "rootUri": raiz.as_uri(),
            "workspaceFolders": [{"uri": raiz.as_uri(), "name": raiz.name}],
            "capabilities": {
                "textDocument": {"publishDiagnostics": {"versionSupport": True}},
                "workspace": {"configuration": True},
            },
        },
    )
    lsp.esperar(lambda m: m.get("id") == rid)
    lsp.avisar("initialized", {})
    return lsp


def uri_de(e: dict, raiz: Path) -> str:
    if e["path"].startswith("<"):
        return (raiz / f".vgp_inline_{e['id']}.py").as_uri()
    return Path(e["path"]).as_uri()


def da_versao(uri: str, versao: int):
    return (
        lambda m: m.get("method") == "textDocument/publishDiagnostics"
        and m["params"]["uri"] == uri
        and m["params"].get("version") == versao
    )


def checar(lsp: Lsp, uri: str, texto: str) -> tuple[list[dict], float]:
    t0 = time.perf_counter()
    versao = lsp.abrir(uri, texto)
    msg = lsp.esperar(da_versao(uri, versao))
    ms = (time.perf_counter() - t0) * 1000
    lsp.fechar(uri)
    return msg["params"]["diagnostics"], ms


def negacao(d: dict) -> str | None:
    """O nome negado, se o diagnóstico é de alta precisão; None se só avisa."""
    regra, msg = d.get("code", ""), d.get("message", "")
    achado = NOME.search(msg)
    nome = achado.group(1) if achado else None
    if regra == "reportUndefinedVariable" or (
        regra == "reportCallIssue" and msg.startswith("No parameter named")
    ):
        return nome
    return _nega_atributo(msg, nome) if regra == "reportAttributeAccessIssue" else None


def _nega_atributo(msg: str, nome: str | None) -> str | None:
    """Atributo: de módulo/import sempre nega; de classe só se a classe é concreta (nome simples, não None)."""
    if any(t in msg for t in NEGA_MODULO):
        return nome
    classe = CLASSE_CONCRETA.search(msg)
    concreta = (
        bool(classe)
        and NOME_SIMPLES.match(classe.group(1))
        and classe.group(1) != "None"
    )
    return nome if concreta else None


def modulo_do_projeto(modulo: str, raiz: Path, origem: Path) -> bool:
    """O módulo mora sob a raiz do projeto (absoluto ou relativo ao arquivo)?"""
    nivel = len(modulo) - len(modulo.lstrip("."))
    partes = [p for p in modulo.lstrip(".").split(".") if p]
    bases = (
        [origem.parent.joinpath(*[".."] * (nivel - 1)).resolve()]
        if nivel
        else [raiz, origem.parent]
    )
    for b in bases:
        alvo = b.joinpath(*partes)
        if alvo.with_suffix(".py").is_file() or (alvo / "__init__.py").is_file():
            return raiz.resolve() in (alvo.resolve().parents) or alvo.resolve() == raiz
    return False


def escopo(d: dict, texto: str, raiz: Path, origem: Path) -> str:
    """proprio | projeto | externo | forma | dinamico — o lado CONSERVADOR (projeto) quando não dá para saber."""
    regra, msg = d.get("code", ""), d.get("message", "")
    if regra == "reportUndefinedVariable":
        # `from X import *` torna o espaço de nomes incognoscível: o verificador não pode afirmar que o nome falta
        return "dinamico" if IMPORT_ESTRELA.search(texto) else "proprio"
    modulo = DE_MODULO.search(msg)
    if modulo is None and "is unknown import symbol" in msg:
        linhas = texto.splitlines()
        i = d.get("range", {}).get("start", {}).get("line", -1)
        achado = FROM_IMPORT.match(linhas[i]) if 0 <= i < len(linhas) else None
        modulo = achado
    if modulo is not None:
        return (
            "projeto" if modulo_do_projeto(modulo.group(1), raiz, origem) else "externo"
        )
    classe = CLASSE_CONCRETA.search(msg)
    achado = NOME.search(msg)
    if classe and achado and DUNDER.match(achado.group(1)):
        # Atributo de protocolo (`__wrapped__` do functools.wraps, `__self__`…): criado em runtime por decorador,
        # descritor ou extensão C — o verificador estático não o modela. Medido: d16e0cd395a8 (`T.html.__wrapped__`).
        return "dinamico"
    if classe and classe.group(1).split(".")[-1] in BUILTIN_CLASSES:
        # Membro de valor builtin (`int.add`, `str.get`) = FORMA do dado, o mesmo que o runtime chama de forma.
        # Medido em 09/10: os 2 falsos inline (`list[int | set]` → `int.add`) e o único falso da P2 em arquivo.
        return "forma"
    return "projeto"


def classe_erro(mensagem: str) -> str:
    """forma | simbolo | nao_classificado — predicado positivo por classe, nunca por eliminação."""
    if FORMA.search(mensagem or ""):
        return "forma"
    if SIMBOLO.search(mensagem or ""):
        return "simbolo"
    return "nao_classificado"


def p95(v: list[float]) -> float:
    return sorted(v)[min(len(v) - 1, math.ceil(0.95 * len(v)) - 1)] if v else math.inf


def estado(corpus: Path, regua: str) -> dict:
    """O manifesto da régua sem deriva (`estado_no_instante_v0d.py`); régua estrita → vazio."""
    m = corpus / "estado_no_instante" / "manifesto.json"
    return json.loads(m.read_text()) if regua == "instante" and m.is_file() else {}


def abrir_overlays(lsp: Lsp, e: dict, est: dict) -> list[str]:
    """Os módulos no instante da entrada e os `contexto` do corpus, abertos em memória (o pyright analisa quem os
    importa contra o conteúdo aberto, não o do disco)."""
    sobre = {o["path"]: CORPUS / o["file"] for o in est.get("overlays", [])}
    sobre |= {c["path"]: CORPUS / c["snap"] for c in e.get("contexto") or []}
    uris = []
    for orig, arq in sobre.items():
        uri = Path(orig).as_uri()
        lsp.abrir(uri, arq.read_text(errors="ignore"))
        uris.append(uri)
    return uris


def negados_em(
    lsp: Lsp, uri: str, texto: str, raiz: Path, origem: Path
) -> tuple[list[dict], float]:
    diags, ms = checar(lsp, uri, texto)
    return [
        {
            "code": d.get("code"),
            "nome": n,
            "msg": d.get("message", "")[:120],
            "escopo": escopo(d, texto, raiz, origem),
        }
        for d in diags
        if (n := negacao(d))
    ], ms


@dataclass
class Registro:
    id: str
    rotulo: str
    origem: str
    classe: str
    classe_erro: str
    esperado: str
    negados: list[dict] = field(default_factory=list)
    ms: float = 0.0


def medir_entrada(lsp: Lsp, raiz: Path, e: dict, est: dict) -> Registro:
    abertos = abrir_overlays(lsp, e, est)
    uri = uri_de(e, raiz)
    origem = Path(e["path"]) if not e["path"].startswith("<") else Path(uri[7:])
    texto = (CORPUS / e["snap"]).read_text(errors="ignore")
    negados, ms = negados_em(lsp, uri, texto, raiz, origem)
    negados = _menos_o_antes(lsp, uri, est, negados, raiz, origem)
    for u in abertos:
        lsp.fechar(u)
    positivo = e["rotulo"] != "negativo"
    return Registro(
        id=e["id"],
        rotulo=e["rotulo"],
        origem=e.get("origem") or "",
        classe=e.get("classe") or "",
        classe_erro=classe_erro(e.get("mensagem") or "") if positivo else "",
        esperado=e.get("nome_esperado") or "",
        negados=negados,
        ms=ms,
    )


def _menos_o_antes(
    lsp: Lsp, uri: str, est: dict, negados: list[dict], raiz: Path, origem: Path
) -> list[dict]:
    """O delta proposto − disco: o que já estava no arquivo 1 ms antes nunca é culpa da edição."""
    if not est.get("disk_file"):
        return negados
    antes_txt = (CORPUS / est["disk_file"]).read_text(errors="ignore")
    ja, _ = negados_em(lsp, uri, antes_txt, raiz, origem)
    antes = {(x["code"], x["nome"]) for x in ja}
    return [x for x in negados if (x["code"], x["nome"]) not in antes]


def balde(origem: str) -> str:
    return "inline" if origem == "inline" else "tmp" if origem == "tmp" else "arquivo"


# Quem NEGA, por política (o resto avisa). Chave: (balde, escopo) → nega?
POLITICAS = {
    "P0 nega tudo": lambda b, esc: True,
    "P1 arquivo do projeto só avisa": lambda b, esc: b != "arquivo",
    "P2 arquivo nega só externo": lambda b, esc: b != "arquivo" or esc == "externo",
    "P3 = P2 e forma só avisa": lambda b, esc: (
        esc != "forma" and (b != "arquivo" or esc == "externo")
    ),
    "P4 = P3 e dinâmico só avisa": lambda b, esc: (
        esc not in ("forma", "dinamico") and (b != "arquivo" or esc == "externo")
    ),
}


BALDES = ("arquivo", "tmp", "inline")


def _razao(lista: list[Registro], pred) -> list[int]:
    return [sum(1 for r in lista if pred(r)), len(lista)]


def _de(pos: list[Registro], b: str, classe: str) -> list[Registro]:
    return [r for r in pos if balde(r.origem) == b and r.classe_erro == classe]


def _achou(r: Registro, nega=None, b: str = "") -> bool:
    """Um achado com o nome esperado; com `nega`, só se a política NEGA aquele escopo naquele balde."""
    return any(
        x["nome"] == r.esperado and (nega is None or nega(b, x["escopo"]))
        for x in r.negados
    )


def _politica(pos: list[Registro], neg: list[Registro], nega) -> dict:
    deny = {
        b: _razao(_de(pos, b, "simbolo"), lambda r, b=b: _achou(r, nega, b))
        for b in BALDES
    }
    falsos = [
        r for r in neg if any(nega(balde(r.origem), x["escopo"]) for x in r.negados)
    ]
    # POR BALDE: os negativos inline/tmp da V5 não se diluem nos de arquivo
    por_balde = {
        b: [
            sum(balde(r.origem) == b for r in falsos),
            sum(balde(r.origem) == b for r in neg),
        ]
        for b in BALDES
    }
    return {"deny": deny, "falso_deny": por_balde, "falsos": [r.id for r in falsos]}


def politicas(regs: list[Registro]) -> dict:
    """Recall de DETECÇÃO (qualquer severidade) e de DENY por balde, só em positivos de SÍMBOLO; falso deny nos
    negativos. A forma do dado sai à parte (fora do alcance estático)."""
    pos = [r for r in regs if r.rotulo != "negativo"]
    neg = [r for r in regs if r.rotulo == "negativo"]
    out: dict = {
        "deteccao": {b: _razao(_de(pos, b, "simbolo"), _achou) for b in BALDES},
        "forma": {b: _razao(_de(pos, b, "forma"), _achou) for b in BALDES},
        "nao_classificado": sum(r.classe_erro == "nao_classificado" for r in pos),
    }
    for nome, nega in POLITICAS.items():
        out[nome] = _politica(pos, neg, nega)
    return out


def selecionar(projeto: Path, limite: int) -> list[dict]:
    linhas = [
        json.loads(x)
        for x in (CORPUS / "entradas.jsonl").read_text().splitlines()
        if x.strip()
    ]
    entradas = [
        e
        for e in linhas
        if e["rotulo"] == "negativo" or str(e.get("reproduz")).lower() == "true"
    ]
    extra = CORPUS / "entradas_inline_neg.jsonl"
    if extra.is_file():
        entradas += [json.loads(x) for x in extra.read_text().splitlines() if x.strip()]
    entradas = [
        e for e in entradas if Path(e["project_root"]) == projeto and compila(e)
    ]
    return entradas[:limite] if limite else entradas


def compila(e: dict) -> bool:
    """Snap que não compila não é o programa que rodou (o juiz exclui o mesmo, nomeado)."""
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            compile((CORPUS / e["snap"]).read_text(errors="ignore"), e["id"], "exec")
    except (SyntaxError, ValueError):
        return False
    return True


def selftest() -> int:
    raiz = Path(__file__).resolve().parents[1]
    casos = {
        "forma: list.get": classe_erro(
            "AttributeError: 'list' object has no attribute 'get'"
        )
        == "forma",
        "forma: NoneType.group": classe_erro(
            "'NoneType' object has no attribute 'group'"
        )
        == "forma",
        "símbolo: módulo": classe_erro(
            "module 'unicodedata' has no attribute 'combending'"
        )
        == "simbolo",
        "símbolo: classe do projeto": classe_erro(
            "'Recibo' object has no attribute 'total'"
        )
        == "simbolo",
        "símbolo: import": classe_erro("cannot import name 'x' from 'scripts.y'")
        == "simbolo",
        "símbolo: nome": classe_erro("NameError: name 'm2' is not defined")
        == "simbolo",
        "não classificado nomeado": classe_erro("ZeroDivisionError: division by zero")
        == "nao_classificado",
        "escopo: nome próprio": escopo(
            {"code": "reportUndefinedVariable", "message": '"x" is not defined'},
            "",
            raiz,
            raiz / "a.py",
        )
        == "proprio",
        "escopo: módulo externo": escopo(
            {
                "code": "reportAttributeAccessIssue",
                "message": '"combending" is not a known attribute of module "unicodedata"',
            },
            "",
            raiz,
            raiz / "a.py",
        )
        == "externo",
        "escopo: membro de classe builtin = forma": escopo(
            {
                "code": "reportAttributeAccessIssue",
                "message": 'Cannot access attribute "get" for class "str"',
            },
            "",
            raiz,
            raiz / "a.py",
        )
        == "forma",
        "escopo: classe desconhecida = projeto (lado conservador)": escopo(
            {
                "code": "reportAttributeAccessIssue",
                "message": 'Cannot access attribute "total" for class "Recibo"',
            },
            "",
            raiz,
            raiz / "a.py",
        )
        == "projeto",
        "escopo: módulo do projeto": escopo(
            {
                "code": "reportAttributeAccessIssue",
                "message": '"x" is not a known attribute of module "scripts.juiz_motor_vgp"',
            },
            "",
            raiz,
            raiz / "a.py",
        )
        == "projeto",
        "escopo: dunder de classe = dinâmico": escopo(
            {
                "code": "reportAttributeAccessIssue",
                "message": 'Cannot access attribute "__wrapped__" for class "FixtureFunction"',
            },
            "",
            raiz,
            raiz / "a.py",
        )
        == "dinamico",
        "escopo: nome sob import * = dinâmico": escopo(
            {"code": "reportUndefinedVariable", "message": '"HTML" is not defined'},
            "from harness import *\nprint(HTML)\n",
            raiz,
            raiz / "a.py",
        )
        == "dinamico",
        "escopo: nome sem import * continua próprio": escopo(
            {"code": "reportUndefinedVariable", "message": '"HTML" is not defined'},
            "from harness import x\nprint(HTML)\n",
            raiz,
            raiz / "a.py",
        )
        == "proprio",
        "negação: união não nega": negacao(
            {
                "code": "reportAttributeAccessIssue",
                "message": 'Cannot access attribute "a" for class "X | None"',
            }
        )
        is None,
    }
    for nome, ok in casos.items():
        print(f"  {'OK ' if ok else 'FALHA'} {nome}")
    return 0 if all(casos.values()) else 1


def resumo(regs: list[Registro], neg: int) -> dict:
    tempos = sorted(r.ms for r in regs)
    pol = politicas(regs)
    return {
        "entradas": len(regs),
        "negativos": neg,
        "p50_ms": tempos[len(tempos) // 2] if tempos else None,
        "p95_ms": p95(tempos),
        **{
            k: v for k, v in pol.items() if not isinstance(v, dict) or "falsos" not in v
        },
        "politicas": {
            k: {"deny": v["deny"], "falso_deny": v["falso_deny"]}
            for k, v in pol.items()
            if isinstance(v, dict) and "falsos" in v
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Sonda da camada 2 (pyright-langserver) no corpus V0d"
    )
    ap.add_argument("--projeto", type=Path, default=Path.home() / "projects/analise")
    ap.add_argument("--regua", choices=("estrita", "instante"), default="instante")
    ap.add_argument("--limite", type=int, default=0)
    ap.add_argument("--json", type=Path)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    projeto = a.projeto.resolve()
    entradas = selecionar(projeto, a.limite)
    est_todo = estado(CORPUS, a.regua)
    lsp = iniciar(projeto)
    try:
        regs = [
            medir_entrada(lsp, projeto, e, est_todo.get(e["id"], {})) for e in entradas
        ]
    finally:
        lsp.p.kill()
    r = resumo(regs, sum(e["rotulo"] == "negativo" for e in entradas))
    if a.json:
        a.json.write_text(
            json.dumps(
                {**r, "regua": a.regua, "registros": [vars(x) for x in regs]},
                ensure_ascii=False,
                indent=1,
            )
        )
    print(json.dumps({"regua": a.regua, **r}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
