#!/usr/bin/env python3
"""AB34: 2x2 factorial A/B of the result compactor (C) and the eviction mod (E).

Frozen spec: ``AB34-preregistro.md`` in this directory (section numbers below refer to
it). Calendar: ``ab34-calendario.json``. Subcommands:

    calendar   print the calendar a seed produces (the file must equal it, section 2)
    status     today's cell and the state of the two flags (read-only)
    arm        put today's two flags in force and verify them (dry-run unless --yes)
    reset      end of the experiment: both flags off (refuses while the calendar runs)
    analyze    per-day outcomes, exclusions, factor effects and the section 6 verdicts

The two flags live in different processes, which is the whole difficulty:

* C = ``TOURING_RESULT_COMPACTOR`` is read with ``std::env::var`` INSIDE the daemon
  (``result_compactor.rs:113-114``), so flipping it means respawning the daemon with a
  different environment. A daemon's environment is whatever its spawner had
  (``daemon_spawn.rs:193-213``: no ``env_clear``), and a hook autostart spawns it from the
  Claude Code session's environment (``touring-hooks/src/main.rs:1266``), so nothing
  pins it. ``arm`` therefore restarts the daemon with the OLD daemon's own environment
  changed in exactly one variable, then READS ``/proc/<pid>/environ`` back and retries.
* E = ``TOURING_COMPACT_EVICT`` is read by the mod from the session environment
  (``register.ts:80``): ``env.TOURING_COMPACT_EVICT`` of ``~/.claude/settings.json``,
  fixed when a session opens (the A3 convention).

Instruments are reused as they are: ``scripts/measure_tool_trio.py`` (calls, errors,
retries, result bytes), ``scripts/r3_a1_kpi.py`` (``hook_injections``) and the human-turn
predicate of ``client/skills/loop-engineering/scripts/hooks/loop_turn_record.py``.

Exit codes: 0 ok, 2 invalid input, 3 armed but NOT verified, 4 not applicable now (the
calendar says so: before day 0, after the end, or a block still running), 1 unexpected.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import fcntl
import functools
import hashlib
import importlib.util
import json
import math
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

CELLS = ("--", "C-", "-E", "CE")  # first letter = compactor on, second = eviction on
FLAG_C = "TOURING_RESULT_COMPACTOR"
FLAG_E = "TOURING_COMPACT_EVICT"
ON = "on"
MIN_TURNS = 20  # section 4: a day with fewer human turns is excluded
MARGIN_REL = -0.10  # section 6: the primary must fall by at least 10%
HARM_PP = 2.0  # section 6: no harm metric may worsen beyond +2 pp (upper CI)
MIN_DF = 2  # section 6: below this the verdict is inconclusive
MAX_RESTARTS = 3
SETTLE_SECONDS = 4.0
CALENDAR_PATH = os.path.join(HERE, "ab34-calendario.json")
DEFAULT_SETTINGS = "~/.claude/settings.json"
DEFAULT_LOG = "~/.claude/touring/ab34/arm.jsonl"
DEFAULT_COMPACTED = "~/.claude/touring/compacted"
DEFAULT_EVICTED = "~/.claude/touring/evicted"
EVICTED_FRAGMENT = "/.claude/touring/evicted/"
COMPACTED_FRAGMENT = "/.claude/touring/compacted/"
RAW_READERS = ("Read", "Bash", "Grep", "Glob")
REQUIRED_KEYS = ("day0", "tz", "seed", "blocks", "daemon_socket", "transcript_dirs")
# Two-sided 95% Student t quantiles, index = degrees of freedom (section 6).
T975 = (
    None,
    12.706,
    4.303,
    3.182,
    2.776,
    2.571,
    2.447,
    2.365,
    2.306,
    2.262,
    2.228,
    2.201,
    2.179,
    2.160,
    2.145,
    2.131,
    2.120,
    2.110,
    2.101,
    2.093,
    2.086,
    2.080,
    2.074,
    2.069,
    2.064,
    2.060,
    2.056,
    2.052,
    2.048,
    2.045,
    2.042,
)
HARM_METRICS = {
    "C": ("erro_pp", "retry_pp", "wasted_pp", "raw_c_pp"),
    "E": ("erro_pp", "retry_pp", "wasted_pp", "raw_e_pp"),
}


def _load_module(name: str, path: str):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod  # dataclasses resolve annotations through sys.modules
    spec.loader.exec_module(mod)
    return mod


def _load_instruments():
    """Import the versioned instruments; a missing one fails loud at import time."""
    scripts = os.path.join(REPO, "scripts")
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    import measure_tool_trio
    import r3_a1_kpi

    turn_path = os.path.join(
        REPO,
        "client",
        "skills",
        "loop-engineering",
        "scripts",
        "hooks",
        "loop_turn_record.py",
    )
    return (
        measure_tool_trio,
        r3_a1_kpi,
        _load_module("ab34_loop_turn_record", turn_path),
    )


m, r3, turns = _load_instruments()


class Refusal(Exception):
    """Invalid input or a refused action: the CLI maps it to exit code 2."""


class NotApplicable(Refusal):
    """The calendar says this action does not apply now: exit code 4, an expected state
    (the systemd units treat it as success; every other refusal stays a failure)."""


# --------------------------------------------------------------------------- calendar


def _shuffled(items: list, rng: random.Random) -> list:
    """Fisher-Yates over ``rng.random()`` only: the Mersenne Twister stream is stable
    across Python versions, ``random.shuffle`` is not guaranteed to be."""
    out = list(items)
    for i in range(len(out) - 1, 0, -1):
        j = int(rng.random() * (i + 1))
        out[i], out[j] = out[j], out[i]
    return out


def schedule_from_seed(seed: int, blocks: int = 2) -> list[list[str]]:
    """Each block is a seeded permutation of the four cells (a complete block)."""
    rng = random.Random(seed)
    return [_shuffled(list(CELLS), rng) for _ in range(blocks)]


def make_calendar(seed: int, day0: str | None = None) -> dict:
    return {
        "day0": day0,
        "tz": "America/Sao_Paulo",
        "seed": seed,
        "blocks": schedule_from_seed(seed),
        "daemon_socket": "/tmp/touring-daemon-1000.sock",
        "transcript_dirs": ["-home-gabrielgadea-projects-touring"],
    }


def _is_int(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _check_clock(cal: dict) -> None:
    if cal["day0"] is not None:
        try:
            dt.date.fromisoformat(cal["day0"])
        except (TypeError, ValueError) as exc:
            raise Refusal("day0 deve ser null ou 'AAAA-MM-DD'") from exc
    try:
        ZoneInfo(cal["tz"])
    except (ZoneInfoNotFoundError, ValueError, TypeError) as exc:
        raise Refusal(f"tz desconhecido: {cal['tz']!r}") from exc


def _check_blocks(cal: dict) -> None:
    if not _is_int(cal["seed"]):
        raise Refusal("seed deve ser um inteiro")
    blocks = cal["blocks"]
    if not isinstance(blocks, list) or not blocks:
        raise Refusal("blocks deve ser uma lista não vazia")
    for block in blocks:
        if not isinstance(block, list) or sorted(block) != sorted(CELLS):
            raise Refusal(f"cada bloco deve conter as 4 células {CELLS} uma vez cada")
    if len(blocks) > len(CELLS):
        raise Refusal(
            f"no máximo {len(CELLS)} blocos: mais que isso repete uma célula na mesma posição"
        )
    if any(
        len(set(position)) != len(position) for position in zip(*blocks, strict=True)
    ):
        raise Refusal(
            "dois blocos põem a mesma célula na mesma posição: troque a semente"
        )
    if blocks != schedule_from_seed(cal["seed"], len(blocks)):
        raise Refusal("blocks não reproduz a semente: a ordem é fixada pelo shuffle")


def _check_scope(cal: dict) -> None:
    sock = cal["daemon_socket"]
    if not isinstance(sock, str) or not os.path.isabs(sock):
        raise Refusal("daemon_socket deve ser um caminho absoluto")
    dirs = cal["transcript_dirs"]
    if (
        not isinstance(dirs, list)
        or not dirs
        or not all(isinstance(d, str) and d for d in dirs)
    ):
        raise Refusal("transcript_dirs deve ser uma lista não vazia de nomes")


def validate_calendar(cal) -> dict:
    """Raise ``Refusal`` unless ``cal`` is exactly what the seed says it must be."""
    if not isinstance(cal, dict):
        raise Refusal("calendário deve ser um objeto JSON")
    missing = [k for k in REQUIRED_KEYS if k not in cal]
    if missing:
        raise Refusal(f"calendário sem as chaves: {', '.join(missing)}")
    _check_clock(cal)
    _check_blocks(cal)
    _check_scope(cal)
    return cal


def load_calendar(path: str) -> dict:
    try:
        with open(path, encoding="utf-8") as fh:
            cal = json.load(fh)
    except OSError as exc:
        raise Refusal(f"calendário ilegível ({path}): {exc}") from exc
    except ValueError as exc:
        raise Refusal(f"calendário não é JSON válido ({path}): {exc}") from exc
    return validate_calendar(cal)


@functools.cache
def _zone(name: str) -> ZoneInfo:
    return ZoneInfo(name)


def parse_ts(text: str) -> dt.datetime:
    """Aware UTC datetime from an ISO string (a naive one is taken as UTC)."""
    stamp = dt.datetime.fromisoformat(text.strip())
    if stamp.tzinfo is None:
        stamp = stamp.replace(tzinfo=dt.UTC)
    return stamp.astimezone(dt.UTC)


def total_days(cal: dict) -> int:
    return sum(len(block) for block in cal["blocks"])


def flat_days(cal: dict) -> list[tuple[int, str]]:
    """``(block_index, cell)`` for every experiment day, in order."""
    return [(b, cell) for b, block in enumerate(cal["blocks"]) for cell in block]


def day_index(stamp: dt.datetime, cal: dict) -> int:
    """Experiment day (local date minus day0) containing ``stamp``; may be negative."""
    local = stamp.astimezone(_zone(cal["tz"])).date()
    return (local - dt.date.fromisoformat(cal["day0"])).days


def day_start_utc(cal: dict, k: int) -> dt.datetime:
    """UTC instant at which experiment day ``k`` opens (local midnight)."""
    day = dt.date.fromisoformat(cal["day0"]) + dt.timedelta(days=k)
    return dt.datetime.combine(day, dt.time(0), tzinfo=_zone(cal["tz"])).astimezone(
        dt.UTC
    )


def level_now(cal: dict, now: dt.datetime):
    """``(day, block, cell)`` in force at ``now`` or None (experiment not running)."""
    if cal["day0"] is None:
        return None
    k = day_index(now, cal)
    days = flat_days(cal)
    if not 0 <= k < len(days):
        return None
    return k, days[k][0], days[k][1]


def want_flags(cell: str) -> tuple[bool, bool]:
    return cell[0] == "C", cell[1] == "E"


def _inactive_reason(cal: dict, now: dt.datetime) -> str:
    if cal["day0"] is None:
        return "day0 não definido"
    if now < day_start_utc(cal, 0):
        return "antes do dia 0"
    return "sequência encerrada"


# ------------------------------------------------------------------- settings.json (E)


def _unique_backup(real: str, now: dt.datetime) -> str:
    stamp = now.strftime("%Y%m%dT%H%M%S%fZ")
    path, n = f"{real}.bak-{stamp}", 0
    while os.path.exists(path):
        n += 1
        path = f"{real}.bak-{stamp}-{n}"
    return path


def _write_atomic(real: str, text: str) -> None:
    """Replace ``real`` with ``text`` via a same-directory temp file, keeping its mode."""
    fd, tmp = tempfile.mkstemp(
        dir=os.path.dirname(real), prefix=".settings-", suffix=".tmp"
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as fh:
            fh.write(text)
            fh.flush()
            os.fsync(fh.fileno())
        shutil.copymode(real, tmp)
        os.replace(tmp, real)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


def _load_settings(real: str) -> tuple[str, dict]:
    """``(raw_text, settings)``; ``Refusal`` unless a JSON object with an object ``env``."""
    try:
        with open(real, encoding="utf-8", newline="") as fh:
            raw = fh.read()
    except OSError as exc:
        raise Refusal(f"settings ilegível ({real}): {exc}") from exc
    try:
        settings = json.loads(raw)
    except ValueError as exc:
        raise Refusal(f"settings não é JSON válido ({real}): {exc}") from exc
    if not isinstance(settings, dict) or not isinstance(settings.get("env", {}), dict):
        raise Refusal("settings: a raiz e a chave env precisam ser objetos JSON")
    return raw, settings


def _set_flag_e(settings: dict, on: bool) -> None:
    if on:
        settings.setdefault("env", {})[FLAG_E] = ON
    else:
        settings.get("env", {}).pop(FLAG_E, None)


def flag_is_on(raw) -> bool:
    """Same predicate as ``result_compactor::flag_is_on`` (``on``/``1``/``true``/``yes``)."""
    return isinstance(raw, str) and raw.strip().lower() in ("on", "1", "true", "yes")


# ------------------------------------------------------------------- host (processes)


def restart_env(environ: dict, want_on: bool) -> dict:
    """The daemon's own environment with ONE variable changed (the compactor flag)."""
    env = dict(environ)
    if want_on:
        env[FLAG_C] = ON
    else:
        env.pop(FLAG_C, None)
    return env


