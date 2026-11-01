#!/usr/bin/env python3
"""cc_build.py — Command Centre builder: cc.json → ~/Work/cc/index.html.

Usage:
    cc_build.py [--config PATH] [--out PATH] [--brain] [--json]

Widgets (data-widget="artifacts|routines|deck"):
    artifacts — artifact ring (last 12 ~/Work/artifacts/) + last 10 runs.log lines
    routines  — routine board JSON + last validate-*.json state
    deck      — skills deck tiles from deck.json

Design: transposition of the RUBRIC "Agentic OS" command centre seen in the
RoboNuggets video (2026-08-21) — near-black warm ground, one accent, section
labels in spaced mono caps, a central artifact ring orbiting a particle cloud,
big tabular clock with 3 timezones, routine board with NEXT/QUEUED badges,
skill tiles carrying model·effort. The accent/bg follow the live Omarchy theme
(the video's "Beetogreen" recolor proves the layout is theme-driven); fallback
is the RUBRIC black+orange itself.

All sources optional: missing → "pendente — <path>", exit 0.
No external requests: only 127.0.0.1 URLs and inline <script> (test-enforced).
"""
from __future__ import annotations

import html as _html
import json
import math
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).parent
_ROUTINES_GEN = _SCRIPT_DIR / "routines_gen.py"

# ---------------------------------------------------------------------------
# Default config
# ---------------------------------------------------------------------------
_DEFAULT_CONFIG: dict[str, str] = {
    "artifacts_dir": "~/Work/artifacts",
    "runs_log": "~/Work/runs.log",
    "state_dir": "~/Work/state",
    "deck_json": "~/.config/omarchy/plugins/gabriel.skills-deck/deck.json",
    "routines_toml": "~/Work/routines.toml",
    "theme_json": "~/.config/omarchy/current/theme/colors.json",
    "out": "~/Work/cc/index.html",
    "brain_moc": "~/Work/brain/moc.md",
    # DENTRO da raiz servida. cc-serve.service publica `%h/Work/cc` e o plano
    # cobra `curl 127.0.0.1:7777/brain/`; com o brain em `~/Work/brain/` essa URL
    # dava 404 — a pagina existia num diretorio que nenhum servidor expunha.
    # Servir `~/Work` inteiro resolveria a URL e vazaria `pessoal/`, `runs.log` e
    # `state/` no localhost; o certo e' o brain morar sob a raiz publicada.
    # Medido 2026-08-23 (S-6.3).
    "brain_out": "~/Work/cc/brain/index.html",
}
# Fallback = the RUBRIC palette itself (frames 00:43-02:21 of the video).
_FALLBACK_THEME = {
    "background": "#0d0b08",
    "foreground": "#ede4d3",
    "accent": "#ff5c1c",
}
_FALLBACK_LIGHT = {
    "background": "#f2ede3",
    "foreground": "#2b2216",
    "accent": "#c2410c",
}


def _load_config(config_path: str | None) -> dict[str, str]:
    cfg = dict(_DEFAULT_CONFIG)
    if config_path:
        p = Path(os.path.expanduser(config_path))
        if p.exists():
            with open(p) as f:
                loaded = json.load(f)
            cfg.update({k: str(v) for k, v in loaded.items()})
    return cfg


def _expand(cfg: dict[str, str], key: str) -> Path:
    return Path(os.path.expanduser(cfg.get(key, _DEFAULT_CONFIG.get(key, ""))))


def _load_theme(cfg: dict[str, str]) -> dict[str, str]:
    p = _expand(cfg, "theme_json")
    if p.exists():
        try:
            data = json.loads(p.read_text())
            return {
                "background": str(data.get("background", _FALLBACK_THEME["background"])),
                "foreground": str(data.get("foreground", _FALLBACK_THEME["foreground"])),
                "accent": str(data.get("accent", _FALLBACK_THEME["accent"])),
            }
        except (json.JSONDecodeError, KeyError):
            pass
    return dict(_FALLBACK_THEME)


# ---------------------------------------------------------------------------
# Data collectors
# ---------------------------------------------------------------------------
def _parse_log_line(line: str) -> dict[str, str] | None:
    """Parse a k=v log line into a dict. Returns None if no k=v pairs found."""
    pairs = re.findall(r"(\w+)=(\S+)", line)
    if not pairs:
        return None
    return dict(pairs)


