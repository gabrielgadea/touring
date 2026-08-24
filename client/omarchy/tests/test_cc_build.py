"""tests/test_cc_build.py — Tests for cc_build.py."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "bin"))

_SCRIPT = Path(__file__).parent.parent / "bin" / "cc_build.py"
_CC_DIR = Path(__file__).parent.parent / "cc"


def _run_build(cfg_path: Path, out_path: Path, extra: list[str] | None = None) -> tuple[int, str]:
    """Run cc_build as subprocess, return (returncode, html_text)."""
    cmd = [sys.executable, str(_SCRIPT), "--config", str(cfg_path), "--out", str(out_path)]
    if extra:
        cmd.extend(extra)
    result = subprocess.run(cmd, capture_output=True, text=True)
    html = out_path.read_text() if out_path.exists() else ""
    return result.returncode, html


# ---------------------------------------------------------------------------
# test_three_widgets_present
# ---------------------------------------------------------------------------
def test_three_widgets_present(tmp_path: Path) -> None:
    cfg = tmp_path / "cc.json"
    out = tmp_path / "index.html"
    cfg.write_text(json.dumps({
        "artifacts_dir": str(tmp_path / "artifacts"),
        "runs_log": str(tmp_path / "runs.log"),
        "state_dir": str(tmp_path / "state"),
        "deck_json": str(tmp_path / "deck.json"),
        "routines_toml": str(tmp_path / "routines.toml"),
    }))
    rc, html = _run_build(cfg, out)
    assert rc == 0
    assert html.count("data-widget=") == 3


# ---------------------------------------------------------------------------
# test_renders_with_empty_sources — all widgets show "pendente"
# ---------------------------------------------------------------------------
def test_renders_with_empty_sources(tmp_path: Path) -> None:
    cfg = tmp_path / "cc.json"
    out = tmp_path / "index.html"
    cfg.write_text(json.dumps({
        "artifacts_dir": str(tmp_path / "no-artifacts"),
        "runs_log": str(tmp_path / "no-runs.log"),
        "state_dir": str(tmp_path / "no-state"),
        "deck_json": str(tmp_path / "no-deck.json"),
        "routines_toml": str(tmp_path / "no-routines.toml"),
    }))
    rc, html = _run_build(cfg, out)
    assert rc == 0
    assert html.count("data-widget=") == 3
    assert html.count("pendente") >= 3


# ---------------------------------------------------------------------------
# test_no_external_requests — no http(s) links to external hosts, no <script src
# ---------------------------------------------------------------------------
def test_no_external_requests(tmp_path: Path) -> None:
    cfg = tmp_path / "cc.json"
    out = tmp_path / "index.html"
    cfg.write_text(json.dumps({
        "artifacts_dir": str(tmp_path / "arts"),
        "runs_log": str(tmp_path / "runs.log"),
        "state_dir": str(tmp_path / "state"),
        "deck_json": str(tmp_path / "deck.json"),
        "routines_toml": str(tmp_path / "routines.toml"),
    }))
    rc, html = _run_build(cfg, out)
    assert rc == 0
    assert not re.search(r"https?://(?!127\.0\.0\.1)", html), (
        "Found external URL in generated HTML"
    )
    assert "<script src" not in html, "Found external <script src> in generated HTML"


# ---------------------------------------------------------------------------
# test_runs_log_parsed — k=v log line appears as table
# ---------------------------------------------------------------------------
def test_runs_log_parsed(tmp_path: Path) -> None:
    cfg = tmp_path / "cc.json"
    out = tmp_path / "index.html"
    runs_log = tmp_path / "runs.log"
    runs_log.write_text(
        "ts=2026-08-23T07:00:12 skill=validate-all exit=0 duration=12s\n"
        "ts=2026-08-23T08:00:05 skill=inbox-digest exit=0 duration=45s\n"
    )
    cfg.write_text(json.dumps({
        "artifacts_dir": str(tmp_path / "arts"),
        "runs_log": str(runs_log),
        "state_dir": str(tmp_path / "state"),
        "deck_json": str(tmp_path / "deck.json"),
        "routines_toml": str(tmp_path / "routines.toml"),
    }))
    rc, html = _run_build(cfg, out)
    assert rc == 0
    assert "<table>" in html
    assert "validate-all" in html
    assert "inbox-digest" in html


# ---------------------------------------------------------------------------
# test_brain_from_moc — h2 >= 3 from test moc.md
# ---------------------------------------------------------------------------
def test_brain_from_moc(tmp_path: Path) -> None:
    moc = tmp_path / "moc.md"
    brain_out = tmp_path / "brain.html"
    moc.write_text(
        "# Root\n\n"
        "## Section One\n\n"
        "Some text with [[a link]] here.\n\n"
        "## Section Two\n\n"
        "- item alpha\n"
        "- item beta\n\n"
        "## Section Three\n\n"
        "Paragraph with [[another wikilink]].\n"
    )
    cfg = tmp_path / "cc.json"
    cfg.write_text(json.dumps({
        "brain_moc": str(moc),
        "brain_out": str(brain_out),
    }))
    result = subprocess.run(
        [sys.executable, str(_SCRIPT), "--config", str(cfg), "--brain"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    html = brain_out.read_text()
    assert html.count("<h2") >= 3, f"Expected >= 3 h2 tags, got: {html.count('<h2')}"
    assert "<a href=" in html  # wikilinks converted


# ---------------------------------------------------------------------------
# test_units_pass_systemd_analyze — verify cc/ unit files
# ---------------------------------------------------------------------------
def test_units_pass_systemd_analyze() -> None:
    if not shutil.which("systemd-analyze"):
        pytest.skip("systemd-analyze not available")

    unit_files = [
        _CC_DIR / "cc.service",
        _CC_DIR / "cc.timer",
        _CC_DIR / "cc-serve.service",
    ]
    missing = [f for f in unit_files if not f.exists()]
    if missing:
        pytest.skip(f"Unit files not found: {missing}")

    result = subprocess.run(
        ["systemd-analyze", "--user", "verify"] + [str(f) for f in unit_files],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        f"systemd-analyze verify failed:\nSTDOUT: {result.stdout}\nSTDERR: {result.stderr}"
    )


def test_brain_output_lives_under_the_served_root() -> None:
    """`brain_out` must sit inside the directory cc-serve.service publishes.

    The unit serves `%h/Work/cc` and the plan's S-6.3 test is
    `curl 127.0.0.1:7777/brain/`. With the default pointing at `~/Work/brain/`
    that URL 404s — the page existed in a directory no server exposed. Widening
    the served root to `~/Work` would fix the URL and publish `pessoal/`,
    `runs.log` and `state/` on localhost, so the page moves instead of the root.
    Measured 2026-08-23.
    """
    import re
    omarchy = Path(__file__).parent.parent
    unit = (omarchy / "cc" / "cc-serve.service").read_text()
    m = re.search(r"--directory\s+(\S+)", unit)
    assert m, f"cc-serve.service has no --directory: {unit!r}"
    served = m.group(1).replace("%h", "~")          # -> ~/Work/cc

    src = (omarchy / "bin" / "cc_build.py").read_text()
    m = re.search(r'"brain_out":\s*"([^"]+)"', src)
    assert m, "cc_build.py has no brain_out default"
    assert m.group(1).startswith(served + "/"), \
        f"brain_out {m.group(1)!r} is outside the served root {served!r}"

    cfg = (omarchy / "cc" / "cc.json.example").read_text()
    m = re.search(r'"brain_out":\s*"([^"]+)"', cfg)
    assert m and m.group(1).startswith(served + "/"), \
        f"cc.json.example brain_out is outside the served root {served!r}"


def test_no_config_is_read_from_inside_the_served_root() -> None:
    """cc.service must not point --config at a file inside the published root.

    `cc-serve.service` serves `%h/Work/cc` with `python3 -m http.server`, which
    hands out every file under it: a cc.json placed there answers on
    127.0.0.1:7777/cc.json with the machine's path layout. Measured 2026-08-23.
    Not a secret, but free exposure, and the command centre renders in a browser
    on that same origin. The config belongs under XDG config, not the web root.
    """
    import re
    omarchy = Path(__file__).parent.parent
    served = re.search(r"--directory\s+(\S+)",
                       (omarchy / "cc" / "cc-serve.service").read_text()).group(1)
    unit = (omarchy / "cc" / "cc.service").read_text()
    m = re.search(r"--config\s+(\S+)", unit)
    assert m, f"cc.service has no --config: {unit!r}"
    assert not m.group(1).startswith(served + "/"), \
        f"cc.service reads its config from inside the served root: {m.group(1)}"