def _sha256_file(path: str) -> str | None:
    try:
        digest = hashlib.sha256()
        with open(path, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError:
        return None


def _read_environ(pid: int) -> dict | None:
    try:
        with open(f"/proc/{pid}/environ", "rb") as fh:
            blob = fh.read()
    except OSError:
        return None
    out = {}
    for item in blob.split(b"\0"):
        if b"=" in item:
            key, _, value = item.partition(b"=")
            out[key.decode(errors="replace")] = value.decode(errors="replace")
    return out


def _json_from(text: str):
    start = text.find("{")
    return json.loads(text[start:]) if start >= 0 else None


class Host:
    """Everything ``arm`` and ``reset`` touch outside the files they own.

    Tests replace it with a fake: nothing here runs in the suite.
    """

    def __init__(self) -> None:
        self.touring = shutil.which("touring") or os.path.expanduser(
            "~/.local/bin/touring"
        )

    def _run(self, argv, env=None, cwd=None, timeout=60):
        return subprocess.run(
            argv,
            capture_output=True,
            text=True,
            env=env,
            cwd=cwd,
            timeout=timeout,
            check=False,
        )

    def daemon_state(self, sock: str) -> dict | None:
        """The live daemon behind ``sock`` (REGRA #19: ``comm`` verified), or None."""
        env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith("TOURING_DAEMON_SOCK")
        }
        out = self._run(
            [self.touring, "daemon-ctl", "status", "-j", "--socket", sock], env=env
        )
        data = _json_from(out.stdout) if out.returncode == 0 else None
        pid = (data or {}).get("daemon_pid")
        if not (data or {}).get("socket_alive") or not _is_int(pid):
            return None
        try:
            with open(f"/proc/{pid}/comm", encoding="utf-8") as fh:
                comm = fh.read().strip()
            cwd = os.readlink(f"/proc/{pid}/cwd")
        except OSError:
            return None
        environ = _read_environ(pid)
        if comm != "touring-daemon" or environ is None:
            return None
        return {
            "pid": pid,
            "exe": data.get("daemon_exe"),
            "exe_sha": _sha256_file(f"/proc/{pid}/exe"),
            "cwd": cwd,
            "flag_raw": environ.get(FLAG_C),
            "flag_on": flag_is_on(environ.get(FLAG_C)),
        }

    def restart_daemon(self, sock: str, want_on: bool, state: dict | None) -> None:
        """``daemon-ctl restart`` under the old daemon's environment and cwd (REGRA #19)."""
        base = _read_environ(state["pid"]) if state else None
        if base is None:
            base = {
                k: v
                for k, v in os.environ.items()
                if not k.startswith("TOURING_DAEMON_SOCK")
            }
        cwd = state["cwd"] if state else REPO
        out = self._run(
            [self.touring, "daemon-ctl", "restart", "--socket", sock, "-j"],
            env=restart_env(base, want_on),
            cwd=cwd,
            timeout=90,
        )
        if out.returncode != 0:
            raise RuntimeError(
                f"daemon-ctl restart exit {out.returncode}: {out.stderr[-300:]}"
            )

    def identity(self, state: dict | None) -> dict:
        """Version and sha of what a release replaces (section 4: one binary per block)."""
        out = self._run([self.touring, "--version"])  # the version goes to stderr
        lines = [ln for ln in (out.stderr + out.stdout).splitlines() if ln.strip()]
        hook = os.path.expanduser("~/.claude/hooks/touring-hook")
        return {
            "touring_version": lines[0].strip() if lines else None,
            "touring_sha": _sha256_file(os.path.realpath(self.touring)),
            "daemon_exe": (state or {}).get("exe"),
            "daemon_sha": (state or {}).get("exe_sha"),
            "hook_sha": _sha256_file(os.path.realpath(hook)),
        }

    def kpi_snapshot(self) -> dict | None:
        """Cumulative code-mode counters; per-day deltas come from two snapshots."""
        try:
            out = self._run([self.touring, "kpi", "-j"], cwd=REPO, timeout=120)
            adherence = (_json_from(out.stdout) or {})["code_mode_adherence"]
            keys = ("runs_total", "runs_ok", "wasted_attempts_retry_pairs")
            return {k: adherence[k] for k in keys}
        except Exception:  # noqa: BLE001 - a missing snapshot never blocks the arm
            return None

    def sleep(self, seconds: float) -> None:
        time.sleep(seconds)


