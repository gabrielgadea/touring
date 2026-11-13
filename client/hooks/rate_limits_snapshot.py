#!/usr/bin/env python3
"""rate_limits_snapshot.py — a cota da franquia, do payload da status line para um arquivo legível.

POR QUE ISTO EXISTE
===================
O payload da status line traz `rate_limits.five_hour` e `rate_limits.seven_day`, cada um com
`used_percentage` (0–100) e `resets_at` (epoch em segundos), mas só para assinantes Pro/Max e só
depois da 1ª resposta da sessão; cada janela pode faltar sozinha
(code.claude.com/docs/en/statusline, lido em 08/10/2026). Só a status line o via: o modelo não
sabia a própria cota antes de lançar um fanout de subagentes — e a franquia de 5 h + semanal é
comum a todos os modelos e paga cada subagente, workflow e mensagem entre sessões.

ESCRITA (padrão: payload no stdin, chamado em background pelo `status_monitor.sh`)
=================================================================================
Grava `~/.claude/touring/rate_limits.json` = `{"schema": 1, "providers": {<provedor>: leitura}}`,
uma leitura por provedor (`backend_provider.provider_of` sobre o `ANTHROPIC_BASE_URL` herdado):
uma sessão irmã nunca sobrescreve a cota da Anthropic. **Ausência nunca vira zero**: payload sem
leitura não cria nem apaga nada. Leitura igual à gravada há menos de 60 s não regrava. Escrita
atômica (tmp + rename) sob `flock`, porque várias sessões alimentam o mesmo arquivo.
Nunca quebra a status line: todo erro sai em stderr com exit 0.

LEITURA
=======
`--show` → a linha da franquia em pt-BR + a IDADE da leitura, a sessão e o modelo que a capturaram e, quando
a leitura anterior era MAIOR na mesma janela (mesmo `resets_at`), o aviso «a franquia caiu de X% para Y% sem
virar a janela — confirme por uma segunda leitura» (exit 3 sem leitura) · `--json` → o arquivo (exit 3 sem
leitura) · `quota_line()` → só a linha da franquia, citada pelo `agent_model_gate` na recusa (o momento de
escolher o modelo é o momento em que a cota importa) · `show_report()` → o texto completo do `--show`.

HISTÓRICO (campo aditivo, 11/11/2026): cada leitura guarda em `previous.windows.<janela>` o ÚLTIMO VALOR
DISTINTO anterior (percentual, `resets_at`, quando, sessão). O schema segue 1 e `windows` segue sendo o
payload cru: leitores antigos não veem diferença. Queda dentro da mesma janela é sinal de leitura velha ou de
sessões com payloads defasados — nunca de franquia que desceu sozinha — e por isso é anunciada, não escondida.

ARGUMENTOS: sem argumentos lê o payload no stdin e grava (o contrato do `status_monitor.sh`, que não passa
nenhum argumento: esse caminho não passa pelo argparse e nunca falha). `--help` só imprime a ajuda; flag
desconhecida sai com 2 e não grava nada.
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import sys
import time
from collections.abc import Mapping
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))  # the hooks dir, resolved through the symlink
from backend_provider import provider_of

SCHEMA = 1
REWRITE_AFTER_S = 60
WINDOW_LABELS = {"five_hour": "5 h", "seven_day": "7 d", "spend_limit": "gasto"}
PROVIDER_LABELS = {"anthropic": "Anthropic", "kimi": "Kimi", "minimax": "MiniMax"}
NO_READING = ("sem leitura de cota registrada (o payload da status line só traz rate_limits para Pro/Max, "
              "depois da 1ª resposta da sessão)")


def snapshot_path(home: Path) -> Path:
    return home / ".claude" / "touring" / "rate_limits.json"


def read_snapshot(home: Path) -> dict[str, Any]:
    """The snapshot, or {} when missing or unreadable (a corrupt file is rewritten on the next reading)."""
    try:
        data = json.loads(snapshot_path(home).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) and isinstance(data.get("providers"), dict) else {}


def extract_windows(payload: Mapping[str, Any]) -> dict[str, dict[str, Any]] | None:
    """Every window that carries a percentage; None when the payload holds no reading at all."""
    limits = payload.get("rate_limits")
    if not isinstance(limits, dict):
        return None
    windows = {name: dict(w) for name, w in limits.items() if isinstance(w, dict) and "used_percentage" in w}
    return windows or None


def record_reading(home: Path, provider: str, payload: Mapping[str, Any], now: float) -> bool:
    """Store the payload's reading under `provider`. True when the file was (re)written."""
    windows = extract_windows(payload)
    if windows is None:
        return False
    path = snapshot_path(home)
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path.with_name(path.name + ".lock"), "w", encoding="utf-8") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        data = read_snapshot(home) or {"schema": SCHEMA, "providers": {}}
        previous = data["providers"].get(provider) or {}
        if previous.get("windows") == windows and now - previous.get("captured_epoch", 0) < REWRITE_AFTER_S:
            return False
        model = payload.get("model")
        model = model if isinstance(model, dict) else {}
        data["schema"] = SCHEMA
        record: dict[str, Any] = {
            "captured_at": datetime.fromtimestamp(now).astimezone().isoformat(timespec="seconds"),
            "captured_epoch": int(now),
            "session_id": payload.get("session_id"),
            "model_id": model.get("id"),
            "cc_version": payload.get("version"),
            "windows": windows,
        }
        history = _last_distinct(previous, windows)
        if history:
            record["previous"] = {"windows": history}
        data["providers"][provider] = record
        tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
        tmp.write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        os.replace(tmp, path)
    return True