def _human_size(nbytes: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if nbytes < 1024:
            return f"{nbytes:.0f} {unit}"
        nbytes //= 1024
    return f"{nbytes:.0f} TB"


def _human_age(mtime: float) -> str:
    secs = int(time.time() - mtime)
    if secs < 60:
        return f"{secs}s"
    if secs < 3600:
        return f"{secs // 60}m"
    if secs < 86400:
        return f"{secs // 3600}h"
    return f"{secs // 86400}d"


def _artifacts_data(cfg: dict[str, str]) -> list[dict[str, str]] | None:
    arts_dir = _expand(cfg, "artifacts_dir")
    if not arts_dir.exists():
        return None
    items = sorted(arts_dir.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True)
    return [
        {
            "name": p.name,
            "size": _human_size(p.stat().st_size),
            "age": _human_age(p.stat().st_mtime),
            "path": str(p),
        }
        for p in items[:12]
    ]


def _runs_data(cfg: dict[str, str]) -> tuple[list[str], list[dict[str, str]]] | None:
    runs_log = _expand(cfg, "runs_log")
    if not runs_log.exists():
        return None
    lines = runs_log.read_text(errors="replace").splitlines()[-10:]
    parsed = [d for d in (_parse_log_line(ln) for ln in lines if ln.strip()) if d]
    if not parsed:
        return ([], [])
    keys: list[str] = []
    for d in parsed:
        for k in d:
            if k not in keys:
                keys.append(k)
    return (keys, parsed)


def _routines_board_json(cfg: dict[str, str]) -> list[dict[str, Any]] | None:
    toml_path = _expand(cfg, "routines_toml")
    if not toml_path.exists():
        return None
    try:
        result = subprocess.run(
            [sys.executable, str(_ROUTINES_GEN), "board", "--json",
             "--toml", str(toml_path)],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0 and result.stdout.strip():
            return json.loads(result.stdout)  # type: ignore[no-any-return]
    except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError):
        pass
    return None


def _last_validate_json(cfg: dict[str, str]) -> dict[str, Any] | None:
    state_dir = _expand(cfg, "state_dir")
    if not state_dir.exists():
        return None
    candidates = sorted(state_dir.glob("validate-*.json"), key=lambda p: p.stat().st_mtime)
    if not candidates:
        return None
    try:
        return json.loads(candidates[-1].read_text())  # type: ignore[no-any-return]
    except (json.JSONDecodeError, OSError):
        return None


def _connectors_data(cfg: dict[str, str]) -> dict[str, Any] | None:
    p = _expand(cfg, "state_dir") / "connectors.json"
    if not p.exists():
        return None
    try:
        return json.loads(p.read_text())  # type: ignore[no-any-return]
    except (json.JSONDecodeError, OSError):
        return None


def _touring_status() -> dict[str, Any] | None:
    """Live nervous-system numbers; cheap fallback when touring is absent."""
    if not shutil.which("touring"):
        return None
    try:
        result = subprocess.run(
            ["touring", "status", "--brief", "-j"],
            capture_output=True, text=True, timeout=8,
        )
        if result.returncode == 0 and result.stdout.strip():
            return json.loads(result.stdout)  # type: ignore[no-any-return]
    except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError):
        pass
    return None


# ---------------------------------------------------------------------------
# Widget renderers
# ---------------------------------------------------------------------------
_RING_COLORS = ("#5ad7d0", "#e05fd0", "#ffd24a", "#6fe08a", "#7f9cff", "#ff8a5c")


def _widget_artifacts(cfg: dict[str, str]) -> tuple[str, bool]:
    """The signature: an artifact ring around the particle cloud + recent runs."""
    arts = _artifacts_data(cfg)
    runs = _runs_data(cfg)
    arts_dir = _expand(cfg, "artifacts_dir")
    runs_log = _expand(cfg, "runs_log")

    if arts is None and runs is None:
        content = (
            f"<p class='pending'>pendente — {_html.escape(str(arts_dir))}</p>"
            f"<p class='pending'>pendente — {_html.escape(str(runs_log))}</p>"
        )
        return content, False

    parts: list[str] = []

    # Ring nodes: left/top computed here (cos/sin, % of the square container).
    # CSS translate(%) is relative to the NODE's own box, not the parent — the
    # first version orbited a 52px circle and every node collapsed onto the
    # cloud (seen in the 23/08 screenshot). Geometry belongs to the builder.
    # Fidelity to the original (frames 00:45/01:48/01:58 re-inspected 23/08):
    # each ball carries a day-age badge + a numbered hover label; the click
    # modal shows category · date, a description line, the path in a field and
    # an OPEN FILE → action; the cloud center itself opens the Second Brain.
    nodes = ""
    n = len(arts or [])
    for i, a in enumerate(arts or []):
        color = _RING_COLORS[i % len(_RING_COLORS)]
        ext = Path(a["name"]).suffix.lstrip(".")[:3] or "art"
        age = a["age"].upper()
        label = a["name"]
        label = re.sub(r"^\d{8}T\d{6}Z-", "", label)
        label = label[:22] + "…" if len(label) > 23 else label
        ang = (i / n) * 2 * math.pi - math.pi / 2
        left = 50 + 44 * math.cos(ang)
        top = 50 + 44 * math.sin(ang)
        desc = f"{a['size']} on disk · left in orbit {a['age']} ago"
        nodes += (
            f"<button class='ring-node' style='left:{left:.2f}%;top:{top:.2f}%;"
            f"--dot:{color}'"
            f" data-path='{_html.escape(a['path'])}'"
            f" data-name='{_html.escape(a['name'])}'"
            f" data-desc='{_html.escape(desc)}'"
            f" data-cat='{_html.escape(ext.upper())} · {_html.escape(age)} AGO'"
            f" data-href='artifacts/{_html.escape(a['name'])}'>"
            f"<span class='ring-num'>{i + 1}</span>"
            f"<span class='ring-ext'>{_html.escape(ext)}</span>"
            f"<span class='ring-age'>{_html.escape(age)}</span>"
            f"<span class='ring-label'>{_html.escape(label)}</span>"
            f"</button>"
        )
    orbit = (
        "<div class='orbit'>"
        "<canvas id='cloud' width='560' height='560' aria-hidden='true'></canvas>"
        f"<div class='ring'>{nodes}</div>"
        "<a class='core-link' href='/brain/' title='Second Brain'></a>"
        "<div class='orbit-hint'>" + (
            "click the core to open the second brain" if n
            else "no artifacts yet — every run leaves one"
        ) + "</div>"
        "</div>"
        "<div class='art-detail' id='art-detail' hidden>"
        "<span class='art-cat' id='art-cat'></span>"
        "<strong id='art-name'></strong>"
        "<p class='art-desc' id='art-desc'></p>"
        "<code id='art-path'></code>"
        "<div class='art-actions'>"
        "<a class='btn-solid' id='art-open' href='#' target='_blank'>OPEN FILE &#8594;</a>"
        "<button id='art-close' class='pill'>CLOSE</button>"
        "</div></div>"
    )
    parts.append(orbit)

    if runs is not None:
        keys, rows = runs
        if rows:
            header = "".join(f"<th>{_html.escape(k)}</th>" for k in keys)

            def _cell(d: dict[str, str], k: str) -> str:
                v = d.get(k, "")
                # Long absolute paths blow the column out of the panel (seen in
                # the 23/08 screenshot); the basename is the information.
                if k == "artifact" and "/" in v:
                    v = Path(v).name
                return _html.escape(v)

            body = "".join(
                "<tr>" + "".join(f"<td>{_cell(d, k)}</td>" for k in keys) + "</tr>"
                for d in rows
            )
            parts.append(
                "<h3>Recent runs</h3>"
                "<div class='scrollwrap'>"
                f"<table><thead><tr>{header}</tr></thead><tbody>{body}</tbody></table>"
                "</div>"
            )
    elif arts is not None:
        parts.append(f"<p class='pending'>pendente — {_html.escape(str(runs_log))}</p>")

    return "\n".join(parts), True