# ------------------------------------------------------------------------ arm / reset


def binary_id(record: dict) -> str | None:
    """Identity of the binaries behind a log record; None when either sha is unknown."""
    binary = record.get("binary") or {}
    touring_sha, daemon_sha = binary.get("touring_sha"), binary.get("daemon_sha")
    if not touring_sha or not daemon_sha:
        return None
    return f"{touring_sha[:16]}+{daemon_sha[:16]}"


def append_log(path: str, record: dict) -> None:
    """One ``write`` of one line in append mode, so concurrent writers never interleave."""
    real = os.path.expanduser(path)
    os.makedirs(os.path.dirname(real), exist_ok=True)
    line = json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n"
    fd = os.open(real, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        os.write(fd, line.encode("utf-8"))
    finally:
        os.close(fd)


def ensure_daemon_flag(host, sock: str, want_on: bool, state: dict | None):
    """Restart until the daemon's environment shows the wanted flag (bounded).

    The restart can lose a race: between the SIGTERM and the respawn a hook of any live
    session may autostart a daemon from ITS environment and win the singleton lock. So
    the answer is read back from ``/proc``, never taken from the restart's exit code.
    """
    attempts, errors = 0, []
    while (
        not (state is not None and state["flag_on"] == want_on)
        and attempts < MAX_RESTARTS
    ):
        attempts += 1
        try:
            host.restart_daemon(sock, want_on, state)
        except Exception as exc:  # noqa: BLE001 - recorded, then re-verified below
            errors.append(repr(exc))
        host.sleep(SETTLE_SECONDS)
        state = host.daemon_state(sock)
    return state, attempts, errors


@dataclass
class Plan:
    event: str
    want_c: bool
    want_e: bool
    cell: str
    day: int | None
    block: int | None


def run_apply(args, cal: dict, host, now: dt.datetime, plan: Plan) -> int:
    """Put ``plan``'s two flags in force, verify them, and log what happened."""
    real = os.path.realpath(os.path.expanduser(args.settings))
    raw, settings = _load_settings(real)
    before_e = settings.get("env", {}).get(FLAG_E)
    sock = cal["daemon_socket"]
    state = host.daemon_state(sock)
    want_e_value = ON if plan.want_e else None
    print(
        f"{plan.event}: dia {plan.day}, célula {plan.cell} (C={plan.want_c}, E={plan.want_e})"
    )
    print(f"E  {FLAG_E}: {before_e!r} -> {want_e_value!r}")
    seen_c = None if state is None else state["flag_raw"]
    print(f"C  {FLAG_C} no daemon: {seen_c!r} -> {ON if plan.want_c else None!r}")
    if not args.yes:
        print("dry-run: nada gravado (use --yes para gravar, com backup e restart)")
        return 0
    lock_path = os.path.join(os.path.dirname(os.path.expanduser(args.log)), "arm.lock")
    os.makedirs(os.path.dirname(lock_path), exist_ok=True)
    with open(lock_path, "w", encoding="utf-8") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            print("outra execução do arm em andamento: nada feito")
            return 0
        return _apply_locked(args, cal, host, now, plan, real, raw, settings, state)


def _write_flag_e(
    real: str, raw: str, settings: dict, want_on: bool, now: dt.datetime
) -> None:
    """Backup, then atomically rewrite ``settings.json`` with the E flag set or removed."""
    shutil.copy2(real, _unique_backup(real, now))
    _set_flag_e(settings, want_on)
    text = json.dumps(settings, indent=2, ensure_ascii=False)
    _write_atomic(real, text + ("\n" if raw.endswith("\n") else ""))


def _apply_locked(args, cal, host, now, plan, real, raw, settings, state) -> int:
    saw = {
        "E_settings": settings.get("env", {}).get(FLAG_E),
        "C_daemon_env": None if state is None else state["flag_raw"],
        "daemon_pid": None if state is None else state["pid"],
    }
    kpi = host.kpi_snapshot()
    actions: list[str] = []
    if saw["E_settings"] != (ON if plan.want_e else None):
        _write_flag_e(real, raw, settings, plan.want_e, now)
        actions.append(f"settings:{'set' if plan.want_e else 'unset'}:{FLAG_E}")
    state, attempts, errors = ensure_daemon_flag(
        host, cal["daemon_socket"], plan.want_c, state
    )
    if attempts:
        actions.append(f"daemon:restart x{attempts}")
    after_e = _load_settings(real)[1].get("env", {}).get(FLAG_E)
    e_ok = after_e == (ON if plan.want_e else None)
    c_ok = state is not None and state["flag_on"] == plan.want_c
    record = {
        "ts": now.isoformat(),
        "event": plan.event,
        "day": plan.day,
        "block": plan.block,
        "cell": plan.cell,
        "want": {"C": plan.want_c, "E": plan.want_e},
        "saw": saw,
        "actions": actions,
        "after": {
            "E_settings": after_e,
            "C_daemon_env": None if state is None else state["flag_raw"],
            "daemon_pid": None if state is None else state["pid"],
        },
        "ok": bool(e_ok and c_ok),
        "errors": errors,
        "binary": host.identity(state),
        "kpi": kpi,
    }
    append_log(args.log, record)
    print(f"{'ok' if record['ok'] else 'NÃO VERIFICADO'}: ações {actions or 'nenhuma'}")
    return 0 if record["ok"] else 3


def cmd_arm(args, cal: dict, host, now: dt.datetime) -> int:
    cur = level_now(cal, now)
    if args.force_cell:
        if cur is not None:
            raise NotApplicable(
                "--force-cell só vale com o calendário inativo (pré-voo)"
            )
        if args.force_cell not in CELLS:
            raise Refusal(f"--force-cell deve ser uma de {CELLS}")
        c, e = want_flags(args.force_cell)
        return run_apply(
            args, cal, host, now, Plan("preflight", c, e, args.force_cell, None, None)
        )
    if cur is None:
        raise NotApplicable(
            f"experimento não ativo ({_inactive_reason(cal, now)}): nada a armar"
        )
    day, block, cell = cur
    c, e = want_flags(cell)
    return run_apply(args, cal, host, now, Plan("arm", c, e, cell, day, block))


def cmd_reset(args, cal: dict, host, now: dt.datetime) -> int:
    """End of the experiment: both flags off. Refuses while the calendar says a block is
    running, so a calendar edit is all it takes to keep the arms in force."""
    cur = level_now(cal, now)
    if cur is not None:
        day, block, cell = cur
        raise NotApplicable(
            f"experimento ativo (dia {day}, bloco {block}, célula {cell}): reset recusado"
        )
    return run_apply(
        args, cal, host, now, Plan("reset", False, False, "--", None, None)
    )


def cmd_status(args, cal: dict, host, now: dt.datetime) -> int:
    cur = level_now(cal, now)
    if cur is None:
        print(f"experimento não ativo ({_inactive_reason(cal, now)})")
    else:
        day, block, cell = cur
        print(f"dia {day}/{total_days(cal) - 1}, bloco {block}, célula {cell}")
    state = host.daemon_state(cal["daemon_socket"])
    env = _load_settings(os.path.realpath(os.path.expanduser(args.settings)))[1].get(
        "env", {}
    )
    print(f"E {FLAG_E} em settings: {env.get(FLAG_E)!r}")
    print(f"C {FLAG_C} no daemon: {None if state is None else state['flag_raw']!r}")
    return 0


def cmd_calendar(args, _cal, _host, _now) -> int:
    cal = make_calendar(args.seed, args.day0)
    validate_calendar(cal)
    text = json.dumps(cal, indent=2, ensure_ascii=False) + "\n"
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write(text)
    else:
        print(text, end="")
    return 0


# ------------------------------------------------------------------------- transcripts


@dataclass
class Sess:
    """One transcript folded into the counters the analysis needs."""

    path: str
    sid: str
    first_ts: str
    last_ts: str
    version: str
    human_turns: int
    bytes: int  # injected + tool-result bytes: what entered the context
    calls: int
    erro: int
    retry: int
    raw_c: int  # tool calls that touch the compactor's raw copies
    raw_e: int  # tool calls that touch the eviction mod's raw copies
    compactions: list = field(default_factory=list)


class _Tap:
    """Counts what the instrument does not, while ``measure_session`` streams the records."""

    def __init__(self) -> None:
        self.first_ts = ""
        self.last_ts = ""
        self.version = ""
        self.human_turns = 0
        self.injected = 0
        self.raw_c = 0
        self.raw_e = 0
        self.compactions: list[dict] = []

    def wrap(self, records):
        for rec in records:
            self._see(rec)
            yield rec

    def _see(self, rec: dict) -> None:
        stamp = rec.get("timestamp")
        if stamp:
            self.first_ts = self.first_ts or stamp
            self.last_ts = stamp
        self.version = self.version or rec.get("version") or ""
        for _emitter, text in r3.hook_injections(rec):
            self.injected += len(text.encode("utf-8"))
        content = (rec.get("message") or {}).get("content")
        kind = rec.get("type")
        if kind == "user":
            self.human_turns += self._is_human(rec, content)
        elif kind == "assistant" and isinstance(content, list):
            for block in content:
                self._raw_reads(block)
        elif kind == "system" and rec.get("subtype") == "compact_boundary":
            self._compaction(rec.get("compactMetadata") or {})

    @staticmethod
    def _is_human(rec: dict, content) -> bool:
        """A person wrote it: not a subagent line, not harness meta, not a tool result."""
        if rec.get("isSidechain") or rec.get("isMeta"):
            return False
        return isinstance(content, (str, list)) and turns.is_human_record(content)

    def _compaction(self, meta: dict) -> None:
        self.compactions.append(
            {
                "trigger": meta.get("trigger"),
                "pre": meta.get("preTokens"),
                "post": meta.get("postTokens"),
                "ms": meta.get("durationMs"),
            }
        )

    def _raw_reads(self, block) -> None:
        if not isinstance(block, dict) or block.get("type") != "tool_use":
            return
        if block.get("name") not in RAW_READERS:
            return
        text = json.dumps(block.get("input") or {}, ensure_ascii=False)
        self.raw_c += COMPACTED_FRAGMENT in text
        self.raw_e += EVICTED_FRAGMENT in text


def fold_session(path: str) -> Sess | None:
    """Fold one transcript; None when it carries no timestamp at all."""
    tap, stats = _Tap(), {}
    m.measure_session(tap.wrap(m.iter_records(path)), stats)
    if not tap.first_ts:
        return None
    return Sess(
        path=path,
        sid=os.path.splitext(os.path.basename(path))[0],
        first_ts=tap.first_ts,
        last_ts=tap.last_ts,
        version=tap.version,
        human_turns=tap.human_turns,
        bytes=tap.injected + sum(s.result_bytes for s in stats.values()),
        calls=sum(s.calls for s in stats.values()),
        erro=sum(s.outcomes["erro"] for s in stats.values()),
        retry=sum(s.retry for s in stats.values()),
        raw_c=tap.raw_c,
        raw_e=tap.raw_e,
        compactions=tap.compactions,
    )


# ----------------------------------------------------------------------------- the log


def read_log(path: str) -> tuple[list[dict], int]:
    """Valid records of the arm log in time order, and the count of lines that were not."""
    real = os.path.expanduser(path)
    if not os.path.exists(real):
        return [], 0
    records, bad = [], 0
    with open(real, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if not line.strip():
                continue
            try:
                rec = json.loads(line)
                parse_ts(rec["ts"])
                ok = isinstance(rec, dict) and rec.get("event") in (
                    "arm",
                    "reset",
                    "preflight",
                )
            except (ValueError, KeyError, TypeError):
                ok = False
            if ok:
                records.append(rec)
            else:
                bad += 1
    records.sort(key=lambda r: parse_ts(r["ts"]))
    return records, bad


def block_binaries(cal: dict, by_day: dict[int, list[dict]]) -> dict[int, set]:
    """Binary ids seen per block over every ``arm`` record of its days (None = unknown)."""
    out: dict[int, set] = collections.defaultdict(set)
    for k, (block, _cell) in enumerate(flat_days(cal)):
        for rec in by_day.get(k, []):
            out[block].add(binary_id(rec))
    return out


def day_exclusions(
    cal: dict, k: int, recs: list[dict], binaries: dict[int, set]
) -> list[str]:
    """Reasons a day cannot count, from the log alone (section 4)."""
    if not recs:
        return ["sem_registro"]
    reasons = []
    if any(not r.get("ok") for r in recs):
        reasons.append("nao_verificado")
    if any(r.get("actions") for r in recs[1:]):
        reasons.append("deriva")
    ids = binaries.get(flat_days(cal)[k][0], set())
    if None in ids:
        reasons.append("binario_ausente")
    elif len(ids) > 1:
        reasons.append("binario_misto")
    return reasons


def count_new_files(directory: str, lo: dt.datetime, hi: dt.datetime) -> int:
    """Files of ``directory`` modified in ``[lo, hi)``: the raw copies a flag leaves behind."""
    real = os.path.expanduser(directory)
    if not os.path.isdir(real):
        return 0
    n = 0
    for entry in os.scandir(real):
        if entry.is_file(follow_symlinks=False):
            mtime = dt.datetime.fromtimestamp(entry.stat().st_mtime, dt.UTC)
            n += lo <= mtime < hi
    return n


# -------------------------------------------------------------------------- statistics


class Singular(Exception):
    """The design matrix has no inverse (a block lost the contrast)."""


def _invert(matrix: list[list[float]]) -> list[list[float]]:
    n = len(matrix)
    a = [
        row[:] + [1.0 if i == j else 0.0 for j in range(n)]
        for i, row in enumerate(matrix)
    ]
    for col in range(n):
        pivot = max(range(col, n), key=lambda r: abs(a[r][col]))
        if abs(a[pivot][col]) < 1e-9:
            raise Singular("matriz do desenho singular")
        a[col], a[pivot] = a[pivot], a[col]
        scale = a[col][col]
        a[col] = [v / scale for v in a[col]]
        for r in range(n):
            if r != col and a[r][col]:
                factor = a[r][col]
                a[r] = [x - factor * y for x, y in zip(a[r], a[col], strict=True)]
    return [row[n:] for row in a]


def fit_ols(x: list[list[float]], y: list[float]) -> dict:
    """Ordinary least squares: ``beta``, standard errors and the residual df."""
    n, p = len(x), len(x[0])
    xtx = [
        [sum(x[r][i] * x[r][j] for r in range(n)) for j in range(p)] for i in range(p)
    ]
    inv = _invert(xtx)
    xty = [sum(x[r][i] * y[r] for r in range(n)) for i in range(p)]
    beta = [sum(inv[i][j] * xty[j] for j in range(p)) for i in range(p)]
    rss = sum((y[r] - sum(x[r][i] * beta[i] for i in range(p))) ** 2 for r in range(n))
    df = n - p
    if df < 1:
        return {"beta": beta, "se": [None] * p, "df": df}
    sigma2 = rss / df
    return {
        "beta": beta,
        "se": [math.sqrt(max(sigma2 * inv[i][i], 0.0)) for i in range(p)],
        "df": df,
    }


def t_quantile(df: int) -> float:
    return T975[df] if 1 <= df < len(T975) else 1.96


def _design(rows: list[dict], interaction: bool) -> tuple[list[list[float]], list[str]]:
    blocks = sorted({r["block"] for r in rows})
    names = (
        ["const"]
        + [f"b{b}" for b in blocks[1:]]
        + ["C", "E"]
        + (["CE"] if interaction else [])
    )
    x = []
    for r in rows:
        row = [1.0] + [1.0 if r["block"] == b else 0.0 for b in blocks[1:]]
        row += [float(r["c"]), float(r["e"])]
        if interaction:
            row.append(float(r["c"] * r["e"]))
        x.append(row)
    return x, names


def _interval(beta: float, se: float | None, df: int, log: bool) -> dict:
    if se is None:
        return {"est": None, "lo": None, "hi": None}
    half = t_quantile(df) * se
    if log:
        return {
            "est": math.exp(beta) - 1,
            "lo": math.exp(beta - half) - 1,
            "hi": math.exp(beta + half) - 1,
        }
    return {"est": beta, "lo": beta - half, "hi": beta + half}


def fit_effects(rows: list[dict], key: str, log: bool) -> dict | None:
    """Main effects of C and E on ``key`` (block fixed effects, no interaction).

    ``log`` fits log(y) and reports the effect as a relative change; otherwise the effect
    is in the unit of ``key`` (percentage points). ``None`` when the design cannot be
    fitted from the days that remain.
    """
    use = [r for r in rows if r.get(key) is not None and (not log or r[key] > 0)]
    if not use:
        return None
    y = [math.log(r[key]) if log else float(r[key]) for r in use]
    try:
        x, names = _design(use, interaction=False)
        fit = fit_ols(x, y)
    except Singular:
        return None
    df = fit["df"]
    out = {"n": len(use), "df": df, "efeitos": {}, "interacao_exploratoria": None}
    for factor in ("C", "E"):
        i = names.index(factor)
        out["efeitos"][factor] = _interval(fit["beta"][i], fit["se"][i], df, log)
    try:
        x2, names2 = _design(use, interaction=True)
        fit2 = fit_ols(x2, y)
        i = names2.index("CE")
        out["interacao_exploratoria"] = _interval(
            fit2["beta"][i], fit2["se"][i], fit2["df"], log
        )
    except Singular:
        pass
    return out


def decide_factor(primary: dict | None, harms: dict[str, dict | None]) -> dict:
    """Section 6 verdict for one factor. ``primary``/``harms`` are ``{est, lo, hi}``."""
    if primary is None or primary["est"] is None or primary.get("df", MIN_DF) < MIN_DF:
        return _verdict("inconclusivo", "graus de liberdade insuficientes no primário")
    known = {n: h for n, h in harms.items() if h is not None and h["hi"] is not None}
    missing = sorted(set(harms) - set(known))
    clear_harm = [n for n, h in known.items() if h["lo"] > HARM_PP]
    harmful = [n for n, h in known.items() if h["hi"] > HARM_PP]
    reduces = primary["est"] <= MARGIN_REL and primary["hi"] < 0
    if clear_harm:
        return _verdict("nao-adotar", f"dano estabelecido: {', '.join(clear_harm)}")
    if missing:
        return _verdict("inconclusivo", f"sinal de dano ausente: {', '.join(missing)}")
    if reduces and not harmful:
        return _verdict("adotar", "redução >= 10% com IC95 sem o zero e sem dano")
    if primary["lo"] > MARGIN_REL:
        return _verdict("nao-adotar", "o IC95 exclui uma redução de 10%")
    if reduces:
        return _verdict("inconclusivo", f"dano não excluído: {', '.join(harmful)}")
    return _verdict("inconclusivo", "o IC95 não decide entre os limiares")


def _verdict(name: str, reason: str) -> dict:
    return {"veredito": name, "motivo": reason}


# --------------------------------------------------------------------------- analysis


def _kpi_wasted_pp(armed: dict[int, dict], k: int, end: dict | None) -> float | None:
    """Percent of code-mode runs that were retry pairs, from two cumulative snapshots."""
    first = (armed.get(k) or {}).get("kpi")
    last = (armed.get(k + 1) or end or {}).get("kpi")
    if not first or not last:
        return None
    runs = last["runs_total"] - first["runs_total"]
    if runs <= 0:
        return None
    wasted = last["wasted_attempts_retry_pairs"] - first["wasted_attempts_retry_pairs"]
    return 100.0 * wasted / runs


def _day_row(cal: dict, k: int, sessions: list[Sess], extras: dict) -> dict:
    block, cell = flat_days(cal)[k]
    c_on, e_on = want_flags(cell)
    calls = sum(s.calls for s in sessions)
    turns_ = sum(s.human_turns for s in sessions)

    def pp(n: int) -> float | None:
        return 100.0 * n / calls if calls else None

    compactions = [c for s in sessions for c in s.compactions]
    return {
        "day": k,
        "block": block,
        "cell": cell,
        "c": int(c_on),
        "e": int(e_on),
        "sessoes": len(sessions),
        "turnos": turns_,
        "chamadas": calls,
        "bytes_por_turno": sum(s.bytes for s in sessions) / turns_ if turns_ else None,
        "erro_pp": pp(sum(s.erro for s in sessions)),
        "retry_pp": pp(sum(s.retry for s in sessions)),
        "raw_c_pp": pp(sum(s.raw_c for s in sessions)),
        "raw_e_pp": pp(sum(s.raw_e for s in sessions)),
        "wasted_pp": extras.get("wasted_pp"),
        "compactacoes": len(compactions),
        "pos_tokens_medio": _mean(
            [c["post"] for c in compactions if _is_int(c["post"])]
        ),
        "versoes_cc": sorted({s.version for s in sessions if s.version}),
        "arquivos_compactados": extras["compacted"],
        "arquivos_evicted": extras["evicted"],
    }


def _mean(values: list) -> float | None:
    return sum(values) / len(values) if values else None


def assign_sessions(cal: dict, sessions: list[Sess], armed_at: dict[int, dt.datetime]):
    """Group sessions by the day they opened; drop the ones a flag change can reach."""
    kept: dict[int, list[Sess]] = collections.defaultdict(list)
    dropped: collections.Counter = collections.Counter()
    last = total_days(cal)
    for s in sessions:
        first, end = parse_ts(s.first_ts), parse_ts(s.last_ts)
        k = day_index(first, cal)
        if not 0 <= k < last:
            dropped["fora_da_sequencia"] += 1
        elif day_index(end, cal) != k:
            dropped["cruza_a_virada"] += 1
        elif k not in armed_at or first < armed_at[k]:
            dropped["aberta_antes_do_arm"] += 1
        else:
            kept[k].append(s)
    return kept, dropped


def _day_reasons(cal, k, row, recs, binaries, unfinished: bool) -> list[str]:
    """Every reason day ``k`` cannot count: the log's, plus the ones the data adds."""
    reasons = day_exclusions(cal, k, recs, binaries)
    if unfinished:
        reasons.append("em_curso")
    if row["turnos"] < MIN_TURNS:
        reasons.append("poucos_turnos")
    if not row["c"] and row["arquivos_compactados"]:
        reasons.append(
            "contaminado_c"
        )  # the daemon is global: a raw copy means C was on
    return reasons


def build_report(cal, records, bad_lines, sessions, as_of, dirs) -> dict:
    """Everything section 6 needs, as one JSON-ready dict."""
    by_day: dict[int, list[dict]] = collections.defaultdict(list)
    for rec in records:
        if rec["event"] == "arm" and _is_int(rec.get("day")):
            by_day[rec["day"]].append(rec)
    armed = {
        k: next((r for r in recs if r.get("ok")), None) for k, recs in by_day.items()
    }
    armed = {k: r for k, r in armed.items() if r}
    armed_at = {k: parse_ts(r["ts"]) for k, r in armed.items()}
    end_record = next((r for r in records if r["event"] == "reset"), None)
    kept, dropped = assign_sessions(cal, sessions, armed_at)
    binaries = block_binaries(cal, by_day)
    rows, retained = [], []
    for k in range(total_days(cal)):
        lo, hi = day_start_utc(cal, k), day_start_utc(cal, k + 1)
        extras = {
            "wasted_pp": _kpi_wasted_pp(armed, k, end_record),
            "compacted": count_new_files(dirs["compacted"], lo, hi),
            "evicted": count_new_files(dirs["evicted"], lo, hi),
        }
        row = _day_row(cal, k, kept.get(k, []), extras)
        row["excluido"] = _day_reasons(
            cal, k, row, by_day.get(k, []), binaries, hi > as_of
        )
        rows.append(row)
        if not row["excluido"]:
            retained.append(row)
    return {
        "as_of": as_of.isoformat(),
        "calendario": {
            "day0": cal["day0"],
            "semente": cal["seed"],
            "blocos": cal["blocks"],
        },
        "log": {"registros": len(records), "linhas_invalidas": bad_lines},
        "sessoes": {"lidas": len(sessions), "descartadas": dict(dropped)},
        "dias": rows,
        "dias_validos": len(retained),
        "dias_excluidos": {r["day"]: r["excluido"] for r in rows if r["excluido"]},
        "fatores": _factors(retained),
        "ressalvas": _caveats(rows, bad_lines),
    }


def _factors(retained: list[dict]) -> dict:
    primary = fit_effects(retained, "bytes_por_turno", log=True)
    harms = {
        key: fit_effects(retained, key, log=False)
        for key in ("erro_pp", "retry_pp", "wasted_pp", "raw_c_pp", "raw_e_pp")
    }
    secondary = {
        key: fit_effects(retained, key, log=False) for key in ("pos_tokens_medio",)
    }
    out = {}
    for factor in ("C", "E"):
        prim = primary["efeitos"][factor] | {"df": primary["df"]} if primary else None
        factor_harms = {}
        for key in HARM_METRICS[factor]:
            fit = harms[key]
            factor_harms[key] = (
                fit["efeitos"][factor] if fit and fit["df"] >= 1 else None
            )
        out[factor] = {
            "primario": prim,
            "danos": factor_harms,
            "decisao": decide_factor(prim, factor_harms),
        }
    out["n"] = None if primary is None else primary["n"]
    out["df"] = None if primary is None else primary["df"]
    out["interacao_exploratoria"] = (
        None if primary is None else primary["interacao_exploratoria"]
    )
    out["secundario_pos_tokens"] = secondary["pos_tokens_medio"]
    return out


def _caveats(rows: list[dict], bad_lines: int) -> list[str]:
    notes = []
    if bad_lines:
        notes.append(f"{bad_lines} linha(s) do log de arm inválidas, ignoradas")
    for r in rows:
        if r["c"] and not r["arquivos_compactados"] and "em_curso" not in r["excluido"]:
            notes.append(
                f"dia {r['day']}: C ligado e nenhum arquivo compactado (tratamento não disparou)"
            )
        if r["e"] and not r["arquivos_evicted"] and "em_curso" not in r["excluido"]:
            notes.append(
                f"dia {r['day']}: E ligado e nenhum arquivo evicted (tratamento não disparou)"
            )
        if not r["e"] and r["arquivos_evicted"]:
            notes.append(
                f"dia {r['day']}: E desligado e {r['arquivos_evicted']} arquivo(s) evicted (sessão antiga?)"
            )
    return notes


def _num(value, digits: int = 1) -> str:
    return "—" if value is None else f"{value:.{digits}f}".replace(".", ",")


def _pct(value) -> str:
    return "—" if value is None else f"{100 * value:+.1f}%".replace(".", ",")


def render_text(rep: dict) -> str:
    """Compact pt-BR summary for a terminal (the JSON stays the full record)."""
    lines = [
        (
            f"AB34 | as_of {rep['as_of']} | semente {rep['calendario']['semente']} | "
            f"dias válidos {rep['dias_validos']}/{len(rep['dias'])}"
        ),
    ]
    for r in rep["dias"]:
        why = f" EXCLUÍDO {','.join(r['excluido'])}" if r["excluido"] else ""
        lines.append(
            f"dia {r['day']} bloco {r['block']} {r['cell']} turnos={r['turnos']} "
            f"bytes/turno={_num(r['bytes_por_turno'], 0)}{why}"
        )
    for factor in ("C", "E"):
        f = rep["fatores"][factor]
        prim = f["primario"] or {}
        lines.append(
            f"fator {factor}: efeito {_pct(prim.get('est'))} IC95 [{_pct(prim.get('lo'))}; {_pct(prim.get('hi'))}] "
            f"-> {f['decisao']['veredito']} ({f['decisao']['motivo']})"
        )
    lines += [f"ressalva: {note}" for note in rep["ressalvas"]]
    return "\n".join(lines)


def cmd_analyze(args, cal: dict, _host, now: dt.datetime) -> int:
    if cal["day0"] is None:
        raise Refusal("experimento não ativado (day0 = null): nada a analisar")
    as_of = parse_ts(args.as_of) if args.as_of else now
    dirs = (
        [d for d in args.dirs.split(",") if d] if args.dirs else cal["transcript_dirs"]
    )
    floor = day_start_utc(cal, 0).timestamp()
    files = [f for f in m.corpus(dirs, args.sessions) if os.path.getmtime(f) >= floor]
    sessions = [s for s in (fold_session(f) for f in files) if s is not None]
    records, bad = read_log(args.log)
    paths = {"compacted": args.compacted_dir, "evicted": args.evicted_dir}
    report = build_report(cal, records, bad, sessions, as_of, paths)
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=1))
    else:
        print(render_text(report))
    return 0


