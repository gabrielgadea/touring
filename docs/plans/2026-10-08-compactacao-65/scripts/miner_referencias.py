#!/usr/bin/env python3
"""MINERADOR DE REFERÊNCIAS — F1, opção (d) do canvas.

O PROBLEMA QUE ELE ATENDE
-------------------------
A compactação 65 moveu 30.169 chars para `references/*.md`. A conservação prova
que eles estão em disco; não prova que chegam ao contexto. Varri 554 hooks:
nenhum carrega um `references/*.md`. O conteúdo chega quando o MODELO decide ler
o ponteiro — e nada no sistema registra se essa leitura acontece.

Este script responde a pergunta que um contador sem denominador não responde:
**este destino deveria voltar para o contexto fixo, ou continua como
referência?** E — o mais importante — declara o que ele NAO decide.

CUSTO DE RUNTIME: ZERO. Não é hook: lê os transcritos que o Claude Code já
grava. Nada novo roda por REQUEST.

POR QUE CONTROLES SAO OBRIGATORIOS AQUI
----------------------------------------
Medido nesta auditoria: o instrumento de glob falhou 4 vezes e o de busca sem
acento 2, cada uma produzindo um NEGATIVO bem-formado ("0 leituras", "orfao",
"nunca carrega"). Um instrumento que só produz números não tem como se
defender. Este traz:
  - CONTROLE POSITIVO — um arquivo que eu SEI ter lido nesta sessao. Se der 0,
    o minerador está cego.
  - CONTROLE DE PADRAO — lido mais vezes nesta sessao; se 0, o parser falhou.
  - CONTROLE NEGATIVO — caminho inexistente. Se der >0, há falso positivo.
Sem os três, a saída é DESCARTÁVEL e o script sai com rc 2.

USO
---
  miner_referencias.py                        # destinos da compactacao 65
  miner_referencias.py --json                 # legivel por maquina
  miner_referencias.py --destinos a.md --pasta /caminho
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
from collections import defaultdict

HOME = pathlib.Path.home()
TRANSCRITOS = HOME / ".claude/projects"

#: Os 7 destinos da compactacao 65 + os 2 que ganharam `paths:` e sairam do
#: contexto fixo. Os 9 juntos respondem: "o que acontece com o conteudo que
#: saiu da sessao?"
DESTINOS: dict[str, pathlib.Path] = {
    "git-safety-protocol.md":
        HOME / ".claude/skills/Touring/references/git-safety-protocol.md",
    "touring-process-hygiene.md":
        HOME / ".claude/skills/Touring/references/touring-process-hygiene.md",
    "touring-per-project.md":
        HOME / ".claude/skills/Touring/references/touring-per-project.md",
    "auto-load-rule.md":
        HOME / ".claude/skills/touring-elite/references/auto-load-rule.md",
    "touring-cli-index-tiers-2-3.md":
        HOME / ".claude/skills/Touring/references/touring-cli-index-tiers-2-3.md",
    "touring-4-pillars-provenance.md":
        HOME / ".claude/skills/Touring/references/touring-4-pillars-provenance.md",
    "00-core-governance-restauracao.md":
        pathlib.Path("/home/gabrielgadea/projects/analise/.claude/docs/"
                     "rules-provenance/00-core-governance-restauracao.md"),
    "disk-hygiene.md": HOME / ".claude/rules/disk-hygiene.md",
    "vgp-protocol.md":
        pathlib.Path("/home/gabrielgadea/projects/analise/.claude/rules/"
                     "vgp-protocol.md"),
}

CTL_POSITIVO = "context_budget.rs"
CTL_PADRAO = "daemon_main.rs"
CTL_NEGATIVO = "arquivo-que-nao-existe-9f3a2b.md"

#: O que o numero NAO decide — impresso junto, nunca escondido.
LIMITS = (
    "Leitura ZERO nao prova orfao: um destino criado hoje nao teve tempo de ser "
    "lido. So conta depois de um intervalo em que o tema apareceu.",
    "Leitura ALTA nao prova valor: pode ser uma unica sessao de depuracao.",
    "O numero nao mede CORRECAO — se alguem leu e agiu errado, o mesmo contador "
    "conta igual.",
    "Nao mede conteudo lido: uma leitura truncada conta como leitura.",
)


def _read_do_bloco(bloco) -> str | None:
    """O `file_path` de um tool_use `Read` dentro de um bloco de conteudo.

    Isolado para segurar o CC: a versao anterior embutia o parse JSON e a
    navegacao na lista no mesmo laco e passou de 15.
    """
    if not isinstance(bloco, list):
        return None
    for c in bloco:
        if not isinstance(c, dict) or c.get("type") != "tool_use":
            continue
        if c.get("name") != "Read":
            continue
        entrada = c.get("input")
        if not isinstance(entrada, dict):
            continue
        fp = entrada.get("file_path")
        return fp if isinstance(fp, str) else ""
    return None


def _alvo_da_linha(linha: str) -> str | None:
    """Basename do arquivo lido nesta linha, ou None se a linha nao interessa."""
    try:
        rec = json.loads(linha)
        bloco = (rec.get("message") or {}).get("content")
    except (ValueError, TypeError, AttributeError):
        return None
    fp = _read_do_bloco(bloco)
    return None if fp is None else pathlib.Path(fp).name


def _leituras_do_arquivo(caminho: pathlib.Path, alvo: set[str],
                         sessoes: dict[str, set[str]],
                         contagem: dict[str, int]) -> int:
    """Conta chamadas `Read` de UM transcrito. Devolve o total de leituras."""
    total = 0
    with caminho.open(encoding="utf-8", errors="ignore") as fh:
        for linha in fh:
            # pre-filtro barato ANTES do parse — sem isto, 1.860 arquivos viram
            # gigabytes de json sob o json.loads.
            if '"Read"' not in linha:
                continue
            nome = _alvo_da_linha(linha)
            if nome is None:
                continue
            total += 1
            contagem[nome] = contagem.get(nome, 0) + 1
            if nome in alvo:
                sessoes[nome].add(caminho.name)
    return total


def medir(destinos: dict[str, pathlib.Path]) -> dict:
    """A medicao. Nao imprime e nao formata — devolve a estrutura."""
    alvo = set(destinos) | {CTL_POSITIVO, CTL_PADRAO, CTL_NEGATIVO}
    sessoes: dict[str, set[str]] = defaultdict(set)
    contagem: dict[str, int] = {}
    transcritos = sorted(TRANSCRITOS.rglob("*.jsonl")) if TRANSCRITOS.is_dir() else []
    leituras_totais = 0
    for t in transcritos:
        leituras_totais += _leituras_do_arquivo(t, alvo, sessoes, contagem)

    ctl = {"positivo": contagem.get(CTL_POSITIVO, 0),
           "padrao": contagem.get(CTL_PADRAO, 0),
           "negativo": contagem.get(CTL_NEGATIVO, 0)}
    return {
        "transcritos": len(transcritos),
        "leituras_de_arquivo_no_total": leituras_totais,
        "controles": ctl,
        "instrumento_confiavel": (ctl["positivo"] > 0 and ctl["padrao"] > 0
                                  and ctl["negativo"] == 0),
        "destinos": {n: {"existe": p.exists(), "leituras": contagem.get(n, 0),
                         "sessoes_distintas": len(sessoes.get(n, ()))}
                     for n, p in sorted(destinos.items())},
        "limites": list(LIMITS),
    }


def imprimir(r: dict) -> None:
    """A apresentacao. Separada de `medir` para que a medicao fique pura."""
    c = r["controles"]
    print(f"transcritos: {r['transcritos']}   "
          f"leituras de arquivo no acervo: {r['leituras_de_arquivo_no_total']}\n")
    print("CONTROLES — sem os tres, o resultado e descartavel:")
    print(f"  positivo  {CTL_POSITIVO:<36} {c['positivo']:>5}  "
          f"{'ok' if c['positivo'] else 'FALHOU — o minerador esta cego'}")
    print(f"  padrao    {CTL_PADRAO:<36} {c['padrao']:>5}  "
          f"{'ok' if c['padrao'] else 'FALHOU'}")
    print(f"  negativo  {CTL_NEGATIVO:<36} {c['negativo']:>5}  "
          f"{'ok' if c['negativo'] == 0 else 'FALHOU — falso positivo'}")
    print(f"  VEREDITO DO INSTRUMENTO: "
          f"{'confiavel' if r['instrumento_confiavel'] else 'NAO CONFIAVEL'}\n")
    print(f"  {'destino':<44} {'existe':>7} {'leituras':>9} {'sessoes':>8}")
    for nome, d in r["destinos"].items():
        existe = "sim" if d["existe"] else "NAO"
        print(f"  {nome:<44} {existe:>7} {d['leituras']:>9} "
              f"{d['sessoes_distintas']:>8}")
    print("\nO QUE ISTO NAO DECIDE:")
    for limite in r["limites"]:
        print(f"  - {limite}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--destinos", nargs="*", default=[])
    ap.add_argument("--pasta", default="", help="raiz dos --destinos relativos")
    args = ap.parse_args(argv)

    destinos = dict(DESTINOS)
    if args.destinos:
        base = pathlib.Path(args.pasta) if args.pasta else pathlib.Path(".")
        for d in args.destinos:
            destinos[pathlib.Path(d).name] = base / d

    r = medir(destinos)
    if args.json:
        print(json.dumps(r, ensure_ascii=False, indent=1))
    else:
        imprimir(r)
    return 0 if r["instrumento_confiavel"] else 2


if __name__ == "__main__":
    sys.exit(main())