_STATUS_BADGE = {
    "next": "b-next", "queued": "b-queued", "fired": "b-fired",
    "success": "b-fired", "failure": "b-fail", "never": "b-queued",
}


def _widget_routines(cfg: dict[str, str]) -> tuple[str, bool]:
    board = _routines_board_json(cfg)
    validate = _last_validate_json(cfg)
    toml_path = _expand(cfg, "routines_toml")

    if board is None:
        content = f"<p class='pending'>pendente — {_html.escape(str(toml_path))}</p>"
        return content, False

    parts: list[str] = []
    if board:
        rows = []
        for e in board:
            status = str(e.get("last_status") or "never")
            badge = _STATUS_BADGE.get(status.lower(), "b-queued")
            runner = str(e.get("runner", "local"))
            nxt = str(e.get("next") or "—")
            rows.append(
                "<tr>"
                f"<td class='t-time'>{_html.escape(str(e.get('at', '')))}</td>"
                f"<td class='t-name'>{_html.escape(str(e.get('id', '')))}"
                f" <span class='runner'>{_html.escape(runner.upper())}</span></td>"
                f"<td class='t-next'>{_html.escape(nxt)}</td>"
                f"<td class='t-status'><span class='badge {badge}'>"
                f"{_html.escape(status.upper())}</span></td>"
                "</tr>"
            )
        parts.append(
            "<table class='board'>"
            "<thead><tr><th>time</th><th>routine</th><th>next</th><th>status</th></tr></thead>"
            f"<tbody>{''.join(rows)}</tbody></table>"
        )

    if validate:
        # Two shapes on disk: the per-phase gate writes {phase, ok, checks};
        # the daily validate-all routine writes {ts, phases:[{phase,ok,checks}]}
        # — reading only the flat one showed "0 ok / 0 fail" (seen 23/08).
        if "phases" in validate:
            phases = validate.get("phases", [])
            checks = [c for ph in phases for c in ph.get("checks", [])]
            overall_ok = bool(phases) and all(ph.get("ok") for ph in phases)
            phase = f"{sum(1 for ph in phases if ph.get('ok'))}/{len(phases)} phases"
        else:
            checks = validate.get("checks", [])
            overall_ok = bool(validate.get("ok"))
            phase = str(validate.get("phase", "?"))
        ok_count = sum(1 for c in checks if c.get("status") == "ok")
        fail_count = sum(1 for c in checks if c.get("status") == "fail")
        badge = "b-fired" if overall_ok else "b-fail"
        parts.append(
            "<div class='board-foot'>"
            f"<span class='badge {badge}'>{_html.escape(phase)}</span> "
            f"last self-proof · {ok_count} ok / {fail_count} fail"
            "</div>"
        )

    return "\n".join(parts), True


def _widget_deck(cfg: dict[str, str]) -> tuple[str, bool]:
    deck_path = _expand(cfg, "deck_json")
    if not deck_path.exists():
        return f"<p class='pending'>pendente — {_html.escape(str(deck_path))}</p>", False

    try:
        skills: list[dict[str, Any]] = json.loads(deck_path.read_text())
    except (json.JSONDecodeError, OSError):
        return f"<p class='pending'>pendente — {_html.escape(str(deck_path))}</p>", False

    tiles = []
    for s in skills:
        label = _html.escape(str(s.get("label", s.get("skill", "?"))))
        skill = _html.escape(str(s.get("skill", "")))
        model = _html.escape(str(s.get("model", "sonnet")))
        effort = _html.escape(str(s.get("effort", "medium")))
        mode = _html.escape(str(s.get("mode", "acceptEdits")))
        cwd = _html.escape(str(s.get("cwd", "~/Work")))
        herdr = bool(s.get("herdr"))
        cmd = f"omarchy-skill-run {skill} {model} {effort} {mode} {cwd}"
        if herdr:
            cmd += " --herdr"
        herdr_tag = "<span class='herdr-tag'>HERDR</span>" if herdr else ""
        tiles.append(
            "<div class='tile'>"
            f"<div class='tile-top'><span class='tile-skill'>/{skill}</span>{herdr_tag}</div>"
            f"<div class='tile-label'>{label}</div>"
            f"<div class='tile-meta'>{model} · {effort}</div>"
            f"<button class='tile-run' data-cmd='{_html.escape(cmd)}'"
            f" title='copy command'>&#9654;</button>"
            "</div>"
        )

    if not tiles:
        return "<p>No skills in deck.</p>", True

    return "<div class='deck'>" + "".join(tiles) + "</div>", True


def _widget_connectors(cfg: dict[str, str]) -> str:
    data = _connectors_data(cfg)
    p = _expand(cfg, "state_dir") / "connectors.json"
    if data is None:
        return f"<p class='pending'>pendente — {_html.escape(str(p))}</p>"
    counts = data.get("counts", {})
    ok = int(counts.get("ok", 0))
    rows = data.get("connectors", [])
    flagged = [c for c in rows if c.get("status") in ("needs-auth", "failed")][:4]
    items = "".join(
        "<li><span class='dot d-warn'></span>"
        f"{_html.escape(str(c.get('name', '')))}"
        f" <span class='mini'>{_html.escape(str(c.get('status', '')))}</span></li>"
        for c in flagged
    )
    mix = "".join(
        f"<i class='seg s-{k}' style='flex:{max(1, int(counts.get(k, 0)))}'></i>"
        for k in ("ok", "needs-auth", "failed", "not-configured")
        if int(counts.get(k, 0)) > 0
    )
    return (
        f"<div class='bignum'>{ok}<span class='bignum-sub'>connectors live</span></div>"
        f"<ul class='flag-list'>{items}</ul>"
        f"<div class='mixbar'>{mix}</div>"
    )


