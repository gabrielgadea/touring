"""Cortex operations via subprocess calls to the touring binary (Tier 2).

Provides PII scanning and CILA intent classification by calling
the touring binary's CLI subcommands directly.
"""

from __future__ import annotations

import json
import shutil
import subprocess
from typing import Any


def _find_touring_binary() -> str:
    """Find the touring binary in PATH or known locations.

    Returns:
        Path to the touring binary.

    Raises:
        FileNotFoundError: If binary not found.
    """
    # Check PATH first
    binary = shutil.which("touring")
    if binary:
        return binary

    # Known locations
    from pathlib import Path

    candidates = [
        Path.home() / ".local" / "bin" / "touring",
        Path.cwd() / ".claude" / "rust" / "target" / "release" / "touring",
    ]
    for c in candidates:
        if c.exists() and c.is_file():
            return str(c)

    raise FileNotFoundError(
        "Touring binary not found. Checked PATH and:\n"
        + "\n".join(f"  {c}" for c in candidates)
    )


def _run_touring(
    args: list[str], *, input_text: str | None = None, timeout: int = 10
) -> dict[str, Any]:
    """Run touring binary with args and capture output.

    Args:
        args: Command arguments (e.g., ["scan-pii", "--text", "..."]).
        input_text: Optional text to send via stdin.
        timeout: Timeout in seconds.

    Returns:
        Dict with stdout, stderr, exit_code.

    Raises:
        FileNotFoundError: If binary not found.
        subprocess.TimeoutExpired: If command times out.
    """
    binary = _find_touring_binary()
    cmd = [binary] + args

    result = subprocess.run(
        cmd,
        input=input_text,
        capture_output=True,
        text=True,
        timeout=timeout,
        env=None,  # inherit parent env
    )

    return {
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
        "exit_code": result.returncode,
    }


def scan_pii(text: str) -> dict[str, Any]:
    """Scan text for PII using the touring binary.

    Args:
        text: Text to scan for personally identifiable information.

    Returns:
        Dict with scan results (pii_found, entities, etc.).
    """
    result = _run_touring(["scan-pii"], input_text=text)

    if result["exit_code"] != 0:
        return {"error": result["stderr"], "exit_code": result["exit_code"]}

    try:
        return json.loads(result["stdout"])
    except json.JSONDecodeError:
        return {"raw_output": result["stdout"], "exit_code": result["exit_code"]}


def classify_intent(text: str) -> dict[str, Any]:
    """Classify intent/CILA level of a prompt using `touring cortex prompt`.

    Uses the full cortex pipeline (48 RegexSet patterns + hooks) instead of
    the simpler `classify-intent` subcommand, ensuring accurate CILA routing
    for ANTT process numbers, pipeline triggers, and multi-agent patterns.

    Args:
        text: Prompt text to classify.

    Returns:
        Dict with cila_level, cila_name, techniques, and full hook output.
    """
    payload = json.dumps({"prompt": text})
    result = _run_touring(["cortex", "prompt"], input_text=payload)

    if result["exit_code"] != 0:
        return {"error": result["stderr"], "exit_code": result["exit_code"]}

    try:
        data = json.loads(result["stdout"])
        # Extract metrics for easier access
        metrics = data.get("metrics", {}).get("classify_intent", {})
        return {
            "cila_level": metrics.get("cila_level", 0),
            "cila_name": metrics.get("cila_name", "Unknown"),
            "requires_code_first": metrics.get("requires_code_first", False),
            "requires_pipeline": metrics.get("requires_pipeline", False),
            "routing_strategy": metrics.get("routing_strategy", "direct"),
            "techniques": metrics.get("techniques", []),
            "raw": data,
        }
    except json.JSONDecodeError:
        return {"raw_output": result["stdout"], "exit_code": result["exit_code"]}


def cortex_event(event: str, payload: str | None = None) -> dict[str, Any]:
    """Send a raw cortex event to the touring binary.

    Args:
        event: Event name (e.g., "session-start", "pre-read").
        payload: Optional JSON payload.

    Returns:
        Dict with event processing results.
    """
    args = ["cortex", event]
    result = _run_touring(args, input_text=payload)

    if result["exit_code"] != 0:
        return {"error": result["stderr"], "exit_code": result["exit_code"]}

    try:
        return json.loads(result["stdout"])
    except json.JSONDecodeError:
        return {"raw_output": result["stdout"], "exit_code": result["exit_code"]}
