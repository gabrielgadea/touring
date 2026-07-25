#!/usr/bin/env python3
"""toon_checkpoint — Emit / load TOON v1.0 checkpoints with blake2b hash chain.

TOON (Token-Optimal Object Notation) v1.0 is the canonical checkpoint format
used by TACO-wt. Each checkpoint embeds:

  * format / format_version / kind   — discriminators
  * topic / wave / timestamp         — metadata
  * hash_chain (blake2b-256)         — content-addressed integrity
  * data                             — the payload

The file is human-readable YAML-like text (no external dep — stdlib only),
but the structure is conceptually equivalent to ``pln2_generator/toon_checkpoint``.

Subcommands
-----------
  emit   — write a new checkpoint
  load   — read an existing checkpoint (verifies hash_chain)
  list   — enumerate checkpoints under a directory
  verify — verify hash chain of a checkpoint

Usage
-----
    python3 toon_checkpoint.py emit --phase W12-complete --data data/W12-aggregate.json
    python3 toon_checkpoint.py load .claude/checkpoints/W12-complete_20260523.toon
    python3 toon_checkpoint.py list .claude/checkpoints/
    python3 toon_checkpoint.py verify .claude/checkpoints/W12-complete_20260523.toon
"""

from __future__ import annotations

import argparse
import hashlib
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

# sys.path bridge — allow running as a standalone script
_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    utcnow_iso,
)

_FORMAT_VERSION = "1.0"
_HASH_ALGO = "blake2b"
_HASH_BYTES = 32  # blake2b-256


# ── Hash chain ────────────────────────────────────────────────────────────


def compute_hash(data: Any) -> str:
    """blake2b-256 hex of JSON-canonicalized data."""
    payload = json.dumps(data, sort_keys=True, ensure_ascii=False, default=str)
    return hashlib.blake2b(payload.encode("utf-8"), digest_size=_HASH_BYTES).hexdigest()


# ── Emit / load (stdlib-only TOON serializer) ─────────────────────────────


def _serialize_value(value: Any, indent: int = 0) -> str:
    """Serialize a Python value to TOON-style text. stdlib-only (no PyYAML)."""
    pad = "  " * indent
    if isinstance(value, dict):
        if not value:
            return "{}"
        lines = []
        for key, val in value.items():
            sub = _serialize_value(val, indent + 1)
            if "\n" in sub or isinstance(val, (dict, list)):
                lines.append(f"{pad}{key}:\n{sub}")
            else:
                lines.append(f"{pad}{key}: {sub}")
        return "\n".join(lines)
    if isinstance(value, list):
        if not value:
            return "[]"
        lines = []
        for item in value:
            sub = _serialize_value(item, indent + 1)
            if "\n" in sub:
                lines.append(f"{pad}-\n{sub}")
            else:
                lines.append(f"{pad}- {sub}")
        return "\n".join(lines)
    if isinstance(value, bool):
        return "true" if value else "false"
    if value is None:
        return "null"
    if isinstance(value, str) and ("\n" in value or ":" in value):
        return json.dumps(value, ensure_ascii=False)
    return str(value)


def emit_checkpoint(
    out_path: Path,
    *,
    kind: str = "wave-checkpoint",
    topic: str,
    wave: str = "",
    intent: str = "",
    data: dict[str, Any],
) -> dict[str, Any]:
    """Write a TOON v1.0 checkpoint to ``out_path``. Returns the envelope dict."""
    out_path.parent.mkdir(parents=True, exist_ok=True)
    envelope: dict[str, Any] = {
        "format": "TOON",
        "format_version": _FORMAT_VERSION,
        "kind": kind,
        "topic": topic,
        "wave": wave,
        "intent": intent,
        "timestamp": utcnow_iso(),
        "hash_algo": _HASH_ALGO,
        "hash_chain": compute_hash(data),
        "data": data,
    }
    out_path.write_text(_serialize_value(envelope), encoding="utf-8")
    return envelope


def load_checkpoint(path: Path) -> dict[str, Any]:
    """Read a TOON v1.0 checkpoint. Raises ValueError on hash mismatch.

    The stdlib reader is intentionally minimal: it relies on JSON-canonicalized
    payload re-hashing for integrity. The textual TOON form is human-readable
    but not parsed back here — callers that need round-trip should use the JSON
    sidecar (next to the .toon file) produced by ``emit_checkpoint``.

    For tests and verification, this function re-reads the JSON-canonical form
    when a ``.json`` sidecar exists; otherwise it tries naive parsing.
    """
    if not path.exists():
        msg = f"Checkpoint not found: {path}"
        raise FileNotFoundError(msg)

    sidecar = path.with_suffix(".json")
    if sidecar.exists():
        envelope = json.loads(sidecar.read_text(encoding="utf-8"))
    else:
        envelope = _naive_parse_toon(path.read_text(encoding="utf-8"))

    data = envelope.get("data", {})
    expected = envelope.get("hash_chain", "")
    actual = compute_hash(data)
    if expected and expected != actual:
        msg = f"Hash mismatch in {path}: expected {expected[:16]}..., got {actual[:16]}..."
        raise ValueError(msg)
    return envelope