def _widget_touring() -> str:
    st = _touring_status()
    if st is None:
        return "<p class='pending'>pendente — touring status</p>"
    idx = st.get("index", {}) if isinstance(st.get("index"), dict) else {}
    wiring = st.get("wiring", {}) if isinstance(st.get("wiring"), dict) else {}
    symbols = idx.get("symbol_count", st.get("symbol_count", "—"))
    orphans = wiring.get("orphan_count", st.get("orphan_count", "—"))
    health = st.get("composite_health_score", "—")
    if isinstance(health, float):
        health = f"{health:.2f}"
    return (
        f"<div class='bignum'>{_html.escape(str(symbols))}"
        "<span class='bignum-sub'>symbols indexed</span></div>"
        "<ul class='stat-list>'>"
        f"<li><span class='mini'>health</span> {_html.escape(str(health))}</li>"
        f"<li><span class='mini'>orphans</span> {_html.escape(str(orphans))}</li>"
        "</ul>"
    )


def _micro_apps() -> str:
    apps = (
        ("Second Brain", "your whole workspace as a living map", "/brain/"),
        ("Windows", "dockur/windows over noVNC", "http://127.0.0.1:8006"),
        ("Routine board", "what runs while you sleep", "#w-routines"),
        ("Skills deck", "one button per SOP", "#w-deck"),
    )
    items = "".join(
        f"<li><a href='{_html.escape(url)}'><strong>{_html.escape(name)}</strong>"
        f"<span class='mini'>{_html.escape(desc)}</span></a></li>"
        for name, desc, url in apps
    )
    return f"<ul class='apps'>{items}</ul>"


