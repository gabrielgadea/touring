#!/usr/bin/env python3
"""test_make_split.py — os testes nomeados que o gate da S-2.0 exige.

O plano (plan.md, S-2.0) pede:
  - `test_split_is_frozen` — hash do split.json registrado; re-gerar com a mesma
    seed reproduz byte-idêntico.

E, por consequência do que o split precisa garantir para o G2:
  - determinismo entre processos (o RNG semeado por estrato sobrevive ao
    interpretador? o `hash()` randomizado por processo NÃO pode entrar);
  - cobertura — todo item recebe exatamente uma parte, e todo rótulo do
    professor é alcançável em `test`;
  - estratificação — as proporções por classe/projeto se mantêm nas 3 partes
    (é o que diferencia isto de um split por hash global);
  - mudança de seed muda o split (prova que a seed é lida, não decorativa).

Roda sem pytest: `python3 test_make_split.py`.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import make_split as ms  # noqa: E402

SPLIT_PATH = ms.CORPUS_DIR / "split.json"
FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    status = "PASS" if ok else "FAIL"
    print(f"  [{status}] {name}" + (f" — {detail}" if detail else ""))
    if not ok:
        FAILURES.append(name)


def test_split_is_frozen() -> None:
    """O gate: mesma seed → mesmos bytes. Regerado do zero, não relido do disco."""
    ok, detail = ms.verify_frozen(SPLIT_PATH, ms.SPLIT_SEED)
    check("test_split_is_frozen", ok, detail)


def test_frozen_across_processes() -> None:
    """O `--check` roda num interpretador novo; se `hash()` ou ordem de dict
    randomizada entrassem, os bytes divergiriam entre processos."""
    proc = subprocess.run(
        [sys.executable, str(HERE / "make_split.py"), "--check"],
        capture_output=True, text=True, timeout=120,
    )
    check(
        "test_frozen_across_processes",
        proc.returncode == 0 and "PASS" in proc.stdout,
        proc.stdout.strip().splitlines()[0] if proc.stdout else proc.stderr[:120],
    )


def test_every_item_has_exactly_one_part() -> None:
    split = ms.build(ms.SPLIT_SEED)
    problems = []
    for name, entry in split["sets"].items():
        ids = {str(r.get("id")) for r in ms.read_corpus_jsonl(ms.CORPUS_DIR / entry["items_file"])}
        parts = entry["assignment"]
        problems += [f"{name}:{i}" for i in ids - set(parts)]
        problems += [f"{name}:{i}" for i in set(parts) - ids]
        bad = [i for i, p in parts.items() if p not in {"train", "val", "test"}]
        problems += [f"{name}:{i}={p}" for i, p in zip(parts, bad)] if bad else []
    check("test_every_item_has_exactly_one_part", not problems, f"{len(problems)} problema(s)")


def test_labels_reachable_in_test() -> None:
    """Todo id que tem rótulo tem que aparecer no split — senão o G2 não consegue
    avaliar um item cujo rótulo ficou órfão de alguma parte."""
    split = ms.build(ms.SPLIT_SEED)
    orphans = []
    for name, entry in split["sets"].items():
        labels = {str(r.get("id")) for r in ms.read_corpus_jsonl(ms.CORPUS_DIR / entry["labels_file"])}
        orphans += [f"{name}:{i}" for i in labels - set(entry["assignment"])]
    check("test_labels_reachable_in_test", not orphans, f"{len(orphans)} rótulo(s) órfão(s)")


def test_heldout_is_non_trivial() -> None:
    """Um held-out vazio ou de 1 item não mede nada. Piso: 10 itens e ≥5% do set."""
    split = ms.build(ms.SPLIT_SEED)
    thin = []
    for name, entry in split["sets"].items():
        n_test = entry["counts"]["test"]
        if n_test < 10 or n_test / entry["n"] < 0.05:
            thin.append(f"{name}: test={n_test}/{entry['n']} ({n_test / entry['n'] * 100:.1f}%)")
    check("test_heldout_is_non_trivial", not thin, "; ".join(thin) or "todos os sets ≥10 e ≥5%")


def test_stratification_preserved() -> None:
    """A proporção de cada estrato se mantém entre as partes. É o que distingue
    este split de um hash global sobre o id."""
    split = ms.build(ms.SPLIT_SEED)
    worst = 0.0
    worst_where = ""
    for name, entry in split["sets"].items():
        key = entry["strat_key"]
        items = {str(r.get("id")): r for r in ms.read_corpus_jsonl(ms.CORPUS_DIR / entry["items_file"])}
        by_stratum: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
        for ident, part in entry["assignment"].items():
            by_stratum[str(items[ident].get(key))][part] += 1
        for stratum, counts in by_stratum.items():
            total = sum(counts.values())
            if total < 5:  # estrato minúsculo: a discrepância é ruído de arredondamento
                continue
            for part in ("train", "val", "test"):
                share = counts[part] / total
                target = dict(ms.FRACTIONS)[part]
                delta = abs(share - target)
                if delta > worst:
                    worst, worst_where = delta, f"{name}/{stratum}/{part}"
    check("test_stratification_preserved", worst <= 0.05, f"pior desvio={worst:.3f} em {worst_where}")


def test_seed_is_load_bearing() -> None:
    """Mudar a seed tem que mudar o split. Se não mudasse, a seed seria enfeite
    e o gate de congelamento seria trivialmente satisfeito."""
    a = ms.split_digest(ms.build(ms.SPLIT_SEED))
    b = ms.split_digest(ms.build(ms.SPLIT_SEED + 1))
    check("test_seed_is_load_bearing", a != b, f"{a[:12]} != {b[:12]}")


def test_artifact_matches_disk() -> None:
    """O que está gravado tem as mesmas contagens que o que o gerador produz —
    fecha a cadeia entre o artefato e o código que o produz."""
    on_disk = json.loads(SPLIT_PATH.read_text(encoding="utf-8"))
    rebuilt = ms.build(ms.SPLIT_SEED)
    same = all(
        on_disk["sets"][name]["counts"] == rebuilt["sets"][name]["counts"]
        and on_disk["sets"][name]["assignment"] == rebuilt["sets"][name]["assignment"]
        for name in rebuilt["sets"]
    )
    check("test_artifact_matches_disk", same, f"split_seed gravado={on_disk['split_seed']}")


def main() -> int:
    print(f"corpus: {ms.CORPUS_DIR}")
    print(f"artefato: {SPLIT_PATH} ({SPLIT_PATH.stat().st_size if SPLIT_PATH.exists() else 0} B)")
    print(f"seed: {ms.SPLIT_SEED}\n")
    for test in (
        test_split_is_frozen,
        test_frozen_across_processes,
        test_every_item_has_exactly_one_part,
        test_labels_reachable_in_test,
        test_heldout_is_non_trivial,
        test_stratification_preserved,
        test_seed_is_load_bearing,
        test_artifact_matches_disk,
    ):
        test()
    print(f"\n{len(FAILURES)} falha(s)" if FAILURES else "\nTODOS OS TESTES PASSARAM")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
