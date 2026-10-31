#!/usr/bin/env python3
"""r3_a1_kpi — porte FIEL do agregador do KPI `context_budget` (para a paridade do S-3).

Duas fontes, e a régua mudou de endereço:

  A RÉGUA da taxonomia — crates/touring-foundation/src/injection_family.rs
    · `FAMILY_MARKERS`   :88   (os marcadores literais; a ORDEM decide)
    · `FAMILY_PATTERNS`  :155  (os padrões de conteúdo)
    · `EMITTERS`         :211  (a marca do emissor → família, C10)
    · `injection_family` :41 · `families` :63 · `attributed_family` :245
    A S-4 (26/10/2026) migrou a taxonomia para este módulo e `context_budget.rs:204`
    re-exporta as funções — a régua é o módulo, não o arquivo que a consome.

  O AGREGADOR — crates/touring-cli/src/cli/context_budget.rs
    · `count_propositions`    :215
    · `hook_injections`       :244   (só `hook_success` :248)
    · `hash_of`               :289
    · `aggregate_lines`       :544   (o laço de injeção)
    · `context_budget_in`     :880   (escolha de transcripts: :909-910)
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

`hash_of` (`:289`) é o `DefaultHasher` do Rust — não reproduzível em Python.
A DUPLICATA é medida por `is_human_turn_text`/hash; aqui a deduplicação é por texto
idêntico, o que é o mesmo predicado com outra função de hash (o Rust usa hash de
`str`; o Python, o próprio texto). A DUPLICATA é a mesma, o número de COLISÕES é
que difere — e colisão de 64-bit em 566 blocos é evento raro, não measurement.
"""
from __future__ import annotations

import json
import re
from collections import OrderedDict
from pathlib import Path

# `injection_family` — a RÉGUA (`crates/touring-foundation/src/injection_family.rs`).
# ORDEM IMPORTA: a primeira agulha que casa vence, e `code-mode-contract` vem
# antes de `code-mode-deny` porque o cabeçalho do contrato começa com as mesmas
# duas palavras do marcador do deny (`CODE MODE — contrato` ⊃ `CODE MODE`).
#
# S-3 (26/10/2026): a taxonomia passou a ser CANÔNICA e por CONTEÚDO — dois
# níveis (marcadores literais e padrões), a mesma ordem e os mesmos nomes da
# régua (`crates/touring-foundation/src/injection_family.rs`) e da política
# (`client/hooks/injection_policy.py`). Este porte é o terceiro espelho: um
# predicado escrito duas vezes nunca se encontra — agora os três são julgados
# pelo mesmo texto, e `test_the_port_mirrors_the_canonical_taxonomy` compara os
# três com o FONTE (nome, ordem, agulhas e padrão).
FAMILY_TABLE: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("code-mode-contract", ("CODE MODE — contrato",)),
    ("code-mode-deny", ("CODE MODE", "G7 re-inspe", "G10 write", "G1 rajada", "G11 ", "G3 edit")),
    ("touring-suggest", ("TOURING SUGGEST",)),
    ("loop-outer", ("LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·")),
    ("session-start", (
        "Touring Knowledge", "ATZILUTH", "SCOUT PERP", "TOURING (slim)", "Painel",
        "Session intelligence", "Touring: collecting session intelligence",
        "Touring: initializing cognitive engine",
    )),
    ("prompt-enhance", ("PROMPT ENHANCEMENT",)),
    ("past-lessons", ("lições de erros passados", "Similar command failed")),
    ("post-tool-echo", ("temporal: reliability", "last fail:", "post-tool-batch")),
    ("evidence-guard", ("EVIDENCE GUARD",)),
    ("debug-stdout", ("[EVENT_READER]", "[POST_QUALITY_GATE", "Hook execution started")),
    ("plugin-hook", ("[SUGESTÃO DE SKILL",)),
    ("pre-write", ("code-workflow", "rust-workflow")),
    ("pre-edit", ("HOTSPOT:", "wiring_predict:", "callers:", "related docs:")),
    ("pre-read", ("defs[", "📌 defs", "🪝 defs")),
    ("post-edit", ("post-edit verification:", "post-read:")),
    ("post-write", ("post-write:",)),
    ("pre-edit-prevention", ("Pre-edit prevention",)),
    ("delegation-nudge", ("[delegation-nudge]",)),
)

