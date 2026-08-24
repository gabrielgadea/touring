#!/usr/bin/env python3
"""settings_stage.py — Gera settings.json em dois estágios para migração.

Stage A: remove hooks cujo command casa com DROP_PATTERN, remove matchers
         sem hooks restantes, e remove enabledPlugins sem diretório local.
Stage B: idêntico ao input (cópia byte-a-byte de conteúdo, re-serializado).

Uso:
  settings_stage.py --in <settings.json> --stage A|B [--out <path>]
                    [--drop-pattern <regex>] [--home <dir>] [--json]
"""

import argparse
import copy
import json
import re
import sys
from pathlib import Path

DEFAULT_DROP_PATTERN = (
    r"touring-hook"
    r"|touring-quality-block-all"
    r"|touring-process-guard"
    r"|touring pre-task-scout"
    r"|gateway-gemini"
    r"|qgis-mcp"
    r"|gitnexus"
)


def build_stage_a(
    data: dict,
    drop_rx: re.Pattern,  # type: ignore[type-arg]
    home_dir: Path,
) -> tuple[dict, dict]:
    """Retorna (data_a, summary)."""
    data_a = copy.deepcopy(data)

    dropped = 0
    kept = 0
    events: dict[str, dict[str, int]] = {}

    hooks_section = data_a.get("hooks", {})
    for event in list(hooks_section.keys()):
        entries = hooks_section[event]
        new_entries = []
        for entry in entries:
            raw_hooks = entry.get("hooks", [])
            new_hooks = []
            for h in raw_hooks:
                cmd = h.get("command") or ""
                if cmd and drop_rx.search(cmd):
                    dropped += 1
                    events.setdefault(event, {"dropped": 0, "kept": 0})
                    events[event]["dropped"] += 1
                else:
                    new_hooks.append(h)
                    kept += 1
                    events.setdefault(event, {"dropped": 0, "kept": 0})
                    events[event]["kept"] += 1
            if new_hooks:
                entry_copy = dict(entry)
                entry_copy["hooks"] = new_hooks
                new_entries.append(entry_copy)
            # else: matcher sem hooks → descartado
        if new_entries:
            hooks_section[event] = new_entries
        else:
            del hooks_section[event]

    # Remove enabledPlugins cujo diretório local não existe
    if "enabledPlugins" in data_a:
        plugins_dir = home_dir / ".claude" / "plugins"
        new_plugins: dict[str, bool] = {}
        for key, val in data_a["enabledPlugins"].items():
            # Mantém apenas plugins que têm diretório local instalado
            # OU que não têm nenhum diretório esperado (plugins de marketplace)
            candidate_dir = plugins_dir / key
            if candidate_dir.is_dir():
                # Plugin local existe: manter
                new_plugins[key] = val
            else:
                # Sem diretório local: plugin de marketplace ou @local ausente
                # Manter plugins de marketplace (sem @local no sufixo)
                # Remover apenas os marcados como @local sem diretório
                provider = key.split("@")[-1] if "@" in key else ""
                if provider.lower() != "local":
                    new_plugins[key] = val
                # @local sem dir: descartado
        data_a["enabledPlugins"] = new_plugins

    summary = {"dropped": dropped, "kept": kept, "events": events}
    return data_a, summary


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Gera settings.json em dois estágios (A=sem hooks Touring, B=original)."
    )
    parser.add_argument("--in", dest="input", required=True, help="settings.json de entrada")
    parser.add_argument("--stage", required=True, choices=["A", "B"], help="A ou B")
    parser.add_argument("--out", default=None, help="Arquivo de saída (padrão: stdout)")
    parser.add_argument("--drop-pattern", default=DEFAULT_DROP_PATTERN,
                        help="Regex para comandos a remover no stage A")
    parser.add_argument("--home", default=str(Path.home()),
                        help="Diretório home (padrão: ~)")
    parser.add_argument("--json", dest="json_summary", action="store_true",
                        help="Imprime resumo JSON no stderr")
    args = parser.parse_args()

    input_path = Path(args.input)
    if not input_path.is_file():
        print(f"FAIL settings-stage input not found: {input_path}", file=sys.stderr)
        return 1

    try:
        with input_path.open(encoding="utf-8") as fh:
            data = json.load(fh)
    except json.JSONDecodeError as exc:
        print(f"FAIL settings-stage invalid JSON: {exc}", file=sys.stderr)
        return 1

    if args.stage == "B":
        output_data = data
        summary: dict = {"dropped": 0, "kept": -1, "events": {}}
    else:
        drop_rx = re.compile(args.drop_pattern)
        output_data, summary = build_stage_a(data, drop_rx, Path(args.home))

    serialized = json.dumps(output_data, indent=2, ensure_ascii=False)

    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(serialized + "\n", encoding="utf-8")
    else:
        print(serialized)

    if args.json_summary:
        print(json.dumps(summary, indent=2), file=sys.stderr)
    else:
        if args.stage == "A":
            print(
                f"stage A: dropped={summary['dropped']} kept={summary['kept']} "
                f"events={len(summary['events'])}",
                file=sys.stderr,
            )
        else:
            print("stage B: identical to input", file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())