# ---------------------------------------------------------------------------
# HTML assembly — RUBRIC-style shell
# ---------------------------------------------------------------------------
# NOTE: plain strings (no .format) so CSS braces stay readable; theme vars are
# injected by _css_vars() below.
_CSS = """
*{box-sizing:border-box;margin:0;padding:0}
html{background:var(--bg)}
body{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;background:
 radial-gradient(1200px 700px at 50% 38%,var(--glow) 0%,transparent 60%),var(--bg);
 color:var(--fg);min-height:100vh;padding:18px 22px 40px}
a{color:inherit;text-decoration:none}
.mono,.t-time,.clock-big,.tz,code,.tile-meta,.mini,.badge,.runner,.ring-age,
 table,.sec-label{font-family:ui-monospace,'Cascadia Mono','JetBrains Mono',monospace}
.grid{display:grid;grid-template-columns:330px minmax(430px,1fr) 350px;gap:18px}
@media(max-width:1180px){.grid{grid-template-columns:1fr 1fr}.col-c{order:-1;grid-column:1/-1}}
@media(max-width:760px){.grid{grid-template-columns:1fr}}
.col{display:flex;flex-direction:column;gap:18px;min-width:0}

/* section shells */
.sec{background:var(--panel);border:1px solid var(--line);border-radius:10px;
 padding:14px 16px 16px;position:relative}
.sec-head{display:flex;align-items:center;justify-content:space-between;
 margin-bottom:12px;border-bottom:1px solid var(--line);padding-bottom:9px}
.sec-label{font-size:11px;letter-spacing:.22em;color:var(--fg);font-weight:600}
.sec-label .glyph{color:var(--accent);margin-right:7px}
.pill{font-family:ui-monospace,monospace;font-size:9px;letter-spacing:.14em;
 color:var(--accent);border:1px solid var(--accent-40);border-radius:999px;
 padding:3px 10px;background:none;cursor:pointer}
.pill:hover{background:var(--accent-soft)}

/* header */
.masthead{text-align:center;padding:6px 0 2px}
.masthead h1{font-size:22px;letter-spacing:.06em;font-weight:800}
.masthead h1 .glyph{color:var(--accent);margin-right:8px}
.masthead h1 em{color:var(--accent);font-style:normal;font-weight:600}
.masthead .sub{font-family:ui-monospace,monospace;font-size:10px;letter-spacing:.3em;
 color:var(--dim);margin-top:5px;text-transform:uppercase}

/* orbit signature — hex-grid ground, geodesic shell, numbered balls */
#w-artifacts{background:
 radial-gradient(circle at 24% 20%,transparent 0,transparent 100%),
 repeating-linear-gradient(60deg,var(--hex) 0 1px,transparent 1px 26px),
 repeating-linear-gradient(-60deg,var(--hex) 0 1px,transparent 1px 26px),
 repeating-linear-gradient(0deg,var(--hex) 0 1px,transparent 1px 26px),
 var(--panel)}
.orbit{position:relative;width:min(100%,560px);aspect-ratio:1;margin:6px auto 2px}
.orbit canvas{position:absolute;inset:0;width:100%;height:100%}
.ring{position:absolute;inset:0}
.ring-node{position:absolute;width:52px;height:52px;
 margin:-26px 0 0 -26px;border-radius:50%;border:1px solid var(--line-strong);
 background:color-mix(in srgb,var(--panel) 82%,transparent);color:var(--fg);cursor:pointer;
 display:flex;flex-direction:column;align-items:center;justify-content:center;gap:1px;
 transition:border-color .15s,box-shadow .15s}
.ring-node:hover{border-color:var(--accent);box-shadow:0 0 14px var(--accent-soft)}
.ring-node::before{content:'';width:5px;height:5px;border-radius:50%;background:var(--dot)}
.ring-num{position:absolute;top:-5px;left:-5px;min-width:14px;height:14px;border-radius:7px;
 background:var(--inset);border:1px solid var(--line-strong);color:var(--dim);
 font-family:ui-monospace,monospace;font-size:8px;display:flex;align-items:center;
 justify-content:center;padding:0 3px}
.ring-ext{font-size:9px;font-weight:700;letter-spacing:.08em;text-transform:uppercase}
.ring-age{font-size:8px;color:var(--dim)}
.ring-label{position:absolute;top:100%;left:50%;transform:translateX(-50%);margin-top:4px;
 font-family:ui-monospace,monospace;font-size:8px;letter-spacing:.04em;color:var(--dim);
 white-space:nowrap;background:color-mix(in srgb,var(--bg) 72%,transparent);
 border-radius:3px;padding:1px 5px;opacity:0;transition:opacity .15s;pointer-events:none}
.ring-node:hover .ring-label,.ring-node:focus-visible .ring-label{opacity:1;color:var(--fg)}
.core-link{position:absolute;left:50%;top:50%;width:34%;height:34%;
 transform:translate(-50%,-50%);border-radius:50%}
.core-link:hover{box-shadow:0 0 60px var(--accent-soft)}
.orbit-hint{position:absolute;left:0;right:0;bottom:-4px;text-align:center;
 font-family:ui-monospace,monospace;font-size:9px;letter-spacing:.2em;color:var(--dim);
 text-transform:uppercase}
.art-detail{margin:14px auto 0;max-width:480px;background:var(--panel);
 border:1px solid var(--accent);border-radius:10px;padding:14px 16px;
 box-shadow:0 0 0 1px var(--accent-soft),0 8px 40px rgba(0,0,0,.55)}
.art-cat{display:block;font-family:ui-monospace,monospace;font-size:9px;
 letter-spacing:.2em;color:var(--accent);margin-bottom:5px}
.art-detail strong{display:block;font-size:14px;margin-bottom:4px}
.art-desc{font-size:11px;color:var(--dim);margin-bottom:8px;line-height:1.5}
.art-detail code{display:block;font-size:10px;color:var(--dim);word-break:break-all;
 background:var(--inset);border-radius:6px;padding:8px 10px;margin-bottom:11px}
.art-actions{display:flex;gap:9px;align-items:center}
.btn-solid{font-family:ui-monospace,monospace;font-size:9.5px;letter-spacing:.12em;
 background:var(--accent);color:var(--bg);border-radius:6px;padding:7px 13px;font-weight:700}
.btn-solid:hover{filter:brightness(1.1)}

/* clock */
.datebar{font-size:10px;letter-spacing:.14em;color:var(--dim);font-family:ui-monospace,monospace}
.clock-big{font-size:34px;font-weight:700;color:var(--accent);
 font-variant-numeric:tabular-nums;letter-spacing:.01em;margin:2px 0 8px}
.tzrow{display:flex;gap:14px;border-top:1px solid var(--line);padding-top:8px}
.tz{font-size:10px;color:var(--dim)}
.tz b{display:block;color:var(--fg);font-size:12px;font-variant-numeric:tabular-nums}

/* big numbers */
.bignum{font-size:34px;font-weight:800;color:var(--accent);line-height:1;
 font-variant-numeric:tabular-nums}
.bignum-sub{display:block;font-family:ui-monospace,monospace;font-size:9px;
 letter-spacing:.2em;color:var(--dim);text-transform:uppercase;margin-top:5px}
.flag-list,.apps,.stat-list{list-style:none;margin-top:10px}
.flag-list li,.stat-list li{font-size:11px;padding:4px 0;border-bottom:1px solid var(--line);
 display:flex;align-items:center;gap:7px}
.mini{font-family:ui-monospace,monospace;font-size:9px;letter-spacing:.1em;color:var(--dim)}
.dot{width:6px;height:6px;border-radius:50%;display:inline-block}
.d-warn{background:var(--accent)}
.mixbar{display:flex;height:5px;border-radius:3px;overflow:hidden;margin-top:12px;gap:2px}
.mixbar i{display:block}
.s-ok{background:var(--accent)}.s-needs-auth{background:var(--dim)}
.s-failed{background:#b3452c}.s-not-configured{background:var(--line-strong)}

/* apps */
.apps li a{display:flex;flex-direction:column;padding:8px 2px;border-bottom:1px solid var(--line)}
.apps li a:hover strong{color:var(--accent)}
.apps strong{font-size:12px}
.apps .mini{margin-top:2px}

/* tables & board */
.scrollwrap{overflow-x:auto}
table{width:100%;border-collapse:collapse;font-size:10.5px}
td{word-break:break-word}
th,td{padding:5px 7px;text-align:left;border-bottom:1px solid var(--line)}
th{font-size:9px;letter-spacing:.18em;text-transform:uppercase;color:var(--dim);font-weight:600}
.board .t-time{color:var(--dim);white-space:nowrap}
.board .t-name{font-family:system-ui,sans-serif;font-size:11.5px}
.runner{font-size:8px;letter-spacing:.12em;color:var(--dim);border:1px solid var(--line-strong);
 border-radius:3px;padding:1px 4px;margin-left:5px}
.badge{font-size:8.5px;letter-spacing:.14em;border-radius:3px;padding:2px 7px;font-weight:700}
.b-next{background:var(--accent);color:var(--bg)}
.b-queued{background:none;border:1px solid var(--line-strong);color:var(--dim)}
.b-fired{background:none;border:1px solid #3f7a4f;color:#7fce93}
.b-fail{background:#b3452c;color:#fff}
.board-foot{margin-top:10px;font-size:10.5px;color:var(--dim);display:flex;
 align-items:center;gap:8px;font-family:ui-monospace,monospace}

/* deck tiles */
.deck{display:grid;grid-template-columns:repeat(2,1fr);gap:10px}
.tile{background:var(--inset);border:1px solid var(--line);border-radius:8px;
 padding:11px 12px;position:relative;min-height:86px}
.tile:hover{border-color:var(--accent-40)}
.tile-top{display:flex;justify-content:space-between;align-items:center}
.tile-skill{font-family:ui-monospace,monospace;font-size:11px;color:var(--accent);font-weight:700}
.herdr-tag{font-size:7.5px;letter-spacing:.14em;color:var(--dim);
 border:1px solid var(--line-strong);border-radius:3px;padding:1px 4px}
.tile-label{font-size:11.5px;margin-top:5px}
.tile-meta{font-size:9px;letter-spacing:.16em;color:var(--dim);text-transform:uppercase;margin-top:3px}
.tile-run{position:absolute;right:10px;bottom:9px;width:22px;height:22px;border-radius:50%;
 border:1px solid var(--accent-40);background:none;color:var(--accent);font-size:9px;
 cursor:pointer;display:flex;align-items:center;justify-content:center}
.tile-run:hover{background:var(--accent-soft)}
.tile-run.copied{background:var(--accent);color:var(--bg)}

.pending{color:var(--dim);font-style:italic;font-size:11.5px}
.foot{margin-top:22px;text-align:center;font-family:ui-monospace,monospace;
 font-size:9px;letter-spacing:.24em;color:var(--dim);text-transform:uppercase}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
@media(prefers-reduced-motion:reduce){.ring-node{transition:none}}
"""