#: Os padrões de conteúdo — as formas sem marcador próprio (o payload que o
#: orçamento do turno stripped, a razão de deny do gate, o programa fundido).
FAMILY_PATTERNS: tuple[tuple[str, tuple["re.Pattern[str]", ...]], ...] = (
    ("code-mode-deny", (re.compile(r"\[G\d+\s"),)),
    ("session-contract", (re.compile(r"MUST\s+touring doctor -j"),)),
    ("pipefail-evidence", (re.compile(r"exited 0, but its output shows errors after a pipe"),)),
    ("generic-noise", (re.compile(r"^\[generic\]"),)),
    ("context-monitor", (re.compile(r"Contexto em \d+%"),)),
    ("compaction-gaps", (re.compile(r"\[COMPACTA[ÇC][ÃA]O"),)),
    ("delegation-nudge", (re.compile(r"\[M3 delega"),)),
    ("failure-echo", (re.compile(r"⚡ last fail:"), re.compile(r"failed \(exit \d+\):"))),
    ("quality-gate", (re.compile(r"\[E\d+\s"),)),
    ("touring-suggest", (re.compile(r"(?m)^\s*(?:MUST|SHOULD|MAY)\s+\S"),)),
)
OTHER = "other"  # o catch-all da régua (injection_family.rs:41-50)

# As 6 familias do TOURING (Errata E1.1) — `prompt-enhance` é do hook do prompt,
# não é nudge de decisão, e E1.1 lista exatamente estas seis.
#
# ATENÇÃO (31/10/2026): esta lista é a operacionalização de E1.1 contra a
# taxonomia de então (6 famílias). A S-4 (26/10) expandiu a taxonomia para 18
# marcadores + 10 padrões. MEDIDO em 31/10/2026 17:04 BRT (`ex.paridade()` sobre
# os 3 transcripts mais recentes do projeto — 794.362 B, 23 famílias com bytes,
# delta de 0,00% por família): das 23, 17 são do TOURING — 514.443 B (64,7%) — e
# esta lista as conta como HARNESS. O denominador de E1.1 mede 164.090 B (20,7%);
# com as 17, o do Touring mede 678.533 B (85,4%). A correção exige re-congelar o
# dataset (E1.1 + S-4 do plano r3): decisão do Gabriel, não uma troca silenciosa
# de lista — e um denominador furado é pior do que um denominador declarado.
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
    """`injection_family` — a régua (injection_family.rs). Porte literal.

    Dois níveis, na mesma ordem da régua: os MARCADORES literais primeiro, os
    PADRÕES de conteúdo depois. O primeiro que casa vence.
    """
    for name, needles in FAMILY_TABLE:
        for n in needles:
            if n in text:
                return name
    for name, patterns in FAMILY_PATTERNS:
        if any(p.search(text) for p in patterns):
            return name
    return OTHER


# `EMITTERS` — a régua (injection_family.rs, C10, 17/10/2026): a marca do EMISSOR que o Claude Code grava em
# cada `hook_success` (`command`: o `statusMessage` do hook, ou a linha de comando). `True` = o hook
# é DONO da saída (vence o texto); `False` = só resgata o que o texto deixa em `other`. A primeira
# agulha que casa vence. `injection_family` (só texto) segue intacta: é ELA que o r3 usa para
# definir o dataset congelado (`is_touring_family`); só a paridade do KPI passa pela atribuição.
EMITTERS: tuple[tuple[str, str, bool], ...] = (
    ("analyzing blast radius", "pre-edit", True),
    ("speculative validation", "pre-write", True),
    ("analyzing file context", "pre-read", True),
    ("checking command history", "pre-bash", True),
    ("syntax pre-check", "pre-edit-prevention", True),
    ("verifying edit quality", "post-edit", True),
    ("registering new file", "post-write", True),
    ("evidence_guard.py", "evidence-guard", True),
    ("CLAUDE_PLUGIN_ROOT", "plugin-hook", True),
    ("prose_integrity_gate.py", "prose-integrity", True),
    ("check_context.sh", "context-monitor", True),
    ("delegation-nudge", "delegation-nudge", True),
    ("touring cli-suggest", "touring-suggest", False),
    ("loop_resume.py", "loop-outer", False),
    ("touring-hook post-bash", "post-tool-echo", False),
)


