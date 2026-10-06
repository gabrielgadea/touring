#!/usr/bin/env python3
"""cjk_experiment.py — where the foreign-script characters inside Portuguese prose come from: checkpoint x context.

Pre-registration: `pre-registro.md` next to this file, committed before any data-producing call. The script
refuses `pilot` and `run` while that file is untracked or differs from HEAD.

Subcommands:
  build     assemble the three prompts; print size and sha256 (no model call)
  pilot     up to 4 harness-validation calls; ONLY metadata is kept, the reply text is dropped unread
  run       collection: N valid calls per cell, order interleaved by the pre-registered seed, resumable
  blind     export the occurrences blind (random id, characters, context) and the key in a separate file
  analyze   merge both raters' labels; T1-T6 one-sided Fisher exact, Holm, Wilson, Cohen's kappa

The detector is the hook's own (`prose_integrity_gate.find`), imported, never copied: the experiment measures
exactly what the gate would see.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import subprocess
import sys
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
PREREG = HERE / "pre-registro.md"
GATE_DIR = Path.home() / ".claude" / "skills" / "Touring" / "scripts" / "hooks"
sys.path.insert(0, str(GATE_DIR))
import prose_integrity_gate as gate  # noqa: E402  (the single source of the signal)

COMMIT = "e0b7bc1c988530ef8893e27422fa0c06235abf75"
MODELS = {"opus": "claude-opus-5-5", "minimax": "MiniMax-M3.1-Flash-Preview"}
CONTEXTS = ("curto", "longo", "longo_cjk")
CELLS = tuple((m, c) for m in MODELS for c in CONTEXTS)
N_PER_CELL = 30
SEED = 20261006
TARGET_CHARS = 400_000
MAX_FILE_CHARS = 60_000
MAX_SNIPPETS = 24
SNIPPET_MAX_CHARS = 300
CONCURRENCY = {"opus": 2, "minimax": 4}
TIMEOUT_S = 600
RETRIES = 2
BUDGET_USD = 120.0
EFFORT = "medium"
MINIMAX_BASE_URL = "https://api.minimax.io/anthropic"
SNIPPET_SOURCE = "crates/touring-server/src/cli/common.rs"
OCC_CONTEXT = 40
ALPHA = 0.05
BASE_RATE_MIN = 10
UNDERSAMPLED = 25
LABELS = ("CORRUPT", "LEGIT", "UNCLEAR")
TESTS = (  # (id, greater cell, lesser cell, question) — pre-registro §6
    ("T1", ("minimax", "curto"), ("opus", "curto"), "checkpoint, contexto curto"),
    ("T2", ("minimax", "longo"), ("opus", "longo"), "checkpoint, contexto longo"),
    ("T3", ("opus", "longo"), ("opus", "curto"), "comprimento, Opus"),
    ("T4", ("minimax", "longo"), ("minimax", "curto"), "comprimento, MiniMax"),
    ("T5", ("opus", "longo_cjk"), ("opus", "longo"), "CJK no contexto, Opus"),
    ("T6", ("minimax", "longo_cjk"), ("minimax", "longo"), "CJK no contexto, MiniMax"),
)

SYSTEM = ("Você é um engenheiro de software sênior revisando código de um projeto em Rust. "
          "Responda sempre em português do Brasil.")
TASK_HEAD = ("Escreva um relatório técnico em português do Brasil, de 700 a 900 palavras, em prosa contínua (sem "
             "tabelas nem listas), dirigido ao mantenedor do projeto. Explique o que o trecho Rust abaixo faz, por "
             "que a segunda tentativa de parse existe, quais riscos de correção você vê e o que você mudaria. Cite "
             "os identificadores do código quando falar deles.")
REFERENCE_HEAD = ("Material de referência: arquivos do repositório deste projeto, na íntegra. Você não precisa "
                  "resumir nem citar este material.")
SECRET_ENV = ("MINIMAX_API_KEY",)
STRIPPED_ENV = ("ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_MODEL",
                "ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                "CLAUDE_CODE_SUBAGENT_MODEL")


class ExperimentError(RuntimeError):
    """A precondition of the pre-registered protocol does not hold; the message names it."""


# ── sources (git at the pinned commit) ───────────────────────────────────────

def _git(*args: str, data: bytes | None = None) -> bytes:
    p = subprocess.run(["git", "-C", str(REPO), *args], input=data, capture_output=True, check=False)
    if p.returncode != 0:
        raise ExperimentError(f"git {' '.join(args[:2])} falhou (rc={p.returncode}): "
                              f"{p.stderr.decode('utf-8', 'replace')[-300:]}")
    return p.stdout


def tracked_paths(commit: str = COMMIT) -> list[str]:
    """Every path tracked at `commit`, sorted."""
    return sorted(_git("ls-tree", "-r", "--name-only", commit).decode("utf-8").splitlines())


def read_blobs(paths: Iterable[str], commit: str = COMMIT) -> dict[str, str]:
    """Contents at `commit` through ONE `git cat-file --batch`; binary blobs (a NUL byte) are left out."""
    paths = list(paths)
    out = _git("cat-file", "--batch", data="".join(f"{commit}:{p}\n" for p in paths).encode("utf-8"))
    blobs: dict[str, str] = {}
    i = 0
    for p in paths:
        nl = out.index(b"\n", i)
        header = out[i:nl].split()
        if len(header) < 3 or header[1] != b"blob":
            i = nl + 1
            continue
        size = int(header[2])
        raw = out[nl + 1:nl + 1 + size]
        i = nl + 1 + size + 1
        if b"\x00" not in raw:
            blobs[p] = raw.decode("utf-8", "replace")
    return blobs


def eligible_prefix_path(path: str) -> bool:
    """The pre-registered file families of the long prefix (pre-registro §2)."""
    depth = path.count("/")
    return ((path.startswith("docs/audits/") and path.endswith(".md") and depth == 2)
            or (path.startswith("crates/touring-cli/src/") and path.endswith(".rs"))
            or (path.startswith("crates/touring-server/src/cli/") and path.endswith(".rs") and depth == 4))


def eligible_snippet_path(path: str) -> bool:
    """Legitimate-CJK sources only: never docs (where the defect is quoted), the mirror, or the gate itself."""
    return not (path.startswith(("docs/", "client/", "docs-site/")) or "prose_integrity" in path)


def task_snippet(blobs: dict[str, str]) -> str:
    """`parse_tolerating_json` + `rejects_global_json`, verbatim from the pinned commit."""
    src = blobs[SNIPPET_SOURCE]
    start = src.index("pub fn parse_tolerating_json")
    tail = src.index("fn rejects_global_json", start)
    end = src.index("\n}\n", tail) + 2
    return src[start:end]


@dataclass(frozen=True)
class Prompt:
    context: str
    text: str
    files: tuple[str, ...]
    snippets: tuple[str, ...]

    @property
    def sha256(self) -> str:
        return hashlib.sha256(self.text.encode("utf-8")).hexdigest()


def _prefix_blocks(blobs: dict[str, str]) -> tuple[list[str], list[str]]:
    blocks, used, total = [], [], 0
    for p in sorted(x for x in blobs if eligible_prefix_path(x)):
        txt = blobs[p]
        if len(txt) > MAX_FILE_CHARS or gate.find(txt):
            continue
        block = f"### arquivo: {p}\n{txt}\n"
        if total + len(block) > TARGET_CHARS:
            cut = block[:TARGET_CHARS - total]
            cut = cut[:cut.rfind("\n") + 1]
            if cut:
                blocks.append(cut)
                used.append(f"{p} (truncado)")
            break
        blocks.append(block)
        used.append(p)
        total += len(block)
    return blocks, used


def _cjk_snippets(blobs: dict[str, str]) -> list[str]:
    out = []
    for p in sorted(x for x in blobs if eligible_snippet_path(x)):
        hits = gate.find(blobs[p])
        if not hits:
            continue
        lines = blobs[p].split("\n")
        line_no = blobs[p].count("\n", 0, hits[0].pos)
        body = "\n".join(lines[max(0, line_no - 1):line_no + 2])[:SNIPPET_MAX_CHARS]
        out.append(f"### trecho: {p}:{line_no + 1}\n{body}\n")
        if len(out) >= MAX_SNIPPETS:
            break
    return out


def interleave(blocks: list[str], inserts: list[str]) -> list[str]:
    """`inserts` spread evenly between `blocks`, in order, none at the very start."""
    if not inserts:
        return list(blocks)
    k, n = len(inserts), len(blocks)
    slots = {min(n, round((i + 1) * n / (k + 1))): [] for i in range(k)}
    for i, s in enumerate(inserts):
        slots[min(n, round((i + 1) * n / (k + 1)))].append(s)
    out = []
    for j, b in enumerate(blocks):
        out.append(b)
        out.extend(slots.get(j + 1, []))
    return out


def build_prompts() -> dict[str, Prompt]:
    """The three pre-registered prompts. Pure function of the pinned commit."""
    paths = tracked_paths()
    blobs = read_blobs(p for p in paths if eligible_prefix_path(p) or eligible_snippet_path(p))
    task = f"{TASK_HEAD}\n\n```rust\n{task_snippet(blobs)}```"
    blocks, used = _prefix_blocks(blobs)
    snippets = _cjk_snippets(blobs)

    def framed(body: list[str]) -> str:
        return f"{REFERENCE_HEAD}\n\n<referencia>\n{''.join(body)}</referencia>\n\n{task}"

    return {
        "curto": Prompt("curto", task, (), ()),
        "longo": Prompt("longo", framed(blocks), tuple(used), ()),
        "longo_cjk": Prompt("longo_cjk", framed(interleave(blocks, snippets)), tuple(used),
                            tuple(s.split("\n", 1)[0] for s in snippets)),
    }


# ── preconditions ────────────────────────────────────────────────────────────

def prereg_problem(repo: Path = REPO, path: Path = PREREG) -> str | None:
    """None when the pre-registration is tracked and identical to HEAD; otherwise the reason."""
    rel = str(path.resolve().relative_to(repo.resolve()))
    tracked = subprocess.run(["git", "-C", str(repo), "ls-files", "--error-unmatch", rel],
                             capture_output=True, check=False)
    if tracked.returncode != 0:
        return f"{rel} não está commitado"
    dirty = subprocess.run(["git", "-C", str(repo), "diff", "--quiet", "HEAD", "--", rel],
                           capture_output=True, check=False)
    if dirty.returncode != 0:
        return f"{rel} difere do HEAD (mudança depois do pré-registro)"
    return None


# ── one call ─────────────────────────────────────────────────────────────────

def build_env(model_key: str, base: dict[str, str] | None = None) -> dict[str, str]:
    """The child's environment: same harness for both checkpoints, only endpoint and model differ."""
    env = dict(os.environ if base is None else base)
    for k in STRIPPED_ENV:
        env.pop(k, None)
    env.update({"TOURING_WORK_OUTER_DISABLED": "1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
                "DISABLE_AUTOUPDATER": "1"})
    if model_key == "minimax":
        key = env.get("MINIMAX_API_KEY")
        if not key:
            raise ExperimentError("MINIMAX_API_KEY ausente do ambiente")
        model = MODELS["minimax"]
        env.update({"ANTHROPIC_BASE_URL": MINIMAX_BASE_URL, "ANTHROPIC_API_KEY": key,
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": model, "ANTHROPIC_DEFAULT_SONNET_MODEL": model,
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": model, "CLAUDE_CODE_SUBAGENT_MODEL": model})
    return env


def claude_command(model_key: str) -> list[str]:
    return ["claude", "-p", "--model", MODELS[model_key], "--system-prompt", SYSTEM, "--tools", "",
            "--setting-sources", "", "--strict-mcp-config", "--no-session-persistence",
            "--output-format", "json", "--effort", EFFORT]


def redact(text: str, env: dict[str, str]) -> str:
    """Every secret value of `env` replaced, so an error tail never carries a key into a record."""
    for k in (*SECRET_ENV, "ANTHROPIC_API_KEY"):
        v = env.get(k)
        if v and len(v) >= 8:
            text = text.replace(v, "<redacted>")
    return text


def call_once(model_key: str, prompt: Prompt) -> dict:
    """One `claude -p` call in an empty temp dir. The returned record never contains the environment."""
    env = build_env(model_key)
    t0 = time.time()
    with tempfile.TemporaryDirectory(prefix="cjk-exp-") as cwd:
        try:
            p = subprocess.run(claude_command(model_key), input=prompt.text, capture_output=True, text=True,
                               cwd=cwd, env=env, timeout=TIMEOUT_S, check=False)
            rc, out, err = p.returncode, p.stdout, p.stderr
        except subprocess.TimeoutExpired:
            rc, out, err = -9, "", f"timeout {TIMEOUT_S}s"
    rec = {"model_key": model_key, "context": prompt.context, "prompt_sha256": prompt.sha256,
           "ts_start": time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(t0)),
           "duration_s": round(time.time() - t0, 2), "rc": rc}
    rec.update(parse_reply(rc, out, err, env))
    return rec


def parse_reply(rc: int, out: str, err: str, env: dict[str, str]) -> dict:
    """The fields of one `claude -p --output-format json` reply; `ok` only for a clean, non-empty result."""
    try:
        d = json.loads(out)
    except (ValueError, TypeError):
        d = None
    if not isinstance(d, dict):
        return {"ok": False, "error": redact((err or out)[-400:], env)}
    result = d.get("result") if isinstance(d.get("result"), str) else ""
    ok = rc == 0 and not d.get("is_error") and bool(result.strip())
    fields = {"ok": ok, "is_error": d.get("is_error"), "subtype": d.get("subtype"), "result": result,
              "usage": d.get("usage"), "total_cost_usd": d.get("total_cost_usd"),
              "models_used": sorted((d.get("modelUsage") or {}).keys()), "num_turns": d.get("num_turns")}
    if not ok:
        fields["error"] = redact((result or err)[-400:], env)
    return fields


def call_with_retries(model_key: str, prompt: Prompt) -> dict:
    rec: dict = {}
    for attempt in range(RETRIES + 1):
        rec = call_once(model_key, prompt)
        rec["attempt"] = attempt + 1
        if rec["ok"]:
            break
    return rec


# ── collection ───────────────────────────────────────────────────────────────

def schedule(seed: int = SEED, n: int = N_PER_CELL) -> list[tuple[str, str, int]]:
    """The 6 x n calls in the pre-registered interleaved order."""
    plan = [(m, c, r) for m, c in CELLS for r in range(n)]
    random.Random(seed).shuffle(plan)
    return plan


class Ledger:
    """Append-only JSONL with a lock; fsync per record so a kill never loses a finished call."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.lock = threading.Lock()

    def records(self) -> list[dict]:
        if not self.path.exists():
            return []
        return [json.loads(l) for l in self.path.read_text(encoding="utf-8").splitlines() if l.strip()]

    def append(self, rec: dict) -> None:
        with self.lock, self.path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
            fh.flush()
            os.fsync(fh.fileno())


def _require_prereg() -> None:
    problem = prereg_problem()
    if problem:
        raise ExperimentError(f"pré-registro inválido: {problem}. Commite-o antes de qualquer chamada.")


def run(out: Path, budget: float = BUDGET_USD) -> dict:
    """Collect the pre-registered sample. Resumable: a (cell, rep) already valid in `out` is skipped."""
    _require_prereg()
    prompts = build_prompts()
    ledger = Ledger(out)
    done = {(r["model_key"], r["context"], r["rep"]) for r in ledger.records() if r.get("ok")}
    spent = [sum(float(r.get("total_cost_usd") or 0) for r in ledger.records())]
    lock = threading.Lock()
    tally = {"ok": 0, "failed": 0, "skipped_budget": 0}

    def job(m: str, c: str, rep: int) -> None:
        with lock:
            if spent[0] >= budget:
                tally["skipped_budget"] += 1
                return
        rec = call_with_retries(m, prompts[c])
        rec.update({"rep": rep, "cell": f"{m}/{c}"})
        ledger.append(rec)
        with lock:
            spent[0] += float(rec.get("total_cost_usd") or 0)
            tally["ok" if rec["ok"] else "failed"] += 1
            print(f"[{tally['ok'] + tally['failed']}] {m}/{c}#{rep} ok={rec['ok']} {rec['duration_s']}s "
                  f"gasto={spent[0]:.2f}", file=sys.stderr, flush=True)

    pools = {m: ThreadPoolExecutor(max_workers=CONCURRENCY[m]) for m in MODELS}
    futures = [pools[m].submit(job, m, c, rep) for m, c, rep in schedule() if (m, c, rep) not in done]
    for f in futures:
        f.result()
    for p in pools.values():
        p.shutdown()
    return {**tally, "spent_usd": round(spent[0], 4), "already_done": len(done)}


def pilot(out: Path) -> list[dict]:
    """Harness validation. Keeps metadata only: the reply text is dropped here, unread."""
    _require_prereg()
    prompts = build_prompts()
    ledger = Ledger(out)
    meta = []
    for m, c in (("opus", "curto"), ("minimax", "curto"), ("opus", "longo"), ("minimax", "longo")):
        rec = call_once(m, prompts[c])
        rec["result_chars"] = len(rec.pop("result", "") or "")
        rec["prompt_chars"] = len(prompts[c].text)
        ledger.append(rec)
        meta.append(rec)
    return meta


# ── blind export ─────────────────────────────────────────────────────────────

def occurrences(text: str) -> list[tuple[int, int]]:
    """[start, end) of every contiguous run of signal characters."""
    runs: list[list[int]] = []
    for h in gate.find(text):
        if runs and h.pos == runs[-1][1]:
            runs[-1][1] = h.pos + 1
        else:
            runs.append([h.pos, h.pos + 1])
    return [(a, b) for a, b in runs]


def blind(runs_path: Path, blind_path: Path, key_path: Path, seed: int = SEED + 1) -> int:
    """Write the raters' file (no cell, model or rep) and the key; return the number of occurrences."""
    rng = random.Random(seed)
    items, keys = [], []
    for idx, rec in enumerate(Ledger(runs_path).records()):
        if not rec.get("ok") or "rep" not in rec:
            continue
        text = rec["result"]
        for a, b in occurrences(text):
            oid = f"o{rng.getrandbits(40):010x}"
            items.append({"id": oid, "chars": text[a:b],
                          "context": text[max(0, a - OCC_CONTEXT):b + OCC_CONTEXT].replace("\n", " ")})
            keys.append({"id": oid, "record": idx, "cell": rec["cell"], "rep": rec["rep"], "start": a, "end": b})
    rng.shuffle(items)
    blind_path.write_text("".join(json.dumps(i, ensure_ascii=False) + "\n" for i in items), encoding="utf-8")
    key_path.write_text("".join(json.dumps(k, ensure_ascii=False) + "\n" for k in keys), encoding="utf-8")
    return len(items)


# ── statistics ───────────────────────────────────────────────────────────────

def fisher_greater(a: int, b: int, c: int, d: int) -> float:
    """One-sided Fisher exact p for the 2x2 table [[a, b], [c, d]]: P(X >= a) under the hypergeometric."""
    row1, col1, n = a + b, a + c, a + b + c + d
    denom = math.comb(n, row1)
    return sum(math.comb(col1, x) * math.comb(n - col1, row1 - x)
               for x in range(a, min(row1, col1) + 1)) / denom


def wilson(k: int, n: int, z: float = 1.96) -> tuple[float, float]:
    """Wilson score interval for k successes in n trials."""
    if n == 0:
        return 0.0, 1.0
    p = k / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    margin = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return max(0.0, centre - margin), min(1.0, centre + margin)


def holm(pvalues: list[float], alpha: float = ALPHA) -> list[bool]:
    """Holm-Bonferroni step-down: which hypotheses are rejected."""
    order = sorted(range(len(pvalues)), key=lambda i: pvalues[i])
    reject = [False] * len(pvalues)
    for rank, i in enumerate(order):
        if pvalues[i] > alpha / (len(pvalues) - rank):
            break
        reject[i] = True
    return reject


def cohen_kappa(pairs: list[tuple[str, str]]) -> float | None:
    """Cohen's kappa over paired nominal labels; None when undefined (no pairs, or chance agreement is 1)."""
    if not pairs:
        return None
    n = len(pairs)
    po = sum(1 for x, y in pairs if x == y) / n
    cats = {x for p in pairs for x in p}
    pe = sum((sum(1 for x, _ in pairs if x == c) / n) * (sum(1 for _, y in pairs if y == c) / n) for c in cats)
    return None if pe == 1 else (po - pe) / (1 - pe)


def _labels(path: Path) -> dict[str, str]:
    out = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            o = json.loads(line)
            if o.get("label") not in LABELS:
                raise ExperimentError(f"rótulo inválido em {path.name}: {o}")
            out[o["id"]] = o["label"]
    return out


def _count_cells(records: list[dict], keys: list[dict], la: dict[str, str], lb: dict[str, str]) -> dict:
    """Per-cell counts of the primary (both raters CORRUPT) and secondary outcomes (§5)."""
    by_record: dict[int, list[dict]] = {}
    for k in keys:
        by_record.setdefault(k["record"], []).append(k)
    cells = {f"{m}/{c}": {"n": 0, "primary": 0, "either": 0, "raw": 0, "occ": 0, "chars": 0} for m, c in CELLS}
    for idx, rec in enumerate(records):
        if not rec.get("ok") or "rep" not in rec:
            continue
        s, occ = cells[rec["cell"]], by_record.get(idx, [])
        s["n"] += 1
        s["chars"] += len(rec["result"])
        s["occ"] += len(occ)
        s["raw"] += bool(occ)
        s["primary"] += any(la[k["id"]] == lb[k["id"]] == "CORRUPT" for k in occ)
        s["either"] += any("CORRUPT" in (la[k["id"]], lb[k["id"]]) for k in occ)
    return cells


def _describe_cell(s: dict) -> None:
    lo, hi = wilson(s["primary"], s["n"])
    s["rate"] = round(s["primary"] / s["n"], 4) if s["n"] else None
    s["wilson95"] = [round(lo, 4), round(hi, 4)]
    s["occ_per_10k_chars"] = round(1e4 * s["occ"] / s["chars"], 3) if s["chars"] else None
    s["undersampled"] = s["n"] < UNDERSAMPLED


def _planned_tests(cells: dict, base_rate_ok: bool) -> list[dict]:
    """T1-T6 one-sided Fisher exact, Holm-Bonferroni, and the §7 verdict of each."""
    tests = []
    for tid, g, l, q in TESTS:
        sg, sl = cells["/".join(g)], cells["/".join(l)]
        p = fisher_greater(sg["primary"], sg["n"] - sg["primary"], sl["primary"], sl["n"] - sl["primary"])
        tests.append({"id": tid, "greater": "/".join(g), "lesser": "/".join(l), "question": q, "p": p})
    unsupported = "inconclusivo" if base_rate_ok else "inconclusivo (taxa-base insuficiente)"
    for t, rejected in zip(tests, holm([t["p"] for t in tests])):
        t.update({"p": round(t["p"], 6), "reject_holm": rejected,
                  "verdict": "sustentado" if rejected else unsupported})
    return tests


def analyze(runs_path: Path, key_path: Path, labels_a: Path, labels_b: Path) -> dict:
    """The pre-registered analysis (§5-§7)."""
    keys = [json.loads(l) for l in key_path.read_text(encoding="utf-8").splitlines() if l.strip()]
    la, lb = _labels(labels_a), _labels(labels_b)
    missing = [k["id"] for k in keys if k["id"] not in la or k["id"] not in lb]
    if missing:
        raise ExperimentError(f"{len(missing)} ocorrência(s) sem os dois rótulos (ex.: {missing[:3]})")
    cells = _count_cells(Ledger(runs_path).records(), keys, la, lb)
    for s in cells.values():
        _describe_cell(s)
    total_primary = sum(s["primary"] for s in cells.values())
    pairs = [(la[k["id"]], lb[k["id"]]) for k in keys]
    kappa = cohen_kappa(pairs)
    return {"cells": cells, "tests": _planned_tests(cells, total_primary >= BASE_RATE_MIN),
            "total_primary": total_primary, "base_rate_ok": total_primary >= BASE_RATE_MIN,
            "occurrences": len(keys), "kappa": None if kappa is None else round(kappa, 4),
            "agreement": round(sum(x == y for x, y in pairs) / len(pairs), 4) if pairs else None}


# ── CLI ──────────────────────────────────────────────────────────────────────

def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("build")
    sp = sub.add_parser("pilot")
    sp.add_argument("--out", type=Path, default=HERE / "pilot.jsonl")
    sr = sub.add_parser("run")
    sr.add_argument("--out", type=Path, default=HERE / "runs.jsonl")
    sr.add_argument("--budget", type=float, default=BUDGET_USD)
    sb = sub.add_parser("blind")
    sb.add_argument("--runs", type=Path, default=HERE / "runs.jsonl")
    sa = sub.add_parser("analyze")
    sa.add_argument("--runs", type=Path, default=HERE / "runs.jsonl")
    sa.add_argument("--labels", type=Path, nargs=2, default=[HERE / "labels-f9.jsonl", HERE / "labels-b4.jsonl"])
    args = ap.parse_args(argv)
    try:
        if args.cmd == "build":
            for c, p in build_prompts().items():
                print(json.dumps({"context": c, "chars": len(p.text), "sha256": p.sha256, "files": len(p.files),
                                  "snippets": len(p.snippets), "signal_in_prompt": len(gate.find(p.text))}))
        elif args.cmd == "pilot":
            for rec in pilot(args.out):
                print(json.dumps(rec, ensure_ascii=False))
        elif args.cmd == "run":
            print(json.dumps(run(args.out, args.budget)))
        elif args.cmd == "blind":
            n = blind(args.runs, HERE / "blind.jsonl", HERE / "blind-key.jsonl")
            print(json.dumps({"occurrences": n, "blind": "blind.jsonl", "key": "blind-key.jsonl"}))
        elif args.cmd == "analyze":
            res = analyze(args.runs, HERE / "blind-key.jsonl", *args.labels)
            (HERE / "analysis.json").write_text(json.dumps(res, ensure_ascii=False, indent=2) + "\n",
                                                encoding="utf-8")
            print(json.dumps(res, ensure_ascii=False, indent=2))
    except ExperimentError as e:
        print(f"erro: {e}", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
