# D20 — Database Performance (F2.7)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_7_db_perf`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: sqlfluff · pganalyze · EverSQL

## Definition

Detecta anti-patterns de performance de DB: **N+1 queries**, índices ausentes, queries não-otimizadas, falta de connection pooling, fetch de colunas/linhas demais. "Meça antes de otimizar" (princípio operacional 8).

## Why it matters

N+1 é o silent killer: funciona em dev (10 linhas), derruba em prod (10k linhas → 10k queries). DB é frequentemente o gargalo dominante; um índice ausente transforma O(n) em full-scan O(n²).

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | queries otimizadas, índices ok |
| 0.5–0.8 | ⚠ Warn | N+1 potencial / índice faltante |
| <0.5 | ❌ Fail | otimizar |

## MUST

```bash
touring-quality check --gate F2.7 --target <FILE>
touring-quality score <FILE> --dims F2.7 --format json
```

## SHOULD

```bash
touring ast grep <FILE> '<query dentro de loop>'         # detectar N+1 estrutural
# Para SQL: lint com sqlfluff; EXPLAIN ANALYZE nas queries quentes
Edit tool --path <FILE> --operation rewrite --pattern '<loop+query>' --replacement '<batch/join>'
```

## MAY

```bash
touring memory recall "quality:F2.7"
```

## Elite best practices (context7)

1. **Eliminar N+1 com fetch em lote / JOIN** — carregar relacionamentos numa query (`WHERE id IN (...)` ou join) em vez de uma query por item do loop. Fonte: pganalyze N+1 detection.
2. **Índice cobrindo `WHERE`/`JOIN`/`ORDER BY` quentes** — validar com `EXPLAIN ANALYZE`; índice composto na ordem certa das colunas. Fonte: EverSQL/pganalyze.
3. **Selecionar só colunas necessárias** — evitar `SELECT *`; reduz I/O e permite index-only scan. Fonte: sqlfluff/pganalyze.
4. **Connection pooling + statement caching** — reusar conexões (pool dimensionado), prepared statements parametrizados (também segurança — D13). [training-data: DB perf]
5. **Paginação com keyset, não OFFSET grande** — `WHERE id > last` em vez de `OFFSET 100000` (que escaneia tudo). [training-data: pagination patterns]

## Common pitfalls

- N+1: `for item in items { db.query(item.id) }`.
- `SELECT *` + falta de índice → full scan.
- `OFFSET` grande para paginação (escaneia+descarta).
- Sem pool: abrir conexão por request.

## Remediation

1. `touring ast grep` → localizar query em loop; `EXPLAIN ANALYZE` nas quentes.
2. Reescrever para batch/join, adicionar índice via migration, configurar pool via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern 'SELECT \*' --replacement 'SELECT col1, col2'` (REGRA #2 canonical workflows — adicionar índice via migration; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + **C06 EDIT-MAJOR**
- Dims relacionadas: D10 (data model), D23 (I/O), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: sqlfluff + pganalyze) — maintained by touring-quality_
