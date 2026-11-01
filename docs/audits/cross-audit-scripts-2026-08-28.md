---
type: AuditReport
title: "cross-audit /home/gabrielgadea/projects/touring/scripts"
description: "Painel cego do flow ADW cross-audit sobre scripts/ (campos description/timestamp adicionados retroativamente em 30/08 — o template do flow gerava date: sem description; corrigido na library e no spec instanciado)."
timestamp: 2026-08-28T12:00:00-03:00
flow: cross-audit (adw)
---

## PURPOSE AUDIT (FACT= por alvo)

## Auditoria — `/home/gabrielgadea/projects/touring/scripts` (TACO-cross-audit · PURPOSE AUDIT + FIX & POTENTIALIZE, propor-nunca-editar)

Nota de escopo: a evidência de wiring (`orphan_symbols=5479`, `dependency_cycles=140`, `harmonious=false`) é do workspace Rust inteiro (202511 linhas de wiring, ver doctor); `scripts/` é majoritariamente Python/Shell fora do grafo AST Rust — não mapeia para símbolos desta árvore, exceto os fixtures Rust em `touring_premium_refactor_2026/staging/` (cobertos abaixo). Os 39 itens de débito foram integralmente contabilizados (32 todo_marker + 2 suppression + 4 unimplemented + 1 skipped_test = 39, confirmado via `scan_debt.py --json`).

FACT=scripts/touring_premium_refactor_2026/ (36/39 débitos: staging/, validate_W*.py, w*.py)=fiel README.md:33 declara "arquivo histórico"/scaffold intencional
FACT=scripts/touring_premium_refactor_2026/staging/w{5,10,12,14}-*/src (TODOs Rust)=fiel README.md:33 "duplicação INTENCIONAL"; ausente de Cargo.toml (grep confirmado)
FACT=scripts/touring_premium_refactor_2026/validate_W*.py (15 stubs "not implemented")=fiel validate_W7.py:4 "Auto-generated... DO NOT edit"; 0 consumidores fora da árvore
FACT=scripts/_archive/generate_w0_premium_artifacts.py=fiel nome do dir + docstring autodeclaram artefato histórico
FACT=scripts/test_ceg_serial_gate_metrics.py:227 (type:ignore)=fiel comentário linhas 214-219 justifica supressão de falso-positivo mypy
FACT=scripts/test_update_touring.py:55 (skipif shellcheck)=fiel skip condicional a binário ausente, padrão CI portável
FACT=scripts/generate_context_mode_plan.py=desvio grava em ~/.claude/rust congelado (CLAUDE.md§1); mover OUTPUT_PATH p/ ~/projects/touring/docs
FACT=scripts/disk-watch.sh+safe-clean.sh=desvio duplicata byte-idêntica não-documentada de ~/.claude/tools/, sem symlink; trocar por symlink ao canônico
FACT=scripts/aggregate_benchmarks.py=fiel docstring bate c/ comportamento (agrega target/criterion/*.csv); CLI standalone
FACT=

## MAP+HARMONY (harmony_map)

```json
{
  "daemon_degraded": false,
  "orphan_symbols": 5479,
  "low_score_modules": 0,
  "dependency_cycles": 140,
  "harmonious": false
}

```

## DEBT (scan_debt)

```json
{
  "root": "/home/gabrielgadea/projects/touring/scripts",
  "total_debt": 39,
  "by_category": {
    "todo_marker": 32,
    "wip_marker": 0,
    "suppression": 2,
    "unimplemented": 4,
    "skipped_test": 1
  },
  "by_file": {
    "/home/gabrielgadea/projects/touring/scripts/test_ceg_serial_gate_metrics.py": [
      {
        "category": "suppression",
        "line": 227,
        "text": "in _serial_groups(list(blk[\"attrs\"]))  # type: ignore[arg-type]"
      }
    ],
    "/home/gabrielgadea/projects/touring/scripts/test_update_touring.py": [
      {
        "category": "skipped_test",
        "line": 55,
        "text": "@pytest.mark.skipif(shutil.which(\"shellcheck\") is None, reason=\"shellcheck not installed\")"
      }
    ],
    "/home/gabrielgadea/projects/touring/scripts/touring_premium_refactor_2026/plan_lib/emit_scripts.py": [
      {
        "category": "todo_marker",
        "line": 67,
        "text": "# TODO: implement real checks. Stub returns False with a clear \"not implemented\" note."
      },
      {
        "category": "todo_marker",
        "line": 402,
        "text": "# TODO: Implement real logic for wave {spec.wave} subtasks {spec.subtask_refs}."
      }
    ],
    "/home/gabrielgadea/projects/touring/scripts/touring_premium_refactor_2026/staging/w10-touring-orchestration/src/decompose.rs": [
      {
        "category": "todo_marker",
        "line": 7,
        "text": "/// TODO: real impl extracted from existing crates."
      }
    ],
    "/home/gabrielgadea/projects/touring/scripts/touring_premium_refactor_2026/staging/w10-touring-orchestration/src/rl_bridge.rs": [
      {
        "category": "todo_marker",
        "line": 7,
        "text": "/// TODO: real impl extracted from existing crates."
      }
    ],
    "/home/gabrielgadea/projects/touring/scripts/touring_premium_refactor_2026/staging/w10-touring-orchestration/src/session.rs": [
      {
        "category": "todo_marker",
        "line": 7,
        "text": "/// TODO: real im
```

## E2E PROOF (prove_invariants)

```json
{
  "project_kind": "unknown",
  "ran": false,
  "exit_zero": null,
  "detail": "no Cargo.toml / pyproject.toml / package.json — cannot auto-detect a test suite (UNVERIFIED)"
}

```