_JS = """
(function(){
  'use strict';
  // clocks — big local + 3 zones
  var pads=function(n){return String(n).padStart(2,'0')};
  function fmt(tz){
    try{return new Intl.DateTimeFormat('en-GB',{timeZone:tz,hour:'2-digit',
      minute:'2-digit',hour12:false}).format(new Date())}catch(e){return '--:--'}
  }
  function tick(){
    var d=new Date();
    var big=document.getElementById('clock-big');
    if(big)big.textContent=pads(d.getHours())+':'+pads(d.getMinutes())+':'+pads(d.getSeconds());
    var zs=document.querySelectorAll('[data-tz]');
    for(var i=0;i<zs.length;i++)zs[i].textContent=fmt(zs[i].getAttribute('data-tz'));
    var db=document.getElementById('datebar');
    if(db){
      var wk=Math.ceil((((d-new Date(d.getFullYear(),0,1))/864e5)+new Date(d.getFullYear(),0,1).getDay()+1)/7);
      db.textContent='W'+pads(wk)+' | '+d.toDateString().replace(/^(\\w+) (\\w+) (\\d+) (\\d+)$/, '$2 $3 $4 ($1)');
    }
  }
  tick();setInterval(tick,1000);

  // artifact ring → detail card (category · date, description, path, OPEN FILE)
  var det=document.getElementById('art-detail');
  if(det){
    var nodes=document.querySelectorAll('.ring-node');
    for(var i=0;i<nodes.length;i++){
      nodes[i].addEventListener('click',function(){
        document.getElementById('art-cat').textContent=this.getAttribute('data-cat');
        document.getElementById('art-name').textContent=this.getAttribute('data-name');
        document.getElementById('art-desc').textContent=this.getAttribute('data-desc');
        document.getElementById('art-path').textContent=this.getAttribute('data-path');
        document.getElementById('art-open').setAttribute('href',this.getAttribute('data-href'));
        det.hidden=false;
      });
    }
    var cl=document.getElementById('art-close');
    if(cl)cl.addEventListener('click',function(){det.hidden=true});
  }

  // deck ▶ copies the exact command
  var runs=document.querySelectorAll('.tile-run');
  for(var j=0;j<runs.length;j++){
    runs[j].addEventListener('click',function(){
      var b=this,cmd=b.getAttribute('data-cmd');
      var done=function(){b.classList.add('copied');setTimeout(function(){b.classList.remove('copied')},900)};
      if(navigator.clipboard&&navigator.clipboard.writeText)
        navigator.clipboard.writeText(cmd).then(done,done);
      else done();
    });
  }

  // the core: geodesic shell + starfield + particle cloud (as in the original)
  var cv=document.getElementById('cloud');
  if(cv&&cv.getContext){
    var ctx=cv.getContext('2d'),W=cv.width,H=cv.height,cx=W/2,cy=H/2;
    var cols=['#5ad7d0','#e05fd0','#ffd24a','#6fe08a','#7f9cff','#ff8a5c','#d8d0c0'];
    var P=[],N=170,rmax=104;
    for(var k=0;k<N;k++){
      var a=Math.random()*Math.PI*2,r=Math.pow(Math.random(),0.6)*rmax;
      P.push({x:cx+Math.cos(a)*r,y:cy+Math.sin(a)*r,
        vx:(Math.random()-0.5)*0.18,vy:(Math.random()-0.5)*0.18,
        c:cols[k%cols.length],s:Math.random()*1.6+0.6});
    }
    // fixed geodesic vertices on the shell (two rings, offset) + stars
    var G=[],ring=[0.86,0.62];
    for(var g=0;g<ring.length;g++)for(var v=0;v<12;v++){
      var ga=(v/12)*Math.PI*2+g*0.26;
      G.push({x:cx+Math.cos(ga)*ring[g]*W/2*0.92,y:cy+Math.sin(ga)*ring[g]*H/2*0.92});
    }
    var S=[];
    for(var s2=0;s2<90;s2++)S.push({x:Math.random()*W,y:Math.random()*H,
      s:Math.random()*1.1+0.3,o:Math.random()*0.5+0.1});
    var still=window.matchMedia&&window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    function shell(){
      ctx.globalAlpha=1;
      for(var t2=0;t2<S.length;t2++){var st=S[t2];
        ctx.globalAlpha=st.o;ctx.fillStyle='#cfc6b4';
        ctx.beginPath();ctx.arc(st.x,st.y,st.s,0,6.29);ctx.fill()}
      ctx.globalAlpha=0.09;ctx.strokeStyle='#b7ac96';ctx.lineWidth=0.5;
      for(var e1=0;e1<G.length;e1++)for(var e2=e1+1;e2<G.length;e2++){
        var gdx=G[e1].x-G[e2].x,gdy=G[e1].y-G[e2].y;
        if(gdx*gdx+gdy*gdy<26000){ctx.beginPath();ctx.moveTo(G[e1].x,G[e1].y);
          ctx.lineTo(G[e2].x,G[e2].y);ctx.stroke()}
      }
    }
    function frame(){
      ctx.clearRect(0,0,W,H);
      shell();
      ctx.globalAlpha=0.12;ctx.strokeStyle='#8a8272';ctx.lineWidth=0.4;
      for(var i=0;i<N;i+=3)for(var j2=i+3;j2<Math.min(i+18,N);j2+=3){
        var dx=P[i].x-P[j2].x,dy=P[i].y-P[j2].y;
        if(dx*dx+dy*dy<2200){ctx.beginPath();ctx.moveTo(P[i].x,P[i].y);
          ctx.lineTo(P[j2].x,P[j2].y);ctx.stroke()}
      }
      ctx.globalAlpha=0.92;
      for(var m=0;m<N;m++){
        var p=P[m];
        if(!still){
          p.x+=p.vx;p.y+=p.vy;
          var ddx=p.x-cx,ddy=p.y-cy;
          if(ddx*ddx+ddy*ddy>rmax*rmax){p.vx-=ddx*0.0004;p.vy-=ddy*0.0004}
        }
        ctx.fillStyle=p.c;ctx.beginPath();ctx.arc(p.x,p.y,p.s,0,6.29);ctx.fill();
      }
      if(!still)requestAnimationFrame(frame);
    }
    frame();
  }
})();
"""


