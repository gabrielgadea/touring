#!/usr/bin/env python3
"""W0.1 — Create reproducible tar snapshot of crates/ + Cargo.{toml,lock}.

Excludes: target/, .touring-cache/, .git/, __pycache__/, *.tmp, *.bak.
Generates SHA-256 sidecar for integrity verification.
Output: docs/baselines/touring-snapshot-pre-refactor-<DATE>.tar.gz (+.sha256)

Usage
-----
    python3 w0_snapshot_tar.py --dry-run
    python3 w0_snapshot_tar.py --output-dir docs/baselines/
    python3 w0_snapshot_tar.py --date 2026-05-11
"""
from __future__ import annotations

import hashlib
import logging
import subprocess
from datetime import UTC, datetime
from pathlib import Path

import script_common as sc

_SCRIPT = "w0_snapshot_tar"

LOGGER = logging.getLogger(__name__)
_ROOT = sc.ROOT
_BASELINES_DIR = _ROOT / "docs" / "baselines"

EXCLUDE_PATTERNS = [
    "--exclude=target",
    "--exclude=.touring-cache",
    "--exclude=.git",
    "--exclude=__pycache__",
    "--exclude=*.tmp",
    "--exclude=*.bak",
    "--exclude=Cargo.toml.tf-bak.*",
]

INCLUDE_PATHS = ["crates", "Cargo.toml", "Cargo.lock"]


def build_parser() -> sc.ArgumentParser:
    """CLI parser."""
    p = sc.new_parser(_SCRIPT, __doc__)
    p.add_argument("--output-dir", type=Path, default=_BASELINES_DIR)
    p.add_argument("--date", type=str, default=datetime.now(UTC).strftime("%Y-%m-%d"))
    p.add_argument("--dry-run", action="store_true",
                   help="Print tar command without executing.")
    return sc.with_verbose(p)


def compute_sha256(path: Path, chunk_size: int = 1 << 16) -> str:
    """Compute SHA-256 hex digest of file in chunks."""
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(chunk_size):
            h.update(chunk)
    return h.hexdigest()


def run(output_dir: Path, date: str, dry_run: bool) -> dict:
    """Create tar snapshot + SHA-256 sidecar."""
    output_dir.mkdir(parents=True, exist_ok=True)
    archive = output_dir / f"touring-snapshot-pre-refactor-{date}.tar.gz"
    sha_file = archive.with_suffix(".sha256")
    cmd = ["tar", *EXCLUDE_PATTERNS, "-czf", str(archive), *INCLUDE_PATHS]
    if dry_run:
        return {"status": "DRY_RUN", "command": " ".join(cmd),
                "archive": str(archive), "sha_file": str(sha_file)}
    LOGGER.info("running tar (may take 30-90s)...")
    proc = subprocess.run(cmd, cwd=_ROOT, capture_output=True, timeout=600, check=False)
    if proc.returncode != 0:
        LOGGER.error("tar failed: %s", proc.stderr.decode("utf-8", errors="replace"))
        return {"status": "ERROR", "returncode": proc.returncode,
                "stderr": proc.stderr.decode("utf-8", errors="replace")}
    size = archive.stat().st_size
    LOGGER.info("archive: %s (%d bytes)", archive, size)
    digest = compute_sha256(archive)
    sha_file.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    return {
        "status": "OK",
        "archive": str(archive.relative_to(_ROOT)),
        "sha_file": str(sha_file.relative_to(_ROOT)),
        "size_bytes": size,
        "sha256": digest,
        "timestamp": sc.now_iso(),
        "excludes": [e.removeprefix("--exclude=") for e in EXCLUDE_PATTERNS],
        "includes": INCLUDE_PATHS,
    }


def _run_and_report(args: sc.Namespace) -> int:
    """Run w0_snapshot_tar, print its summary; the return value is the exit code."""
    result = run(args.output_dir, args.date, args.dry_run)
    sc.emit_json(result)
    return 0 if result.get("status") in ("OK", "DRY_RUN") else 1


def main() -> int:
    """Entry point of w0_snapshot_tar."""
    return sc.main_frame(build_parser, _run_and_report, __name__)


if __name__ == "__main__":
    raise SystemExit(main())
