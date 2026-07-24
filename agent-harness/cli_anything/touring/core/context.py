"""Context compiler operations — Tier 2 (subprocess, <200ms)."""
from __future__ import annotations

import json
import subprocess

from .cortex import _find_touring_binary


def compile_context(intent: str, files: str, max_tokens: int = 2000) -> dict:
    """Compile context for given files and intent.

    Calls: touring context compile --intent "..." --files "..." --max-tokens N
    """
    try:
        binary = _find_touring_binary()
        result = subprocess.run(
            [binary, "context", "compile", "--intent", intent, "--files", files, "--max-tokens", str(max_tokens)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError:
                return {"context": result.stdout.strip(), "source": "raw"}
        return {"error": result.stderr.strip() or f"exit code {result.returncode}"}
    except FileNotFoundError:
        return {"error": "touring binary not found"}
    except subprocess.TimeoutExpired:
        return {"error": "timeout (30s)"}
