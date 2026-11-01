"""Scaffolding shared by the per-wave ``w<N>_*.py`` sub-scripts of the premium refactor.

Every sub-script has the same skeleton around its own logic: the workspace paths, an
``argparse`` parser that ends with ``--output-dir`` and ``-v``, a ``main`` that configures
logging and maps ``KeyboardInterrupt``/errors to exit codes, a JSON summary on stdout and a
JSON report written next to the data files. That skeleton used to be pasted into ~40 files;
it lives here once.

Each script keeps its own ``_ROOT`` / ``_DATA_DIR`` / ``_STAGING_DIR`` module constants (the
tests monkeypatch them on the module), seeded from the values below.
"""
from __future__ import annotations

import json
import logging
import sys
from argparse import ArgumentParser, Namespace, RawDescriptionHelpFormatter
from collections.abc import Callable, Iterator
from datetime import UTC, datetime
from pathlib import Path

__all__ = ["ArgumentParser", "Namespace"]  # annotation types the sub-scripts import from here

ROOT = Path(__file__).resolve().parents[2]
DATA_DIR = ROOT / "scripts" / "touring_premium_refactor_2026" / "data"
STAGING_DIR = ROOT / "scripts" / "touring_premium_refactor_2026" / "staging"
CRATES_DIR = ROOT / "crates"


def now_iso() -> str:
    """UTC timestamp in ISO-8601, as every report records it."""
    return datetime.now(UTC).isoformat()


def new_parser(prog: str, doc: str | None) -> ArgumentParser:
    """Parser whose ``--help`` shows the script's module docstring verbatim."""
    return ArgumentParser(prog=prog, description=doc, formatter_class=RawDescriptionHelpFormatter)


def with_verbose(p: ArgumentParser) -> ArgumentParser:
    """Add the trailing ``-v/--verbose`` flag every sub-script ends its parser with."""
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def with_output_dir_and_verbose(p: ArgumentParser, default_dir: Path) -> ArgumentParser:
    """Add ``--output-dir`` (default ``default_dir``) followed by ``-v/--verbose``."""
    p.add_argument("--output-dir", type=Path, default=default_dir)
    return with_verbose(p)


def emit_json(obj: object, *, ensure_ascii: bool = False) -> None:
    """Print ``obj`` as indented JSON on stdout (the sub-scripts' summary line)."""
    sys.stdout.write(json.dumps(obj, indent=2, ensure_ascii=ensure_ascii) + "\n")


def main_frame(
    build_parser: Callable[[], ArgumentParser],
    body: Callable[[Namespace], int],
    logger_name: str,
) -> int:
    """Parse, configure logging, run ``body(args)`` and map interruption/errors to exit codes."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    logger = logging.getLogger(logger_name)
    try:
        return body(args)
    except KeyboardInterrupt:
        logger.warning("interrupted")
        return 130
    except Exception:
        logger.exception("error")
        return 1


def crate_rust_sources(crates_dir: Path) -> Iterator[tuple[Path, Path, str]]:
    """``(crate_dir, rs_file, text)`` of every readable ``*.rs`` under each ``touring-*/src``.

    Crates sorted by name; a crate without ``src/`` and a file that cannot be read are skipped.
    """
    for crate_dir in sorted(crates_dir.glob("touring-*")):
        src = crate_dir / "src"
        if not src.exists():
            continue
        for rs in src.rglob("*.rs"):
            try:
                content = rs.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            yield crate_dir, rs, content


def write_json_report(json_path: Path, report: dict, root: Path) -> dict:
    """Write ``report`` to ``json_path`` and record where, relative to ``root``."""
    json_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    report["json_path"] = str(json_path.relative_to(root))
    return report
