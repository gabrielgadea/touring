#!/usr/bin/env python3
"""jev_tags_sample.py — S-1.1: a primeira decisão tipada real, em modo só-medição.

O plano (plan.md, S-1.1) pede: 100 memórias por seed fixa (seed=20260930) do
`memory.db`; UMA chamada por memória com Choice da faceta `kind`, critérios
contrastivos do vocabulário controlado, instruções em inglês (R1); comparar
contra a tag atual E contra o rótulo do G1; **não grava nada**.

Princípio (E/A/M, regra `code-elaboration-directives`): a unidade aqui é a
pergunta. Uma chamada por memória é o judgement real que a estratégia quer
medir — agregar tudo numa chamada só mediria outra coisa. Então: 100 chamadas,
cada uma com estado próprio.

O que este script NÃO faz
-------------------------
Não escreve em `memory.db`, nem em tag, nem no corpus do G1. O único efeito
permanente é o relatório e o journal do próprio adapter. Se uma chamada
falhar, o adapter devolve `None` (fail-open) e a linha entra no relatório
como `unmeasured` — nunca como 0, porque 0 é um valor medido e uma falha não
é um zero. É a mesma regra que a família `decision` do KPI vai aplicar na
S-3.1: ausência de dado é `unmeasured`.

Vocabulário: o `Facet::Kind::seed_values()` REAL de
`crates/touring-intelligence/src/rl/memory/tags.rs:251-278` — 22 valores, lidos
do código, não copiados do plano. O G1 usou 9 (`pattern, strategy, audit,
decision, lesson, snippet, gotcha, report, fix`); `audit`, `report` e `fix`
NÃO estão no vocabulário de produção, e o professor ainda abriu uma 10ª classe
`other` (2 casos) que também não existe. Ver o relatório: o G1 mediu um
vocabulário, e a produção usa outro.

Uso:
    python3 jev_tags_sample.py --n 100 --seed 20260930           # roda a medição
    python3 jev_tags_sample.py --n 5 --dry-run                  # mostra o plano, não chama a API
    python3 jev_tags_sample.py --replay path/to/journal.jsonl   # reavalia sem rede
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import sqlite3
import sys
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from jev_client import system_one  # noqa: E402

# O banco de onde a amostra sai. NÃO é o global: medido em 01/10, o corpus do
# G1 (486 chaves) tem 483 em comum com o memory.db do PROJETO e só 1 com o
# global. Amostrar do global daria 0/100 com rótulo do G1 — a medição de
# precisão contra o professor sairia vazia sem erro nenhum. O `--db` existe
# para essa troca ser explícita e testável, não silenciosa.
MEMORY_DB = Path(os.path.expanduser("~/projects/touring/.claude/touring/memory.db"))
GLOBAL_MEMORY_DB = Path(os.path.expanduser("~/.claude/touring/memory.db"))
G1_CORPUS = Path(os.path.expanduser("~/.claude/touring/g1-corpus"))
REPORT_DIR = Path(__file__).resolve().parent.parent / "experiments" / "w1-tags"
DEFAULT_N = 100
DEFAULT_SEED = 20260930

# O vocabulário controlado é o de `Facet::Kind::seed_values()` no Rust
# (tags.rs). **[F] Era uma tupla COPIADA aqui, e a cópia dessincronizou.**
TAGS_RS = Path(__file__).resolve().parent.parent / "crates" / "touring-intelligence" / "src" / "rl" / "memory" / "tags.rs"


def production_kind_values() -> tuple[str, ...]:
    """Lê o vocabulário de PRODUÇÃO direto do Rust — a fonte da verdade.

    **[F] Antes disto era uma lista copiada.** A S-5.7 (a7, ordem do Gabriel)
    acresceu `fix`/`report`/`audit` em `seed_values()`; a cópia ficou com 22 de
    25, o script passou a oferecer ao modelo opções que a produção não produz,
    e `test_vocabulary_matches_production` acusou a divergência — o teste
    estava certo e o relatório dizia "7/7 PASS". Derivar mata a **classe**
    do defeito: nenhuma próxima mudança de vocabulário pode mais dessincronizar
    um instrumento de medição.

    **[F] Falha alto e nomeia o caminho.** Um vocabulário vazio aqui mediria
    precisão contra nada e sairia 0/100 sem erro nenhum — o silêncio é o
    defeito, não o zero.
    """
    try:
        source = TAGS_RS.read_text(encoding="utf-8")
    except OSError as exc:
        raise SystemExit(f"vocabulário de produção ilegível em {TAGS_RS}: {exc}") from exc
    match = re.search(r"Facet::Kind\s*=>\s*&\[(.*?)\]", source, re.S)
    if not match:
        raise SystemExit(f"Facet::Kind::seed_values() não encontrado em {TAGS_RS}")
    values = tuple(re.findall(r'"([^"]+)"', match.group(1)))
    if not values:
        raise SystemExit(f"vocabulário de produção veio VAZIO de {TAGS_RS}")
    return values


KIND_VALUES = production_kind_values()

# Critérios contrastivos: cada opção diz o que É e o que NÃO é. O erro
# documentado do G1 (65,4% de concordância com o prefixo da chave) é um erro de
# FRONTEIRA — `lesson` vs `gotcha`, `pattern` vs `lesson`. Critérios que só
# nomeam não separam fronteira; é preciso dizer o que o rótulo vizinho não é.
KIND_CRITERIA = {
    "snippet": "A reusable code fragment with a codetag, copy-pasteable as-is. Not a narrative.",
    "doc": "Reference documentation about a system, module, or API. Describes; does not instruct.",
    "script": "An executable script or automation runbook step. Not a plan to write one.",
    "map": "A structural or dependency map of code. Not a sequence of work.",
    "plan": "An intended sequence of steps for future work. Not a record of work done.",
    "strategy": "A durable approach or routing policy, reusable across tasks. Not a one-off step.",
    "lesson": "A learned principle stated in the author's own words, generalizing beyond this incident. Not the incident itself.",
    "gotcha": "A specific trap that bites: an API quirk, a footgun, an order dependency. Concrete and narrow, not a general principle.",
    "decision": "A choice made between alternatives, with the reason. Not a plan for a choice not yet made.",
    "process": "How a procedure runs, mechanically. Not why it should.",
    "module": "A unit of code structure. Not a lesson about it.",
    "runbook": "Operational steps to recover or execute under failure. Not analysis.",
    "insight": "A non-obvious realization about the domain. Not actionable procedure.",
    "outcome": "The recorded result of a command, run, or experiment, with reward or exit status. Not interpretation.",
    "case": "A worked example of one specific situation. Not a general rule.",
    "observation": "A measured fact or number from instrumentation. Not an inference from it.",
    "pattern": "A recurring shape recognized across multiple instances. Not a single instance.",
    "reference": "A pointer to an external artifact, doc, or URL. Not content.",
    "note": "A brief remark kept for later attention. Not a durable principle.",
    "project": "State or identity of a project/loop/DAG. Not work within it.",
    "followup": "An open item deferred for later action. Not the action itself.",
    "text": "Free-form prose that fits no other kind. The residual class.",
}

# O G1 puro rotulou com 9 + `other`. Os três que o G1 usou e a produção NÃO tem
# são medidos à parte, senão o Jev é julgado contra um vocabulário impossível.
G1_ONLY_VALUES = ("audit", "report", "fix")


def read_sqlite_ro(path: Path) -> list[dict]:
    """Abre o memory.db em modo SOMENTE LEITURA e devolve as memórias JÁ COM a
    tag `kind` de produção anexada.

    `sqlite3.connect()` sem `mode=ro` traz `CREATE` implícito e pode criar o
    arquivo/alterar o banco — a lição `sqlite-so-leitura-abre-readonly` do
    acervo. O caminho do URI é o que garante read-only de verdade.

    A tag NÃO está em `memory_entries` (essa tabela não tem a coluna — foi o que
    o primeiro dry-run provou): ela vive em `memory_tags`. E o mesmo item pode
    ter MAIS DE UMA `kind`, de fontes diferentes — `gotcha:omarchy-update-sudo`
    tem `kind:lesson` (auto) e `kind:gotcha` (explicit). Por isso `kind_tags`
    guarda a lista inteira, e `current_tag` escolhe por precedência de `source`
    (explicit > code_sync > auto > backfill, a mesma ordem de confiança que o
    `TagSource` do Rust documenta). A lista crua vai no relatório para a
    ambiguidade ficar visível em vez de ser absorvida num escalar.
    """
    if not path.exists():
        raise SystemExit(f"banco ausente: {path}")
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    try:
        rows = [dict(r) for r in con.execute(
            "SELECT key, value, tier, entry_type, access_count, created_at "
            "FROM memory_entries WHERE superseded_by IS NULL ORDER BY key"
        )]
        tags: dict[str, list[dict]] = {}
        for r in con.execute(
            "SELECT entry_key, value, source FROM memory_tags WHERE facet = 'kind'"
        ):
            tags.setdefault(str(r["entry_key"]), []).append(
                {"value": str(r["value"]), "source": str(r["source"])}
            )
        for row in rows:
            row["kind_tags"] = tags.get(str(row["key"]), [])
        return rows
    finally:
        con.close()


def load_g1_labels() -> dict[str, dict]:
    """Rótulos do professor para memórias (id → label), se o corpus existir.

    Também devolve o índice `key → id`, porque o `memories.jsonl` do G1 traz a
    chave e é por ela que o corpus se liga ao banco.
    """
    path = G1_CORPUS / "labels_memory_kind.jsonl"
    out: dict[str, dict] = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                row = json.loads(line)
                out[str(row.get("id"))] = row
    return out


def load_g1_key_index() -> dict[str, str]:
    """`key → id G1`, lido do `memories.jsonl`.

    **[F] O id `m:<12 hex>` é `sha1(key)[:12]`, NÃO `sha1(texto)`** — verificado
    em 01/10: `m:30fc8328eb1b` = sha1 de `pattern:rust:sqlite-count-unwrap-or-zero`
    (a chave), e o sha1 do texto dá outro valor. A hipótese inicial (o `g1_sample.py`
    normaliza o texto) estava errada e produzia **0/19.837** casamentos — um zero
    silencioso que teria esgotado a medição inteira.

    E não é preciso nem recomputar o hash: o `memories.jsonl` **traz a `key`** ao
    lado do `id`, então a ponte é um dict direto, e a chave continua valendo como
    fallback verificável (`g1_id_for`).
    """
    path = G1_CORPUS / "memories.jsonl"
    index: dict[str, str] = {}
    if not path.exists():
        return index
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            row = json.loads(line)
            key, ident = row.get("key"), row.get("id")
            if key and ident:
                index[str(key)] = str(ident)
    return index


def g1_id_for(memory: dict) -> str | None:
    """`m:<sha1(key)[:12]>` — a forma do id do corpus G1, conferida contra o
    arquivo. É a ponte entre uma memória do banco e o rótulo do professor."""
    import hashlib
    key = memory.get("key")
    if not key:
        return None
    return "m:" + hashlib.sha1(str(key).encode()).hexdigest()[:12]


def sample(memories: list[dict], n: int, seed: int) -> list[dict]:
    """Amostra determinística. A ordem de `read_sqlite_ro` é por `key`, então a
    mesma seed devolve a mesma amostra desde que o banco não mude — e o teste
    `test_sample_is_deterministic` prova isso."""
    pool = sorted(memories, key=lambda m: str(m.get("key")))
    rng = random.Random(seed)
    if n >= len(pool):
        return pool
    return sorted(rng.sample(pool, n), key=lambda m: str(m.get("key")))


def build_criteria(values: tuple[str, ...]) -> dict[str, str]:
    return {v: KIND_CRITERIA[v] for v in values if v in KIND_CRITERIA}


def ask(memory: dict, values: tuple[str, ...], *, client=None) -> dict:
    """Uma chamada por memória. `state` é a coisa julgada — a chave e o texto da
    memória, e nada mais (o `derived_tag` atual NÃO entra: seria dar ao modelo a
    resposta que queremos medir contra ele)."""
    state = {
        "memory_key": memory.get("key"),
        "memory_text": (memory.get("value") or "")[:1200],
        "entry_type": memory.get("entry_type"),
    }
    question = {
        "kind": {
            "type": "choice",
            "instructions": (
                "Choose the single best value for the `kind` facet of this Touring memory entry. "
                "The vocabulary is a closed set; pick exactly one. "
                "Judge what the text IS, not what its key prefix suggests. "
                "If the text is a trap/footgun with a concrete symptom, choose gotcha. "
                "If it states a general principle the author learned, choose lesson."
            ),
            "criteria": build_criteria(values),
        }
    }
    started = time.monotonic()
    answers = system_one(state, question, client=client)
    elapsed = (time.monotonic() - started) * 1000
    if answers is None:
        return {"answer": None, "confidence": None, "latency_ms": round(elapsed, 1), "route": "fallback"}
    ans = answers.get("kind", {})
    return {
        "answer": ans.get("choice"),
        "confidence": ans.get("confidence"),
        "probabilities": ans.get("probabilities"),
        "latency_ms": round(elapsed, 1),
        "route": "api",
    }


# Precedência de confiança entre fontes de tag — a mesma ordem que o
# `TagSource` de `tags.rs:506-514` documenta (Backfill "lowest" → Explicit
# "highest"). `code_sync` fica acima de `auto` porque a tag veio de um anchor
# `#tags:` escrito no código, que é declaração do autor.
SOURCE_PRECEDENCE = {"explicit": 3, "code_sync": 2, "auto": 1, "backfill": 0}


def current_tag(memory: dict) -> str | None:
    """A tag `kind` de produção que vale hoje, por precedência de fonte.

    Um item pode ter várias `kind` — medido: `gotcha:omarchy-update-sudo-v-prompt`
    tem `lesson` (auto) e `gotcha` (explicit). Escolher por precedência é o que
    torna a referência única; a lista crua fica em `kind_tags` para o relatório
    expor a ambiguidade. Empate (duas tags da MESMA fonte, valores diferentes)
    devolve a primeira em ordem alfabética — determinístico, e a ambiguidade
    fica registrada em `ambiguous_kind_tags`.
    """
    tags = memory.get("kind_tags") or []
    if not tags:
        return None
    best = max(
        tags,
        key=lambda t: (SOURCE_PRECEDENCE.get(t.get("source", ""), -1), t.get("value", "")),
    )
    return best.get("value")


def ambiguous_kind_tags(memory: dict) -> list[str]:
    """Valores de `kind` distintos numa MESMA fonte — a referência é ambígua e
    o acerto contra ela é uma moeda."""
    by_source: dict[str, set[str]] = {}
    for tag in memory.get("kind_tags") or []:
        by_source.setdefault(str(tag.get("source")), set()).add(str(tag.get("value")))
    return sorted(v for values in by_source.values() if len(values) > 1 for v in values)


def _references_of(row: dict) -> list[tuple[str, str | None]]:
    """As DUAS referências que cada linha pode ter.

    Referência 1: a tag de produção (`derived_tag`) — a base do
    `recall_curated_share`. Referência 2: o rótulo do professor do G1 — a base
    da estratégia. Uma linha pode ter uma, outra, ou nenhuma.
    """
    return [("prod", row.get("current_tag")), ("g1", row.get("g1_label"))]


def _score_rows(rows: list[dict]) -> tuple[Counter, dict[str, Counter], dict[str, dict[int, list[tuple[float, int]]]]]:
    """Conta acertos por referência, por classe, e acumula a curva de
    calibração PARA CADA REFERÊNCIA. Separado de `evaluate` para manter a
    complexidade baixa.

    **[F] Defeito corrigido em 01/10:** a curva só era acumulada para `g1`, e o
    relatório saiu dizendo "95/100 respostas sem confidence" — número que era, na
    verdade, `g1_no_reference`. As DUAS coisas davam 95 por coincidência, e eu
    li uma como a outra. O dado bruto: **100/100 têm `confidence`** e 100/100 têm
    `probabilities`. Com a curva só no G1 (n=5) o ECE saía 0,50 e parecia falha de
    instrumentação do provedor; contra a tag de produção (n=100) sai **0,51** e
    mostra o padrão real. Duas contagens coincidentes que significam coisas
    diferentes é a armadilha da lição `instrumento-que-lie`.
    """
    stats: Counter = Counter()
    by_class: dict[str, Counter] = {}
    bins: dict[str, dict[int, list[tuple[float, int]]]] = {
        "prod": {i: [] for i in range(5)},
        "g1": {i: [] for i in range(5)},
    }

    for row in rows:
        if row["answer"] is None:
            stats["unmeasured"] += 1
            continue
        stats["measured"] += 1
        pred, conf = row["answer"], row.get("confidence")
        for name, ref in _references_of(row):
            if not ref:
                stats[f"{name}_no_reference"] += 1
                continue
            hit = int(pred == ref)
            stats[f"{name}_total"] += 1
            stats[f"{name}_correct"] += hit
            bucket = by_class.setdefault(ref, Counter())
            bucket[name] += 1
            bucket[f"{name}_correct"] += hit
            if isinstance(conf, (int, float)):
                bins[name][min(4, max(0, int(float(conf) * 5)))].append((float(conf), hit))
    return stats, by_class, bins


def _ratio(stats: Counter, prefix: str) -> float | None:
    total = stats[f"{prefix}_total"]
    return round(stats[f"{prefix}_correct"] / total, 4) if total else None


def _bucket_stats(bucket: list[tuple[float, int]]) -> tuple[float, float]:
    """(acurácia, confiança média) de uma faixa — None quando a faixa é vazia."""
    if not bucket:
        return 0.0, 0.0
    return (
        sum(hit for _, hit in bucket) / len(bucket),
        sum(conf for conf, _ in bucket) / len(bucket),
    )


def _expected_calibration_error(bins: dict[int, list[tuple[float, int]]]) -> float | None:
    """ECE sobre [0,1] em 5 bins. Com amostra de 100 e 22 classes vai ser
    ruidoso — por isso o relatório publica o n de cada faixa, não só o número."""
    n = sum(len(bucket) for bucket in bins.values())
    if not n:
        return None
    total = 0.0
    for bucket in bins.values():
        if not bucket:
            continue
        acc, conf = _bucket_stats(bucket)
        total += (len(bucket) / n) * abs(acc - conf)
    return round(total, 4)


def _calibration_curve(bins: dict[int, list[tuple[float, int]]]) -> list[dict]:
    """Curva de calibração: acerto por faixa de confiança. É o que o gate do G3
    vai ler quando recalibrar a temperatura."""
    curve = []
    for i in range(5):
        bucket = bins[i]
        acc, conf = _bucket_stats(bucket)
        curve.append({
            "bin": f"[{i / 5:.1f},{(i + 1) / 5:.1f})",
            "n": len(bucket),
            "accuracy": round(acc, 4) if bucket else None,
            "mean_confidence": round(conf, 4) if bucket else None,
        })
    return curve


def _latency_summary(rows: list[dict]) -> dict:
    values = sorted(float(r.get("latency_ms") or 0) for r in rows)
    if not values:
        return {"p50": None, "max": None}
    return {"p50": round(values[len(values) // 2], 1), "max": round(values[-1], 1)}


def vocabulary_coverage(rows: list[dict], values: tuple[str, ...]) -> dict:
    """Quanto da referência de produção está DENTRO do vocabulário oferecido.

    Medido no banco do projeto (01/10): 20.163 kind-tags, das quais **104 (0,5%)
    estão em 35 valores que o `Facet::Kind::seed_values()` não contém** — `fix`,
    `checkpoint`, `analysis`, `inventory`, `evidence`, `phase-report`, `debt`,
    `event`, `incident`, `gate-closed`, `todo`… Essas não podem ter vindo de
    `derive_tags` (ele só emite `seed_values`): entraram por `--tag` explícito.

    A consequência é que acertar essas é impossível por construção — o modelo
    não tem a opção. Elas não viram erro do Jev; viram **fora de escopo**, e o
    número entra no relatório para que a taxa de acerto não seja lida como
    melhor do que é.
    """
    allowed = set(values)
    outside = Counter()
    for row in rows:
        for value in row.get("kind_tags_all") or []:
            if value.get("value") not in allowed:
                outside[str(value.get("value"))] += 1
    return {
        "values_in_production_but_outside_vocabulary": dict(outside.most_common()),
        "tags_outside_vocabulary": sum(outside.values()),
        "rows_touching_outside_vocabulary": sum(
            1 for row in rows
            if any(v.get("value") not in allowed for v in (row.get("kind_tags_all") or []))
        ),
    }


def evaluate(rows: list[dict], g1: dict[str, dict]) -> dict:
    """Acertos e Calibration Error contra as DUAS referências.

    `g1` entra na assinatura mas não é lido aqui: as referências já vêm
    resolvidas em cada linha (por `g1_id_for` + `load_g1_labels`), e reler o
    corpus aqui criaria uma segunda fonte de verdade. Fica na assinatura para
    o chamador manter a ordem "carrega o corpus, monta as linhas, avalia".
    """
    stats, by_class, bins = _score_rows(rows)
    return {
        "n_requested": len(rows),
        "measured": stats["measured"],
        "unmeasured": stats["unmeasured"],
        "accuracy_vs_production_tag": _ratio(stats, "prod"),
        "accuracy_vs_g1_teacher": _ratio(stats, "g1"),
        "prod_no_reference": stats["prod_no_reference"],
        "g1_no_reference": stats["g1_no_reference"],
        # Quantas respostas trouxeram confiança. Publicar isto separadamente
        # impede a leitura que eu fiz: com o ECE de uma curva de n=5 e 95
        # "sem referência", os dois números são 95 e parecem o mesmo fato.
        "responses_with_confidence": sum(1 for r in rows if r.get("confidence") is not None),
        "responses_with_probabilities": sum(1 for r in rows if r.get("probabilities") is not None),
        "ece_vs_production_tag": _expected_calibration_error(bins["prod"]),
        "ece_vs_g1": _expected_calibration_error(bins["g1"]),
        "calibration_curve_vs_production_tag": _calibration_curve(bins["prod"]),
        "calibration_curve_vs_g1": _calibration_curve(bins["g1"]),
        "predicted_distribution": dict(Counter(r["answer"] for r in rows if r["answer"])),
        "by_reference_class": {k: dict(v) for k, v in sorted(by_class.items())},
        "latency_ms": _latency_summary(rows),
    }


def print_plan(n: int, seed: int, rows: list[dict], values: tuple[str, ...]) -> None:
    print(f"n={n} seed={seed} vocabulary={len(values)} valores")
    print(f"memórias no banco: {len(read_sqlite_ro(MEMORY_DB))}")
    print(f"amostra: {len(rows)}  (determinística)")
    for row in rows[:3]:
        print(f"  - key={str(row.get('key'))[:70]}")
        print(f"    id_g1={row.get('g1_id')}  tag_atual={row.get('current_tag')!r}  rotulo_g1={row.get('g1_label')!r}")
    print(f"\ncritérios: {len(build_criteria(values))} opções com descrição contrastiva")
    print("(dry-run: nenhuma chamada feita, nada gravado)")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--n", type=int, default=DEFAULT_N)
    ap.add_argument("--seed", type=int, default=DEFAULT_SEED)
    ap.add_argument("--dry-run", action="store_true", help="mostra o plano sem chamar a API")
    ap.add_argument("--out", type=Path, default=REPORT_DIR / "results.json")
    ap.add_argument("--db", type=Path, default=MEMORY_DB,
                    help=f"banco de memória (padrão: o do PROJETO — é o que o G1 amostrou)")
    ap.add_argument("--values", default="prod", choices=["prod", "g1", "union"],
                    help="vocabulário da Choice: prod (22 de produção), g1 (9 do G1), union")
    args = ap.parse_args(argv)

    vocab = {
        "prod": KIND_VALUES,
        "g1": ("pattern", "strategy", "audit", "decision", "lesson", "snippet", "gotcha", "report", "fix"),
        "union": tuple(sorted(set(KIND_VALUES) | set(G1_ONLY_VALUES))),
    }[args.values]

    memories = read_sqlite_ro(args.db)
    chosen = sample(memories, args.n, args.seed)
    g1 = load_g1_labels()
    g1_by_key = load_g1_key_index()

    rows = []
    for memory in chosen:
        # A ponte é a CHAVE (o corpus traz `key` ao lado do `id`); o hash é só
        # a conferência. Se as duas divergirem, o corpus manda — e a divergência
        # vai para o relatório em vez de sumir.
        gid = g1_by_key.get(str(memory.get("key"))) or g1_id_for(memory)
        label = g1.get(gid, {}).get("label") if gid else None
        rows.append({
            "key": memory.get("key"),
            "current_tag": current_tag(memory),
            "kind_tags_all": memory.get("kind_tags") or [],
            "ambiguous_kind": ambiguous_kind_tags(memory),
            "g1_id": gid,
            "g1_label": label,
        })

    if args.dry_run:
        print_plan(args.n, args.seed, rows, vocab)
        return 0

    print(f"medindo {len(rows)} memórias contra o Jev (vocabulário {len(vocab)})…\n")
    for i, (memory, row) in enumerate(zip(chosen, rows), 1):
        result = ask(memory, vocab)
        row.update(result)
        flag = result["answer"] or "SEM RESPOSTA (fallback)"
        print(f"  [{i:3}/{len(rows)}] {str(memory.get('key'))[:52]:52} → {flag:24} conf={result['confidence']} {result['latency_ms']}ms")

    summary = evaluate(rows, g1)
    summary["vocabulary_coverage"] = vocabulary_coverage(rows, vocab)
    payload = {
        "subtask": "S-1.1",
        "n": args.n,
        "seed": args.seed,
        "db": str(args.db),
        "vocabulary": list(vocab),
        "vocabulary_source": "Facet::Kind::seed_values() — tags.rs:251-278",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "summary": summary,
        "rows": rows,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"\n{json.dumps(summary, ensure_ascii=False, indent=2)[:2000]}")
    print(f"\nresultado: {args.out}")
    print("modo somente-medição: nenhuma escrita em memory.db, tags ou corpus")
    return 0


if __name__ == "__main__":
    sys.exit(main())