def _css_vars(theme: dict[str, str]) -> str:
    bg, fg, accent = theme["background"], theme["foreground"], theme["accent"]
    lt = _FALLBACK_LIGHT
    return (
        ":root{"
        f"--bg:{bg};--fg:{fg};--accent:{accent};"
        f"--panel:color-mix(in srgb,{bg} 86%,{fg} 6%);"
        f"--inset:color-mix(in srgb,{bg} 78%,{fg} 6%);"
        f"--line:color-mix(in srgb,{fg} 12%,transparent);"
        f"--line-strong:color-mix(in srgb,{fg} 26%,transparent);"
        f"--dim:color-mix(in srgb,{fg} 55%,{bg} 45%);"
        f"--glow:color-mix(in srgb,{accent} 7%,transparent);"
        f"--accent-40:color-mix(in srgb,{accent} 40%,transparent);"
        f"--accent-soft:color-mix(in srgb,{accent} 14%,transparent);"
        f"--hex:color-mix(in srgb,{fg} 3%,transparent);"
        "}"
        "@media(prefers-color-scheme:light){:root{"
        f"--bg:{lt['background']};--fg:{lt['foreground']};--accent:{lt['accent']};"
        f"--panel:color-mix(in srgb,{lt['background']} 92%,{lt['foreground']} 4%);"
        f"--inset:color-mix(in srgb,{lt['background']} 85%,{lt['foreground']} 6%);"
        f"--line:color-mix(in srgb,{lt['foreground']} 14%,transparent);"
        f"--line-strong:color-mix(in srgb,{lt['foreground']} 30%,transparent);"
        f"--dim:color-mix(in srgb,{lt['foreground']} 55%,{lt['background']} 45%);"
        f"--glow:color-mix(in srgb,{lt['accent']} 7%,transparent);"
        f"--accent-40:color-mix(in srgb,{lt['accent']} 40%,transparent);"
        f"--accent-soft:color-mix(in srgb,{lt['accent']} 14%,transparent);"
        f"--hex:color-mix(in srgb,{lt['foreground']} 4%,transparent);"
        "}}"
    )


def _sec(label: str, glyph: str, content: str, *, widget: str | None = None,
         anchor: str | None = None, pill: str | None = None) -> str:
    attrs = f" data-widget=\"{widget}\"" if widget else ""
    aid = f" id=\"{anchor}\"" if anchor else ""
    pill_html = f"<span class='pill'>{_html.escape(pill)}</span>" if pill else ""
    return (
        f"<section class='sec'{attrs}{aid}>"
        f"<div class='sec-head'><span class='sec-label'>"
        f"<span class='glyph'>{glyph}</span>{_html.escape(label)}</span>{pill_html}</div>"
        f"{content}</section>"
    )


def build_html(cfg: dict[str, str]) -> tuple[str, dict[str, Any]]:
    """Generate self-contained HTML. Returns (html, metadata)."""
    theme = _load_theme(cfg)

    arts_html, arts_ok = _widget_artifacts(cfg)
    rout_html, rout_ok = _widget_routines(cfg)
    deck_html, deck_ok = _widget_deck(cfg)
    conn_html = _widget_connectors(cfg)
    tour_html = _widget_touring()

    meta = {
        "artifacts": "ok" if arts_ok else "pending",
        "routines": "ok" if rout_ok else "pending",
        "deck": "ok" if deck_ok else "pending",
    }

    clock = (
        "<div class='datebar' id='datebar'></div>"
        "<div class='clock-big' id='clock-big'>--:--:--</div>"
        "<div class='tzrow'>"
        "<span class='tz'>UTC<b data-tz='UTC'>--:--</b></span>"
        "<span class='tz'>LISBOA<b data-tz='Europe/Lisbon'>--:--</b></span>"
        "<span class='tz'>SF<b data-tz='America/Los_Angeles'>--:--</b></span>"
        "</div>"
    )

    now = time.strftime("%Y-%m-%dT%H:%M:%S")
    body = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>TACO Agentic OS</title>
