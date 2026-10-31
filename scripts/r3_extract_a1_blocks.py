#!/usr/bin/env python3
"""r3_extract_a1_blocks — S-3: extrai os BLOCOS DE INJEÇÃO (A1) e o universo
de EXEMPT + duplicatas, com a PARIDADE medida contra o KPI `context_budget`.

Plano: docs/plans/2026-10-02-jev-harness-r3/plan.md:81-88
Critérios: CRITERIOS-ACEITACAO.md §1, §3, Errata E1.1
Porte do KPI: scripts/r3_a1_kpi.py — fiel à RÉGUA da taxonomia
(`crates/touring-foundation/src/injection_family.rs`, a canônica desde a S-4 de
26/10/2026) e ao agregador (`crates/touring-cli/src/cli/context_budget.rs`).

## Duas populations, e por que isso é declarado e não escondido

| population | o que é | para quê |
|---|---|---|
| **KPI** | `attachment.type == "hook_success"` (context_budget.rs:248) | a paridade de 1% (plan S-3) |
| **r3 universe** | `hook_success` **e** `hook_additional_context` | o gate de relevância, que tem de ver também o que o KPI ignora |

CRITERIOS §1 define "bloco de injeção" como `hook_additional_context`/`hook_success`;
o KPI conta só o segundo. O universe do r3 é um **SUPERSET** do KPI, e a paridade só
vale sobre a interseção. O relatório de C-A1-4 traz as duas com a decomposição
Touring × harness explícita (E1.1).

## EXEMPT (CRITERIOS §1)

Família `code-mode-deny`, ou qualquer bloco com marcador MUST / REGRA # / CRITICAL /
BLOCK / deny / erro de hook / permissão. **Nenhum braço pode descartar um EXEMPT** —
a lista de marcadores é congelada no dataset (`exempt_markers.json`, hash no manifesto).

## Privacidade

Saída em `~/.claude/touring/r3-data/a1_blocks.jsonl` — o repo é público e bloco de
injeção carrega texto de comando/prompt. Nada de texto vai para o git.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter, OrderedDict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import r3_a1_kpi as kpi  # noqa: E402

PROJ = os.path.expanduser("~/.claude/projects/-home-gabrielgadea-projects-touring")
OUT_DIR = Path("~/.claude/touring/r3-data").expanduser()
SINCE = "2026-08-26"

# EXEMPT — CRITERIOS §1. A lista vai para o dataset e é congelada por hash; mudá-la
# depois de ver o resultado invalida o braço (o mesmo princípio do τ).
EXEMPT_MARKERS = (
    "MUST", "REGRA #", "CRITICAL", "BLOCK", "deny", "DENY",
    "hook error", "hook_error", "permission",
)
# A família code-mode-deny é EXEMPT por definição (CRITERIOS §1).
EXEMPT_FAMILIES = ("code-mode-deny",)
EXEMPT_FAMILY = "code-mode-deny"

DUPLICATE_WINDOW_TURNS = 5   # plano S-3: "duplicata exata (janela de 5 turnos)"

# Tipos de attachment que o r3 universe enxerga (o KPI só o primeiro).
R3_ATTACHMENT_TYPES = ("hook_success", "hook_additional_context")


def is_exempt(text: str, family: str) -> bool:
    """EXEMPT = família code-mode-deny OU qualquer marcador da lista congelada."""
    if family in EXEMPT_FAMILIES:
        return True
    return any(m in text for m in EXEMPT_MARKERS)


def _json(line: str) -> dict | None:
    """Uma linha do transcript parseada, ou None se estiver malformada.

    Um journal truncado não pode virar "nenhuma oferta" em silêncio — nem derrubar
    a extração inteira.
    """
    try:
        rec = json.loads(line)
    except ValueError:
        return None
    return rec if isinstance(rec, dict) else None


def _bloco_de(session: str, turno: int, tipo: str, text: str, dup: bool) -> dict:
    """UM bloco de injeção, já classificado. Puro — o `extract` só orquestra."""
    familia = kpi.injection_family(text)
    props = kpi.count_propositions(text)
    return {
        "session": session, "turno": turno, "attachment_type": tipo,
        "family": familia,
        "is_touring_family": familia in kpi.TOURING_FAMILIES,
        "is_exempt": is_exempt(text, familia),
        "is_duplicate": dup,
        "bytes": len(text.encode("utf-8")),
        "propositions": props,
        "head": text.split("\n", 1)[0][:120],
    }


def _acumula(agg_kpi: dict, por_familia: dict, bloco: dict) -> None:
    """Acumula UM bloco no agregado do KPI e no agregado por família."""
    if bloco["attachment_type"] == kpi.HOOK_ATTACHMENT_TYPE:
        agg_kpi["bytes"] += bloco["bytes"]
        agg_kpi["blocks"] += 1
    c = por_familia.setdefault(bloco["family"], Counter())
    c["bytes"] += bloco["bytes"]
    c["blocks"] += 1
    c["propositions"] += bloco["propositions"]
    if bloco["propositions"] == 0:
        c["zero_prop"] += 1
    if bloco["is_exempt"]:
        c["exempt"] += 1


def _json(line: str) -> dict | None:
    """Uma linha do transcript parseada, ou None se estiver malformada.

    Um journal truncado não pode virar "nenhuma oferta" em silêncio — nem derrubar
    a extração inteira.
    """
    try:
        rec = json.loads(line)
    except ValueError:
        return None
    return rec if isinstance(rec, dict) else None


def _bloco_de(session: str, turno: int, tipo: str, text: str, dup: bool) -> dict:
    """UM bloco de injeção, já classificado. Puro — o `extract` só orquestra."""
    familia = kpi.injection_family(text)
    props = kpi.count_propositions(text)
    return {
        "session": session, "turno": turno, "attachment_type": tipo,
        "family": familia,
        "is_touring_family": familia in kpi.TOURING_FAMILIES,
        "is_exempt": is_exempt(text, familia),
        "is_duplicate": dup,
        "bytes": len(text.encode("utf-8")),
        "propositions": props,
        "head": text.split("\n", 1)[0][:120],
    }


def _acumula(agg_kpi: dict, por_familia: dict, bloco: dict) -> None:
    """Acumula UM bloco no agregado do KPI e no agregado por família."""
    if bloco["attachment_type"] == kpi.HOOK_ATTACHMENT_TYPE:
        agg_kpi["bytes"] += bloco["bytes"]
        agg_kpi["blocks"] += 1
    c = por_familia.setdefault(bloco["family"], Counter())
    c["bytes"] += bloco["bytes"]
    c["blocks"] += 1
    c["propositions"] += bloco["propositions"]
    if bloco["propositions"] == 0:
        c["zero_prop"] += 1
    if bloco["is_exempt"]:
        c["exempt"] += 1


def _eventos_do_transcript(p: Path) -> list[tuple[str, str, str]]:
    """`(kind, tipo, texto)` dos eventos de um transcript, em ordem.

    `kind` é `user_turn` (um turno de pessoa), `bash` (comando pedido) ou `offer`
    (bloco injetado — `tipo` é o `attachment.type`).
    """
    eventos: list[tuple[str, str, str]] = []
    try:
        content = p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return eventos
    for line in content.splitlines():
        try:
            rec = json.loads(line)
        except ValueError:
            continue
        if rec.get("type") == "user":
            eventos.append(("user_turn", "", ""))
        c = (rec.get("message") or {}).get("content")
        if not isinstance(c, list):
            continue
        for b in c:
            if b.get("type") == "tool_use" and b.get("name") == "Bash":
                cmd = (b.get("input") or {}).get("command", "")
                if cmd:
                    eventos.append(("bash", "", cmd))
            elif b.get("type") == "tool_result":
                att = b.get("attachment")
                if isinstance(att, dict) and att.get("type") in R3_ATTACHMENT_TYPES:
                    eventos.append(("offer", att.get("type", ""), ""))
    return eventos


def extract(transcript_paths: list[Path], since: str = SINCE) -> tuple[list[dict], dict]:
    """Extrai os blocos do universe r3, marca EXEMPT e duplicata, e agrega."""
    blocos: list[dict] = []
    agg_kpi = {"bytes": 0, "blocks": 0}
    por_familia: OrderedDict[str, Counter] = OrderedDict()
    # deduplicação: texto idênculo visto nos últimos DUPLICATE_WINDOW_TURNS turnos
    visto_por_texto: dict[str, list[int]] = {}

    for p in transcript_paths:
        turno = 0
        try:
            content = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in content.splitlines():
            rec = _json(line)
            if rec is None:
                continue
            if rec.get("type") == "user":
                turno += 1
                continue
            att = rec.get("attachment")
            if not isinstance(att, dict) or att.get("type") not in R3_ATTACHMENT_TYPES:
                continue
            for text in _texts_do_attachment(att):
                # duplicata exata dentro da janela de 5 turnos (S-3)
                hist = visto_por_texto.setdefault(text, [])
                dup = any(abs(x - turno) <= DUPLICATE_WINDOW_TURNS for x in hist)
                hist.append(turno)
                b = _bloco_de(p.stem[:8], turno, att.get("type", ""), text, dup)
                blocos.append(b)
                _acumula(agg_kpi, por_familia, b)

    resumo = {
        "n_blocos": len(blocos),
        "por_familia": {k: dict(v) for k, v in por_familia.items()},
        "universe_kpi": agg_kpi,
        "touring_only": {
            "bytes": sum(b["bytes"] for b in blocos if b["is_touring_family"]),
            "blocks": sum(1 for b in blocos if b["is_touring_family"]),
        },
        "harness_only": {
            "bytes": sum(b["bytes"] for b in blocos if not b["is_touring_family"]),
            "blocks": sum(1 for b in blocos if not b["is_touring_family"]),
        },
    }
    return blocos, resumo


def _texts_do_attachment(att: dict) -> list[str]:
    """Mesma travessia de `hook_injections` (context_budget.rs:244-295), sem o filtro
    de `type` — quem filtra o `type` aqui é o universe r3, não esta função."""
    out = []
    for key in kpi.RAW_KEYS:
        raw = att.get(key)
        if not isinstance(raw, str) or not raw.strip():
            continue
        try:
            parsed = json.loads(raw)
        except ValueError:
            out.append(raw)
            continue
        if isinstance(parsed, dict):
            hso = parsed.get("hookSpecificOutput")
            if isinstance(hso, dict):
                for k in kpi.HSO_KEYS:
                    s = hso.get(k)
                    if isinstance(s, str):
                        out.append(s)
            s = parsed.get("systemMessage")
            if isinstance(s, str):
                out.append(s)
    return out


def paridade(max_tentativas: int = 3) -> dict:
    """Mede a paridade com `touring kpi -j` sobre a MESMA população (plan S-3).

    Comparar com um JSON de KPI guardado em disco é comparar com o passado: o
    corpus cresce a cada turno, e o KPI lê os 3 transcripts mais recentes
    (context_budget.rs:40). As duas leituras têm de ser do mesmo instante.

    E "o mesmo instante" não basta: o porte e o KPI escolhem, cada um, os 3
    transcripts mais recentes do PROJETO — de todas as sessões, não só desta —, e
    outra sessão que escreve entre as duas leituras troca a população. Em
    04/10/2026 o KPI leu um transcript do touring-a4 que o porte não leu
    (36.459 + 14.633 = 51.092 B de `code-mode-deny`, exato) e o teste reprovou
    um porte correto (cross-audit X9). Por isso a lista é lida antes e depois do
    KPI, e a contagem que o KPI varreu é comparada à do porte: se mudou, repete;
    se não estabiliza em `max_tentativas`, declara "corpus em movimento" — nunca
    um veredito sobre populações diferentes. A mesma lição vale para quem lê a
    razão de uma família: os blocos contados são de todas as sessões do projeto.
    """
    ultimo = ""
    for _ in range(max_tentativas):
        lido = _ler_mesma_populacao()
        if "erro" in lido:
            return lido
        if lido["estavel"]:
            return _comparar(lido["arquivos"], lido["meu"], lido["kpi"])
        ultimo = lido["divergencia"]
    return {"erro": f"corpus em movimento: a população mudou nas {max_tentativas} leituras ({ultimo})"}


class _KpiIndisponivel(Exception):
    """O `touring kpi -j` não devolveu um `context_budget` legível."""


def _kpi_context_budget() -> dict:
    """O `context_budget` do KPI real. Toda falha vira [`_KpiIndisponivel`]: um `touring` fora do
    PATH levanta `FileNotFoundError`, que é `OSError` e seria lido como "transcripts indisponíveis"."""
    try:
        p = subprocess.run(["touring", "kpi", "-j"], capture_output=True, text=True,
                           timeout=180)
        return json.loads(p.stdout)["context_budget"]
    except Exception as e:  # noqa: BLE001 — a falha é o dado, não uma exceção
        raise _KpiIndisponivel(e) from e


def _ler_mesma_populacao() -> dict:
    """Uma leitura do porte e do KPI, e se as duas viram a mesma população."""
    # A leitura do diretório de transcripts está DENTRO do try: um diretório
    # ausente (CI) levanta FileNotFoundError e uma leitura negada (sandbox)
    # levanta PermissionError, e nenhum dos dois é um erro de teste — é a
    # ausência do dado, que o relatório tem de declarar em vez de estourar.
    try:
        arquivos = kpi.transcripts_do_kpi(Path(PROJ))
        meu = kpi.agregar(arquivos)
        k = _kpi_context_budget()
        depois = kpi.transcripts_do_kpi(Path(PROJ))
    except _KpiIndisponivel as e:
        return {"erro": f"KPI indisponível: {e}"}
    except OSError as e:
        return {"erro": f"transcripts indisponiveis: {e}"}
    antes_nomes = [a.name for a in arquivos]
    depois_nomes = [d.name for d in depois]
    estavel = antes_nomes == depois_nomes and k.get("transcripts_scanned") == meu.get("transcripts_scanned")
    divergencia = (f"antes={[n[:8] for n in antes_nomes]} depois={[n[:8] for n in depois_nomes]} "
                   f"varridos porte={meu.get('transcripts_scanned')} kpi={k.get('transcripts_scanned')}")
    return {"arquivos": arquivos, "meu": meu, "kpi": k, "estavel": estavel, "divergencia": divergencia}


def _comparar(arquivos: list[Path], meu: dict, k: dict) -> dict:
    """O veredito de paridade, só para uma população que as duas leituras viram igual."""
    kf = k["by_family"]
    linhas, todos_ok = [], True
    for fam in sorted(set(list(meu["by_family"]) + list(kf))):
        mb = meu["by_family"].get(fam, {}).get("bytes", 0)
        kb = kf.get(fam, {}).get("bytes", 0)
        delta = 100.0 * (mb - kb) / kb if kb else 0.0
        if abs(delta) > 1.0:
            todos_ok = False
        linhas.append({"family": fam, "meu_bytes": mb, "kpi_bytes": kb,
                       "delta_pct": round(delta, 4)})
    return {
        "transcripts": [p.name[:8] for p in arquivos],
        "kpi_transcripts_scanned": k.get("transcripts_scanned"),
        "familias": linhas,
        "paridade_1pct": todos_ok,
        "meu_blocks": meu["injected_blocks"],
        "kpi_blocks": sum(v.get("blocks", 0) for v in kf.values()),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--projeto-dir", default=PROJ)
    ap.add_argument("--out", default=str(OUT_DIR / "a1_blocks.jsonl"))
    ap.add_argument("--resumo-out", default=str(OUT_DIR / "a1_resumo.json"))
    ap.add_argument("--desde", default=SINCE)
    args = ap.parse_args()

    d = Path(args.projeto_dir)
    todos = [p for p in d.glob("*.jsonl") if p.stat().st_mtime > 0]
    todos.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    blocos, resumo = extract(todos, since=args.desde)

    par = paridade()
    resumo["paridade"] = par
    resumo["exempt_markers"] = list(EXEMPT_MARKERS)
    resumo["duplicate_window_turns"] = DUPLICATE_WINDOW_TURNS
    resumo["r3_attachment_types"] = list(R3_ATTACHMENT_TYPES)

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        for b in blocos:
            f.write(json.dumps(b, ensure_ascii=False) + "\n")
    Path(args.resumo_out).write_text(json.dumps(resumo, ensure_ascii=False, indent=1),
                                     encoding="utf-8")

    print(f"blocos (universe r3) : {resumo['n_blocos']}")
    print(f"  só do Touring       : {resumo['touring_only']['blocks']} blocos / "
          f"{resumo['touring_only']['bytes']} B")
    print(f"  harness/other      : {resumo['harness_only']['blocks']} blocos / "
          f"{resumo['harness_only']['bytes']} B")
    ex = sum(1 for b in blocos if b["is_exempt"])
    dup = sum(1 for b in blocos if b["is_duplicate"])
    print(f"  EXEMPT             : {ex} · duplicatas(5 turnos): {dup}")
    print()
    print("paridade com `touring kpi -j`:")
    for l in par.get("familias", []):
        marca = "" if abs(l["delta_pct"]) <= 1.0 else "  <<<"
        print(f"  {l['family']:18s} meu {l['meu_bytes']:8d}  kpi {l['kpi_bytes']:8d}  "
              f"{l['delta_pct']:+6.2f}%{marca}")
    print(linha_paridade(par))
    print(f"-> {out}")
    return 0


def linha_paridade(par: dict) -> str:
    """The parity verdict line. A measurement that did not happen is NOT a failed one: "corpus em
    movimento" or an unavailable KPI printed "NAO <<<", the same line as a real 1% divergence."""
    if "erro" in par:
        return f"  PARIDADE 1%: NAO MEDIDA — {par['erro']}"
    return f"  PARIDADE 1%: {'OK' if par.get('paridade_1pct') else 'NAO <<<'}"


if __name__ == "__main__":
    sys.exit(main())
