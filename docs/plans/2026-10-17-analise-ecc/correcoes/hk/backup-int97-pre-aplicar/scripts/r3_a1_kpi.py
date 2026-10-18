#!/usr/bin/env python3
"""r3_a1_kpi — porte FIEL do agregador do KPI `context_budget` (para a paridade do S-3).

Fonte da verdade (crates/touring-cli/src/cli/context_budget.rs):
  · `injection_family`      :141-185  (tabela :142-178, catch-all :184)
  · `count_propositions`    :196-203
  · `hook_contexts`         :218-251
  · `aggregate_lines`       :505-541  (o laço de injeção)
  · `context_budget`        :780-818  (escolha de transcripts: :791-816)
  · `MAX_TRANSCRIPTS` = 3   :40 · `MAX_BYTES` = 48 MiB :42

## O que a paridade PROVA (e o que não prova)

O S-3 exige que os bytes por família fiquem dentro de 1% do `touring kpi -j`. Isso
só vale se o extrator e o KPI lerem **o mesmo corpus com a mesma regra**. Duas
armadilhas medidas aqui:

1. **O KPI lê 3 transcripts, não todos** (`MAX_TRANSCRIPTS = 3`, `context_budget.rs:40`),
   os mais recentes por mtime (`:801`). Comparar o extrator sobre 456 transcripts
   com o KPI sobre 3 daria um número que não é paridade, é KINDER.
2. **Só `attachment.type == "hook_success"` conta** (`:222`). O `hook_additional_context`
   NÃO entra no KPI — e a definição de "bloco de injeção" do r3 (CRITERIOS §1) cita
   os dois. O universe do r3 é, portanto, um SUPERSET do universo do KPI, e a
   paridade de 1% só vale sobre a interseção (hook_success). Isso é declarado no
   relatório, não escondido.

`hash_of` (`:253-257`) é o `DefaultHasher` do Rust — não reproduzível em Python.
A DUPLICATA é medida por `is_human_turn_text`/hash; aqui a deduplicação é por texto
idêntico, o que é o mesmo predicado com outra função de hash (o Rust usa hash de
`str`; o Python, o próprio texto). A DUPLICATA é a mesma, o número de COLISÕES é
que difere — e colisão de 64-bit em 566 blocos é evento raro, não measurement.
"""
from __future__ import annotations

import json
from collections import OrderedDict
from pathlib import Path

# `injection_family` — context_budget.rs:142-178. ORDEM IMPORTA: a primeira
# agulha que casa vence (:179-183), e `code-mode-deny` vem antes de todas.
FAMILY_TABLE: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("code-mode-deny", ("CODE MODE", "G7 re-inspe", "G10 write", "G1 rajada", "G11 ", "G3 edit")),
    ("touring-suggest", ("TOURING SUGGEST",)),
    ("loop-outer", ("LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·")),
    ("session-start", ("Touring Knowledge", "ATZILUTH", "SCOUT PERP", "TOURING (slim)", "Painel")),
    ("prompt-enhance", ("PROMPT ENHANCEMENT",)),
    ("past-lessons", ("lições de erros passados", "Similar command failed")),
    ("post-tool-echo", ("temporal: reliability", "last fail:", "post-tool-batch")),
)
OTHER = "other"  # o catch-all, :184

# As 6 famílias do TOURING (Errata E1.1) — `prompt-enhance` é do hook do prompt,
# não é nudge de decisão, e E1.1 lista exatamente estas seis.
TOURING_FAMILIES = (
    "touring-suggest", "code-mode-deny", "loop-outer",
    "session-start", "post-tool-echo", "past-lessons",
)

MAX_TRANSCRIPTS = 3   # :40
MAX_BYTES = 48 * 1024 * 1024   # :42

# `hook_contexts` — :218-251: SÓ `hook_success` (`:222`).
HOOK_ATTACHMENT_TYPE = "hook_success"
# As chaves percorridas, nesta ordem: ["stdout", "content"] (:226)
RAW_KEYS = ("stdout", "content")
# E dentro do JSON parseado: hookSpecificOutput.{additionalContext, systemMessage}
# e o systemMessage de topo (:236-243)
HSO_KEYS = ("additionalContext", "systemMessage")


def injection_family(text: str) -> str:
    """`injection_family` — context_budget.rs:141-185. Porte literal da tabela."""
    for name, needles in FAMILY_TABLE:
        for n in needles:
            if n in text:
                return name
    return OTHER


