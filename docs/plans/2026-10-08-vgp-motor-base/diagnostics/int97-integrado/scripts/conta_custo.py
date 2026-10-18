#!/usr/bin/env python3
"""Launcher that measures the COST of one command from the outside (phase D, pre-registered 18/10/2026).

Why: wall time of the p95 tail measures the machine (shared by 6+ sessions, 78-96 C), not the binary. Retired
user-space instructions and tree CPU time do not depend on who else wants the CPU. Nothing in the measured program
changes: the launcher forks, opens hardware counters on the CHILD's pid, releases it, waits, and records.

    conta_custo.py --out <arquivo.json> -- <comando...>

Mechanism (the same one `perf stat <cmd>` uses): the child blocks on a pipe; the parent opens one counter per hybrid
PMU on the child's pid with `disabled=1, enable_on_exec=1, inherit=1, exclude_kernel=1, exclude_hv=1`; the child is
released and `exec`s the command, which is what turns the counters on (the python between fork and exec is not
counted); `inherit` makes every descendant count, summed into the parent counter as each one exits (the motor's
pyright-langserver child is waited for in its drop). Counters: `PERF_TYPE_HARDWARE`, `config =
PERF_COUNT_HW_INSTRUCTIONS | (pmu_type << 32)` for `cpu_core` and `cpu_atom` (types read from sysfs; a machine with
only a generic `cpu` PMU uses the generic event).

Output (JSON, written atomically): instr (sum), instr_core, instr_atom, cpu_ms ((ru_utime + ru_stime) of wait4,
descendants included), wall_ms, rc, erro, plus pmus / t_enabled_ms / t_running_ms for audit. A counter that cannot be
opened, a PMU of the sum that cannot be read, an exec that failed, or a read of 0 after a successful exec give
`instr: null` and `erro: <why>` — NEVER 0 (absence is not zero). The command still runs and its stdout, stderr and
exit status pass through untouched (rc of a signal death = 128 + signal, as a shell reports it).

Exit: the child's exit status; 64 on a malformed command line (nothing ran).
"""

import contextlib
import ctypes
import json
import os
import platform
import signal
import struct
import sys
import time
from pathlib import Path

SYSFS = Path("/sys/bus/event_source/devices")
SYS_PERF_EVENT_OPEN_X86_64 = 298
PERF_TYPE_HARDWARE = 0
PERF_COUNT_HW_INSTRUCTIONS = 1
PERF_FLAG_FD_CLOEXEC = 8
PERF_FORMAT_TOTAL_TIME_ENABLED = 1
PERF_FORMAT_TOTAL_TIME_RUNNING = 2
# perf_event_attr.flags bits (include/uapi/linux/perf_event.h)
F_DISABLED = 1 << 0
F_INHERIT = 1 << 1
F_EXCLUDE_KERNEL = 1 << 5
F_EXCLUDE_HV = 1 << 6
F_ENABLE_ON_EXEC = 1 << 12
FLAGS = F_DISABLED | F_INHERIT | F_EXCLUDE_KERNEL | F_EXCLUDE_HV | F_ENABLE_ON_EXEC
PR_SET_PDEATHSIG = 1
USO = 64  # EX_USAGE


class PerfEventAttr(ctypes.Structure):
    """struct perf_event_attr up to PERF_ATTR_SIZE_VER8 (136 bytes); unused fields stay zero."""

    _fields_ = [
        ("type", ctypes.c_uint32),
        ("size", ctypes.c_uint32),
        ("config", ctypes.c_uint64),
        ("sample_period", ctypes.c_uint64),
        ("sample_type", ctypes.c_uint64),
        ("read_format", ctypes.c_uint64),
        ("flags", ctypes.c_uint64),
        ("wakeup_events", ctypes.c_uint32),
        ("bp_type", ctypes.c_uint32),
        ("config1", ctypes.c_uint64),
        ("config2", ctypes.c_uint64),
        ("branch_sample_type", ctypes.c_uint64),
        ("sample_regs_user", ctypes.c_uint64),
        ("sample_stack_user", ctypes.c_uint32),
        ("clockid", ctypes.c_int32),
        ("sample_regs_intr", ctypes.c_uint64),
        ("aux_watermark", ctypes.c_uint32),
        ("sample_max_stack", ctypes.c_uint16),
        ("reserved_2", ctypes.c_uint16),
        ("aux_sample_size", ctypes.c_uint32),
        ("reserved_3", ctypes.c_uint32),
        ("sig_data", ctypes.c_uint64),
        ("config3", ctypes.c_uint64),
    ]


