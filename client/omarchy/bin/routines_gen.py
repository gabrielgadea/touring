#!/usr/bin/env python3
"""routines_gen.py — Routine board: routines.toml → systemd --user units.

Commands:
    apply    [--toml PATH] [--prefix PATH] [--no-reload] [--dry]
    board    [--toml PATH] [--json]
    validate [--toml PATH]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from datetime import datetime
from pathlib import Path
from typing import Any

_DAYS = {"Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"}
_DEFAULT_TOML = "~/Work/routines.toml"
_DEFAULT_PREFIX = "~/.config/systemd/user"
_DEFAULT_DEFAULTS: dict[str, str] = {
    "model": "sonnet",
    "effort": "medium",
    "mode": "acceptEdits",
    "cwd": "~/Work",
    "log": "~/Work/runs.log",
}


# ---------------------------------------------------------------------------
# OnCalendar conversion
# ---------------------------------------------------------------------------
def oncalendar_from_at(at: str) -> str:
    """Convert routine 'at' field to systemd OnCalendar expression.

    Supported formats:
        "HH:MM"          → "*-*-* HH:MM:00"
        "DayName HH:MM"  → "DayName *-*-* HH:MM:00"
        "*:00/N"         → "*-*-* 00/N:00:00"
        "daily"          → "*-*-* 00:00:00"
        "hourly"         → "*-*-* *:00:00"
    """
    s = at.strip()
    if s == "daily":
        return "*-*-* 00:00:00"
    if s == "hourly":
        return "*-*-* *:00:00"
    m = re.fullmatch(r"\*:00/(\d+)", s)
    if m:
        return f"*-*-* 00/{m.group(1)}:00:00"
    parts = s.split(" ", 1)
    if len(parts) == 2 and parts[0] in _DAYS:
        hm = _parse_hhmm(parts[1])
        if hm:
            return f"{parts[0]} *-*-* {hm}:00"
    hm = _parse_hhmm(s)
    if hm:
        return f"*-*-* {hm}:00"
    raise ValueError(
        f"Cannot convert 'at={at!r}' to OnCalendar. "
        "Accepted: HH:MM, DayName HH:MM, *:00/N, daily, hourly."
    )


def _parse_hhmm(s: str) -> str | None:
    m = re.fullmatch(r"(\d{1,2}):(\d{2})", s.strip())
    if not m:
        return None
    h, mi = int(m.group(1)), int(m.group(2))
    if 0 <= h <= 23 and 0 <= mi <= 59:
        return f"{h:02d}:{mi:02d}"
    return None


# ---------------------------------------------------------------------------
# TOML loading
# ---------------------------------------------------------------------------
def load_toml(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Load routines.toml; returns (defaults, routines)."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    defaults: dict[str, Any] = {**_DEFAULT_DEFAULTS, **data.get("defaults", {})}
    routines: list[dict[str, Any]] = data.get("routine", [])
    return defaults, routines


def _resolve(r: dict[str, Any], defaults: dict[str, Any]) -> dict[str, Any]:
    """Merge routine with defaults, replacing ~ with %h in path fields."""
    merged: dict[str, Any] = {**defaults, **r}
    for key in ("cwd", "log"):
        if key in merged and merged[key]:
            merged[key] = str(merged[key]).replace("~", "%h")
    return merged


# ---------------------------------------------------------------------------
# Unit file generation
# ---------------------------------------------------------------------------
def _escape_sq(s: str) -> str:
    """Escape string for single-quote context in systemd ExecStart."""
    return s.replace("'", "'\\''")


def service_content(rid: str, resolved: dict[str, Any]) -> str:
    """Generate [Unit]+[Service] for routine-<rid>.service."""
    cwd = str(resolved.get("cwd", "%h/Work"))
    if "skill" in resolved:
        model = resolved.get("model", "sonnet")
        effort = resolved.get("effort", "medium")
        mode = resolved.get("mode", "acceptEdits")
        skill = resolved["skill"]
        # Use /usr/bin/env so PATH is resolved at runtime (no absolute path needed)
        exec_start = (
            f"/usr/bin/env omarchy-skill-run {skill} {model} {effort} {mode} {cwd}"
        )
    elif "cmd" in resolved:
        # `%h`, NOT `$HOME`. systemd parses `$VAR` in ExecStart itself, before the
        # shell ever sees it, and rejected the whole path as a variable name:
        #   Invalid environment variable name evaluates to an empty string:
        #   HOME/projects/touring/client/omarchy/bin/validate_all.sh
        # -> status=2/INVALIDARGUMENT, and `routine-validate-all` could never
        # start. `%h` is systemd's own home specifier, already used for cwd/log by
        # _resolve and for every other path in this unit. Literal `%` is escaped
        # first so a command containing one is not read as a specifier.
        # Measured 2026-08-23 on the first real `systemctl --user start`.
        cmd = str(resolved["cmd"]).replace("%", "%%").replace("~", "%h")
        exec_start = f"/usr/bin/env bash -lc '{_escape_sq(cmd)}'"
    else:
        raise ValueError(f"Routine {rid!r} must have 'skill' or 'cmd'")

    return (
        "[Unit]\n"
        f"Description=Omarchy routine: {rid}\n"
        "\n"
        "[Service]\n"
        "Type=oneshot\n"
        # systemd --user units do NOT read ~/.bashrc: their PATH is the fixed
        # /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin, which
        # omits ~/.local/bin, where omarchy-skill-run and touring live. Without
        # this line every skill routine died with exit 127 while the timer kept
        # reporting "scheduled" (critic-robustness P0, 23/08/2026).
        "Environment=PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin\n"
        "Environment=OMARCHY_SKILL_RUN_LOG=%h/Work/runs.log\n"
        # `StandardOutput=append:` opens the file itself and fails if the parent
        # directory is missing — systemd never creates it.
        "ExecStartPre=/usr/bin/mkdir -p %h/Work/logs %h/Work/state %h/Work/artifacts\n"
        f"ExecStart={exec_start}\n"
        f"WorkingDirectory={cwd}\n"
        f"StandardOutput=append:%h/Work/logs/{rid}.log\n"
        "StandardError=inherit\n"
    )


def timer_content(rid: str, at: str) -> str:
    """Generate [Unit]+[Timer]+[Install] for routine-<rid>.timer."""
    oncal = oncalendar_from_at(at)
    return (
        "[Unit]\n"
        f"Description=Omarchy routine timer: {rid}\n"
        "\n"
        "[Timer]\n"
        f"OnCalendar={oncal}\n"
        "Persistent=true\n"
        "RandomizedDelaySec=60\n"
        "\n"
        "[Install]\n"
        "WantedBy=timers.target\n"
    )


# ---------------------------------------------------------------------------
# Cloud routine
# ---------------------------------------------------------------------------
def _cloud_md_entry(r: dict[str, Any]) -> str:
    rid = r.get("id", "unknown")
    at = r.get("at", "")
    repo = r.get("repo", "(configure repo)")
    prompt = r.get("prompt", "(configure prompt)")
    desc = r.get("description", rid)
    return (
        f"## {rid}\n\n"
        f"- **Schedule**: `{at}`\n"
        f"- **Repo**: `{repo}`\n"
        f"- **Prompt**: {prompt}\n"
        f"- **Description**: {desc}\n\n"
        f"```\n/schedule {at} @claude {prompt}\n```\n\n"
    )


# ---------------------------------------------------------------------------
# Systemd queries (graceful on failure)
# ---------------------------------------------------------------------------
def _systemctl(*args: str) -> tuple[int, str]:
    try:
        r = subprocess.run(
            ["systemctl", "--user", *args],
            capture_output=True,
            text=True,
            timeout=5,
        )
        return r.returncode, r.stdout
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return -1, ""


def _list_timers() -> list[dict[str, Any]]:
    rc, out = _systemctl("list-timers", "--all", "--output=json")
    if rc != 0 or not out.strip():
        return []
    try:
        return json.loads(out)  # type: ignore[no-any-return]
    except json.JSONDecodeError:
        return []


def _service_status(rid: str) -> tuple[str | None, str | None]:
    rc, out = _systemctl(
        "show",
        f"routine-{rid}.service",
        "-p",
        "Result,ExecMainStatus,ExecMainExitTimestamp",
    )
    if rc != 0 or not out.strip():
        return None, None
    props: dict[str, str] = {}
    for line in out.splitlines():
        k, sep, v = line.partition("=")
        if sep:
            props[k.strip()] = v.strip()
    ts = props.get("ExecMainExitTimestamp", "")
    last_run = ts if ts and ts not in {"n/a", ""} else None
    # systemd answers `Result=success` for a unit that has NEVER run — the field
    # is the outcome of the last run, and "no run yet" reads as the success
    # default. Reporting it verbatim made the board show every routine green from
    # the moment it was generated: a status that cannot distinguish "ran and
    # passed" from "never fired" attests nothing. The exit timestamp is the
    # discriminator (empty until the first run). Measured 2026-08-23.
    result = (props.get("Result") or None) if last_run else "never"
    return result, last_run


def _next_trigger(rid: str, timers: list[dict[str, Any]]) -> str | None:
    for t in timers:
        if t.get("unit", "") == f"routine-{rid}.timer":
            next_us = t.get("next", 0)
            if next_us and next_us > 0:
                try:
                    dt = datetime.fromtimestamp(next_us / 1_000_000)
                    return dt.strftime("%Y-%m-%dT%H:%M:%S")
                except (ValueError, OSError):
                    pass
    return None


# ---------------------------------------------------------------------------
# Board
# ---------------------------------------------------------------------------
def board_entries(
    routines: list[dict[str, Any]],
    defaults: dict[str, Any],  # noqa: ARG001
) -> list[dict[str, Any]]:
    """Build board entry list, querying systemd for live status."""
    timers = _list_timers()
    entries = []
    for r in routines:
        rid = str(r["id"])
        at = str(r.get("at", ""))
        runner = str(r.get("runner", "local"))
        if runner == "local":
            next_t = _next_trigger(rid, timers)
            last_status, last_run = _service_status(rid)
        else:
            next_t, last_status, last_run = None, None, None
        entries.append(
            {
                "id": rid,
                "at": at,
                "runner": runner,
                "next": next_t,
                "last_status": last_status,
                "last_run": last_run,
            }
        )
    return entries


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------
def _validate_routines(
    routines: list[dict[str, Any]], defaults: dict[str, Any]
) -> tuple[list[str], bool]:
    """Return (error_list, bypass_seen)."""
    errors: list[str] = []
    bypass_seen = False
    seen: set[str] = set()
    for r in routines:
        rid = str(r.get("id", ""))
        if not rid:
            errors.append("routine missing 'id'")
            continue
        if rid in seen:
            errors.append(f"duplicate id: {rid!r}")
        seen.add(rid)
        mode = str(r.get("mode", defaults.get("mode", "acceptEdits")))
        if mode == "bypassPermissions":
            errors.append(f"routine {rid!r}: mode='bypassPermissions' is forbidden")
            bypass_seen = True
        runner = str(r.get("runner", defaults.get("runner", "local")))
        if runner != "cloud" and "skill" not in r and "cmd" not in r:
            errors.append(f"routine {rid!r}: must have 'skill' or 'cmd'")
        at = str(r.get("at", ""))
        if at:
            try:
                oncalendar_from_at(at)
            except ValueError as exc:
                errors.append(f"routine {rid!r}: {exc}")
    return errors, bypass_seen


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------
def cmd_apply(args: argparse.Namespace) -> int:
    toml_path = Path(os.path.expanduser(args.toml))
    prefix = Path(os.path.expanduser(args.prefix))
    dry: bool = args.dry
    no_reload: bool = args.no_reload

    if not toml_path.exists():
        print(f"ERROR: not found: {toml_path}", file=sys.stderr)
        return 1

    defaults, routines = load_toml(toml_path)
    errors, bypass_seen = _validate_routines(routines, defaults)
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    if bypass_seen:
        return 2
    if errors:
        return 1

    prefix.mkdir(parents=True, exist_ok=True)
    local_ids: set[str] = set()
    cloud_recs: list[dict[str, Any]] = []

    for r in routines:
        rid = str(r["id"])
        resolved = _resolve(r, defaults)
        runner = str(resolved.get("runner", "local"))

        if runner == "cloud":
            cloud_recs.append(r)
            continue

        local_ids.add(rid)
        svc = service_content(rid, resolved)
        tmr = timer_content(rid, str(r.get("at", "")))
        svc_path = prefix / f"routine-{rid}.service"
        tmr_path = prefix / f"routine-{rid}.timer"
        # Write files in both dry and non-dry (--dry skips systemctl, not file writes)
        svc_path.write_text(svc)
        tmr_path.write_text(tmr)
        tag = "[dry] " if dry else ""
        print(f"{tag}wrote {svc_path}")
        print(f"{tag}wrote {tmr_path}")

    if cloud_recs:
        cloud_out = Path(os.path.expanduser("~/Work/state/routines-cloud.md"))
        body = "# Cloud Routines\n\nSchedule with `/schedule` on GitHub.\n\n"
        for r in cloud_recs:
            body += _cloud_md_entry(r)
        if dry:
            print(f"[dry] would write {cloud_out}")
        else:
            cloud_out.parent.mkdir(parents=True, exist_ok=True)
            cloud_out.write_text(body)
            print(f"wrote {cloud_out}")

    # Orphan removal (non-dry only)
    for path in list(prefix.glob("routine-*.service")):
        rid = path.stem.removeprefix("routine-")
        if rid not in local_ids:
            tmr_path = prefix / f"routine-{rid}.timer"
            if dry:
                print(f"[dry] would remove orphan {path}")
                if tmr_path.exists():
                    print(f"[dry] would remove orphan {tmr_path}")
            else:
                path.unlink(missing_ok=True)
                tmr_path.unlink(missing_ok=True)
                print(f"removed orphan {path}")

    # Systemctl (skip if dry or no-reload)
    if not dry and not no_reload:
        _systemctl("daemon-reload")
        for rid in sorted(local_ids):
            rc, _ = _systemctl("enable", "--now", f"routine-{rid}.timer")
            if rc != 0:
                print(f"WARNING: failed to enable routine-{rid}.timer", file=sys.stderr)
    elif dry:
        for rid in sorted(local_ids):
            print(f"[dry] would run: systemctl --user enable --now routine-{rid}.timer")

    return 0


def cmd_board(args: argparse.Namespace) -> int:
    toml_path = Path(os.path.expanduser(args.toml))
    if not toml_path.exists():
        if args.json:
            print("[]")
        else:
            print("No routines.toml found.")
        return 0
    defaults, routines = load_toml(toml_path)
    entries = board_entries(routines, defaults)
    if args.json:
        print(json.dumps(entries, indent=2))
        return 0
    print(f"{'Time':<14}{'ID':<22}{'Runner':<8}{'Next':<22}{'Last Status':<16}Last Run")
    print("-" * 100)
    for e in entries:
        print(
            f"{e['at']:<14}{e['id']:<22}{e['runner']:<8}"
            f"{str(e['next'] or 'n/a'):<22}{str(e['last_status'] or 'n/a'):<16}"
            f"{e['last_run'] or 'n/a'}"
        )
    return 0


def cmd_validate(args: argparse.Namespace) -> int:
    toml_path = Path(os.path.expanduser(args.toml))
    if not toml_path.exists():
        print(f"ERROR: not found: {toml_path}", file=sys.stderr)
        return 1
    try:
        defaults, routines = load_toml(toml_path)
    except Exception as exc:  # noqa: BLE001
        print(f"ERROR: TOML parse error: {exc}", file=sys.stderr)
        return 1
    errors, bypass_seen = _validate_routines(routines, defaults)
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    if bypass_seen:
        return 2
    if errors:
        return 1
    print(f"OK: {len(routines)} routine(s) valid")
    return 0


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------
def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="routines_gen.py",
        description="Manage Omarchy routines: routines.toml → systemd --user units",
    )
    sub = p.add_subparsers(dest="command", required=True)

    ap = sub.add_parser("apply", help="Write unit files and (optionally) reload systemd")
    ap.add_argument("--toml", default=_DEFAULT_TOML, metavar="PATH")
    ap.add_argument("--prefix", default=_DEFAULT_PREFIX, metavar="PATH")
    ap.add_argument("--no-reload", dest="no_reload", action="store_true",
                    help="Skip daemon-reload and enable")
    ap.add_argument("--dry", action="store_true",
                    help="Write unit files but skip all systemctl calls")

    bp = sub.add_parser("board", help="Print routine board")
    bp.add_argument("--toml", default=_DEFAULT_TOML, metavar="PATH")
    bp.add_argument("--json", action="store_true", help="Output JSON array")

    vp = sub.add_parser("validate", help="Validate routines.toml without writing files")
    vp.add_argument("--toml", default=_DEFAULT_TOML, metavar="PATH")

    return p


def main(argv: list[str] | None = None) -> int:
    """Entry point; returns exit code."""
    parser = _build_parser()
    args = parser.parse_args(argv)
    if args.command == "apply":
        return cmd_apply(args)
    if args.command == "board":
        return cmd_board(args)
    if args.command == "validate":
        return cmd_validate(args)
    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
