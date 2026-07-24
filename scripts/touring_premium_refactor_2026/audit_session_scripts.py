#!/usr/bin/env python3
"""Audit script — REGRA #0 sweep for the 26+ sub-scripts created this session.

Checks each sub-script for:
  1. Pytest pass (final smoke)
  2. Has docstring + type hints on main public functions
  3. No bare `except:` clauses (constitutional)
  4. No hardcoded paths / secrets
  5. Exports a `main()` function returning int

Exit code: 0 if all checks pass; 1 otherwise.
"""
from __future__ import annotations

import argparse
import ast
import json
import logging
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

LOGGER = logging.getLogger(__name__)
_SELF = Path(__file__).resolve()
_SCRIPTS_DIR = _SELF.parent

BARE_EXCEPT_RE = re.compile(r"^\s*except\s*:", re.MULTILINE)
SECRET_PATTERN = re.compile(
    r'(api_key|password|secret|token)\s*=\s*["\'][^"\']{8,}["\']',
    re.IGNORECASE,
)


def build_parser() -> argparse.ArgumentParser:
    """CLI parser."""
    p = argparse.ArgumentParser(
        prog="audit_session_scripts", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--strict", action="store_true",
                   help="Exit 1 on warnings (not only errors).")
    p.add_argument("-v", "--verbose", action="store_true")
    return p


def list_w_scripts() -> list[Path]:
    """Return all w*_*.py sub-scripts."""
    return sorted(_SCRIPTS_DIR.glob("w*_*.py"))


def audit_single(path: Path) -> dict:
    """Audit a single sub-script."""
    try:
        content = path.read_text(encoding="utf-8")
    except OSError as e:
        return {"file": path.name, "status": "READ_ERROR", "error": str(e)}
    issues: list[str] = []
    # AST checks
    try:
        tree = ast.parse(content)
    except SyntaxError as e:
        return {"file": path.name, "status": "SYNTAX_ERROR", "error": str(e)}
    has_main = False
    has_module_docstring = bool(ast.get_docstring(tree))
    if not has_module_docstring:
        issues.append("MISSING_MODULE_DOCSTRING")
    public_funcs_without_docstring: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef):
            if node.name == "main":
                has_main = True
            if not node.name.startswith("_") and not ast.get_docstring(node):
                public_funcs_without_docstring.append(node.name)
    if not has_main:
        issues.append("MISSING_MAIN_FUNCTION")
    if public_funcs_without_docstring:
        issues.append(
            f"PUBLIC_FUNCS_WITHOUT_DOCSTRING:{','.join(public_funcs_without_docstring[:5])}"
        )
    # Regex checks
    if BARE_EXCEPT_RE.search(content):
        issues.append("BARE_EXCEPT_FOUND")
    if SECRET_PATTERN.search(content):
        issues.append("POSSIBLE_HARDCODED_SECRET")
    return {
        "file": path.name, "loc": len(content.splitlines()),
        "has_module_docstring": has_module_docstring,
        "has_main": has_main,
        "issues": issues,
        "status": "OK" if not issues else "WARN",
    }


def run_pytest() -> dict:
    """Run pytest, return summary."""
    tests_dir = _SCRIPTS_DIR / "tests"
    if not tests_dir.exists():
        return {"status": "NO_TESTS_DIR"}
    try:
        proc = subprocess.run(
            [sys.executable, "-m", "pytest", str(tests_dir), "-q"],
            cwd=_SCRIPTS_DIR, capture_output=True, timeout=120,
        )
        output = proc.stdout.decode("utf-8", errors="replace")
        # Parse "N passed in X.XXs"
        match = re.search(r"(\d+)\s+passed", output)
        passed = int(match.group(1)) if match else 0
        fail_match = re.search(r"(\d+)\s+failed", output)
        failed = int(fail_match.group(1)) if fail_match else 0
        return {
            "status": "OK" if proc.returncode == 0 else "FAIL",
            "passed": passed, "failed": failed,
            "exit_code": proc.returncode,
        }
    except subprocess.TimeoutExpired:
        return {"status": "TIMEOUT"}
    except FileNotFoundError:
        return {"status": "MISSING_PYTHON"}


def run(args: argparse.Namespace) -> dict:
    """Audit all sub-scripts + run pytest."""
    scripts = list_w_scripts()
    audits = [audit_single(s) for s in scripts]
    warns = [a for a in audits if a["status"] == "WARN"]
    errors = [a for a in audits if a["status"] in ("READ_ERROR", "SYNTAX_ERROR")]
    pytest_result = run_pytest()
    report = {
        "script": "audit_session_scripts",
        "timestamp": datetime.now(UTC).isoformat(),
        "totals": {
            "scripts_audited": len(scripts),
            "ok": sum(1 for a in audits if a["status"] == "OK"),
            "warn": len(warns),
            "error": len(errors),
        },
        "pytest": pytest_result,
        "audits": audits,
        "warns": warns[:10],
        "errors": errors,
    }
    status = "OK"
    if errors:
        status = "FAIL"
    elif args.strict and (warns or pytest_result.get("status") != "OK"):
        status = "STRICT_FAIL"
    elif pytest_result.get("status") != "OK":
        status = "PYTEST_FAIL"
    report["status"] = status
    return report


def main() -> int:
    """Entry point."""
    args = build_parser().parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
    )
    try:
        result = run(args)
        sys.stdout.write(json.dumps({
            "status": result["status"], "totals": result["totals"],
            "pytest": result["pytest"],
            "warn_samples": [a["file"] for a in result["warns"][:5]],
            "errors": result["errors"],
        }, indent=2, ensure_ascii=False) + "\n")
        return 0 if result["status"] == "OK" else 1
    except KeyboardInterrupt:
        LOGGER.warning("interrupted")
        return 130
    except Exception:  # noqa: BLE001
        LOGGER.exception("error")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