class ContadorIndisponivel(Exception):
    """A counter the measurement needs could not be opened or read: the count is absent, not zero."""


def pmus(raiz: Path = SYSFS) -> list[tuple[str, int | None]]:
    """The hybrid PMUs to sum, as (name, sysfs type); `[("cpu", None)]` = the generic event on a non-hybrid CPU."""
    achadas = []
    for nome in ("cpu_core", "cpu_atom"):
        arq = raiz / nome / "type"
        if arq.is_file():
            achadas.append((nome, int(arq.read_text().strip())))
    return achadas or [("cpu", None)]


def _perf_event_open(attr: PerfEventAttr, pid: int) -> int:
    """perf_event_open(2) for `pid` on any CPU (x86_64). Raises OSError with the kernel's errno."""
    if platform.machine() != "x86_64":
        raise OSError(95, f"arquitetura {platform.machine()} sem syscall mapeada (so x86_64)")
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long
    fd = libc.syscall(
        ctypes.c_long(SYS_PERF_EVENT_OPEN_X86_64),
        ctypes.byref(attr),
        ctypes.c_int(pid),
        ctypes.c_int(-1),
        ctypes.c_int(-1),
        ctypes.c_ulong(PERF_FLAG_FD_CLOEXEC),
    )
    if fd < 0:
        e = ctypes.get_errno()
        raise OSError(e, os.strerror(e))
    return int(fd)


def abrir_contadores(pid: int) -> list[tuple[str, int]]:
    """One instruction counter per PMU on `pid`. ALL must open: a sum that silently lacks one PMU is wrong."""
    abertos: list[tuple[str, int]] = []
    for nome, tipo in pmus():
        attr = PerfEventAttr()
        attr.type = PERF_TYPE_HARDWARE
        attr.size = ctypes.sizeof(PerfEventAttr)
        attr.config = PERF_COUNT_HW_INSTRUCTIONS | ((tipo or 0) << 32)
        attr.read_format = PERF_FORMAT_TOTAL_TIME_ENABLED | PERF_FORMAT_TOTAL_TIME_RUNNING
        attr.flags = FLAGS
        try:
            abertos.append((nome, _perf_event_open(attr, pid)))
        except OSError as e:
            for _, fd in abertos:
                os.close(fd)
            raise ContadorIndisponivel(f"{nome}: perf_event_open falhou: {e.strerror or e}") from e
    return abertos


def ler_contador(fd: int) -> tuple[int, int, int]:
    """(value, time_enabled_ns, time_running_ns) — value already includes the exited descendants (inherit)."""
    buf = os.read(fd, 24)
    if len(buf) != 24:
        raise ContadorIndisponivel(f"leitura curta do contador ({len(buf)} de 24 bytes)")
    return struct.unpack("=QQQ", buf)


def cpu_ms(uso) -> float:
    """CPU time of the waited-for tree: wait4's rusage already folds in the reaped descendants."""
    return (uso.ru_utime + uso.ru_stime) * 1000.0


def codigo_de_saida(status: int) -> int:
    """Exit status of the child; a death by signal reads 128 + signal, as a shell reports it."""
    c = os.waitstatus_to_exitcode(status)
    return c if c >= 0 else 128 - c


def _filho(go_r: int, go_w: int, err_r: int, err_w: int, pai: int, cmd: list[str]) -> None:
    """Runs in the forked child; never returns (exec or _exit)."""
    codigo = 127
    try:
        os.close(go_w)
        os.close(err_r)
        # the judge's timeout kills only this launcher: take the command with it (kept across exec)
        ctypes.CDLL(None, use_errno=True).prctl(PR_SET_PDEATHSIG, signal.SIGKILL, 0, 0, 0)
        if os.getppid() != pai or os.read(go_r, 1) != b"1":
            os._exit(125)
        os.close(go_r)
        os.execvp(cmd[0], cmd)
    except (OSError, ValueError) as e:
        codigo = 126 if isinstance(e, PermissionError) else 127
        with contextlib.suppress(OSError):
            os.write(err_w, f"exec {cmd[0]}: {type(e).__name__}: {e}".encode())
    finally:
        os._exit(codigo)  # whatever happened above, the child NEVER returns into the caller's frames


