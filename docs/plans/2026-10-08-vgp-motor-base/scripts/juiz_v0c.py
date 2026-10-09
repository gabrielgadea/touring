"""Judge of V0c (fix every live error): exit 0 only when every item of v0c_itens.jsonl has exactly one valid triage.

Triage rows live in ~/.claude/touring/vgp/v0c_triagem.jsonl, one JSON per line:
  {"id": "<item id>", "veredito": "corrigido|ambiental|não-reproduz|falso-positivo-do-instrumento|recusa-correta",
   (recusa-correta = the code rightly refused an invalid input; the evidence shows the guard AND the input that hit it)
   "evidencia": "<what was read/run and what it showed, ≥ 40 chars>",
   "comando": "<the command that decided it>", "saida": "<its decisive output line>",
   "teste": "<path::test_func — REQUIRED for corrigido: a test that fails without the fix and passes with it>",
   "arquivos": ["<files changed>"]}

For `corrigido` the judge re-runs the test itself (vivos.rerun_test) and demands it passes today.
Usage: python3 juiz_v0c.py [--sem-testes]   (exit 0 = converged; 1 = gaps, each one listed)
"""

import json
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import vivos  # noqa: E402  (same directory; reuses the exact rerun predicate of V0b)

VGP = Path.home() / ".claude/touring/vgp"
# inconclusivo-replay (round 3): the judge's own replay could not decide (a /tmp file it could not rebuild, a
# timeout, an ambiguous command) — accepted ONLY when the replay record shows that blocker; goes on to V0d
VEREDITOS = {"corrigido", "ambiental", "não-reproduz", "falso-positivo-do-instrumento", "recusa-correta",
             "inconclusivo-replay"}


def specific_to(row: dict, item: dict) -> bool:
    """The evidence speaks of THIS error: it names the file (basename) or its line, AND the exception class or a
    quoted token of its message. A template pasted over unrelated errors fails here (found 08/10/2026: one text about
    PYTHONPATH justified a ConnectionError, a PermissionError, an ImportError and a TypeError)."""
    ev = f"{row.get('evidencia', '')} {row.get('comando', '')} {row.get('saida', '')}"
    where = Path(item.get("path", "")).name in ev or f":{item.get('line')}" in ev or f"linha {item.get('line')}" in ev
    tokens = [t for t in re.findall(r"'([^']{4,})'", item.get("message", ""))]
    what = item.get("kind", "") in ev or any(t in ev for t in tokens)
    return where and what


def shared_across_kinds(triagem: list[dict], itens: dict) -> set[str]:
    """Ids whose evidence text is shared with an item of a DIFFERENT exception class — a template, not a triage."""
    kinds_by_text: dict[str, set] = {}
    for t in triagem:
        kinds_by_text.setdefault(str(t.get("evidencia", "")).strip(), set()).add(itens.get(t.get("id"), {}).get("kind"))
    return {str(t.get("id")) for t in triagem if len(kinds_by_text[str(t.get("evidencia", "")).strip()]) > 1}


def reproduction_gaps(row: dict, item: dict, run_tests: bool) -> list[str]:
    """Round 2 (08/10/2026): the verdict must rest on reproducing the ORIGINAL error, not a neighbour of it.
    Found: all 30 pytest items "verified" with `python3 file.py` (which runs no test), and 18 "ambiental" proved
    with `No module named 'scripts'` while the original error was a ValueError, a PermissionError…"""
    path = Path(item.get("path", ""))
    node = vivos.test_node(path, int(item.get("line") or 0)) if path.is_file() else None
    return _ambiental_gap(row, item) + _pytest_gap(row, node, run_tests) + _script_gap(row, item, node)


def _ambiental_gap(row: dict, item: dict) -> list[str]:
    kind = item.get("kind", "")
    if row.get("veredito") == "ambiental" and kind and kind not in str(row.get("saida", "")):
        return [f"ambiental sem reproduzir a classe original ({kind}) na saída"]
    return []


def _pytest_gap(row: dict, node: str | None, run_tests: bool) -> list[str]:
    """A test item is decided by the judge itself, through pytest — never by `python3 file.py`."""
    if not node or not run_tests or row.get("veredito") == "corrigido":
        return []
    status, detail = vivos.rerun_test(node)
    if status == "vivo-teste":
        return [f"o teste falha HOJE ({detail}) — exige corrigido"]
    if row.get("veredito") == "não-reproduz" and status != "teste-passa-hoje":
        return [f"não-reproduz sem o teste passar hoje pelo pytest ({detail})"]
    return []


def replay_record(item_id: str) -> dict | None:
    p = VGP / "replay" / f"{item_id}.json"
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None


def _replay_blocker(rec: dict | None) -> str | None:
    """Why a replay record cannot decide the item (None = it can)."""
    if rec is None:
        return "sem registro do replay (rode scripts/replay.py --ids <id>)"
    if rec["command_source"] in ("ambíguo", "não-achado"):
        return f"replay sem o comando original ({rec['command_source']})"
    if rec["timed_out"]:
        return f"replay estourou o tempo ({rec['wall_s']} s): inconclusivo"
    if rec["unrebuilt"] or rec["incomplete"]:
        return f"replay sem reconstruir /tmp: {(rec['unrebuilt'] + rec['incomplete'])[:2]}"
    return None


