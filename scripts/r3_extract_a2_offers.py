#!/usr/bin/env python3
"""r3_extract_a2_offers — S-2: extrai OFERTAS DE ROTA (A2) dos transcripts e
calcula B0 = porte fiel de `classify_route_outcome` aplicado sequencialmente.

Plano: docs/plans/2026-10-02-jev-harness-r3/plan.md:72-79
Critérios: CRITERIOS-ACEITACAO.md §1, §2, Errata E1.2/E1.4
Porte B0: scripts/r3_b0.py (fiel a cli_suggester.rs:4482-4509)

## O marcador da oferta, e por que este

`record_route_offer` (cli_suggester.rs:4533) é chamado de UM lugar só: o deny do
G1, `cli_suggester.rs:5670`. O que o modelo vê é a resposta do deny
(`cli_suggester.rs:5685-5698`), cuja 1ª linha é sempre:

    [CODE MODE · rajada] Nª inspeção `<classe>` em 300s — a 1ª já executou intacta; ...

e cujo corpo carrega a rota: `touring run --lang <lang> --code '<programa>'`.

A oferta também vive em `route_offers()`, um cache `moka` com **TTL de 60 s**
(`cli_suggester.rs:4572`) e sem journal — nada é persistido. Os transcripts são,
portanto, a ÚNICA fonte durável do par oferta→desfecho, e o marcador acima é o
que a torna reconstruível.

## Janela (Errata E1.4)

Os próximos **K = 3 comandos Bash** da mesma sessão, cortados na próxima oferta
da mesma sessão (uma oferta nova substitui a anterior: `record_route_offer`
insere pela chave da sessão, `cli_suggester.rs:4536`).
Texto COMPLETO, sem truncagem (Errata 4, E4.2).

## Privacidade

Este script NUNCA escreve no repositório. A saída vai para
`~/.claude/touring/r3-data/a2_offers.jsonl` (regra do lote: o repo é público e
texto de comando truncado não entra nele).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from r3_b0 import b0_decision

PROJ = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring")
OUT_DIR = Path("~/.claude/touring/r3-data").expanduser()

# Primeiro corte: 2026-08-26 (data pedida no plano S-2).
SINCE = "2026-08-26"
K_WINDOW = 3
# MORTO por decisão (Errata 4, E4.2): o texto do comando é gravado COMPLETO, sem
# truncagem. A 1ª versão truncava `cmds` a 200 chars enquanto calculava o B0 sobre
# a janela inteira, e o B0 guardado deixava de ser reproduzível sobre o texto
# guardado (9 de 198 decisões divergentes). Fica a constante porque o número
# documenta a regressão — se alguém reintroduzir o corte, o teste
# `test_comando_acima_de_200_chars_e_preservado_inteiro` reprova.
MAX_CMD_CHARS = 200

# `cli_suggester.rs:5686` — a 1ª linha do deny do G1, o único call site de
# record_route_offer (:5670).
#
# O `^` estava ERRADO na 1ª versão e custou 82 ofertas: o texto do deny NUNCA
# chega ao transcript sozinho — o Claude Code embrulha o hook em
# `PreToolUse:<Tool> hook error: ` ANTES do payload (medido em 02/10 nos denies
# de 25/09 e de 02/10, ambos com o marcador na posição 28). Ancorar no início
# da string só casa quando o wrapper não está presente. O predicado é "o
# marcador aparece no texto", não "o texto começa pelo marcador".
OFFER_MARKER = re.compile(r"\[CODE MODE · rajada\]")
# citações (is_error != true) agrupadas por sessão, para o resumo — uma sessão sem
# deny genuíno não produz oferta, mas a citação é reportada.
CITADAS_SO: list[tuple[str, int]] = []
# A unidade do shell-quoting é `'` + conteudo + `'`, e a aspa INTERNA aparece
# como `'` \ `'` `'` (abre, barra, aspa, fecha). A 1ª versão do regex tratava
# `\'` como token isolado, que não existe na sequência real: num programa que
# COMEÇA com aspa escapada (`'\''a b'\''`) devolvia `\a b\` em vez de `'a b'`.
# A rota: `touring run --lang <lang> --code '<programa>'`.
#
# **A ÂNCORA É A ASPA DE FECHO SEGUIDA DE QUEBRA.** Medido em 02/10 nos 180
# denies reais: 174 dos programas são MULTILINHA, e a aspa simples interna ao
# programa chega CRUA (o suggester não escapa — `cli_suggester.rs:5685-5698`
# monta `--code '<programa>'` e o rodapé do gate na linha seguinte).
#
# Três gramáticas, três falhas medidas:
#   · `(.*?)` não-guloso → fecha na 1ª aspa interna: devolve um PREFIXO em
#     **91 de 180 (51%)**, uma delas com a rota vazia.
#   · `(.*)` guloso → vai até a ÚLTIMA aspa do texto: come o rodapé inteiro.
#   · `'(.*?)'\s*(?:\n|$)` → ancora na quebra, mas fecha na PRIMEIRA aspa
#     seguida de quebra: ainda cortava 16 programas multilinha com aspa.
#   · **`'(.*)'\n`** → guloso até a ÚLTIMA `'` seguida de quebra: acerta os seis
#     casos de teste e os 180 denies reais.
_LANG_ROTA = re.compile(r"touring run --lang (\w+) --code ")
ROUTE_IN_BODY = re.compile(r"--code '(.*)'\n", re.DOTALL)


def programa_da_rota(texto: str) -> str | None:
    """Extrai o programa da rota, ou None se o texto não trouxer rota."""
    m = ROUTE_IN_BODY.search(texto)
    if not m:
        return None
    return m.group(1)


def lang_da_rota(texto: str) -> str | None:
    """A linguagem proposta na rota (`bash`/`python`), independentemente do programa."""
    m = _LANG_ROTA.search(texto)
    return m.group(1) if m else None


def _iter_events(path: Path):
    """Devolve (ts, kind, payload) por linha do transcript, skipping o malformado.

    Um journal truncado não é motivo para abortar a extração — é motivo para
    contá-lo, senão o silêncio vira 'nenhuma oferta' sem quem diga.
    """
    bad = 0
    with open(path, errors="replace") as f:
        for line in f:
            try:
                d = json.loads(line)
            except ValueError:
                bad += 1
                continue
            ts = d.get("timestamp", "")
            if ts and ts < SINCE:
                continue
            yield ts, d, bad
    if bad:
        yield "", {"__bad_lines__": bad}, bad


def _eventos_do_transcript(path: Path) -> tuple[list[tuple[str, str, str]], int, int]:
    """`(eventos, linhas_malformadas, citacoes)` de um transcript.

    Um evento é `(ts, "bash", comando)` para um comando pedido, `(ts, "offer",
    texto)` para um deny GENUÍNO do G1, e `(ts, "user", "")` para um turno.

    **Oferta = `is_error is True` E o marcador.** Sem o `is_error`, o texto do deny
    que outro comando CITOU (grep em transcript, saída de relatório) entra como
    oferta — medido em 02/10: 200 candidatos, 180 denies genuínos e 20 citações, e
    **4 das citações traziam a rota completa no corpo**, então "marcador + rota
    casada" NÃO discrimina. As citações são contadas e reportadas
    (`n_excluded_quoted`), nunca descartadas em silêncio.
    """
    eventos: list[tuple[str, str, str]] = []
    bad = 0
    citadas = 0
    # `_iter_events` conta as linhas malformadas e fecha com um registro-sentinela que carrega o
    # total (sem `type` nem `message`, logo inerte aqui): o último `bad` visto é o total.
    for ts, d, bad in _iter_events(path):
        if d.get("type") == "user":
            eventos.append((ts, "user", ""))
        c = (d.get("message") or {}).get("content")
        if not isinstance(c, list):
            continue
        for b in c:
            _acumula_evento(b, ts, eventos)
            if (b.get("type") == "tool_result" and isinstance(b.get("content"), str)
                    and OFFER_MARKER.search(b["content"]) and b.get("is_error") is not True):
                citadas += 1
    return eventos, bad, citadas


def _acumula_evento(b: dict, ts: str, eventos: list) -> None:
    """UM bloco de mensagem → 0 ou 1 evento."""
    if b.get("type") == "tool_use" and b.get("name") == "Bash":
        cmd = (b.get("input") or {}).get("command", "")
        if cmd:
            eventos.append((ts, "bash", cmd))
    elif b.get("type") == "tool_result":
        cc = b.get("content")
        if isinstance(cc, str) and cc.strip() and OFFER_MARKER.search(cc) \
                and b.get("is_error") is True:
            eventos.append((ts, "offer", cc))


def _janela_ate_a_proxima_oferta(eventos, i) -> tuple[list[str], bool]:
    """Os próximos K comandos Bash depois do evento `i`, e se uma oferta nova cortou.

    E1.4: a janela é K=3 chamadas **Bash** da mesma sessão, cortada na próxima
    oferta (uma oferta nova substitui a anterior — `record_route_offer` insere
    pela chave da sessão, `cli_suggester.rs:4536`).
    """
    janela: list[str] = []
    for _ts, tipo, texto in eventos[i + 1:]:
        if tipo == "offer":
            return janela, True
        if tipo == "bash":
            janela.append(texto)
            if len(janela) >= K_WINDOW:
                break
    return janela, False


def _oferta_de(path: Path, ts: str, texto: str, janela: list[str], cortada: bool) -> dict:
    """Monta o registro de UMA oferta, com o B0 calculado sobre a janela.

    SEM TRUNCAGEM (Errata 4, E4.2). A 1ª versão guardava `cmds` e `rota_proposta`
    cortados a 200 chars enquanto calculava o B0 sobre a janela completa, e o B0
    guardado deixava de ser reproduzível sobre o texto guardado (9 de 198
    divergentes). O texto continua fora do repo.
    """
    decisao, valor = b0_decision(janela, k=K_WINDOW)
    return {
        "session": path.stem[:8],
        "ts": ts,
        "lang": lang_da_rota(texto),
        "rota_proposta": programa_da_rota(texto),
        "janela_k": K_WINDOW,
        "cmds": list(janela),
        "n_cmds": len(janela),
        "cortada_em_nova_oferta": cortada,
        "b0_decisao": decisao,
        "b0_valor": valor,
    }


def extract_session(path: Path) -> list[dict]:
    """Ofertas + janela K=3 Bash da sessão, em ordem cronológica.

    Uma sessão = um arquivo de transcript. A chave do runtime é a sessão
    (`cli_suggester.rs:4535`), então nunca cruzamos arquivos.
    """
    # 1) passe: os eventos do transcript, em ordem (a distinção oferta/citação
    # está em `_acumula_evento` e em `_eventos_do_transcript`).
    eventos, bad, citadas = _eventos_do_transcript(path)
    ofertas: list[dict] = []
    if not eventos:
        # sessão sem NENHUM deny genuíno: as citações são contadas e o
        # summarise/reportas, mas a sessão não produz oferta. Emitir um registro
        # sem decisão contaminaria o estrato de B0 com um `None`.
        if citadas:
            CITADAS_SO.append((path.stem[:8], citadas))
        return []
    eventos.sort(key=lambda e: e[0])

    # 2) passe: cada oferta abre uma janela; a próxima oferta CORTA a anterior (E1.4)
    for i, (ts, tipo, texto) in enumerate(eventos):
        if tipo != "offer":
            continue
        janela, cortada = _janela_ate_a_proxima_oferta(eventos, i)
        ofertas.append(_oferta_de(path, ts, texto, janela, cortada))

    if ofertas and bad:
        ofertas[0]["linhas_malformadas_no_transcript"] = bad
    if ofertas and citadas:
        # POR SESSAO, uma vez: repetir o mesmo total em cada oferta da sessão
        # faria a soma do relatório multiplicar as citações pelo nº de ofertas.
        ofertas[0]["n_excluded_quoted"] = citadas
    return ofertas


def main() -> int:
    global SINCE
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--projeto-dir", default=PROJ)
    ap.add_argument("--out", default=str(OUT_DIR / "a2_offers.jsonl"))
    ap.add_argument("--desde", default=SINCE)
    args = ap.parse_args()
    SINCE = args.desde

    d = Path(args.projeto_dir)
    todas: list[dict] = []
    for tp in sorted(d.glob("*.jsonl"), key=lambda p: p.stat().st_mtime):
        todas.extend(extract_session(tp))

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        f.writelines(json.dumps(o, ensure_ascii=False) + "\n" for o in todas)

    dec = Counter(o["b0_decisao"] for o in todas)
    vals = Counter(str(o["b0_valor"]) for o in todas)
    sessoes = len({o["session"] for o in todas})
    com_seguimento = sum(1 for o in todas if o["n_cmds"] > 0)
    print(f"ofertas          : {len(todas)}")
    print(f"sessões          : {sessoes}")
    print(f"com ≥1 Bash depois: {com_seguimento} ({100.0 * com_seguimento / max(1, len(todas)):.1f}%)")
    print(f"B0 decisão       : {dict(dec)}")
    print(f"B0 valor         : {dict(vals)}")
    print(f"cortadas por nova oferta: {sum(1 for o in todas if o['cortada_em_nova_oferta'])}")
    citadas = sum(o.get("n_excluded_quoted", 0) for o in todas) + sum(n for _s, n in CITADAS_SO)
    print(f"EXCLUÍDAS por citação (is_error != true): {citadas}  <- contadas, não descartadas em silêncio")
    # invariante: o B0 guardado tem de ser reproduzível sobre o texto guardado
    divergentes = 0
    from r3_b0 import b0_decision
    for o in todas:
        if o.get("b0_decisao") is None:
            continue
        d2, _v2 = b0_decision(o["cmds"], k=o["janela_k"])
        if d2 != o["b0_decisao"]:
            divergentes += 1
    print(f"INVARIANTE B0(cmds guardados) == b0_decisao guardado: "
          f"{'OK, 0 divergentes' if divergentes == 0 else f'{divergentes} DIVERGENTES <<<'}")
    print(f"-> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
