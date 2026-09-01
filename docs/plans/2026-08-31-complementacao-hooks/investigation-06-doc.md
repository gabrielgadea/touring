---
type: Investigation
title: "F1 — Root cause do 06_documentation FAIL (composite 0.7798)"
description: "Investiga por que o gate 06_documentation FAIL no elite_aggregate. Causa: gen_reference.py --validate reporta DRIFT em docs/reference/hooks.md e docs/reference/modules.md — o gate bloqueia porque os arquivos generated estão out of sync com o source."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
tags: [investigation, root-cause, gate-06, documentation-drift]
timestamp: "2026-08-31T20:08:00-03:00"
---

# F1 — Root cause do 06_documentation FAIL

## TL;DR

O gate `06_documentation` no `docs/elite_aggregate.py` (linha 64) executa `docs/gen_reference.py --validate`. Esse comando retorna **DRIFT** quando os arquivos `docs/reference/{generators,mcp-tools,hooks,commands,modules}.md` ficam out-of-sync com o source Rust. O composite marca FAIL (score=0.0) quando drift é detectado.

## Investigação (passo a passo, executado)

### 1. Localizar a regra do gate

```bash
$ grep -n "06_documentation" docs/elite_aggregate.py
64:    ("06_documentation", "gen_reference.py", "block", "--validate"),
85:    "06_documentation": 1.0,
```

O gate 06 delega para `gen_reference.py --validate` (script auto-gerador de docs em `docs/reference/*.md`).

### 2. Reproduzir o FAIL

```bash
$ python3 docs/gen_reference.py --validate
DRIFT: hooks.md, modules.md out of sync - run docs/gen_reference.py
$ echo $?
0
```

(O script imprime DRIFT mas exit 0 — quem consome esse output é o elite_aggregate que trata como FAIL.)

### 3. Inspecionar o source do `gen_reference.py`

```python
# docs/gen_reference.py linha 26-27
ROOT = Path(__file__).resolve().parent.parent
REF = ROOT / "docs" / "reference"
```

Assume layout `docs/gen_reference.py` (script atual). Confere: file está em `/home/gabrielgadea/projects/touring/docs/gen_reference.py` ✅.

### 4. Identificar arquivos out-of-sync

```bash
$ ls -la docs/reference/
hooks.md
modules.md
...
```

Dois arquivos estão fora de sync: `hooks.md` e `modules.md`. Provavelmente foram gerados antes de mudanças recentes no source (post-F3, post-F1.5) e não foram regenerados.

## Root cause

**Documentos gerados em `docs/reference/` ficaram stale após as entregas F1-F4 da complementacao-hooks.** O script auto-gerador detecta a drift e o gate 06 marca FAIL por design (CI drift gate — padrão de segurança).

## Plano de fix

**Ação concreta**: rodar `python3 docs/gen_reference.py` (sem `--validate`) para regenerar os 2 arquivos.

```bash
$ python3 docs/gen_reference.py
# vai atualizar hooks.md e modules.md para refletir o estado atual do source
```

**Após o fix, re-rodar composite**:
```bash
$ python3 docs/elite_aggregate.py --check
# 06_documentation: deve passar de FAIL (0.00) para PASS (1.00)
```

## Estimativa

**Effort**: 5min (1 comando + 1 re-check).
**Risco**: baixo — o script é idempotente; a drift é benigna (auto-gerado pode ser regenerado).

## Próximo passo

F2 (15_dependencies_advisories FAIL) em paralelo.Após F1+F2 done:
- Aplicar fix do F1 (`gen_reference.py` regenerate)
- Aplicar fix do F2 (cargo-deny)
- Re-rodar composite
- Esperado: composite ≥ 0.80 Gold

---

_v1.0 — 2026-08-31 20:08 BRT | Root cause: drift em docs/reference/{hooks,modules}.md | Fix: rodar `python3 docs/gen_reference.py` | effort: 5min | risco: baixo_