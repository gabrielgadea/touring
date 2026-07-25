#!/usr/bin/env python3
"""Shared helpers for /Touring skill premium scripts.

Pure standard library — no third-party dependencies. Three responsibilities:

1. Wrap the ``touring`` CLI with fail-open fallback (daemon-degraded mode).
2. Provide dual-output helpers (machine JSON + human-readable text).
3. Common workspace + file utilities used across all premium scripts.

Importable as a module by sibling scripts; running directly performs an
environment self-check (touring availability, daemon health, workspace root).
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Sequence

HOME = Path.home()
DEFAULT_TIMEOUT = 12.0


# === touring CLI wrapper ===================================================

@dataclass
class TouringResult:
    """Result of a touring CLI invocation."""

    args: list[str]
    exit_code: int
    stdout: str
    stderr: str
    parsed: Any = None
    daemon_degraded: bool = False

    @property
    def ok(self) -> bool:
        return self.exit_code == 0 and not self.daemon_degraded


def touring_run(args: Sequence[str], *, timeout: float = DEFAULT_TIMEOUT,
                parse_json: bool = True) -> TouringResult:
    """Run ``touring <args>`` and capture stdout/stderr/exit.

    Fail-open: returns a TouringResult with ``daemon_degraded=True`` instead of
    raising when the daemon is down, touring is absent, or the call times out.
    When ``parse_json`` is true and stdout decodes as JSON, ``.parsed`` carries
    the value; otherwise it stays ``None`` and callers fall back to ``.stdout``.
    """
    cmd = ["touring", *args]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except FileNotFoundError:
        return TouringResult(list(args), 127, "", "touring: command not found",
                              daemon_degraded=True)
    except subprocess.TimeoutExpired:
        return TouringResult(list(args), 124, "", f"timeout after {timeout}s",
                              daemon_degraded=True)
    except OSError as exc:
        return TouringResult(list(args), 1, "", str(exc), daemon_degraded=True)
    res = TouringResult(list(args), proc.returncode, proc.stdout, proc.stderr)
    if parse_json and proc.stdout.strip():
        try:
            res.parsed = json.loads(proc.stdout)
        except json.JSONDecodeError:
            pass
    if proc.returncode != 0 and "daemon" in proc.stderr.lower():
        res.daemon_degraded = True
    return res


def touring_available() -> bool:
    """True when the touring daemon answers a doctor probe."""
    return touring_run(["doctor", "-j"], timeout=8.0).ok


def require_touring() -> None:
    """Exit 4 if touring CLI is missing entirely (not just daemon down)."""
    if shutil.which("touring") is None:
        emit_error("touring CLI not found in PATH — rebuild via update-touring")
        sys.exit(4)


# === file + workspace utilities ============================================

def resolve_file(path_str: str) -> Path:
    """Resolve a path argument, exiting 2 if it does not exist."""
    p = Path(path_str).expanduser().resolve()
    if not p.exists():
        emit_error(f"path not found: {p}")
        sys.exit(2)
    return p


def find_workspace_root(start: Path | None = None) -> Path | None:
    """Walk up from ``start`` to find a Cargo workspace root."""
    cur = (start or Path.cwd()).resolve()
    for ancestor in [cur, *cur.parents]:
        cargo = ancestor / "Cargo.toml"
        if cargo.is_file():
            try:
                if "[workspace]" in cargo.read_text(encoding="utf-8", errors="replace"):
                    return ancestor
            except OSError:
                continue
    return None


def is_rust_file(path: Path) -> bool:
    return path.suffix == ".rs"


def is_code_file(path: Path) -> bool:
    return path.suffix in {".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".go",
                            ".c", ".cpp", ".h", ".hpp", ".java", ".kt", ".swift"}


def iter_code_files(root: Path, *, exts: set[str] | None = None,
                    excludes: tuple[str, ...] = ("target", "node_modules", ".git")) -> list[Path]:
    """List code files under ``root`` honoring excludes."""
    out: list[Path] = []
    exts = exts or {".rs", ".py", ".ts", ".tsx"}
    if root.is_file():
        return [root] if root.suffix in exts else []
    for path in root.rglob("*"):
        if not path.is_file() or path.suffix not in exts:
            continue
        if any(part in excludes for part in path.parts):
            continue
        out.append(path)
    return sorted(out)


# === output helpers ========================================================

def emit_json(payload: Any, *, indent: int = 2) -> None:
    """Write JSON to stdout (UTF-8, indented by default)."""
    sys.stdout.write(json.dumps(payload, indent=indent, ensure_ascii=False, default=str))
    sys.stdout.write("\n")


def emit_error(msg: str) -> None:
    """Write an error line to stderr without crashing the caller."""
    sys.stderr.write(f"[error] {msg}\n")


def emit_section(title: str, *, char: str = "=") -> None:
    """Print a section header to stdout."""
    bar = char * max(40, len(title))
    print(f"\n{bar}\n{title}\n{bar}")


def emit_kv(key: str, value: Any, *, width: int = 24) -> None:
    """Print an aligned key: value line."""
    print(f"  {key:<{width}} {value}")


def emit_table(rows: list[list[Any]], *, headers: list[str] | None = None) -> None:
    """Print a simple aligned table to stdout."""
    if not rows and not headers:
        return
    string_rows = [[str(c) for c in r] for r in rows]
    all_rows = ([headers] if headers else []) + string_rows
    widths = [max(len(r[i]) for r in all_rows) for i in range(len(all_rows[0]))]
    if headers:
        print("  " + "  ".join(f"{h:<{widths[i]}}" for i, h in enumerate(headers)))
        print("  " + "  ".join("-" * w for w in widths))
    for row in string_rows:
        print("  " + "  ".join(f"{c:<{widths[i]}}" for i, c in enumerate(row)))


def traffic_light(score: float, *, good: float = 0.8, warn: float = 0.5) -> str:
    """Map a 0-1 score to a GREEN / YELLOW / RED verdict label."""
    if score >= good:
        return "GREEN"
    if score >= warn:
        return "YELLOW"
    return "RED"


# === R2 invariants: density + fail-soft + correctness (master-command ready) =

BRIEF_MAX_ITEMS = 5
BRIEF_MAX_BYTES = 2048


def slim_for_brief(payload: Any, *, max_items: int = BRIEF_MAX_ITEMS) -> Any:
    """I-density. Recursively shrink a payload for ``--brief``: arrays longer than
    ``max_items`` keep their first ``max_items`` entries and gain a trailing
    ``{"_elided": N}`` marker; dicts are slimmed value-by-value. The shape and all
    scalars (exit/error/degraded flags) survive — only the bulk is dropped."""
    if isinstance(payload, list):
        head = [slim_for_brief(x, max_items=max_items) for x in payload[:max_items]]
        if len(payload) > max_items:
            head.append({"_elided": len(payload) - max_items})
        return head
    if isinstance(payload, dict):
        return {k: slim_for_brief(v, max_items=max_items) for k, v in payload.items()}
    return payload


def emit_brief(payload: Any) -> None:
    """I-density. Emit a compact (no-indent) JSON digest with large arrays elided.
    Stays under ~2KB for typical payloads (a second, tighter pass runs if not);
    never masks an error/exit/degraded field — those are scalars kept verbatim."""
    text = json.dumps(slim_for_brief(payload), ensure_ascii=False, default=str,
                      separators=(",", ":"))
    if len(text.encode("utf-8")) > BRIEF_MAX_BYTES:
        text = json.dumps(slim_for_brief(payload, max_items=2), ensure_ascii=False,
                          default=str, separators=(",", ":"))
    sys.stdout.write(text + "\n")


def emit_result(payload: Any, args) -> None:
    """Dispatch machine output by the common flags: ``--brief`` (compact digest)
    takes precedence over ``--json`` (full JSON). Returns False when neither flag
    is set, so a caller can fall through to its bespoke human rendering:

        if not emit_result(payload, args):
            <human-readable rendering>
    """
    if getattr(args, "brief", False):
        emit_brief(payload)
        return True
    if getattr(args, "json", False):
        emit_json(payload)
        return True
    return False


def mark_degraded(payload: dict, degraded: bool, reason: str = "") -> dict:
    """I-fail-soft. Annotate a payload with the ``degraded`` flag so a consumer
    never reads a degraded-daemon / stale-index result as authoritative."""
    payload["degraded"] = bool(degraded)
    if degraded and reason:
        payload["degraded_reason"] = reason
    return payload


def grep_fallback(symbol: str, root: Path | str, *, max_hits: int = 20) -> list[str]:
    """I-fail-soft (VP-Scout Chain 7). When the wiring/index daemon is degraded,
    confirm a symbol directly in source via ripgrep/grep so a stale index never
    yields a wrong verdict with confidence. Returns up to ``max_hits`` ``file:line``
    hits (empty list = genuinely absent OR no search tool available)."""
    pat = rf"\b{symbol}\b"
    for tool in (["rg", "-n", "--no-heading", pat, str(root)],
                 ["grep", "-rnE", pat, str(root)]):
        try:
            proc = subprocess.run(tool, capture_output=True, text=True, timeout=15.0)
        except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
            continue
        if proc.returncode in (0, 1):  # 0 = hits, 1 = no hits — both are valid results
            return [ln for ln in proc.stdout.splitlines() if ln.strip()][:max_hits]
    return []


def quality_score(path: Path | str, *, timeout: float = DEFAULT_TIMEOUT) -> dict[str, Any]:
    """I-correctness. The 50-dim composite + tier from ``touring-quality`` — the
    authority — NOT ``ast meta``'s quality_score proxy (which mis-scored a Diamond
    file 0.00). Fail-soft: returns a ``degraded`` payload when the binary/daemon is
    unavailable, so the caller can fall back to Chain 7 rather than trust a wrong 0."""
    cmd = ["touring-quality", "score", str(path), "--format", "json"]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError) as exc:
        return mark_degraded({}, True, f"touring-quality unavailable: {exc}")
    if proc.returncode != 0 or not proc.stdout.strip():
        return mark_degraded({}, True, "touring-quality returned no score")
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return mark_degraded({}, True, "touring-quality output not JSON")
    return {"composite": data.get("composite"), "tier": data.get("tier"),
            "blockers": data.get("blockers", []), "degraded": False}


def parse_definitions(parsed: Any) -> list[dict[str, Any]]:
    """Extract the definition list from ``index find -j`` / ``ast find -j`` output.

    Both wrap their hits in a ``{"count": N, "definitions": [...]}`` envelope, NOT a
    bare list — a script that treated ``.parsed`` as a list silently saw zero hits
    even for indexed symbols. Tolerates a bare list too, for compatibility across
    touring versions; returns only the ``dict`` entries.
    """
    if isinstance(parsed, dict):
        defs = parsed.get("definitions")
        parsed = defs if isinstance(defs, list) else []
    if not isinstance(parsed, list):
        return []
    return [d for d in parsed if isinstance(d, dict)]


# === verdict + scoring =====================================================

@dataclass
class Verdict:
    """A GO/CAUTION/NO_GO decision with reasoning + structured signals."""

    decision: str
    score: float
    reasons: list[str] = field(default_factory=list)
    signals: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


# === arg parsing common patterns ===========================================

def add_common_args(parser) -> None:
    """Inject --json / --brief / --quiet / --timeout into any argparse parser."""
    parser.add_argument("--json", action="store_true",
                        help="Emit machine-readable JSON (default: human text)")
    parser.add_argument("--brief", action="store_true",
                        help="Emit a compact JSON digest (<2KB; large arrays elided with a count)")
    parser.add_argument("--quiet", action="store_true",
                        help="Suppress non-essential output")
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT,
                        help=f"Touring CLI timeout per call (default {DEFAULT_TIMEOUT}s)")


# === self-check ============================================================

def _self_check() -> int:
    print("lib_touring.py self-check:")
    have_cli = shutil.which("touring") is not None
    print(f"  touring CLI in PATH: {have_cli}")
    daemon_ok = touring_available() if have_cli else False
    print(f"  daemon responsive:    {daemon_ok}")
    if daemon_ok:
        doctor = touring_run(["doctor", "-j"])
        if isinstance(doctor.parsed, list):
            ok = sum(1 for c in doctor.parsed if isinstance(c, dict) and c.get("status") == "ok")
            print(f"  doctor checks ok:     {ok}/{len(doctor.parsed)}")
    workspace = find_workspace_root()
    print(f"  workspace root:       {workspace or '(none from cwd)'}")
    return 0 if daemon_ok else 1


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] in ("--help", "-h"):
        print(__doc__)
        sys.exit(0)
    sys.exit(_self_check())