def _script_gap(row: dict, item: dict, node: str | None) -> list[str]:
    """Round 3 (08/10/2026): `não-reproduz` rests on the judge's OWN replay of the whole original invocation
    (scripts/replay.py, isolated by bwrap), never on the command a delegate says it ran — one admitted writing a
    command it never executed. A replay that fails for another reason must be explained by quoting its last line."""
    rec = replay_record(item["id"])
    check = {"ambiental": _ambiental_replay_gap, "falso-positivo-do-instrumento": _fp_replay_gap,
             "inconclusivo-replay": _inconclusivo_gap}.get(
        str(row.get("veredito")))
    if check:
        return check(item, rec)
    if row.get("veredito") != "não-reproduz" or node:
        return []
    blocker = _replay_blocker(rec)
    if blocker or rec is None:
        return [f"não-reproduz {blocker}"]
    return _nao_reproduz_gap(row, item, rec)


def _strong(rec: dict | None) -> bool:
    """The replay decides AND shows the original class in the item's own frame (file:line)."""
    return rec is not None and not _replay_blocker(rec) and bool(rec.get("frame_in_output")) and rec["kind_in_output"]


def _ambiental_replay_gap(item: dict, rec: dict | None) -> list[str]:
    if rec is not None and not _replay_blocker(rec) and not rec["kind_in_output"]:
        return [f"ambiental, mas o replay não mostra a classe original ({item.get('kind')})"]
    return []


def _inconclusivo_gap(item: dict, rec: dict | None) -> list[str]:
    blocker = _replay_blocker(rec)
    if blocker is None:
        return ["inconclusivo-replay, mas o replay DECIDE (sem bloqueio no registro) — triagem exigida"]
    if rec and "sem registro" not in blocker:
        return []
    return [f"inconclusivo-replay sem registro do replay para {item['id']}"]


def _fp_replay_gap(item: dict, rec: dict | None) -> list[str]:
    if _strong(rec):
        return [f"o replay reproduz {item.get('kind')} no frame do item — o instrumento acertou, não é falso positivo"]
    return []


def _last_error_line(tail: str) -> str:
    lines = [ln.strip() for ln in tail.splitlines() if ln.strip()]
    errs = [ln for ln in lines if re.search(r"\w(Error|Exception)\b|error:|FAILED", ln)]
    return (errs or lines or [""])[-1]


def _nao_reproduz_gap(row: dict, item: dict, rec: dict) -> list[str]:
    if _strong(rec):
        return [f"o replay REPRODUZ {item.get('kind')} no frame {Path(item['path']).name}:{item.get('line')} — "
                "não-reproduz é falso"]
    last = _last_error_line(rec["tail"])
    if (rec["rc"] != 0 or rec["kind_in_output"]) and last[:60] not in f"{row.get('evidencia', '')} {row.get('saida', '')}":
        return [f"o replay falhou ou mostra a classe fora do frame (rc={rec['rc']}: {last[:90]!r}); a triagem não cita"]
    return []


def load(name: str) -> list[dict]:
    p = VGP / name
    return [json.loads(ln) for ln in p.open(encoding="utf-8")] if p.exists() else []


def gaps_of(row: dict, run_tests: bool) -> list[str]:
    out = []
    if row.get("veredito") not in VEREDITOS:
        out.append(f"veredito inválido {row.get('veredito')!r}")
    if len(str(row.get("evidencia", ""))) < 40:
        out.append("evidência curta (< 40 chars)")
    if not row.get("comando") or not row.get("saida"):
        out.append("sem comando/saída que decidiu")
    if row.get("veredito") == "corrigido":
        out += _fixed_gaps(row, run_tests)
    return out


def _fixed_gaps(row: dict, run_tests: bool) -> list[str]:
    if "::" not in str(row.get("teste", "")):
        return ["corrigido sem teste (path::func)"]
    if not row.get("arquivos"):
        return ["corrigido sem arquivos alterados"]
    if run_tests:
        status, detail = vivos.rerun_test(row["teste"])
        if status != "teste-passa-hoje":
            return [f"teste não passa hoje: {detail}"]
    return []


def problems_of(itens: dict, triagem: list[dict], run_tests: bool) -> list[str]:
    """Every gap, named: missing or duplicated triage, ids outside the list, and invalid rows."""
    per_id = Counter(t.get("id") for t in triagem)
    out = [f"{i}: sem triagem" for i in itens if per_id[i] == 0]
    out += [f"{i}: {n} triagens (exige 1)" for i, n in per_id.items() if n > 1]
    out += [f"{t.get('id')}: id fora da lista" for t in triagem if t.get("id") not in itens]
    for t in triagem:
        out += [f"{t.get('id')}: {g}" for g in gaps_of(t, run_tests)]
    return out + substance_gaps(itens, triagem, run_tests)


def substance_gaps(itens: dict, triagem: list[dict], run_tests: bool) -> list[str]:
    """Rounds 1 and 2: the evidence speaks of THIS error, is no template, and reproduces the ORIGINAL error."""
    known = [t for t in triagem if t.get("id") in itens]
    out = [f"{i}: evidência genérica, repetida em erro de outra classe" for i in sorted(shared_across_kinds(triagem, itens))]
    out += [f"{t['id']}: a evidência não fala DESTE erro (arquivo ou linha + classe ou termo da mensagem)"
            for t in known if not specific_to(t, itens[t["id"]])]
    return out + [f"{t['id']}: {g}" for t in known for g in reproduction_gaps(t, itens[t["id"]], run_tests)]


def main() -> int:
    itens = {r["id"]: r for r in load("v0c_itens.jsonl")}
    triagem = load("v0c_triagem.jsonl")
    problems = problems_of(itens, triagem, "--sem-testes" not in sys.argv)
    print(f"itens {len(itens)} · triagens {len(triagem)} · vereditos {dict(Counter(t.get('veredito') for t in triagem))}")
    for p in problems[:40]:
        print("  FALTA", p)
    print(f"veredito: {'CONVERGIU' if not problems and itens else f'{len(problems)} pendências'}")
    return 0 if not problems and itens else 1


if __name__ == "__main__":
    sys.exit(main())