def _pct(value: Any) -> str:
    number = float(value)
    return f"{int(number)}%" if number.is_integer() else f"{number:.1f}".replace(".", ",") + "%"


def _age(seconds: float) -> str:
    seconds = max(0, int(seconds))
    if seconds < 60:
        return f"há {seconds} s"
    return f"há {seconds // 60} min" if seconds < 3600 else f"há {seconds // 3600} h"


def _reset(resets_at: Any, now: float) -> str:
    if not isinstance(resets_at, (int, float)):
        return "sem hora de reset"
    if resets_at <= now:
        return "janela já zerou — dado velho"
    when = datetime.fromtimestamp(resets_at).astimezone()
    today = datetime.fromtimestamp(now).astimezone().date()
    return f"zera {when:%H:%M}" if when.date() == today else f"zera {when:%d/%m %H:%M}"


def _last_distinct(previous: Mapping[str, Any], windows: Mapping[str, Any]) -> dict[str, dict[str, Any]]:
    """Per window, the last DISTINCT value before the one being recorded: a changed percentage pushes the old
    reading into history; an unchanged one carries the existing history forward untouched."""
    old_windows = previous.get("windows") if isinstance(previous.get("windows"), dict) else {}
    old_history = previous.get("previous") if isinstance(previous.get("previous"), dict) else {}
    carried = old_history.get("windows") if isinstance(old_history.get("windows"), dict) else {}
    history: dict[str, dict[str, Any]] = {}
    for name, window in windows.items():
        old = old_windows.get(name)
        if isinstance(old, dict) and old.get("used_percentage") != window.get("used_percentage"):
            history[name] = {
                "used_percentage": old.get("used_percentage"),
                "resets_at": old.get("resets_at"),
                "captured_epoch": previous.get("captured_epoch"),
                "session_id": previous.get("session_id"),
            }
        elif isinstance(carried.get(name), dict):
            history[name] = carried[name]
    return history


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _clock(epoch: float, now: float) -> str:
    """A reset instant as HH:MM (today) or dd/mm HH:MM, past or future — unlike `_reset`, which judges freshness."""
    when = datetime.fromtimestamp(epoch).astimezone()
    return f"{when:%H:%M}" if when.date() == datetime.fromtimestamp(now).astimezone().date() else f"{when:%d/%m %H:%M}"


