"""Judge of V0d (V0d-spec.md): exit 0 = the replay corpus is ready to measure the VGP motor.

Reads ~/.claude/touring/vgp/corpus_v0d/: entradas.jsonl, snap/<id>.py, negativos_candidatos.json
({"seed": 20261008, "candidatos": [ids, canonical order]}) and replay_v0d/<id>.json (the builder's replay of each
positive). It trusts no field: re-hashes every snapshot, re-compiles every negative, re-draws the sample, re-reads
every replay record, and RE-RUNS a seeded sample of positives itself through `replay.run_isolated` — with the
snapshot (positive control: the error reproduces in its own frame) and, for one positive whose file today no longer
holds the invented name, without it (negative control: reproduction depends on the snapshot, not on today's code).
Usage: python3 juiz_v0d.py [--amostra 3] [--selftest]
"""

import hashlib
import json
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import replay  # noqa: E402  # pyright: ignore[reportMissingImports]

C = Path.home() / ".claude/touring/vgp/corpus_v0d"
SEED, N_NEG, MIN_RATE, CAP = 20261008, 300, 0.70, 300
KIND = {"nome": "NameError", "atributo": "AttributeError", "import": "ImportError", "kwarg": "TypeError"}


def classe_de(row: dict) -> str | None:
    """The invented-API class of a V0a row (the builder imports this — one definition for both sides)."""
    msg, kind = row.get("message", ""), row.get("kind")
    if kind == "NameError":
        return "nome"
    if kind == "AttributeError" and "has no attribute" in msg:
        return "atributo"
    if kind == "ImportError" and "cannot import name" in msg:
        return "import"
    if kind == "TypeError" and "unexpected keyword argument" in msg:
        return "kwarg"
    return None


def load() -> list[dict]:
    p = C / "entradas.jsonl"
    return [json.loads(ln) for ln in p.open(encoding="utf-8")] if p.exists() else []


def _overlay(r: dict) -> list[dict]:
    """What a replay lays over the real tree: the entry's own snapshot plus the `contexto` files it declared."""
    return [{"path": r["path"], "snap": r.get("snap", ""), "sha256": r.get("snap_sha256")}, *r.get("contexto", [])]


def snap_gaps(rows: list[dict]) -> list[str]:
    ids = [r["id"] for r in rows]
    out = ["ids repetidos"] if len(ids) != len(set(ids)) else []
    for r in rows:
        for f in _overlay(r):
            p = C / str(f["snap"])
            if not p.is_file():
                out.append(f"{r['id']}: snapshot ausente ({f['snap']})")
            elif hashlib.sha256(p.read_bytes()).hexdigest() != f.get("sha256"):
                out.append(f"{r['id']}: sha256 não confere ({f['snap']})")
    return out


def negative_gaps(rows: list[dict]) -> list[str]:
    neg = [r for r in rows if r["rotulo"] == "negativo"]
    out = [] if len(neg) >= N_NEG else [f"negativos {len(neg)} < {N_NEG}"]
    p = C / "negativos_candidatos.json"
    cand = json.loads(p.read_text(encoding="utf-8")) if p.exists() else {}
    pool = cand.get("candidatos", [])
    if cand.get("seed") != SEED:
        out.append(f"negativos_candidatos.json ausente ou sem a semente {SEED}")
    elif set(random.Random(SEED).sample(pool, min(N_NEG, len(pool)))) != {r["id"] for r in neg}:
        out.append("re-sorteio com a semente não devolve os mesmos ids")
    for r in neg:
        try:
            compile((C / r["snap"]).read_text(encoding="utf-8"), r["path"], "exec")
        except (SyntaxError, ValueError, OSError) as e:
            out.append(f"{r['id']}: negativo não compila ({type(e).__name__})")
    return out


def _proved(r: dict) -> bool:
    """Read back from the replay record, never from the entry's own `reproduz` field."""
    p = C / "replay_v0d" / f"{r['id']}.json"
    rec = json.loads(p.read_text(encoding="utf-8")) if p.exists() else {}
    return bool(rec.get("kind_in_output") and rec.get("frame_in_output"))


def _field_gaps(pos: list[dict], v0a: set[str], proved: set[str]) -> list[str]:
    out = [f"{r['id']}: positivo sem classe/command/linha" for r in pos
           if r.get("classe") not in KIND or not r.get("command") or not r.get("linha")]
    covered = {r.get("classe") for r in pos if r["id"] in proved}
    return out + [f"classe {c}: presente na V0a e sem positivo que reproduza" for c in sorted(v0a - covered)]


def positive_gaps(rows: list[dict], v0a: set[str]) -> list[str]:
    pos = [r for r in rows if r["rotulo"] == "positivo"]
    proved = {r["id"] for r in pos if _proved(r)}
    out = _field_gaps(pos, v0a, proved)
    out += [f"{r['id']}: reproduz ≠ registro de replay" for r in pos if bool(r.get("reproduz")) != (r["id"] in proved)]
    rate = len(proved) / len(pos) if pos else 0.0
    if rate < MIN_RATE:
        out.append(f"controle positivo do corpus: {len(proved)}/{len(pos)} = {rate:.0%} < {MIN_RATE:.0%}")
    return out


def _rerun(r: dict, with_snap: bool) -> bool:
    """Does the judge's OWN run show the class in the entry's frame? Inline programs carry their code in the
    command, so only their `contexto` is laid; everything else lays the snapshot at its real path."""
    laid = (r.get("contexto", []) if r.get("origem") == "inline" else _overlay(r)) if with_snap else []
    files = {f["path"]: (C / f["snap"]).read_text(encoding="utf-8") for f in laid}
    work = C / "juiz_work" / f"{r['id']}-{int(with_snap)}"
    _, _, out = replay.run_isolated(work, r["command"], r.get("cwd", ""), files, CAP)
    out = replay.ANSI.sub("", out)
    return KIND[r["classe"]] in out and replay.frame_in_output({"path": r["path"], "line": r["linha"]}, out)


