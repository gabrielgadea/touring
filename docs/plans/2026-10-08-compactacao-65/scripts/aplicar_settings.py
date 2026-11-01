"""Apply the 08/10/2026 compaction settings (Gabriel: "autocompactar aos 65%") to every registry.

- env: CLAUDE_AUTOCOMPACT_PCT_OVERRIDE=65; COMPACT_FOCUS and CONTEXT_THRESHOLD retired.
- Stop: auto_compact.sh retired (it told the model to type /compact, which a model cannot do,
  in the TODO-CAPS "CRITICAL" shape REGRA #20 classifies as injection; and it consumed a global
  flag shared by every live session).
- SessionStart (item 6): warm-ups and panels only on startup|resume|clear|fork; on compaction
  only what restores state (env file, code-mode contract, loop state, ANTT workflow).

Usage: python3 aplicar_settings.py [--apply]   (default is a dry run that prints the diff)
"""

import json
import os
import shutil
import sys
import time
from pathlib import Path

HOME = Path.home()
BUNDLE = Path(__file__).resolve().parents[1]
FRESH_ONLY = "startup|resume|clear|fork"
# Substrings that identify a SessionStart command which only warms up or shows a panel.
WARMUPS = ("touring-hook session-start", "rl_warmup", "session_startup_intelligence", "investigate_on_start",
           "scout_perpetuo_reminder", "painel_emanacao", "guidance_injection_hook", "warmup_index",
           "gpu_service_activator")
RETIRED_ENV = ("COMPACT_FOCUS", "CONTEXT_THRESHOLD")
TARGETS = (
    (HOME / ".claude/settings.json", True),
    (HOME / "projects/touring/client/claude/hooks.settings.json", False),
    (HOME / "projects/analise/.claude/settings.local.json", False),
)


def split_session_start(groups):
    """Return SessionStart groups with warm-ups moved under the fresh-session matcher."""
    fresh, always = [], []
    for grp in groups:
        if grp.get("matcher") == FRESH_ONLY:
            fresh.extend(grp.get("hooks") or [])
            continue
        for hook in grp.get("hooks") or []:
            cmd = hook.get("command", "")
            (fresh if any(w in cmd for w in WARMUPS) else always).append(hook)
    out = []
    if fresh:
        out.append({"matcher": FRESH_ONLY, "hooks": fresh})
    if always:
        out.append({"hooks": always})
    return out


def transform(cfg, is_user_settings):
    hooks = cfg.get("hooks", cfg) if "hooks" in cfg else cfg
    if "SessionStart" in hooks:
        hooks["SessionStart"] = split_session_start(hooks["SessionStart"])
    if "Stop" in hooks:
        hooks["Stop"] = [g for g in hooks["Stop"]
                         if not any("auto_compact.sh" in h.get("command", "") for h in g.get("hooks") or [])]
    if is_user_settings:
        env = cfg.setdefault("env", {})
        for key in RETIRED_ENV:
            env.pop(key, None)
        env["CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"] = "65"
    return cfg


def summary(cfg):
    hooks = cfg.get("hooks", cfg) if "hooks" in cfg else cfg
    lines = []
    for grp in hooks.get("SessionStart") or []:
        names = [Path(h.get("command", "?").split()[-1]).name for h in grp.get("hooks") or []]
        lines.append(f"  SessionStart matcher={grp.get('matcher')!r}: {names}")
    stop = [Path(h.get("command", "?").split()[-1]).name for g in hooks.get("Stop") or [] for h in g.get("hooks") or []]
    lines.append(f"  Stop: {stop}")
    env = cfg.get("env") or {}
    lines.append(f"  env: AUTOCOMPACT={env.get('CLAUDE_AUTOCOMPACT_PCT_OVERRIDE')} "
                 f"retired_present={[k for k in RETIRED_ENV if k in env]}")
    return "\n".join(lines)


def write_atomic(path, cfg):
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(cfg, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    json.loads(tmp.read_text(encoding="utf-8"))
    os.replace(tmp, path)


def main():
    apply = "--apply" in sys.argv
    stamp = time.strftime("%Y%m%d-%H%M%S")
    for path, is_user in TARGETS:
        cfg = json.loads(path.read_text(encoding="utf-8"))
        new = transform(json.loads(json.dumps(cfg)), is_user)
        print(f"## {path}\n{summary(new)}")
        if apply and new != cfg:
            backup = BUNDLE / "settings-backup" / f"{path.name}.{stamp}"
            shutil.copy2(path, backup)
            write_atomic(path, new)
            reread = json.loads(path.read_text(encoding="utf-8"))
            print(f"  APPLIED (backup {backup.name}); reread_equal={reread == new}")


if __name__ == "__main__":
    main()