def _naive_parse_toon(text: str) -> dict[str, Any]:
    """Naive TOON parser — top-level scalars only.

    Used as a fallback when no JSON sidecar exists. Reads ``key: value`` pairs
    at indent 0 and stops at the first ``data:`` line (which marks the start
    of a nested block).
    """
    result: dict[str, Any] = {}
    nested_data_lines: list[str] = []
    in_data = False
    for raw_line in text.splitlines():
        if in_data:
            nested_data_lines.append(raw_line)
            continue
        if raw_line.startswith("data:"):
            in_data = True
            continue
        if ": " in raw_line and not raw_line.startswith(" "):
            key, val = raw_line.split(": ", 1)
            result[key.strip()] = val.strip()
    # If we captured data lines, try to read them as plain JSON
    if nested_data_lines:
        joined = "\n".join(nested_data_lines)
        try:
            result["data"] = json.loads(joined)
        except json.JSONDecodeError:
            result["data"] = {}
    return result


def list_checkpoints(directory: Path) -> list[Path]:
    """List all ``.toon`` files in ``directory`` sorted by mtime descending."""
    if not directory.exists():
        return []
    return sorted(directory.glob("*.toon"), key=lambda p: p.stat().st_mtime, reverse=True)


def verify_checkpoint(path: Path) -> bool:
    """Verify hash chain of a checkpoint. Returns True iff valid."""
    try:
        load_checkpoint(path)
        return True
    except (FileNotFoundError, ValueError):
        return False


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="toon_checkpoint", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p_emit = sub.add_parser("emit", help="Write a new TOON checkpoint")
    p_emit.add_argument("--phase", required=True, help="Phase / topic identifier")
    p_emit.add_argument("--wave", default="", help="Optional wave id (e.g. W12)")
    p_emit.add_argument("--intent", default="", help="One-line intent")
    p_emit.add_argument("--data", type=Path, required=True,
                        help="Path to a JSON file containing the payload")
    p_emit.add_argument("--out-dir", type=Path,
                        default=Path(".claude/checkpoints"),
                        help="Output directory")
    p_emit.add_argument("-j", "--json", dest="json_only", action="store_true")

    p_load = sub.add_parser("load", help="Read + verify a TOON checkpoint")
    p_load.add_argument("path", type=Path)
    p_load.add_argument("-j", "--json", dest="json_only", action="store_true")

    p_list = sub.add_parser("list", help="List .toon files in a directory")
    p_list.add_argument("directory", type=Path, nargs="?",
                        default=Path(".claude/checkpoints"))
    p_list.add_argument("-j", "--json", dest="json_only", action="store_true")

    p_verify = sub.add_parser("verify", help="Verify hash chain of a checkpoint")
    p_verify.add_argument("path", type=Path)

    for sp in (p_emit, p_load, p_list, p_verify):
        sp.add_argument("--apply", action="store_true",
                        help="No-op for toon_checkpoint (kept for symmetry).")
        sp.add_argument("-v", "--verbose", action="store_true")

    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Dispatch to subcommand."""
    if args.command == "emit":
        date_str = datetime.now(UTC).strftime("%Y%m%d")
        out_path = args.out_dir / f"{args.phase}_{date_str}.toon"
        data = json.loads(args.data.read_text(encoding="utf-8"))
        envelope = emit_checkpoint(
            out_path,
            kind="wave-checkpoint" if args.wave else "phase-checkpoint",
            topic=args.phase,
            wave=args.wave,
            intent=args.intent,
            data=data,
        )
        # Also write JSON sidecar for reliable round-trip
        out_path.with_suffix(".json").write_text(
            json.dumps(envelope, indent=2, ensure_ascii=False, default=str),
            encoding="utf-8",
        )
        return {
            "status": "OK",
            "path": str(out_path),
            "json_sidecar": str(out_path.with_suffix(".json")),
            "hash_chain": envelope["hash_chain"],
        }

    if args.command == "load":
        envelope = load_checkpoint(args.path)
        return {"status": "OK", **envelope}

    if args.command == "list":
        paths = list_checkpoints(args.directory)
        return {
            "status": "OK",
            "directory": str(args.directory),
            "count": len(paths),
            "checkpoints": [str(p) for p in paths],
        }

    if args.command == "verify":
        ok = verify_checkpoint(args.path)
        return {"status": "OK" if ok else "FAIL",
                "path": str(args.path), "valid": ok}

    msg = f"Unknown command: {args.command}"
    raise ValueError(msg)


def main() -> int:
    """CLI entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps(result, indent=2, ensure_ascii=False, default=str) + "\n")
        if result.get("status") == "OK":
            return EXIT_OK
        return EXIT_FAIL
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except FileNotFoundError as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("toon_checkpoint failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
