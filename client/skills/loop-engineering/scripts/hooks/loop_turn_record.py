#!/usr/bin/env python3
"""loop_turn_record.py — o executor da CLASSE B: o registro do turno, escrito por código.

P4/S-4.3 do plano `2026-09-04-economia-de-contexto`. Fecha a última subtarefa
aberta, e fecha-a pelo mecanismo que o diagnóstico D1 nomeou.

O QUE ESTE ARQUIVO RESOLVE
    O manifesto do OUTER classifica cada artefato por CUSTO DE CONTEXTO:

        A · derivado por código   — `loop_diagnose.py`, um exit code, o trace
        B · subproduto da resposta — o que o modelo já escreveu no turno
        C · escrito à mão          — o ledger, a estratégia: um DESVIO do raciocínio

    Medido em 965 avaliações: os de classe A saem (não custam raciocínio); os de
    classe C não saem — 74% dos runs NUNCA os produziram, e os que "passaram"
    foram liberados pelo runaway guard em `cont=30`. Um gate contornado em 74%
    dos casos por esgotamento de contador não é gate: é pedágio.

    A tese do Gabriel (04/09/2026): *"talvez o raciocínio e 'gate' possa ser como
    é esta skill, na qual a etapa de interação é o próprio gate"*. Está certa, e
    o motivo é preciso — o que torna um `decision-canvas` barato não é a
    interação, é que **a saída do gate É o entregável**. O custo marginal é zero
    porque o artefato já ia existir.

    A classe B é essa forma, materializada: a exigência continua sendo um
    ARTEFATO EM DISCO (Lei L3 intacta — nada de "eu digo que raciocinei"), mas
    quem o escreve é este script, lendo o que o turno já produziu. Muda o AUTOR
    e o MOMENTO, nunca a exigência.

POR QUE NÃO INTERROMPE O RACIOCÍNIO
    Roda no Stop hook, isto é, DEPOIS que o turno terminou. Nesse instante o
    transcript já contém a resposta inteira. Não há nada a interromper: o texto
    de que o registro é feito já foi escrito, para o humano, pelo motivo do
    humano. O gate cobra do plano de EXECUÇÃO o que antes cobrava do plano de
    RACIOCÍNIO — que é literalmente o defeito D1.

INVARIANTE
    Fail-open em tudo. Se o transcript não existe, se o turno é vazio, se o
    disco recusa — sai 0 e diz o que não deu. Um gate cujo executor pode
    derrubar o turno seria pior que o pedágio que ele substitui.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from pathlib import Path

# Um cabeçalho, uma linha de tabela, um item de lista, uma citação: a ESPINHA
# estrutural da resposta.
SPINE = re.compile(r"^\s*(#{1,6}\s|\||[-*+]\s|\d+[.)]\s|>\s|```)")
# …e a ênfase, ONDE QUER QUE ESTEJA na linha. A primeira versão exigia negrito
# no INÍCIO da linha e extraiu 2 linhas de um turno inteiro de trabalho: num
# turno assim a decisão viaja em negrito no meio da frase e em identificadores
# entre crases, quase nunca num cabeçalho. A regra estreita não errou o
# instrumento — errou o que conta como estrutura.
EMPHASIS = re.compile(r"\*\*[^*]+\*\*|`[^`]+`")
# Um bloco de código no meio da resposta é evidência: entra inteiro.
FENCE = re.compile(r"^\s*```")
MAX_SPINE_LINES = 400
# As mais recentes ficam: num turno longo é o fim que explica onde ele parou.
MAX_ACTION_LINES = 60


def transcript_dir_for(home: Path, project_root: Path) -> Path:
    """O diretório de transcripts do Claude Code para uma raiz de projeto.

    Mesma derivação que `context_budget.rs::transcript_dir_for` — todo byte não
    alfanumérico vira `-`. Duas implementações da mesma regra é exatamente o
    defeito que este plano diagnostica (D2), então a regra fica declarada nos
    dois lugares com o mesmo teste: `~/projects/touring` -> `-home-...-touring`.
    """
    slug = "".join(c if c.isalnum() else "-" for c in str(project_root))
    return home / ".claude" / "projects" / slug


def newest_transcript(cwd: str) -> Path | None:
    """O transcript mais recente do projeto, ou None."""
    try:
        d = transcript_dir_for(Path.home(), Path(cwd).resolve())
        files = sorted(d.glob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)
        return files[0] if files else None
    except Exception:  # noqa: BLE001 — fail-open
        return None


# Marca um record `user` que o HARNESS escreveu, não a pessoa.
#
# O Claude Code arquiva como record de usuário várias coisas de máquina: a saída
# de um slash command, o banner de caveat, e o resumo injetado quando o contexto
# é compactado. Nenhuma carrega `tool_result`, então o predicado óbvio ("record
# de usuário sem tool_result é turno humano") conta todas — e aqui isso decide
# ONDE O REGISTRO COMEÇA: um `/compact` no meio do trabalho truncava o registro
# na cauda do turno.
#
# Medido em 04/09/2026 sobre 5 transcripts reais: 35 de 172 (20,3%).
#
# FONTE ÚNICA: esta lista casa, palavra por palavra, com `NON_HUMAN_TURN_MARKERS`
# em `crates/touring-cli/src/cli/context_budget.rs`. Duas implementações da mesma
# regra é o defeito D2 do próprio plano — por isso existe
# `scripts/test_human_turn_predicate_parity.py`, que lê O EXECUTOR do lado Rust
# e exige que esta declaração case com ele (o guard cruzado D8).
NON_HUMAN_TURN_MARKERS = (
    "This session is being continued from a previous conversation",
    "<local-command-stdout>",
    "<local-command-caveat>",
    "Caveat: The messages below were generated by the user while running local commands",
)


def is_human_turn_text(text: str) -> bool:
    """Uma pessoa escreveu isto, ou o harness? Ver `NON_HUMAN_TURN_MARKERS`.

    Estreito de propósito: um turno legítimo pode carregar um `<system-reminder>`
    anexado, e a dúvida conta como humana — errar para o lado de fechar o turno
    perde contexto do registro, nunca inventa.
    """
    return not any(m in text for m in NON_HUMAN_TURN_MARKERS)


def is_human_record(content) -> bool:
    """O record fecha o turno? Só se uma pessoa o escreveu."""
    if isinstance(content, str):
        return is_human_turn_text(content)
    if isinstance(content, list):
        if any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content):
            return False
        text = "".join(
            b.get("text") or "" for b in content if isinstance(b, dict) and b.get("type") == "text"
        )
        return is_human_turn_text(text)
    # Nem string nem lista: forma desconhecida. Fecha o turno (conservador — o
    # registro fica menor, jamais contaminado por outro turno).
    return True


def action_brief(block: dict) -> str:
    """O que uma chamada FEZ, numa linha.

    Um turno de trabalho decide agindo, não narrando: sem isto o registro de um
    turno com dezenas de chamadas seria quase vazio, e um artefato de gate quase
    vazio é decoração — a falha "infra desligada não é infra pronta".
    """
    tool = block.get("name") or "?"
    inp = block.get("input")
    if not isinstance(inp, dict):
        return tool
    for key in ("command", "file_path", "pattern", "query", "path", "url", "prompt"):
        val = inp.get(key)
        if isinstance(val, str) and val.strip():
            one = " ".join(val.split())
            return f"{tool}: {one[:160]}"
    return tool


def last_assistant_turn(transcript: Path) -> tuple[str, int, list[str]]:
    """Todo texto — e toda ação — do assistente desde a última fala HUMANA.

    Devolve `(texto, blocos, ações)`. A fronteira é o turno humano e não o
    último record porque uma resposta longa é escrita em vários records: cortar
    no último devolveria a frase final e chamaria isso de registro.
    """
    chunks: list[str] = []
    actions: list[str] = []
    blocks = 0
    try:
        with transcript.open(errors="replace") as fh:
            for raw in fh:
                try:
                    rec = json.loads(raw)
                except Exception:  # noqa: BLE001 — linha corrompida não invalida o resto
                    continue
                kind = rec.get("type")
                content = (rec.get("message") or {}).get("content")
                if kind == "user":
                    # Um record de usuário que carrega tool_result é a máquina
                    # respondendo, não a pessoa falando: não fecha o turno. E
                    # nem todo record SEM tool_result é fala humana — ver
                    # `is_human_turn_text`, medido em 20,3% dos casos.
                    if is_human_record(content):
                        chunks.clear()
                        actions.clear()
                        blocks = 0
                elif kind == "assistant":
                    if isinstance(content, str):
                        chunks.append(content)
                        blocks += 1
                    elif isinstance(content, list):
                        for b in content:
                            if not isinstance(b, dict):
                                continue
                            if b.get("type") == "text":
                                text = b.get("text") or ""
                                if text.strip():
                                    chunks.append(text)
                                    blocks += 1
                            elif b.get("type") == "tool_use":
                                actions.append(action_brief(b))
    except Exception:  # noqa: BLE001 — fail-open
        return "", 0, []
    return "\n".join(chunks), blocks, actions


def spine_of(text: str) -> list[str]:
    """A espinha estrutural da resposta: o que um leitor futuro consulta.

    É um digest DERIVADO, não uma truncagem: mantém cabeçalho, tabela, lista,
    negrito e bloco de código — as formas em que uma decisão viaja — e descarta
    a prosa de ligação. A medição de P5 (04/09) diz por que essa é a forma
    certa: as linhas que voltam a ser usadas estão distribuídas por igual pelo
    documento (decis 9,6% · 5,7% · 6,3% · 6,6% · 7,4% · 6,8% · 6,0% · 6,3% ·
    6,8% · 5,9%), então cortar por POSIÇÃO perde o que foi usado. Cortar por
    ESTRUTURA, não.
    """
    out: list[str] = []
    in_fence = False
    for line in text.split("\n"):
        if FENCE.match(line):
            in_fence = not in_fence
            out.append(line.rstrip())
            continue
        if in_fence or SPINE.match(line) or EMPHASIS.search(line):
            out.append(line.rstrip())
        if len(out) >= MAX_SPINE_LINES:
            out.append(f"… espinha truncada em {MAX_SPINE_LINES} linhas")
            break
    return out


def render(marker: dict, text: str, blocks: int, actions: list[str] | None = None) -> str:
    """O registro, como documento OKF."""
    actions = actions or []
    spine = spine_of(text)
    stamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    topic = str(marker.get("topic") or marker.get("flow") or "turno").strip()
    plan = marker.get("bundle") or ""
    head = [
        "---",
        "type: TurnRecord",
        f'title: "Registro do turno — {topic[:80]}"',
        'description: "Artefato de classe B: a espinha estrutural da resposta '
        'do turno, extraída por código depois que o turno terminou."',
        f"plan: {Path(plan).name if plan else 'n/a'}",
        f"timestamp: {stamp}",
        "okf_version: 1",
        "---",
        "",
        f"# Registro do turno — {topic}",
        "",
        f"> Escrito por `loop_turn_record.py` (classe B) em {stamp}. "
        f"O modelo não parou para escrevê-lo: a fonte é o próprio turno, "
        f"lido do transcript **depois** que ele terminou.",
        "",
        f"- blocos de resposta: **{blocks}**",
        f"- caracteres do turno: **{len(text)}**",
        f"- linhas de espinha extraídas: **{len(spine)}**",
        f"- ações executadas: **{len(actions)}**",
        "",
        "## Espinha da resposta",
        "",
    ]
    if not spine:
        head.append("_(o turno não trouxe estrutura — nem cabeçalho, nem tabela, "
                    "nem lista, nem ênfase, nem bloco de código)_")
    tail: list[str] = []
    if actions:
        tail = ["", "## O que o turno fez", ""]
        counts: dict[str, int] = {}
        for a in actions:
            counts[a.split(":")[0]] = counts.get(a.split(":")[0], 0) + 1
        tail.append(
            "Por ferramenta: "
            + ", ".join(f"**{k}** {v}×" for k, v in sorted(counts.items(), key=lambda kv: -kv[1]))
        )
        tail.append("")
        for a in actions[-MAX_ACTION_LINES:]:
            tail.append(f"- `{a}`")
        if len(actions) > MAX_ACTION_LINES:
            tail.append(f"- … {len(actions) - MAX_ACTION_LINES} ações anteriores elididas "
                        f"(as mais recentes ficam)")
    return "\n".join(head + spine + tail) + "\n"


def materialize(marker: dict, out_path: Path, transcript: Path | None = None) -> dict:
    """Escreve o registro do turno. Nunca levanta; sempre devolve o veredito."""
    cwd = marker.get("cwd") or marker.get("scope") or os.getcwd()
    # Precedência: o que o CHAMADOR passou > o que o HARNESS nomeou > palpite.
    #
    # `newest_transcript` escolhe pelo mtime, e isso é um palpite que erra na
    # topologia real: com duas sessões CC no mesmo projeto (o cenário que a
    # REGRA #19 existe para tratar), o mais recente pode ser o da OUTRA sessão —
    # e o registro deste turno sairia com o trabalho de outra pessoa dentro.
    # O payload do hook carrega `transcript_path`; usar o que o harness nomeia
    # é identidade, não heurística.
    named = marker.get("transcript_path")
    t = transcript or (Path(named) if named else None) or newest_transcript(str(cwd))
    if t is None or not t.exists():
        return {"written": False, "reason": "sem transcript legivel"}
    text, blocks, actions = last_assistant_turn(t)
    if not text.strip() and not actions:
        return {"written": False, "reason": "turno sem texto nem acao do assistente"}
    try:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        body = render(marker, text, blocks, actions)
        out_path.write_text(body, encoding="utf-8")
    except Exception as exc:  # noqa: BLE001 — fail-open
        return {"written": False, "reason": f"escrita recusada: {exc}"}
    return {
        "written": True,
        "path": str(out_path),
        "bytes": len(body),
        "turn_chars": len(text),
        "blocks": blocks,
    }


def record_id(marker: dict | None) -> str:
    """Identidade ESTÁVEL do registro: uma armação do flow, um arquivo.

    A primeira forma usava um carimbo de tempo, e o gate reavalia várias vezes
    por run — cada avaliação escrevia um arquivo novo. Duas consequências, ambas
    ruins: o diretório cresce sem limite, e a cláusula vira vacuamente
    verdadeira (o artefato "aparece" sempre porque acabou de ser criado), o que
    é um gate que não gateia.

    Derivar de `session_id` + `flow_armed_at` torna o artefato IDEMPOTENTE por
    armação: a segunda avaliação encontra o arquivo da primeira e não reescreve
    nada. É a mesma disciplina da REGRA #17 — identidade derivada de critério,
    nunca de ordem ou de relógio.
    """
    if marker:
        sess = str(marker.get("session_id") or "nosession")[:12]
        armed = marker.get("flow_armed_at") or marker.get("created_at")
        if armed:
            return f"{sess}-{int(float(armed))}"
        return sess
    return time.strftime("%Y%m%dT%H%M%S")


def default_out(scope: str, bundle: str | None, marker: dict | None = None) -> Path:
    """Onde o registro mora: no bundle quando há um, senão no estado da skill.

    Nunca dentro do projeto sem bundle — um artefato de gate não polui a árvore
    de código de quem não pediu por ele.
    """
    rid = record_id(marker)
    if bundle:
        return Path(bundle) / "turns" / f"turn-{rid}.md"
    slug = "".join(c if c.isalnum() else "-" for c in str(scope))
    return Path.home() / ".claude" / "loop-engineering" / "turns" / slug / f"turn-{rid}.md"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--marker", help="JSON do marker (ou '-' para stdin)")
    ap.add_argument("--scope", default=os.getcwd())
    ap.add_argument("--bundle")
    ap.add_argument("--transcript")
    ap.add_argument("--out")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)

    marker: dict = {}
    if a.marker == "-":
        try:
            marker = json.loads(sys.stdin.read() or "{}")
        except Exception:  # noqa: BLE001
            marker = {}
    elif a.marker:
        try:
            marker = json.loads(a.marker)
        except Exception:  # noqa: BLE001
            marker = {}
    marker.setdefault("cwd", a.scope)
    marker.setdefault("scope", a.scope)
    if a.bundle:
        marker["bundle"] = a.bundle

    out = Path(a.out) if a.out else default_out(marker["scope"], marker.get("bundle"))
    res = materialize(marker, out, Path(a.transcript) if a.transcript else None)
    print(json.dumps(res, ensure_ascii=False, indent=2 if not a.json else None))
    # Fail-open por construção: um executor de gate jamais derruba o turno.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