def count_propositions(text: str) -> int:
    """`count_propositions` — context_budget.rs:196-203.

    Conta linhas cujo `trim_start` começa por MUST/SHOULD/MAY — a densidade que a
    invariante de injeção (:187-194) exige.
    """
    n = 0
    for line in text.split("\n"):
        t = line.lstrip()
        if t.startswith("MUST") or t.startswith("SHOULD") or t.startswith("MAY"):
            n += 1
    return n


def hook_contexts(rec: dict) -> list[str]:
    """`hook_contexts` — context_budget.rs:218-251.

    Só `attachment.type == "hook_success"` (:222). Para cada chave de
    `["stdout", "content"]` (:226): tenta parsear como JSON; se casar, puxa
    `hookSpecificOutput.{additionalContext, systemMessage}` e o `systemMessage` de
    topo; se NÃO casar, o texto cru entra mesmo assim (:246-247) — o hook
    imprimiu texto puro e ele ainda ocupa a janela.
    """
    att = rec.get("attachment")
    if not isinstance(att, dict):
        return []
    if att.get("type") != HOOK_ATTACHMENT_TYPE:
        return []
    out: list[str] = []
    for key in RAW_KEYS:
        raw = att.get(key)
        if not isinstance(raw, str) or not raw.strip():
            continue
        try:
            parsed = json.loads(raw)
        except ValueError:
            # :246-247 — não é JSON: o texto puro ainda custa a janela
            out.append(raw)
            continue
        if isinstance(parsed, dict):
            hso = parsed.get("hookSpecificOutput")
            if isinstance(hso, dict):
                for k in HSO_KEYS:
                    s = hso.get(k)
                    if isinstance(s, str):
                        out.append(s)
            s = parsed.get("systemMessage")
            if isinstance(s, str):
                out.append(s)
    return out


def transcripts_do_kpi(transcript_dir: Path) -> list[Path]:
    """`context_budget.rs:791-816` — os transcripts que o KPI LÊ.

    Só `*.jsonl` (:794), ordenados por mtime MAIS RECENTE PRIMEIRO (:801) e
    limitados a `MAX_TRANSCRIPTS` (:40, aplicado em :805). Reproduzir isto é o que
    torna a paridade possível — comparar populations diferentes não é paridade.
    """
    arquivos = [p for p in transcript_dir.iterdir() if p.suffix == ".jsonl"]
    arquivos.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return arquivos[:MAX_TRANSCRIPTS]


def agregar(paths: list[Path]) -> dict:
    """Reproduz o laço de injeção de `aggregate_lines` (:516-541) sobre os
    transcripts dados. Devolve o agregado por família, na MESMA forma do KPI.

    Não reproduz `hash_of` (DefaultHasher do Rust, :253-257): a deduplicação usa
    o texto idêntico, que é o MESMO predicado com outra função de hash.
    """
    fam = OrderedDict()
    seen: dict[str, int] = OrderedDict()
    total_bytes = 0
    total_blocks = 0
    dup_bytes = 0
    dup_blocks = 0
    bytes_scanned = 0
    capped = False
    n_transcripts = 0

    for p in paths:
        if bytes_scanned >= MAX_BYTES:      # :806-809
            capped = True
            break
        try:
            content = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        bytes_scanned += len(content.encode("utf-8", errors="replace"))
        n_transcripts += 1
        for line in content.splitlines():
            try:
                rec = json.loads(line)
            except ValueError:
                continue
            for text in hook_contexts(rec):
                familia = injection_family(text)
                ln = len(text.encode("utf-8"))
                props = count_propositions(text)
                total_bytes += ln
                total_blocks += 1
                e = fam.setdefault(familia, {
                    "blocks": 0, "bytes": 0, "propositions": 0,
                    "zero_proposition_blocks": 0,
                    "duplicate_bytes": 0, "duplicate_blocks": 0,
                })
                e["bytes"] += ln
                e["blocks"] += 1
                e["propositions"] += props
                if props == 0:
                    e["zero_proposition_blocks"] += 1
                seen[text] = seen.get(text, 0) + 1
                if seen[text] > 1:
                    dup_bytes += ln
                    dup_blocks += 1
                    e["duplicate_bytes"] += ln
                    e["duplicate_blocks"] += 1
    return {
        "transcripts_scanned": n_transcripts,
        "bytes_scanned": bytes_scanned,
        "capped": capped,
        "injected_bytes": total_bytes,
        "injected_blocks": total_blocks,
        "duplicate_bytes": dup_bytes,
        "duplicate_blocks": dup_blocks,
        "by_family": fam,
    }