# ------------------------------------------------------------------------------- CLI


def build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--calendar", default=CALENDAR_PATH, help="caminho do ab34-calendario.json"
    )
    common.add_argument("--settings", default=DEFAULT_SETTINGS)
    common.add_argument("--log", default=DEFAULT_LOG, help="JSONL do arm/reset")
    ap = argparse.ArgumentParser(
        prog="ab34_arm.py", description=__doc__.splitlines()[0]
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser(
        "status", parents=[common], help="célula de hoje e estado das duas flags"
    )
    for name, text in (
        ("arm", "arma a célula do dia"),
        ("reset", "fim: as duas flags desligadas"),
    ):
        p = sub.add_parser(name, parents=[common], help=text)
        p.add_argument(
            "--yes", action="store_true", help="grava de verdade (default: dry-run)"
        )
        if name == "arm":
            p.add_argument(
                "--force-cell", default="", help="pré-voo: célula fora do calendário"
            )
    cal_p = sub.add_parser(
        "calendar", parents=[common], help="calendário de uma semente"
    )
    cal_p.add_argument("--seed", type=int, required=True)
    cal_p.add_argument("--day0", default=None)
    cal_p.add_argument("--out", default="")
    an = sub.add_parser(
        "analyze", parents=[common], help="análise pré-registrada (seção 6)"
    )
    an.add_argument(
        "--dirs",
        default="",
        help="pastas em ~/.claude/projects (default: o calendário)",
    )
    an.add_argument("--sessions", type=int, default=2000)
    an.add_argument(
        "--as-of", default="", help="instante de referência ISO UTC (default: agora)"
    )
    an.add_argument("--compacted-dir", default=DEFAULT_COMPACTED)
    an.add_argument("--evicted-dir", default=DEFAULT_EVICTED)
    an.add_argument("--json", action="store_true")
    return ap


COMMANDS = {
    "status": cmd_status,
    "arm": cmd_arm,
    "reset": cmd_reset,
    "calendar": cmd_calendar,
    "analyze": cmd_analyze,
}


def main(argv=None, host=None, now: dt.datetime | None = None) -> int:
    args = build_parser().parse_args(argv)
    now = now or dt.datetime.now(dt.UTC)
    try:
        if args.cmd == "calendar":
            return cmd_calendar(args, None, None, now)
        cal = load_calendar(args.calendar)
        return COMMANDS[args.cmd](args, cal, host or Host(), now)
    except NotApplicable as exc:
        print(f"recusado: {exc}", file=sys.stderr)
        return 4
    except Refusal as exc:
        print(f"recusado: {exc}", file=sys.stderr)
        return 2
    except Exception as exc:  # noqa: BLE001 - the CLI contract is "exit 1, never a traceback"
        print(f"erro inesperado: {exc!r}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
