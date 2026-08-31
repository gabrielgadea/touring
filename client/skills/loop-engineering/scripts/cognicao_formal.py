#!/usr/bin/env python3
"""cognicao_formal — F4 dos Mundos da Criação: o ciclo de medição formal.

O alvo do treino (Gabriel, 30/08/2026) é a OPERAÇÃO FORMAL, invariante de
domínio: dar forma (emanação→concepção), estruturar (cadeia/fases) e testar o
esquema contra a realidade. Este módulo é a régua dessa forma:

- ``esquema``   — as 7 operações NOMEADAS (nomear torna o esquema evocável;
                  Gick & Holyoak: schema induction exige nome + superfícies
                  variadas com esquema constante).
- ``medir``     — dado um texto (o prompt cru de uma criação), detecta quais
                  operações já estão presentes ANTES de o rito perguntar →
                  ``cognicao.antecipacao_ratio``. Curva subindo = Sistema 1
                  operando o esquema (a internalização vira número).
- ``registrar`` — grava COMO Gabriel decidiu num gate humano (decisão,
                  previsão, pergunta-que-destravou) com facetas
                  ``#domain:cognicao-gabriel``.
- ``padrao``    — o agregado que o Painel de Atziluth devolve (média, curva,
                  operações mais ausentes, domínios praticados).

Detector v0: heurístico LEXICAL — cobre o que declara e SÓ o que declara
(um texto pode conter a operação em formulação que as assinaturas não
alcançam; a v1 pode usar juiz semântico). Fail-open em tudo: este módulo
jamais bloqueia uma sessão.

Journal: ``~/.claude/touring/cognicao_journal.jsonl`` (override:
``COGNICAO_JOURNAL``). Formato por linha: JSON com ``kind`` ∈
{antecipacao, registro, painel_emitido}.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# O esquema formal — as 7 operações nomeadas (a constante pedagógica central).
# Cada nome é curto e evocável DE PROPÓSITO: o objetivo é que Gabriel evoque
# "a Pergunta da Fronteira" fora deste sistema, em qualquer domínio da vida.
# ---------------------------------------------------------------------------

ESQUEMA: list[dict[str, str]] = [
    {
        "chave": "emanacao",
        "nome": "A Pergunta da Emanação",
        "pergunta": "O que quer nascer? (a intenção crua, declarada)",
        "assinatura": r"(a inten[cç][aã]o [eé]|o que eu quero [eé]|quero que\b|o objetivo [eé]|a ideia [eé]|percebo que (precisamos|podemos)|pensei que precisamos)",
    },
    {
        "chave": "finalidade",
        "nome": "A Pergunta do Telos",
        "pergunta": "Para quê? Para quem? (a finalidade além do artefato)",
        "assinatura": r"(para qu[eê]\b|para quem\b|a fim de|finalidades?\b|prop[oó]sito|de (forma|modo) que)",
    },
    {
        "chave": "imagem",
        "nome": "A Pergunta da Imagem",
        "pergunta": "Como é o pronto? (o estado final, descrito como imagem)",
        "assinatura": r"(resultado final|quando (estiver )?pronto|entreg[aá]vel|deve (ficar|conter|ter|produzir|apresentar)|formato final|se parece)",
    },
    {
        "chave": "fronteira",
        "nome": "A Pergunta da Fronteira",
        "pergunta": "O que fica fora? (anti-goals; o que esta criação NÃO é)",
        "assinatura": r"(n[aã]o (deve|quero|precisa|[eé] (o caso|sobre))|fora do escopo|exceto\b|sem incluir|anti-?goal|limites? d[oe])",
    },
    {
        "chave": "cadeia",
        "nome": "A Pergunta da Cadeia",
        "pergunta": "Quais os elos se→então até o objetivo? (o caminho causal)",
        "assinatura": r"(etapas?\b|fases?\b|passo a passo|se\s*(->|→)\s*ent[aã]o|primeiro\b.{3,80}(depois|em seguida|ent[aã]o)|encadeamento|sequ[eê]ncia de)",
    },
    {
        "chave": "custo-invisivel",
        "nome": "A Pergunta do Custo Invisível",
        "pergunta": "O que pode dar errado? O que foi descartado sem admitir? (pré-mortem)",
        "assinatura": r"(riscos?\b|pode dar errado|trade-?offs?|alternativas?\b|pr[eé]-?mortem|descartei|custos? (oculto|invis[ií]vel)|contrapartida)",
    },
    {
        "chave": "pronto",
        "nome": "A Pergunta do Pronto",
        "pergunta": "Como saberemos, medido? (critério de aceitação — nunca sensação)",
        "assinatura": r"(crit[eé]rios? de (aceita[cç][aã]o|pronto|valida[cç][aã]o)|como saberemos|medid[oa]s?\b|kpis?\b|gates?\b|exit ?(0|code)|testes? que prov)",
    },
]

_COMPILADAS: dict[str, re.Pattern[str]] = {
    op["chave"]: re.compile(op["assinatura"], re.IGNORECASE) for op in ESQUEMA
}


def journal_path() -> Path:
    """Resolve o journal (env ``COGNICAO_JOURNAL`` vence; default no harness)."""
    env = os.environ.get("COGNICAO_JOURNAL")
    if env:
        return Path(env)
    return Path.home() / ".claude" / "touring" / "cognicao_journal.jsonl"


def journal_append(entry: dict[str, Any]) -> bool:
    """Anexa uma linha ao journal; fail-open (False em erro, nunca exceção)."""
    try:
        path = journal_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
        return True
    except OSError:
        return False


def journal_lines(kind: str | None = None, last_n: int = 200) -> list[dict[str, Any]]:
    """Últimas ``last_n`` entradas (opcionalmente filtradas por ``kind``).

    Linha ilegível é pulada — o journal é append-only de processos que podem
    morrer no meio (o mesmo contrato do run_journal do touring).
    """
    path = journal_path()
    if not path.exists():
        return []
    out: list[dict[str, Any]] = []
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError:
        return []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue
        if kind is None or entry.get("kind") == kind:
            out.append(entry)
    return out[-last_n:]


# ---------------------------------------------------------------------------
# medir — o antecipacao_ratio
# ---------------------------------------------------------------------------


def medir_texto(texto: str) -> dict[str, Any]:
    """Detecta quais das 7 operações formais o texto já cobre.

    Retorna presentes/ausentes/ratio + o trecho que casou (transparência: o
    sinal viaja com o veredito, nunca só o número — a régua se mostra).
    """
    presentes: list[str] = []
    sinais: dict[str, str] = {}
    for op in ESQUEMA:
        m = _COMPILADAS[op["chave"]].search(texto)
        if m:
            presentes.append(op["chave"])
            sinais[op["chave"]] = m.group(0)[:60]
    ausentes = [op["chave"] for op in ESQUEMA if op["chave"] not in presentes]
    return {
        "presentes": presentes,
        "ausentes": ausentes,
        "ratio": round(len(presentes) / len(ESQUEMA), 3),
        "sinais": sinais,
        "detector": "heuristico-lexical-v0",
    }


def cmd_medir(args: argparse.Namespace) -> int:
    """Mede um texto (arg, arquivo ou stdin) e opcionalmente grava no journal."""
    if args.texto:
        texto = args.texto
    elif args.arquivo:
        try:
            texto = Path(args.arquivo).read_text(encoding="utf-8")
        except OSError as e:
            print(json.dumps({"error": f"arquivo ilegível: {e}"}))
            return 1
    else:
        texto = sys.stdin.read()
    resultado = medir_texto(texto)
    if args.gravar:
        resultado_journal = {
            "kind": "antecipacao",
            "ts": int(time.time()),
            "ratio": resultado["ratio"],
            "presentes": resultado["presentes"],
            "dominio": args.dominio,
            "hash": hashlib.sha256(texto.encode()).hexdigest()[:12],
        }
        resultado["gravado"] = journal_append(resultado_journal)
    print(json.dumps(resultado, ensure_ascii=False, indent=2))
    return 0


# ---------------------------------------------------------------------------
# registrar — o espelho dos gates humanos
# ---------------------------------------------------------------------------


def cmd_registrar(args: argparse.Namespace) -> int:
    """Registra como Gabriel decidiu num gate humano (journal + memória)."""
    ts = int(time.time())
    entry = {
        "kind": "registro",
        "tipo": args.tipo,
        "ts": ts,
        "texto": args.texto,
        "dominio": args.dominio,
        "criacao": args.criacao,
        "pergunta_destravou": args.pergunta_destravou,
    }
    ok = journal_append(entry)
    memoria = False
    if not os.environ.get("COGNICAO_NO_MEMORY"):
        # Best-effort: a memória do touring é o grafo de longo prazo; o journal
        # local é a fonte da série. Falha aqui nunca derruba o registro.
        try:
            key = f"cognicao:{args.tipo}:{ts}"
            r = subprocess.run(
                ["touring", "memory", "store", key, args.texto,
                 "--tag", "#domain:cognicao-gabriel", "--tag", "#kind:event"],
                capture_output=True, timeout=10, check=False,
            )
            memoria = r.returncode == 0
        except (OSError, subprocess.SubprocessError):
            memoria = False
    print(json.dumps({"gravado": ok, "memoria": memoria, "ts": ts}))
    return 0 if ok else 1


# ---------------------------------------------------------------------------
# padrao — o agregado que o Painel de Atziluth devolve
# ---------------------------------------------------------------------------


def padrao_data(last_n: int = 50) -> dict[str, Any]:
    """Agrega o journal em um retrato do treino: curva, ausências, domínios."""
    medicoes = journal_lines("antecipacao", last_n)
    registros = journal_lines("registro", last_n)
    if not medicoes and not registros:
        return {"n": 0, "nota": "sem medições ainda — a série começa no primeiro uso"}
    out: dict[str, Any] = {"n": len(medicoes)}
    if medicoes:
        ratios = [m.get("ratio", 0.0) for m in medicoes]
        out["ratio_medio"] = round(sum(ratios) / len(ratios), 3)
        ult = ratios[-5:]
        out["ratio_ultimos5"] = round(sum(ult) / len(ult), 3)
        ausencias: dict[str, int] = {}
        chaves = {op["chave"] for op in ESQUEMA}
        for m in medicoes:
            for chave in chaves - set(m.get("presentes", [])):
                ausencias[chave] = ausencias.get(chave, 0) + 1
        out["mais_ausentes"] = sorted(ausencias, key=lambda k: -ausencias[k])[:3]
        dominios: dict[str, int] = {}
        for m in medicoes:
            d = m.get("dominio") or "sem-dominio"
            dominios[d] = dominios.get(d, 0) + 1
        out["dominios"] = dominios
        par = _par_cross_dominio(medicoes, chaves)
        if par:
            out["par_cross_dominio"] = par
    out["registros"] = len(registros)
    return out


def _par_cross_dominio(medicoes: list[dict[str, Any]],
                       chaves: set[str]) -> dict[str, Any] | None:
    """As 2 medições mais recentes de domínios DISTINTOS — a matéria-prima do
    apontador de isomorfismo (Gick & Holyoak: o esquema só se abstrai quando a
    mesma forma é vista em superfícies diferentes). O apontamento semântico é
    do modelo no turno; aqui viaja só o par endereçado."""
    ultima = None
    for m in reversed(medicoes):
        d = m.get("dominio")
        if not d:
            continue
        if ultima is None:
            ultima = m
        elif d != ultima.get("dominio"):
            def _lado(x: dict[str, Any]) -> dict[str, Any]:
                return {
                    "dominio": x.get("dominio"),
                    "hash": x.get("hash", ""),
                    "ausentes": sorted(chaves - set(x.get("presentes", []))),
                }
            return {"a": _lado(ultima), "b": _lado(m)}
    return None


def cmd_padrao(args: argparse.Namespace) -> int:
    """Imprime o padrão agregado (consumido pelo Painel e por Gabriel)."""
    print(json.dumps(padrao_data(args.n), ensure_ascii=False, indent=2))
    return 0


def cmd_esquema(_args: argparse.Namespace) -> int:
    """Imprime as 7 operações nomeadas — o artefato pedagógico do rito."""
    print(json.dumps(
        [{k: op[k] for k in ("chave", "nome", "pergunta")} for op in ESQUEMA],
        ensure_ascii=False, indent=2,
    ))
    return 0


def main(argv: list[str] | None = None) -> int:
    """CLI: esquema | medir | registrar | padrao."""
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("esquema", help="as 7 operações formais nomeadas")

    m = sub.add_parser("medir", help="antecipacao_ratio de um texto")
    m.add_argument("--texto", help="texto inline")
    m.add_argument("--arquivo", help="caminho de arquivo")
    m.add_argument("--gravar", action="store_true", help="anexa ao journal")
    m.add_argument("--dominio", default=None,
                   help="superfície praticada: codigo|texto|negocio|vida")

    r = sub.add_parser("registrar", help="registra decisão/previsão de gate humano")
    r.add_argument("--tipo", required=True,
                   choices=["decisao", "previsao", "destravamento"])
    r.add_argument("--texto", required=True)
    r.add_argument("--dominio", default=None)
    r.add_argument("--criacao", default=None, help="slug da criação")
    r.add_argument("--pergunta-destravou", dest="pergunta_destravou", default=None)

    pa = sub.add_parser("padrao", help="agregado do journal (o espelho)")
    pa.add_argument("--n", type=int, default=50)

    args = p.parse_args(argv)
    return {"esquema": cmd_esquema, "medir": cmd_medir,
            "registrar": cmd_registrar, "padrao": cmd_padrao}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
