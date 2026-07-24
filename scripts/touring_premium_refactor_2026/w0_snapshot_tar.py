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

import argparse
import hashlib
import json
import logging
import subprocess
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_ROOT = Path(__file__).resolve().parents[2]
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


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="w0_snapshot_tar", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--output-dir", type=Path, default=_BASELINES_DIR)
    p.add_argument("--date", type=str, default=datetime.now(UTC).strftime("%Y-%m-%d"))
    p.add_argument("--dry-run", action="store_true",
                   help="Print tar command without executing.")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


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
    proc = subprocess.run(cmd, cwd=_ROOT, capture_output=True, timeout=600)
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
        "timestamp": datetime.now(UTC).isoformat(),
        "excludes": [e.removeprefix("--exclude=") for e in EXCLUDE_PATTERNS],
        "includes": INCLUDE_PATHS,
    }


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args.output_dir, args.date, args.dry_run)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0 if result.get("status") in ("OK", "DRY_RUN") else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