<style>
{_css_vars(theme)}
{_CSS}
</style>
</head>
<body>
<header class="masthead">
<h1><span class="glyph">&#9671;</span>TACO <em>Agentic OS</em></h1>
<div class="sub">Gabriel Gadea &nbsp;|&nbsp; Touring nervous system</div>
</header>
<div class="grid">
<div class="col col-l">
{_sec("MICRO APPS", "&#9638;", _micro_apps())}
{_sec("CALENDAR", "&#9711;", clock, pill="America/Sao_Paulo")}
{_sec("TOURING", "&#9881;", tour_html)}
</div>
<div class="col col-c">
{_sec("ARTIFACTS", "&#10022;", arts_html, widget="artifacts", anchor="w-artifacts")}
</div>
<div class="col col-r">
{_sec("CONNECTORS", "&#9993;", conn_html)}
{_sec("SKILLS DECK", "&#9889;", deck_html, widget="deck", anchor="w-deck", pill="deck.json")}
{_sec("ROUTINES", "&#8801;", rout_html, widget="routines", anchor="w-routines")}
</div>
</div>
<div class="foot">built {now} &middot; show, don't store</div>
<script>
{_JS}
</script>
</body>
</html>"""
    return body, meta


# ---------------------------------------------------------------------------
# Brain builder
# ---------------------------------------------------------------------------
def _md_to_html(md: str) -> str:
    """Minimal markdown → HTML: headings, lists, [[wikilinks]], paragraphs."""
    lines = md.splitlines()
    out: list[str] = []
    in_ul = False
    for raw in lines:
        line = raw.rstrip()
        # Headings
        hm = re.match(r"^(#{1,3})\s+(.*)", line)
        if hm:
            if in_ul:
                out.append("</ul>")
                in_ul = False
            level = len(hm.group(1))
            text = _process_inline(hm.group(2))
            out.append(f"<h{level}>{text}</h{level}>")
            continue
        # List items
        lm = re.match(r"^[-*]\s+(.*)", line)
        if lm:
            if not in_ul:
                out.append("<ul>")
                in_ul = True
            text = _process_inline(lm.group(1))
            out.append(f"<li>{text}</li>")
            continue
        # Blank line
        if not line.strip():
            if in_ul:
                out.append("</ul>")
                in_ul = False
            continue
        # Paragraph
        if in_ul:
            out.append("</ul>")
            in_ul = False
        text = _process_inline(line)
        out.append(f"<p>{text}</p>")
    if in_ul:
        out.append("</ul>")
    return "\n".join(out)


def _process_inline(text: str) -> str:
    """Replace [[wikilinks]] with <a> and escape HTML."""
    parts = re.split(r"\[\[([^\]]+)\]\]", text)
    result = ""
    for i, part in enumerate(parts):
        if i % 2 == 0:
            result += _html.escape(part)
        else:
            slug = _html.escape(part.replace(" ", "-").lower())
            label = _html.escape(part)
            result += f'<a href="#{slug}">{label}</a>'
    return result


def build_brain(cfg: dict[str, str]) -> str:
    """Generate brain HTML from moc.md. Returns output path."""
    moc_path = _expand(cfg, "brain_moc")
    brain_out = _expand(cfg, "brain_out")
    theme = _load_theme(cfg)
    if not moc_path.exists():
        content = f"<p class='pending'>pendente — {_html.escape(str(moc_path))}</p>"
    else:
        md = moc_path.read_text(errors="replace")
        content = _md_to_html(md)
    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Second Brain</title>
<style>
{_css_vars(theme)}
{_CSS}
body{{max-width:820px;margin:2rem auto;display:block}}
h1,h2,h3{{margin-top:1.6rem;letter-spacing:.02em}}
h1{{color:var(--accent)}}
h2{{font-size:1.05rem}}
ul{{padding-left:1.5rem;margin:0.5rem 0}}
li{{margin:0.25rem 0;font-size:0.92rem}}
p{{margin:0.5rem 0;line-height:1.65;font-size:0.92rem}}
a{{color:var(--accent)}}
</style>
</head>
<body>
{content}
</body>
</html>"""
    brain_out.parent.mkdir(parents=True, exist_ok=True)
    brain_out.write_text(html)
    return str(brain_out)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    import argparse

    p = argparse.ArgumentParser(
        prog="cc_build.py",
        description="Build Command Centre HTML from cc.json",
    )
    p.add_argument("--config", metavar="PATH", help="Path to cc.json")
    p.add_argument("--out", metavar="PATH", help="Output HTML path (overrides config)")
    p.add_argument("--brain", action="store_true", help="Generate brain/index.html")
    p.add_argument("--json", dest="json_out", action="store_true",
                   help="Print build metadata as JSON")
    args = p.parse_args(argv)

    cfg = _load_config(args.config)
    if args.out:
        cfg["out"] = args.out

    result_meta: dict[str, Any] = {}

    if args.brain:
        out_path = build_brain(cfg)
        result_meta["brain"] = out_path
    else:
        html, meta = build_html(cfg)
        out_path_str = os.path.expanduser(cfg["out"])
        out_path = Path(out_path_str)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(html)
        # OPEN FILE → serves the real artifact: expose artifacts_dir under the
        # served root via symlink (cc-serve publishes only ~/Work/cc; the ring
        # links to artifacts/<name>). Never overwrite a real directory.
        arts_dir = _expand(cfg, "artifacts_dir")
        link = out_path.parent / "artifacts"
        if arts_dir.exists() and not link.exists() and not link.is_symlink():
            link.symlink_to(arts_dir)
        result_meta.update(meta)
        result_meta["out"] = str(out_path)

    if args.json_out:
        print(json.dumps(result_meta, indent=2))
    else:
        print(f"wrote {result_meta.get('out', result_meta.get('brain', ''))}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
