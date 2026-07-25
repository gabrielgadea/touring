#!/usr/bin/env python3
"""scaffold_wave — Generate the directory + sub-scripts for a new wave.

Reads canonical Jinja2 templates from ``assets/templates/`` and renders:
  * ``W<N>/<sub_name>.py`` for each forensic sub-script
  * ``W<N>/validate_W<N>.py`` for the wave validator
  * ``W<N>/tests/conftest.py`` for the test suite
  * ``W<N>/tests/test_<sub_name>.py`` stub for each sub-script (when --with-tests)

Templates honor lessons L3 (uniform whitespace) and L8 (validator is a sub-script).
Wave directories are placed under the plan directory; if no plan exists yet,
scaffold_wave creates the minimal layout.

Usage
-----
    python3 scaffold_wave.py --plan touring-premium-refactor-2026 \\
                             --wave W12 \\
                             --title "Test Debt Repayment" \\
                             --sub-scripts discover apply
    python3 scaffold_wave.py --plan myplan --wave W01 \\
                             --title "Initial Forensic Sweep" \\
                             --sub-scripts discover --with-tests --critical
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from lib import (  # noqa: E402  pylint: disable=wrong-import-position
    EXIT_FAIL,
    EXIT_INTERRUPTED,
    EXIT_OK,
    EXIT_STRUCTURAL,
    is_kebab,
    is_sub_name,
    is_wave_id,
    utcnow_iso,
)

# Jinja2 — load lazily so we can degrade with a helpful message
try:
    from jinja2 import Environment, FileSystemLoader, select_autoescape
    _JINJA2_AVAILABLE = True
except ImportError:
    _JINJA2_AVAILABLE = False
    Environment = None  # type: ignore[assignment]
    FileSystemLoader = None  # type: ignore[assignment]
    select_autoescape = None  # type: ignore[assignment]

_ASSETS_DIR = _SCRIPT_DIR.parent / "assets" / "templates"


# ── Template engine ───────────────────────────────────────────────────────


def _jinja_env() -> Any:
    """Build a Jinja2 Environment pointed at our assets directory."""
    if not _JINJA2_AVAILABLE:
        msg = "Jinja2 is required (pip install jinja2) — falling back to plain copy."
        logging.getLogger(__name__).warning(msg)
        return None
    return Environment(
        loader=FileSystemLoader(str(_ASSETS_DIR)),
        autoescape=select_autoescape(disabled_extensions=("j2", "tmpl")),
        keep_trailing_newline=True,
        trim_blocks=True,
        lstrip_blocks=True,
    )


def render_template(env: Any, template_name: str, ctx: dict[str, Any]) -> str:
    """Render a Jinja2 template with the given context.

    Falls back to a raw read of the template file if Jinja2 is unavailable.
    The raw read is only useful for very simple templates; expect issues for
    templates that rely on loops/conditionals.
    """
    if env is None:
        raw = (_ASSETS_DIR / template_name).read_text(encoding="utf-8")
        for key, val in ctx.items():
            raw = raw.replace("{{ " + key + " }}", str(val))
        return raw
    template = env.get_template(template_name)
    return template.render(**ctx)


# ── Scaffolding ───────────────────────────────────────────────────────────


def scaffold_wave_dir(
    plan_dir: Path,
    wave: str,
    *,
    title: str,
    sub_scripts: list[str],
    critical: bool,
    with_tests: bool,
    apply_mutation: bool,
    objective: str = "",
) -> dict[str, Any]:
    """Create the wave directory structure with sub-scripts + validator + tests.

    Returns a report dict (the standard envelope).
    """
    env = _jinja_env()
    wave_dir = plan_dir / wave
    data_dir = plan_dir / "data"
    staging_dir = plan_dir / "staging"

    created: list[str] = []
    skipped: list[str] = []

    if apply_mutation:
        wave_dir.mkdir(parents=True, exist_ok=True)
        data_dir.mkdir(parents=True, exist_ok=True)
        staging_dir.mkdir(parents=True, exist_ok=True)
        if with_tests:
            (wave_dir / "tests").mkdir(parents=True, exist_ok=True)

    # 1. Render each sub-script
    for sub_name in sub_scripts:
        if not is_sub_name(sub_name):
            skipped.append(f"{sub_name}.py (invalid name)")
            continue
        target = wave_dir / f"{sub_name}.py"
        if target.exists() and apply_mutation:
            skipped.append(str(target))
            continue
        body = render_template(env, "forensic_script.py.j2", {
            "wave": wave,
            "sub_name": sub_name,
            "plan": plan_dir.name,
            "objective": objective or title,
            "docstring": f"Forensic sub-script for {wave} — {title}.",
            "emit_staging": True,
            "root_depth": 2,
            "scan_function": "workspace",
            "scan_glob": "*.py",
            "severities": "P0/P1/P2/P3",
        })
        if apply_mutation:
            target.write_text(body, encoding="utf-8")
        created.append(str(target.relative_to(plan_dir.parent) if plan_dir.parent.exists() else target))

    # 2. Render the wave validator
    validator_target = wave_dir / f"validate_{wave}.py"
    if not (validator_target.exists() and apply_mutation):
        body = render_template(env, "validate_wave.py.j2", {
            "wave": wave,
            "plan": plan_dir.name,
            "expected_subs": sub_scripts,
            "root_depth": 2,
        })
        if apply_mutation:
            validator_target.write_text(body, encoding="utf-8")
        created.append(str(validator_target))
    else:
        skipped.append(str(validator_target))

    # 3. Render test scaffolding (optional)
    if with_tests:
        conftest_target = wave_dir / "tests" / "conftest.py"
        if not (conftest_target.exists() and apply_mutation):
            body = render_template(env, "conftest.py.j2", {
                "wave": wave,
                "plan": plan_dir.name,
                "sub_name": sub_scripts[0] if sub_scripts else "main",
            })
            if apply_mutation:
                conftest_target.write_text(body, encoding="utf-8")
            created.append(str(conftest_target))
        else:
            skipped.append(str(conftest_target))

        for sub_name in sub_scripts:
            test_target = wave_dir / "tests" / f"test_{sub_name}.py"
            if test_target.exists() and apply_mutation:
                skipped.append(str(test_target))
                continue
            test_body = _render_test_stub(wave, sub_name)
            if apply_mutation:
                test_target.write_text(test_body, encoding="utf-8")
            created.append(str(test_target))

    return {
        "wave": wave,
        "title": title,
        "critical": critical,
        "plan_dir": str(plan_dir),
        "files_created": created,
        "files_skipped": skipped,
        "with_tests": with_tests,
    }


def _render_test_stub(wave: str, sub_name: str) -> str:
    """Inline test stub (kept simple — no separate template)."""
    return (
        f'"""Tests for {wave}.{sub_name}."""\n\n'
        "from __future__ import annotations\n\n"
        "import pytest\n\n"
        f"import {sub_name}  # noqa: F401  — import the sub-script under test\n\n\n"
        "class TestRun:\n"
        '    """Integration tests for run()."""\n\n'
        "    def test_dry_run_returns_ok(self, mock_workspace) -> None:\n"
        '        """A dry-run on the mock workspace must return status=OK."""\n'
        "        # TODO: build args, invoke run(), assert status\n"
        "        assert True\n\n"
        "    def test_apply_off_by_default(self, mock_workspace) -> None:\n"
        '        """--apply must be opt-in (L9)."""\n'
        "        # TODO: assert mutation phase did not execute\n"
        "        assert True\n"
    )


# ── CLI ───────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    parser = argparse.ArgumentParser(prog="scaffold_wave", description=__doc__)
    parser.add_argument("--plan", required=True,
                        help="Plan id (kebab-case). Used as the plan directory name.")
    parser.add_argument("--plan-dir", type=Path, default=None,
                        help="Override plan directory location (defaults to scripts/<plan>).")
    parser.add_argument("--wave", required=True,
                        help="Wave id (e.g. W12 or W12.3).")
    parser.add_argument("--title", required=True,
                        help="Human-readable wave title.")
    parser.add_argument("--sub-scripts", nargs="+", default=["discover"],
                        help="Snake_case sub-script names to scaffold (default: 'discover').")
    parser.add_argument("--objective", default="",
                        help="One-sentence objective (defaults to the title).")
    parser.add_argument("--critical", action="store_true",
                        help="Mark the wave as on the critical path (informational).")
    parser.add_argument("--with-tests", action="store_true",
                        help="Generate tests/ scaffolding alongside sub-scripts.")
    parser.add_argument("--apply", action="store_true",
                        help="Write files to disk. Default is dry-run.")
    parser.add_argument("--output-dir", type=Path, default=Path("data"),
                        help="Where the JSON report lands.")
    parser.add_argument("-j", "--json", dest="json_only", action="store_true")
    parser.add_argument("-v", "--verbose", action="store_true")
    return parser


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Validate args + scaffold."""
    if not is_kebab(args.plan):
        msg = f"--plan '{args.plan}' is not kebab-case"
        raise ValueError(msg)
    if not is_wave_id(args.wave):
        msg = f"--wave '{args.wave}' does not match W<N> pattern"
        raise ValueError(msg)
    for sub in args.sub_scripts:
        if not is_sub_name(sub):
            msg = f"--sub-script '{sub}' is not a valid snake_case identifier"
            raise ValueError(msg)

    plan_dir = args.plan_dir or Path.cwd() / "scripts" / args.plan
    plan_dir = plan_dir.resolve()

    scaffold = scaffold_wave_dir(
        plan_dir,
        args.wave,
        title=args.title,
        sub_scripts=args.sub_scripts,
        critical=args.critical,
        with_tests=args.with_tests,
        apply_mutation=args.apply,
        objective=args.objective,
    )

    return {
        "status": "OK",
        "script": "scaffold_wave",
        "timestamp": utcnow_iso(),
        "apply": args.apply,
        "wave": args.wave,
        "scaffold": scaffold,
    }


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
        return EXIT_OK
    except KeyboardInterrupt:
        return EXIT_INTERRUPTED
    except (ValueError, FileNotFoundError) as exc:
        logging.getLogger(__name__).error("%s", exc)
        return EXIT_STRUCTURAL
    except Exception:  # noqa: BLE001
        logging.getLogger(__name__).exception("scaffold_wave failed")
        return EXIT_FAIL


if __name__ == "__main__":
    raise SystemExit(main())