def attributed_family(text: str, emitter: str | None) -> str:
    """`attributed_family` — a régua (injection_family.rs, C10). Dono do emissor > agulhas do texto >
    reserva do emissor > `other`. Sem marca (Claude Code antigo) é só o texto."""
    claimed = None
    if emitter:
        for needle, family, owns in EMITTERS:
            if needle in emitter:
                claimed = (family, owns)
                break
    if claimed and claimed[1]:
        return claimed[0]
    family = injection_family(text)
    if family == OTHER and claimed:
        return claimed[0]
    return family


def count_propositions(text: str) -> int:
    """`count_propositions` — context_budget.rs:215.

    Conta linhas cujo `trim_start` começa por MUST/SHOULD/MAY — a densidade que a
    invariante de injeção do KPI exige.
    """
    n = 0
    for line in text.split("\n"):
        t = line.lstrip()
        if t.startswith("MUST") or t.startswith("SHOULD") or t.startswith("MAY"):
            n += 1
    return n


def hook_contexts(rec: dict) -> list[str]:
    """Só os textos de `hook_injections` — a forma de antes da atribuição por emissor."""
    return [text for _, text in hook_injections(rec)]


def hook_injections(rec: dict) -> list[tuple[str | None, str]]:
    """`hook_injections` — context_budget.rs:244 (C10; era `hook_contexts`): pares
    (marca do emissor, texto). A marca é `attachment.command`.

    Só `attachment.type == "hook_success"` (:248). Para cada chave de
    `["stdout", "content"]`: tenta parsear como JSON; se casar, puxa
    `hookSpecificOutput.{additionalContext, systemMessage}` e o `systemMessage` de
    topo; se NÃO casar, o texto cru entra mesmo assim — o hook imprimiu texto
    puro e ele ainda ocupa a janela.
    """
    att = rec.get("attachment")
    if not isinstance(att, dict):
        return []
    if att.get("type") != HOOK_ATTACHMENT_TYPE:
        return []
    command = att.get("command")
    emitter = command if isinstance(command, str) else None
    out: list[tuple[str | None, str]] = []
    for key in RAW_KEYS:
        raw = att.get(key)
        if not isinstance(raw, str) or not raw.strip():
            continue
        try:
            parsed = json.loads(raw)
        except ValueError:
            # :246-247 — não é JSON: o texto puro ainda custa a janela
            out.append((emitter, raw))
            continue
        if isinstance(parsed, dict):
            hso = parsed.get("hookSpecificOutput")
            if isinstance(hso, dict):
                for k in HSO_KEYS:
                    s = hso.get(k)
                    if isinstance(s, str):
                        out.append((emitter, s))
            s = parsed.get("systemMessage")
            if isinstance(s, str):
                out.append((emitter, s))
    return out


def transcripts_do_kpi(transcript_dir: Path) -> list[Path]:
    """`context_budget.rs:909-910` — os transcripts que o KPI LÊ.

    Só `*.jsonl`, ordenados por mtime MAIS RECENTE PRIMEIRO e limitados a
    `MAX_TRANSCRIPTS` (:40, aplicado em :909). Reproduzir isto é o que torna a
    paridade possível — comparar populations diferentes não é paridade.
    """
    arquivos = [p for p in transcript_dir.iterdir() if p.suffix == ".jsonl"]
    arquivos.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return arquivos[:MAX_TRANSCRIPTS]


def agregar(paths: list[Path]) -> dict:
    """Reproduz o laço de injeção de `aggregate_lines` (:544) sobre os
    transcripts dados. Devolve o agregado por família, na MESMA forma do KPI.

    Não reproduz `hash_of` (DefaultHasher do Rust, :289): a deduplicação usa
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
        if bytes_scanned >= MAX_BYTES:      # :909-910
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
            for emitter, text in hook_injections(rec):
                familia = attributed_family(text, emitter)
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