def _today_fixed(r: dict) -> bool:
    p = Path(r["path"])
    if r.get("origem") != "atribuido" or not r.get("nome_esperado") or not p.is_file():
        return False
    return r["nome_esperado"] not in p.read_text(encoding="utf-8", errors="replace")


def rerun_gaps(rows: list[dict], k: int) -> list[str]:
    pos = sorted((r for r in rows if r["rotulo"] == "positivo" and _proved(r)), key=lambda r: r["id"])
    sample = random.Random(SEED).sample(pos, min(k, len(pos)))
    out = [f"{r['id']}: o juiz re-executou COM o snapshot e não reproduziu" for r in sample if not _rerun(r, True)]
    fixed = next((r for r in pos if _today_fixed(r)), None)
    if fixed is None:
        return out + ["controle negativo indisponível: nenhum positivo atribuído cujo arquivo de hoje perdeu o nome"]
    if _rerun(fixed, False):
        out.append(f"{fixed['id']}: SEM o snapshot ainda reproduz — o corpus mede o código de hoje, não a edição")
    return out


def v0a_classes() -> set[str]:
    rows = [json.loads(ln) for ln in (replay.VGP / "erros_inventario.jsonl").open(encoding="utf-8")]
    return {c for r in rows if r.get("lang") == "python" and r.get("bucket") in ("atribuído", "inline")
            if (c := classe_de(r))}


def all_gaps(rows: list[dict], v0a: set[str], k: int) -> list[str]:
    if not rows:
        return ["entradas.jsonl ausente ou vazio"]
    static = snap_gaps(rows) + negative_gaps(rows) + positive_gaps(rows, v0a)
    return static or rerun_gaps(rows, k)  # re-running costs minutes: only once the files themselves are sound


# ---------- selftest: a synthetic corpus in a temporary root, the valid case and one mutant per guard ----------

def _put(entry_id: str, path: str, text: str, **fields) -> dict:
    (C / "snap").mkdir(exist_ok=True)
    (C / "snap" / f"{entry_id}.py").write_text(text, encoding="utf-8")
    sha = hashlib.sha256(text.encode()).hexdigest()
    return {"id": entry_id, "path": path, "snap": f"snap/{entry_id}.py", "snap_sha256": sha, **fields}


def _synthetic() -> list[dict]:
    alvo = C / "alvo.py"
    alvo.write_text('print("corrigido")\n', encoding="utf-8")
    pos = _put("pos-0", str(alvo), "print(nome_inventado_v0d)\n", rotulo="positivo", classe="nome",
               origem="atribuido", nome_esperado="nome_inventado_v0d", command=f"python3 {alvo}", cwd=str(C),
               linha=1, reproduz=True)
    (C / "replay_v0d").mkdir()
    (C / "replay_v0d/pos-0.json").write_text(json.dumps({"kind_in_output": True, "frame_in_output": True}))
    cand = [f"neg-{i:03d}" for i in range(N_NEG + 50)]
    (C / "negativos_candidatos.json").write_text(json.dumps({"seed": SEED, "candidatos": cand}))
    negs = [_put(i, f"/x/{i}.py", "x = 1\n", rotulo="negativo", classe=None)
            for i in random.Random(SEED).sample(cand, N_NEG)]
    return [pos, *negs]


def _case(mutate) -> list[str]:
    global C
    real = C
    with tempfile.TemporaryDirectory() as d:
        C = Path(d)
        try:
            return all_gaps(mutate(_synthetic()), {"nome"}, 1)
        finally:
            C = real


def _sha_errado(rows: list[dict]) -> list[dict]:
    return [{**rows[0], "snap_sha256": "0" * 64}, *rows[1:]]


def _registro_sem_frame(rows: list[dict]) -> list[dict]:
    (C / "replay_v0d/pos-0.json").write_text(json.dumps({"kind_in_output": True, "frame_in_output": False}))
    return rows


def _comando_que_nao_erra(rows: list[dict]) -> list[dict]:
    return [{**rows[0], "command": "python3 -c pass"}, *rows[1:]]


def _hoje_ainda_quebrado(rows: list[dict]) -> list[dict]:
    Path(rows[0]["path"]).write_text("print(outro_nome_v0d)\n", encoding="utf-8")
    return rows


def selftest() -> int:
    mutants = {"sha do snapshot": _sha_errado, "um negativo a menos": lambda rows: rows[:-1],
               "registro sem frame": _registro_sem_frame, "re-execução não reproduz": _comando_que_nao_erra,
               "controle negativo (hoje ainda erra)": _hoje_ainda_quebrado}
    valid = _case(lambda rows: rows)
    caught = {name: bool(_case(m)) for name, m in mutants.items()}
    print(f"corpus válido sem pendências: {not valid} {valid[:3]}")
    print("mutantes pegos:", " · ".join(f"{n} {v}" for n, v in caught.items()))
    return 0 if not valid and all(caught.values()) else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    rows = load()
    k = int(sys.argv[sys.argv.index("--amostra") + 1]) if "--amostra" in sys.argv else 3
    gaps = all_gaps(rows, v0a_classes(), k)
    pos = sum(r["rotulo"] == "positivo" for r in rows)
    print(f"entradas {len(rows)} · positivos {pos} · negativos {len(rows) - pos}")
    for g in gaps[:40]:
        print("  FALTA", g)
    print(f"veredito: {'PRONTO' if not gaps else f'{len(gaps)} pendências'}")
    return 0 if not gaps else 1


if __name__ == "__main__":
    sys.exit(main())
