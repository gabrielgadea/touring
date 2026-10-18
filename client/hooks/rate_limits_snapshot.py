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
`--show` → uma linha em pt-BR (exit 3 sem leitura) · `--json` → o arquivo (exit 3 sem leitura) ·
`quota_line()` → a mesma linha, citada pelo `agent_model_gate` na recusa (o momento de escolher
o modelo é o momento em que a cota importa).
"""

from __future__ import annotations

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
        data["providers"][provider] = {
            "captured_at": datetime.fromtimestamp(now).astimezone().isoformat(timespec="seconds"),
            "captured_epoch": int(now),
            "session_id": payload.get("session_id"),
            "model_id": model.get("id"),
            "cc_version": payload.get("version"),
            "windows": windows,
        }
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


def main(argv: list[str]) -> int:
    home = Path.home()
    if "--show" in argv:
        line = quota_line(home)
        sys.stdout.write((line or NO_READING) + "\n")
        return 0 if line else 3
    if "--json" in argv:
        data = read_snapshot(home)
        sys.stdout.write(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
        return 0 if data else 3
    return _write_from_stdin(home)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
