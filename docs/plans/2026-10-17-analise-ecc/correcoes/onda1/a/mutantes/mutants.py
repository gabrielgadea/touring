#!/usr/bin/env python3
"""mutants.py — mutation runner da onda 1A (hooks e cota).

Para cada regra nova há um CONTROLE sem mutação (a suíte inteira passa) e depois N mutantes: uma substituição
exata de texto numa CÓPIA do arquivo-alvo (o worktree nunca é mexido), a suíte apontada para a cópia por variável
de ambiente, e o mutante só conta como morto se a suíte reprova POR ASSERÇÃO (rc 1). rc 2/3/4 do pytest (coleta,
erro interno) ou `bash -n` falho = mutante inválido, que não conta. Padrão que não ocorre exatamente uma vez no
alvo = erro do runner (nunca um mutante «morto» por vácuo).

    python3 mutants.py [--check-patterns] [--only PREFIXO] [--out mutantes.json]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

WT = Path("/home/gabrielgadea/projects/touring-wt-onda1")
HERE = Path(__file__).resolve().parent
DELIVERY = HERE.parent
TESTS = WT / "client" / "hooks" / "tests"

SUITES: dict[str, dict] = {
    "cp": {"tests": [TESTS / "test_config_protection.py"], "cwd": TESTS},
    "reg": {"tests": [TESTS / "test_config_protection.py", TESTS / "test_s49_install_claude_hooks.py"], "cwd": TESTS},
    "s49": {"tests": [TESTS / "test_s49_install_claude_hooks.py"], "cwd": TESTS},
    "quota": {"tests": [TESTS / "test_status_monitor_quota.py"], "cwd": TESTS},
    "snap": {"tests": [WT / "scripts" / "test_snapshot_safety.py"], "cwd": WT / "scripts"},
    "ck": {"tests": [DELIVERY / "vivos" / "test_checkpoint_command.py"], "cwd": DELIVERY / "vivos"},
}


@dataclass
class Mutant:
    id: str
    suite: str
    target: Path
    old: str
    new: str
    why: str
    env_key: str = ""
    extra: dict = field(default_factory=dict)


CP = WT / "client/hooks/config_protection.py"
IN = WT / "scripts/install_claude_hooks.py"
SM = WT / "client/hooks/status_monitor.sh"
CC = WT / "client/hooks/check_context.sh"
SS = WT / "scripts/snapshot_safety.py"
HS = WT / "client/claude/hooks.settings.json"
CK = DELIVERY / "vivos" / "checkpoint.md"


def cp(i: str, old: str, new: str, why: str) -> Mutant:
    return Mutant(f"CP{i}", "cp", CP, old, new, why, "CONFIG_PROTECTION_HOOK")


def sm(i: str, old: str, new: str, why: str) -> Mutant:
    return Mutant(f"SM{i}", "quota", SM, old, new, why, "QUOTA_HOOKS_DIR")


def cc(i: str, old: str, new: str, why: str) -> Mutant:
    return Mutant(f"CC{i}", "quota", CC, old, new, why, "QUOTA_HOOKS_DIR")


MUTANTS: list[Mutant] = [
    # ── config_protection.py ─────────────────────────────────────────────────────────────────────────
    cp("01", 'if os.environ.get(KILL_SWITCH) == "1":', "if False:", "kill switch ignorado"),
    cp("02", '"permissionDecision": "ask"', '"permissionDecision": "deny"', "ask vira deny"),
    cp("03", "    if _is_protected_name(raw):", "    if _is_protected_name(raw) and path.exists():", "criar config deixa de perguntar"),
    cp("04", 'lower = path.replace("\\\\", "/").lower()', 'lower = path.replace("\\\\", "/")', "nome sensível a caixa"),
    cp("05", 'PROTECTED_SUFFIXES = ("/.cargo/mutants.toml",)', "PROTECTED_SUFFIXES = ()", ".cargo/mutants.toml sem proteção"),
    cp("06", '"eslint.config.", ', "", "prefixo eslint.config. removido"),
    cp("07", '"pyright", "pytest", "coverage", "bandit")', '"pyright", "pytest", "bandit")', "coverage fora das tabelas de lint"),
    cp("08", "        return _toml_semantic(before) != _toml_semantic(after)", "        return False", "pyproject nunca muda"),
    cp("09", "    except (tomllib.TOMLDecodeError, TypeError, ValueError):\n        pass", "    except (tomllib.TOMLDecodeError, TypeError, ValueError):\n        return False", "sem queda para o texto em TOML inválido"),
    cp("10", "return text.replace(old, new) if replace_all else text.replace(old, new, 1)", "return text.replace(old, new, 1)", "replace_all ignorado"),
    cp("11", "    for edit in edits:\n        pair", "    for edit in edits[:1]:\n        pair", "MultiEdit só aplica a 1ª edição"),
    cp("12", "return sorted((_allowed_lints(after) - _allowed_lints(before)).elements())", "return []", "#![allow] novo nunca detectado"),
    cp("13", 'RUST_ROOTS = frozenset({"lib.rs", "main.rs"})', 'RUST_ROOTS = frozenset({"lib.rs", "main.rs", "module.rs", "mod.rs"})', "escopo Rust alargado"),
    cp("14", 'INNER_ALLOW = re.compile(r"^[ \\t]*#!', 'INNER_ALLOW = re.compile(r"#!', "âncora de início de linha removida (menção em comentário vira allow)"),
    cp("15", 'raw = tool_input.get("file_path") or tool_input.get("notebook_path")', 'raw = tool_input.get("file_path")', "NotebookEdit sem notebook_path"),
    cp("16", "    if tool not in EDIT_TOOLS:", "    if False:", "Read de ruff.toml pergunta"),
    cp("17", "    except Exception as exc:  # noqa: BLE001", "    except ZeroDivisionError as exc:  # noqa: BLE001", "fail-open quebrado"),
    cp("18", "    if not path.is_absolute() and isinstance(cwd, str) and cwd:", "    if False:", "caminho relativo não resolvido contra o cwd"),
    cp("19", '    "flake8", "pycodestyle"', '    "pycodestyle"', "flake8 fora das seções ini de lint"),
    cp("20", "return [line for line in kept if line.strip() and not line.lstrip().startswith(comment_marks)]", "return [line for line in kept if line.strip()]", "comentário conta como mudança de régua"),
    cp("21", "mudar a régua para passar no check é o atalho que a REGRA #21 proíbe; corrija o código, ", "mudar a régua é arriscado; corrija o código, ", "razão sem a REGRA #21"),
    cp("22", "sys.stdout.write(json.dumps(verdict) + \"\\n\")", "sys.stdout.write(json.dumps(verdict, ensure_ascii=False) + \"\\n\")", "veredito não-ASCII"),
    cp("23", '"mypy.ini", ".pre-commit-config.yaml",', '"mypy.ini",', ".pre-commit-config.yaml sem proteção"),
    cp("24", 'PROTECTED_PREFIXES = (".eslintrc", "eslint.config.", ".prettierrc", "prettier.config.")', 'PROTECTED_PREFIXES = (".eslintrc", "eslint.config.", "prettier.config.")', ".prettierrc* sem proteção"),
    cp("26", "    return _content_verdict(kind, tool, tool_input, path)", "    return None", "veredito por conteúdo desligado"),
    # ── install_claude_hooks.py (L5) ────────────────────────────────────────────────────────────────
    Mutant("L501", "s49", IN, "        if event in WORKTREE_EVENTS and WORKTREE_RECORDER.search(command):", "        if False:", "L5 desligada", "INSTALL_CLAUDE_HOOKS_SCRIPT"),
    Mutant("L502", "s49", IN, "worktree-(?:create|remove)", "worktree-create", "L5 só pega create", "INSTALL_CLAUDE_HOOKS_SCRIPT"),
    Mutant("L503", "s49", IN, 'WORKTREE_EVENTS = frozenset({"WorktreeCreate", "WorktreeRemove"})', 'WORKTREE_EVENTS = frozenset({"WorktreeCreate"})', "L5 só no evento create", "INSTALL_CLAUDE_HOOKS_SCRIPT"),
    Mutant("L504", "s49", IN, 'WORKTREE_RECORDER = re.compile(r"touring-hook\\s+worktree', 'WORKTREE_RECORDER = re.compile(r"touring-hook\\s+nope', "L5 não casa o gravador", "INSTALL_CLAUDE_HOOKS_SCRIPT"),
    # ── hooks.settings.json (registro) ──────────────────────────────────────────────────────────────
    Mutant("RG01", "reg", HS, '"command": "python3 $HOME/.claude/hooks/config_protection.py",', '"command": "python3 $HOME/.claude/hooks/config_protection_x.py",', "registro aponta para outro script", "HOOKS_SETTINGS_SOURCE"),
    Mutant("RG02", "reg", HS, '"matcher": "Edit|Write|MultiEdit|NotebookEdit",\n      "hooks": [\n        {\n          "type": "command",\n          "command": "python3 $HOME/.claude/hooks/config_protection.py",', '"matcher": "Edit|Write|MultiEdit",\n      "hooks": [\n        {\n          "type": "command",\n          "command": "python3 $HOME/.claude/hooks/config_protection.py",', "NotebookEdit fora do matcher", "HOOKS_SETTINGS_SOURCE"),
    Mutant("RG03", "reg", HS, '"command": "python3 $HOME/.claude/hooks/config_protection.py",\n          "timeout": 5', '"command": "python3 $HOME/.claude/hooks/config_protection.py",\n          "timeout": 30', "timeout 30", "HOOKS_SETTINGS_SOURCE"),
    Mutant("RG04", "reg", HS, '"command": "python3 $HOME/.claude/hooks/config_protection.py",\n          "timeout": 5', '"command": "python3 $HOME/.claude/hooks/config_protection.py",\n          "if": "Edit(*.toml)",\n          "timeout": 5', "if no registro", "HOOKS_SETTINGS_SOURCE"),
    # ── status_monitor.sh ───────────────────────────────────────────────────────────────────────────
    sm("01", 'elif [ "$1" -ge "$QUOTA_WARN" ]', 'elif [ "$1" -gt "$QUOTA_WARN" ]', "fronteira amarela 70 exclusiva"),
    sm("02", 'if [ "$1" -ge "$QUOTA_ALERT" ]; then printf', 'if [ "$1" -gt "$QUOTA_ALERT" ]; then printf', "fronteira vermelha 85 exclusiva"),
    sm("03", "QUOTA_ALERT=${QUOTA_ALERT_PCT:-85}", "QUOTA_ALERT=${QUOTA_ALERT_PCT:-90}", "limiar padrão 90"),
    sm("04", "QUOTA_WARN=${QUOTA_WARN_PCT:-70}", "QUOTA_WARN=${QUOTA_WARN_PCT:-75}", "amarelo a partir de 75"),
    sm("05", 'then (if . > 100 then 100 else . end | floor) else "-" end;', "then (if . > 100 then 100 else . end | floor) else 0 end;", "ausente vira 0%"),
    sm("06", "local mins=$((($1 + 59) / 60))", "local mins=$(($1 / 60))", "contagem arredonda para baixo"),
    sm("07", ' && [ $((Q5_RESET - QUOTA_NOW)) -le 36000 ]', "", "reset implausível não é escondido"),
    sm("08", ' && [ "$Q5_RESET" -gt "$QUOTA_NOW" ]', "", "reset no passado é mostrado"),
    sm("09", '        if [ "$Q5_PCT" -ge "$QUOTA_ALERT" ]; then\n            QUOTA_KEY', '        if [ "$Q5_PCT" -ge 95 ]; then\n            QUOTA_KEY', "marcador só aos 95"),
    sm("10", '    if [[ "$Q5_PCT" =~ ^[0-9]{1,3}$ ]]; then\n        if [ "$Q5_PCT" -ge', '    if true; then\n        if [ "$Q5_PCT" -ge', "leitura ausente desarma o marcador"),
    sm("11", '            rm -f "${ALERT_DIR}/${SESSION}.quota" "${ALERT_DIR}/${SESSION}.quota.delivered"', "            :", "queda abaixo do limiar não limpa"),
    sm("12", '"${ALERT_DIR}/${SESSION}.quota" "${ALERT_DIR}/${SESSION}.quota.delivered"', '"${ALERT_DIR}/${SESSION}.quota"', "limpa o marcador mas não o selo de entrega"),
    sm("13", "printf '%s\\nresets=%s\\n' \"$Q5_PCT\" \"$QUOTA_KEY\"", "printf '%s\\n' \"$Q5_PCT\"", "marcador sem a chave da janela"),
    sm("14", "def pct: if type == \"number\" and . >= 0 then (if . > 100 then 100 else . end | floor)", "def pct: if type == \"number\" and . >= 0 then (if . > 100 then 100 else . end)", "porcentagem não é truncada"),
    sm("15", "if type == \"number\" and . >= 0 then (if . > 100 then 100 else . end | floor)", "if type == \"number\" and . >= 0 then (. | floor)", "porcentagem sem teto de 100"),
    sm("16", 'def pct: if type == "number" and . >= 0', "def pct: if . >= 0", "tipo não verificado (string vira 100%)"),
    sm("17", '$(quota_color "$Q7_PCT")7d', '$(quota_color "$Q5_PCT")7d', "7d colorido pela porcentagem de 5h"),
    sm("18", "${WIN_BADGE}${COST_SEG}${QUOTA_SEG}\"\nelif", "${WIN_BADGE}${COST_SEG}\"\nelif", "COMPACT sem o segmento"),
    sm("19", 'echo -e "${BOLD}${YELLOW}SALVE${RESET} ${BRIGHT_YELLOW}${bar}${RESET} ${BOLD}${YELLOW}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}${QUOTA_SEG}"', 'echo -e "${BOLD}${YELLOW}SALVE${RESET} ${BRIGHT_YELLOW}${bar}${RESET} ${BOLD}${YELLOW}${TOKENS_FMT}/${WINDOW_FMT} (${CONTEXT_PCT_DISPLAY})${RESET} ${DIM}│${RESET} ${BRIGHT_CYAN}${MODEL}${RESET}${WIN_BADGE}${COST_SEG}"', "SALVE sem o segmento"),
    sm("20", '        QUOTA_PARTS+=" ${DIM}↻$(fmt_countdown $((Q5_RESET - QUOTA_NOW)))${RESET}"', "        :", "contagem regressiva nunca aparece"),
    sm("21", 'QUOTA_PARTS="${QUOTA_PARTS:+$QUOTA_PARTS ${DIM}·${RESET} }$(quota_color "$Q7_PCT")7d ${Q7_PCT}%${RESET}"', 'QUOTA_PARTS="$(quota_color "$Q7_PCT")7d ${Q7_PCT}%${RESET}"', "7d substitui o 5h em vez de somar"),
    # ── check_context.sh ────────────────────────────────────────────────────────────────────────────
    cc("01", '    printf \'%s\\n\' "$key" > "$QUOTA_DELIVERED" 2>/dev/null || true', "    :", "entrega não é selada (repete a cada chamada)"),
    cc("02", '    [[ "$key" != "$done_key" ]] || return 0', '    [[ -z "$done_key" ]] || return 0', "uma vez por sessão, não por janela"),
    cc("03", "        (( left > 0 )) || return 0\n", "", "janela já zerada ainda é anunciada"),
    cc("04", '    [[ -n "$QUOTA_MSG" ]] && emit "$QUOTA_MSG"\n    exit 0', "    exit 0", "o alerta de cota depende do contexto falar"),
    cc("05", '[[ -n "$QUOTA_MSG" ]] && MSG="${MSG}"$\'\\n\\n\'"${QUOTA_MSG}"', ":", "mensagem combinada perde a cota"),
    cc("06", 'MSG="${MSG}"$\'\\n\\n\'"${QUOTA_MSG}"', 'MSG="${QUOTA_MSG}"$\'\\n\\n\'"${MSG}"', "cota antes do contexto"),
    cc("07", '    journal true "" "$SESSION" "$pct" "" "quota_5h"', "    :", "entrega sem linha no journal"),
    cc("08", "rate_limits_snapshot.py --show", "rate_limits_snapshot.py --json", "ponteiro acionável errado"),
    cc("09", '    [[ -n "$pct" ]] || return 0\n', "", "porcentagem ilegível é anunciada"),
    cc("10", 'QUOTA_FLAG="${ALERT_DIR}/${SESSION}.quota"', 'QUOTA_FLAG="${ALERT_DIR}/quota"', "marcador não é por sessão"),
    cc("11", 'clock=$(date -d "@$key" +%H:%M 2>/dev/null)', 'clock=$(date -d "@$key" +%M 2>/dev/null)', "hora do reset errada"),
    # ── snapshot_safety.py ──────────────────────────────────────────────────────────────────────────
    Mutant("SS01", "snap", SS, 'commit = git(wt, "commit-tree", tree, "-p", head, "-m", msg)', 'commit = git(wt, "commit-tree", tree, "-m", msg)', "commit sem pai", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS02", "snap", SS, 'if git(wt, "rev-parse", "--verify", "-q", ref, check=False):', "if False:", "ref existente é sobrescrita", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS03", "snap", SS, "        if oversized:\n            return \"\", oversized", "        if False:\n            return \"\", oversized", "arquivo acima do teto não recusa", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS04", "snap", SS, '        git(wt, "add", "-A", index=idx)', '        git(wt, "add", "-A")', "git add -A no ÍNDICE REAL", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS05", "snap", SS, 'if not a.prefix.startswith("safety/"):', "if False:", "prefixo fora de safety/ aceito", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS06", "snap", SS, "    if not changed:\n        return {**out, \"status\": \"COBERTO-PELO-HEAD\"}", "    if False:\n        return {**out, \"status\": \"COBERTO-PELO-HEAD\"}", "árvore limpa cria ref", "SNAPSHOT_SAFETY_SCRIPT"),
    Mutant("SS07", "snap", SS, "    if dry_run:\n        return {**out, \"status\": \"DRY-RUN\", \"ref\": ref}", "    if False:\n        return {**out, \"status\": \"DRY-RUN\", \"ref\": ref}", "dry-run grava", "SNAPSHOT_SAFETY_SCRIPT"),
    # ── checkpoint.md ───────────────────────────────────────────────────────────────────────────────
    Mutant("CK01", "ck", CK, 'if [ "$SNAP_OK" -eq 1 ] && [ "$MEM_RC" -eq 0 ]; then', "if true; then", "reward incondicional", "CHECKPOINT_MD"),
    Mutant("CK02", "ck", CK, 'then exit 1; fi\n```', 'then exit 0; fi\n```', "falha vira exit 0", "CHECKPOINT_MD"),
    Mutant("CK03", "ck", CK, "TOPIC=$(cat <<'__TOPIC__'", "TOPIC=$(cat <<__TOPIC__", "heredoc sem aspas: tópico hostil executa", "CHECKPOINT_MD"),
    Mutant("CK04", "ck", CK, "--tier semantic --tag", "--tier working --tag", "tier errado", "CHECKPOINT_MD"),
    Mutant("CK05", "ck", CK, "SLUG=$(printf '%s' \"$TOPIC\" | python3 -c", "SLUG=$(printf '%s' \"$TOPIC\" | tr -d ' ' | python3 -c 'import sys; print(sys.stdin.read())' #", "slug sem saneamento", "CHECKPOINT_MD"),
    Mutant("CK06", "ck", CK, 'if [ -z "${TOPIC//[[:space:]]/}" ]; then echo "Uso: /checkpoint <tópico>"; exit 2; fi', ":", "tópico vazio aceito", "CHECKPOINT_MD"),
    Mutant("CK07", "ck", CK, "SUMMARY=$(cat <<'__RESUMO__'", "SUMMARY=$(cat <<__RESUMO__", "resumo sem aspas", "CHECKPOINT_MD"),
    Mutant("CK08", "ck", CK, 'touring memory store "$KEY"', 'touring memory store "x"', "chave fixa", "CHECKPOINT_MD"),
    Mutant("CK09", "ck", CK, 'case "$SNAP_STATUS" in OK|COBERTO-PELO-HEAD) [ "$SNAP_RC" -eq 0 ] && SNAP_OK=1 ;; esac', 'SNAP_OK=1', "RECUSADO conta como snapshot feito", "CHECKPOINT_MD"),
]


def count_of(text: str, old: str) -> int:
    return text.count(old)


def check_patterns() -> int:
    bad = 0
    for m in MUTANTS:
        n = count_of(m.target.read_text(encoding="utf-8"), m.old)
        if n != 1:
            print(f"PADRAO {m.id}: ocorre {n}x em {m.target.name} -> {m.old[:70]!r}")
            bad += 1
    print(f"padrões verificados: {len(MUTANTS)}, problemas: {bad}")
    return bad


def run_suite(suite: str, env_extra: dict[str, str], timeout: int = 900) -> tuple[int, str]:
    spec = SUITES[suite]
    cmd = [sys.executable, "-m", "pytest", *map(str, spec["tests"]), "-x", "-q", "-p", "no:cacheprovider", "--no-header"]
    env = {**os.environ, "SNAPSHOT_SAFETY": str(SS), **env_extra}
    done = subprocess.run(cmd, cwd=spec["cwd"], capture_output=True, text=True, env=env, timeout=timeout, check=False)
    return done.returncode, done.stdout + done.stderr


def prepare(m: Mutant, tmp: Path) -> dict[str, str]:
    """The mutated copy and the env var that points the suite at it."""
    text = m.target.read_text(encoding="utf-8").replace(m.old, m.new, 1)
    if m.suite == "quota":
        hooks_copy = tmp / "hooks"
        hooks_copy.mkdir()
        for entry in (WT / "client" / "hooks").iterdir():
            if entry.is_file() and entry.name != m.target.name:
                (hooks_copy / entry.name).symlink_to(entry)
        mutated = hooks_copy / m.target.name
        mutated.write_text(text, encoding="utf-8")
        mutated.chmod(0o755)
        if m.target.suffix == ".sh":
            ok = subprocess.run(["bash", "-n", str(mutated)], capture_output=True, text=True, check=False)
            if ok.returncode != 0:
                raise ValueError(f"bash -n: {ok.stderr.strip()}")
        return {"QUOTA_HOOKS_DIR": str(hooks_copy)}
    mutated = tmp / m.target.name
    mutated.write_text(text, encoding="utf-8")
    mutated.chmod(0o755)
    if m.target.suffix == ".py":
        compile(text, str(mutated), "exec")
    if m.suite == "reg":
        json_ok = __import__("json").loads(text)  # noqa: F841 — an invalid JSON mutant would "die" for the wrong reason
    return {m.env_key: str(mutated)}


def first_failure(output: str) -> str:
    match = re.search(r"^FAILED (\S+)", output, re.MULTILINE)
    return match.group(1).split("::", 1)[-1] if match else ""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check-patterns", action="store_true")
    parser.add_argument("--only", default="")
    parser.add_argument("--out", default=str(HERE / "mutantes.json"))
    args = parser.parse_args()
    if check_patterns():
        return 2
    if args.check_patterns:
        return 0

    started = time.time()
    controls = {}
    for suite in sorted({m.suite for m in MUTANTS if m.id.startswith(args.only)}):
        rc, out = run_suite(suite, {})
        controls[suite] = rc
        print(f"CONTROLE {suite}: rc={rc} {out.strip().splitlines()[-1] if out.strip() else ''}")
    if any(rc != 0 for rc in controls.values()):
        print("controle reprovado: nenhum mutante é interpretável", file=sys.stderr)
        return 3

    results = []
    for m in MUTANTS:
        if not m.id.startswith(args.only):
            continue
        with tempfile.TemporaryDirectory(prefix=f"mut-{m.id}-") as td:
            try:
                env = prepare(m, Path(td))
            except (ValueError, SyntaxError) as exc:
                results.append({"id": m.id, "why": m.why, "status": "invalido", "detail": str(exc)[:200]})
                print(f"{m.id} INVALIDO ({exc})")
                continue
            rc, out = run_suite(m.suite, env)
        status = "morto" if rc == 1 else ("sobreviveu" if rc == 0 else "invalido")
        killer = first_failure(out) if status == "morto" else ""
        results.append({"id": m.id, "why": m.why, "suite": m.suite, "status": status, "rc": rc, "killer": killer})
        print(f"{m.id:6} {status:10} {m.why}  <- {killer}")
        if status == "invalido":
            print(out[-600:])

    valid = [r for r in results if r["status"] in {"morto", "sobreviveu"}]
    killed = [r for r in valid if r["status"] == "morto"]
    summary = {"controles": controls, "total": len(results), "validos": len(valid), "mortos": len(killed),
               "sobreviventes": [r["id"] for r in valid if r["status"] == "sobreviveu"],
               "invalidos": [r["id"] for r in results if r["status"] == "invalido"],
               "segundos": round(time.time() - started)}
    Path(args.out).write_text(json.dumps({"summary": summary, "results": results}, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False))
    shutil.rmtree(Path(tempfile.gettempdir()) / "pytest-of-mutants", ignore_errors=True)
    return 0 if len(killed) == len(valid) and not summary["invalidos"] else 1


if __name__ == "__main__":
    sys.exit(main())
