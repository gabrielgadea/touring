#!/usr/bin/env python3
"""minimax_solver.py - a model-backed SWE-bench solver using MiniMax-M3.

Master Plan E.W2: the credit-authorized bridge between the deterministic
`touring-eval` harness and a real LLM. This is the ONLY component that calls a
model and therefore the ONLY one that costs API credits — by design, it lives
OUTSIDE the harness so that scoring stays free and reproducible.

It reads each instance from a SWE-bench-lite dataset, asks MiniMax-M3 (via the
MiniMax Anthropic-compatible endpoint) to fix the bug, and writes the answer in
the harness `file:<dir>` solver layout:
    <out_dir>/<instance_id>.files.json   {"path": "full new content", ...}
    <out_dir>/<instance_id>.meta.json    {"tokens": int, "claims_resolved": bool, ...}

Then score it deterministically:
    eval/swe_bench/harness.py run --solver file:<out_dir> --dataset <ds> --out report.json

Secrets: the API key is read from an environment variable (default MINIMAX_API_KEY)
- never hardcoded. The endpoint and model are configurable flags.

Usage:
  eval/swe_bench/solvers/minimax_solver.py --dataset <ds.jsonl> --out-dir <patches/>
  eval/swe_bench/solvers/minimax_solver.py --dataset <ds.jsonl> --out-dir <patches/> \
      --model MiniMax-M3 --base-url https://api.minimax.io/anthropic --max-tokens 8000
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Optional

DEFAULT_MODEL = "MiniMax-M3"
DEFAULT_BASE_URL = "https://api.minimax.io/anthropic"
DEFAULT_KEY_ENV = "MINIMAX_API_KEY"
ANTHROPIC_VERSION = "2023-06-01"

SYSTEM_PROMPT = (
    "You are an expert software engineer resolving a bug report. Fix ONLY the described "
    "bug; keep all existing tests and unrelated code UNCHANGED. Output your fix as one or "
    "more edit blocks, each preceded by the file path, in EXACTLY this format:\n"
    "<relative/path>\n"
    "<<<<<<< SEARCH\n"
    "<lines copied verbatim from the current file>\n"
    "=======\n"
    "<the replacement lines>\n"
    ">>>>>>> REPLACE\n"
    "Copy the SEARCH lines character-for-character (including indentation) so they match "
    "exactly. Keep edits minimal and targeted. Output ONLY edit blocks - no prose, no "
    "markdown fences."
)

SEARCH_RE = re.compile(
    r"(?P<path>[^\n`]+?)\n+<{5,}\s*SEARCH\s*\n(?P<search>.*?)\n={5,}\s*\n"
    r"(?P<replace>.*?)\n>{5,}\s*REPLACE",
    re.DOTALL,
)


class MinimaxError(RuntimeError):
    """Raised when the MiniMax endpoint cannot be reached or returns no usable text."""


def build_prompt(problem_statement: str, files: dict) -> str:
    """Render the issue + current repository files into a single user message."""
    parts = [f"ISSUE:\n{problem_statement}\n", "CURRENT FILES:"]
    for path, content in files.items():
        parts.append(f"\n--- {path} ---\n{content}")
    parts.append(
        "\n\nReturn ONLY SEARCH/REPLACE edit blocks (path line, then <<<<<<< SEARCH / "
        "======= / >>>>>>> REPLACE) for the lines you change. Copy SEARCH lines verbatim "
        "from the files above. No prose, no markdown fences."
    )
    return "\n".join(parts)


def call_minimax(prompt: str, *, api_key: str, base_url: str, model: str,
                 max_tokens: int, timeout: int) -> tuple:
    """Call the MiniMax Anthropic-compatible Messages API.

    Returns (text, tokens). Raises MinimaxError on transport/parse failure.
    """
    url = base_url.rstrip("/") + "/v1/messages"
    payload = {
        "model": model,
        "max_tokens": max_tokens,
        "temperature": 0,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": prompt}],
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "x-api-key": api_key,
            "anthropic-version": ANTHROPIC_VERSION,
            "content-type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "ignore")[:300]
        raise MinimaxError(f"HTTP {e.code}: {detail}") from e
    except urllib.error.URLError as e:
        raise MinimaxError(f"network error: {e.reason}") from e
    except (json.JSONDecodeError, ValueError) as e:
        raise MinimaxError(f"bad JSON response: {e}") from e

    # Reasoning models emit a `thinking` block then a `text` block; prefer text,
    # but fall back to thinking content (the edit blocks may live there if the
    # answer was truncated against max_tokens).
    blocks = body.get("content") or []
    text = "".join(b.get("text", "") for b in blocks if b.get("type") == "text").strip()
    usage = body.get("usage") or {}
    tokens = int(usage.get("input_tokens", 0)) + int(usage.get("output_tokens", 0))
    if not text:
        text = "".join(b.get("thinking", "") for b in blocks if b.get("type") == "thinking").strip()
    if not text:
        raise MinimaxError(f"empty text in response (usage={usage})")
    return text, tokens


def apply_search_replace(files: dict, text: str) -> dict:
    """Apply Aider-style SEARCH/REPLACE edit blocks onto the oracle file contents.

    Each block changes one region of one file by exact-string match. Returns
    {path: new_content} for files actually changed (empty if no block applied).
    """
    changed: dict = {}
    for m in SEARCH_RE.finditer(text):
        path = m.group("path").strip().strip("`").strip()
        search, replace = m.group("search"), m.group("replace")
        base = changed.get(path, files.get(path))
        if base is None:
            for k in files:  # tolerate path given with/without a leading prefix
                if k.endswith(path) or path.endswith(k):
                    path, base = k, changed.get(k, files[k])
                    break
        if base is None or search not in base:
            continue
        changed[path] = base.replace(search, replace, 1)
    return changed


def extract_files(text: str) -> dict:
    """Extract the {"files": {...}} object from a model reply, tolerant of fences."""
    cleaned = text.strip()
    if cleaned.startswith("```"):
        # drop the opening fence line and any trailing fence
        cleaned = cleaned.split("\n", 1)[1] if "\n" in cleaned else cleaned
        if cleaned.rstrip().endswith("```"):
            cleaned = cleaned.rstrip()[: -3]
    start, end = cleaned.find("{"), cleaned.rfind("}")
    if start == -1 or end == -1 or end <= start:
        raise MinimaxError("no JSON object found in reply")
    try:
        obj = json.loads(cleaned[start: end + 1])
    except json.JSONDecodeError as e:
        raise MinimaxError(f"reply JSON parse failed: {e}") from e
    files = obj.get("files", obj)
    if not isinstance(files, dict) or not files:
        raise MinimaxError("reply has no 'files' mapping")
    return {str(k): str(v) for k, v in files.items()}


def solve_instance(inst: dict, *, api_key: str, base_url: str, model: str,
                   max_tokens: int, timeout: int) -> tuple:
    """Solve one instance. Returns (files_or_None, meta_dict)."""
    files = inst.get("files") or {}
    prompt = build_prompt(inst.get("problem_statement", ""), files)
    try:
        text, tokens = call_minimax(
            prompt, api_key=api_key, base_url=base_url, model=model,
            max_tokens=max_tokens, timeout=timeout)
        # Prefer SEARCH/REPLACE (small output, robust on large files); fall back to
        # a full-file JSON reply for models/instances that answer that way.
        patch_files = apply_search_replace(files, text)
        if not patch_files:
            patch_files = extract_files(text)
        return patch_files, {"tokens": tokens, "claims_resolved": True, "model": model}
    except MinimaxError as e:
        return None, {"tokens": 0, "claims_resolved": False, "model": model, "error": str(e)}


def load_instances(path: Path) -> list:
    """Load a SWE-bench-lite JSONL dataset into a list of dicts."""
    out = []
    for i, line in enumerate(path.read_text().splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError as e:
            raise SystemExit(f"{path}:{i}: invalid JSONL: {e}")
    return out


def main(argv: Optional[list] = None) -> int:
    ap = argparse.ArgumentParser(
        prog="minimax-solver", description="MiniMax-M3 SWE-bench solver (writes file:<dir> patches).")
    ap.add_argument("--dataset", type=Path, required=True)
    ap.add_argument("--out-dir", type=Path, required=True)
    ap.add_argument("--model", default=DEFAULT_MODEL)
    ap.add_argument("--base-url", default=DEFAULT_BASE_URL)
    ap.add_argument("--key-env", default=DEFAULT_KEY_ENV, help="env var holding the API key")
    ap.add_argument("--max-tokens", type=int, default=32000)
    ap.add_argument("--timeout", type=int, default=300)
    args = ap.parse_args(argv)

    api_key = os.environ.get(args.key_env, "")
    if not api_key:
        raise SystemExit(f"missing API key: set ${args.key_env}")

    instances = load_instances(args.dataset)
    args.out_dir.mkdir(parents=True, exist_ok=True)
    solved = 0
    for inst in instances:
        iid = inst.get("instance_id", "unknown")
        files, meta = solve_instance(
            inst, api_key=api_key, base_url=args.base_url, model=args.model,
            max_tokens=args.max_tokens, timeout=args.timeout)
        (args.out_dir / f"{iid}.meta.json").write_text(json.dumps(meta))
        if files is not None:
            (args.out_dir / f"{iid}.files.json").write_text(json.dumps(files))
            solved += 1
            print(f"  [{args.model}] {iid}: {len(files)} file(s), {meta['tokens']} tokens",
                  file=sys.stderr)
        else:
            print(f"  [{args.model}] {iid}: NO PATCH ({meta.get('error')})", file=sys.stderr)
    print(f"minimax-solver: produced {solved}/{len(instances)} patches -> {args.out_dir}",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