def _drop_note(label: str, current: Mapping[str, Any], earlier: Mapping[str, Any], now: float) -> str | None:
    """A warning (drop inside the SAME window) or a note (the window turned); None when nothing can be claimed."""
    before, after = earlier.get("used_percentage"), current.get("used_percentage")
    if not (_is_number(before) and _is_number(after)) or before <= after:
        return None
    reset_now, reset_then = current.get("resets_at"), earlier.get("resets_at")
    if not (_is_number(reset_now) and _is_number(reset_then)):
        return None  # without a reset time on both sides "same window" cannot be proven: say nothing
    if reset_now != reset_then:
        return (f"Nota {label}: a janela virou (reset {_clock(reset_then, now)} → {_clock(reset_now, now)}): "
                f"a queda de {_pct(before)} para {_pct(after)} é esperada.")
    if reset_now <= now:
        return None  # the window is over: line 1 already says "janela já zerou — dado velho"
    who = f", sessão {earlier['session_id']}" if earlier.get("session_id") else ""
    when = _age(now - earlier["captured_epoch"]) if _is_number(earlier.get("captured_epoch")) else "em hora desconhecida"
    return (f"AVISO {label}: a franquia caiu de {_pct(before)} para {_pct(after)} sem virar a janela — "
            f"confirme por uma segunda leitura (mesmo reset: {_reset(reset_now, now)}; leitura anterior {when}{who}).")


def show_report(home: Path, provider: str = "anthropic", now: float | None = None) -> str | None:
    """The `--show` text: the quota line, who/when captured it, and the drop warnings. None when nothing was recorded."""
    now = time.time() if now is None else now
    line = quota_line(home, provider, now)
    if line is None:
        return None
    record = read_snapshot(home)["providers"][provider]
    captured = record.get("captured_epoch")
    parts = [f"Leitura capturada {_age(now - captured)}" if _is_number(captured) else "Leitura capturada em hora desconhecida"]
    if record.get("captured_at"):
        parts[0] += f" ({record['captured_at']})"
    if record.get("session_id"):
        parts.append(f"sessão {record['session_id']}")
    if record.get("model_id"):
        parts.append(f"modelo {record['model_id']}")
    lines = [line, " · ".join(parts)]
    history = record.get("previous", {}).get("windows", {}) if isinstance(record.get("previous"), dict) else {}
    for name, window in record["windows"].items():
        earlier = history.get(name) if isinstance(history, dict) else None
        note = _drop_note(WINDOW_LABELS.get(name, name), window, earlier, now) if isinstance(earlier, dict) else None
        if note:
            lines.append(note)
    return "\n".join(lines)


def quota_line(home: Path, provider: str = "anthropic", now: float | None = None) -> str | None:
    """One pt-BR line with every recorded window of `provider`; None when nothing was recorded."""
    now = time.time() if now is None else now
    record = read_snapshot(home).get("providers", {}).get(provider)
    if not isinstance(record, dict) or not isinstance(record.get("windows"), dict) or not record["windows"]:
        return None
    parts = [f"{WINDOW_LABELS.get(name, name)} {_pct(w.get('used_percentage', 0))} ({_reset(w.get('resets_at'), now)})"
             for name, w in record["windows"].items()]
    label = PROVIDER_LABELS.get(provider, provider)
    return f"Franquia {label} (lida {_age(now - record.get('captured_epoch', now))}): " + " · ".join(parts)


def _write_from_stdin(home: Path) -> int:
    try:
        payload = json.loads(sys.stdin.buffer.read().decode("utf-8", errors="replace"))
        if isinstance(payload, dict):
            record_reading(home, provider_of(os.environ.get("ANTHROPIC_BASE_URL")), payload, time.time())
    except Exception as exc:  # noqa: BLE001 — the status line must render whatever happens here
        sys.stderr.write(f"rate_limits_snapshot: leitura não gravada ({type(exc).__name__}: {exc})\n")
    return 0


def _parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(
        prog="rate_limits_snapshot.py",
        description="A cota da franquia (payload da status line → ~/.claude/touring/rate_limits.json) e a sua leitura.",
        epilog="Sem argumentos: lê o payload da status line no stdin e grava a leitura (contrato do status_monitor.sh).",
    )
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--show", action="store_true",
                      help="a franquia em pt-BR + idade/sessão/modelo da leitura + aviso de queda sem virar a janela (exit 3 sem leitura)")
    mode.add_argument("--json", action="store_true", help="o arquivo de leituras como JSON (exit 3 sem leitura)")
    return ap


def main(argv: list[str]) -> int:
    home = Path.home()
    if not argv:  # the status line's call: stdin writer, no argparse on the path that must never fail
        return _write_from_stdin(home)
    args = _parser().parse_args(argv)
    if args.show:
        text = show_report(home)
        sys.stdout.write((text or NO_READING) + "\n")
        return 0 if text else 3
    data = read_snapshot(home)
    sys.stdout.write(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
    return 0 if data else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