def _liberar_e_esperar(pid: int, go_w: int) -> tuple[int, object, float]:
    """Releases the child (the exec turns the counters on) and reaps it; SIGINT/TERM/HUP/QUIT go on to the child."""
    antigos = {}
    for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT):
        antigos[s] = signal.signal(s, lambda sig, _f: _repassa(pid, sig))
    try:
        t0 = time.perf_counter()
        os.write(go_w, b"1")
        os.close(go_w)
        _, status, uso = os.wait4(pid, 0)
        return status, uso, (time.perf_counter() - t0) * 1000.0
    finally:
        for s, h in antigos.items():
            signal.signal(s, h)


def _contar(contadores: list[tuple[str, int]], r: dict, erros: list[str]) -> None:
    """Reads every counter into `r`. Any reading problem leaves `instr` null with the reason — never a 0."""
    try:
        lidos = {n: ler_contador(fd) for n, fd in contadores}
    except (ContadorIndisponivel, OSError) as e:
        erros.append(f"leitura do contador falhou: {e}")
        return
    total = sum(v for v, _, _ in lidos.values())
    if total <= 0:
        erros.append("o contador leu 0 instrucoes depois de um exec bem-sucedido")
        return
    r["instr"] = total
    if "cpu" not in lidos:
        r["instr_core"] = lidos.get("cpu_core", (None,))[0]
        r["instr_atom"] = lidos.get("cpu_atom", (None,))[0]
    r["t_enabled_ms"] = round(max(e for _, e, _ in lidos.values()) / 1e6, 3)
    r["t_running_ms"] = {n: round(x / 1e6, 3) for n, (_, _, x) in lidos.items()}


def executar(cmd: list[str]) -> dict:
    go_r, go_w = os.pipe()  # parent -> child: counters are armed, go
    err_r, err_w = os.pipe()  # child -> parent: why exec failed (closed by a successful exec: both are CLOEXEC)
    pai = os.getpid()
    pid = os.fork()
    if pid == 0:
        _filho(go_r, go_w, err_r, err_w, pai, cmd)
    os.close(go_r)
    os.close(err_w)
    erros: list[str] = []
    contadores: list[tuple[str, int]] = []
    try:
        contadores = abrir_contadores(pid)
    except ContadorIndisponivel as e:
        erros.append(str(e))  # the command still runs: the judge needs its output either way
    status, uso, wall = _liberar_e_esperar(pid, go_w)
    falha_exec = os.read(err_r, 4096).decode(errors="replace")
    os.close(err_r)
    r = {"instr": None, "instr_core": None, "instr_atom": None, "cpu_ms": round(cpu_ms(uso), 3),
         "wall_ms": round(wall, 3), "rc": codigo_de_saida(status), "erro": None,
         "pmus": [n for n, _ in pmus()], "t_enabled_ms": None, "t_running_ms": {}}
    if falha_exec:
        erros.append(falha_exec)  # the counters were never enabled: their 0 would be a lie
    elif contadores:
        _contar(contadores, r, erros)
    for _, fd in contadores:
        os.close(fd)
    r["erro"] = "; ".join(erros) or None
    return r


def _repassa(pid: int, sig: int) -> None:
    try:
        os.kill(pid, sig)
    except ProcessLookupError:
        pass


def _args(argv: list[str]) -> tuple[Path, list[str]] | None:
    if "--" not in argv:
        return None
    i = argv.index("--")
    cabeca, cmd = argv[:i], argv[i + 1 :]
    if len(cabeca) == 2 and cabeca[0] == "--out":
        saida = cabeca[1]
    elif len(cabeca) == 1 and cabeca[0].startswith("--out="):
        saida = cabeca[0][len("--out=") :]
    else:
        return None
    return (Path(saida), cmd) if saida and cmd else None


def main(argv: list[str] | None = None) -> int:
    ar = _args(sys.argv[1:] if argv is None else argv)
    if ar is None:
        print("uso: conta_custo.py --out <arquivo.json> -- <comando...>", file=sys.stderr)
        return USO
    saida, cmd = ar
    r = executar(cmd)
    tmp = saida.with_name(f".{saida.name}.{os.getpid()}.tmp")
    try:
        tmp.write_text(json.dumps(r, ensure_ascii=False, sort_keys=True), encoding="utf-8")
        os.replace(tmp, saida)
    except OSError as e:
        print(f"conta_custo: nao gravei {saida}: {e}", file=sys.stderr)
    return r["rc"]


if __name__ == "__main__":
    sys.exit(main())
