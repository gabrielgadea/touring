# D10 — Data Model (F1.10)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_10_data_model`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: sqlfluff · Prisma validate · `/rust-lang/rust` (type-driven design)

## Definition

Avalia o modelo de dados: design de schema, normalização, relacionamentos, e modelagem de domínio em tipos (newtypes, enums para estados, "make illegal states unrepresentable"). Para DB: índices, chaves, padrões de acesso.

## Why it matters

O modelo de dados é a fundação — erros nele propagam para todo o código. Modelagem fraca causa N+1, queries lentas, e estados inválidos representáveis (bugs por construção). Em Rust, o type system é a ferramenta primária: um enum bem-desenhado elimina classes inteiras de bug.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | tipos expressam invariantes |
| 0.5–0.8 | ⚠ Warn | primitive obsession / estados inválidos |
| <0.5 | ❌ Fail | remodelar |

## MUST

```bash
touring-quality check --gate F1.10 --target <FILE>
touring-quality score <FILE> --dims F1.10 --format json
```

## SHOULD

```bash
touring ast rust-semantic <FILE>                        # structs/enums/derives/relacionamentos
touring ast overview <FILE> -j
# Para SQL/migrations (.sql): lint externo (sqlfluff) + revisão de índices
```

## MAY

```bash
touring memory recall "quality:F1.10"
```

## Elite best practices (context7)

1. **Make illegal states unrepresentable** — usar `enum` para estados mutuamente exclusivos em vez de flags booleanas combinadas; o compilador garante a invariante. [training-data: rust type-driven design]
2. **Newtype contra primitive obsession** — `struct UserId(u64)`, `struct Email(String)` com validação no construtor; impede misturar IDs/strings de domínios diferentes. Fonte: rust patterns (newtype).
3. **Normalizar até a 3NF, desnormalizar conscientemente** — evitar anomalias de update; desnormalização só com justificativa de perf medida. [training-data: DB design]
4. **Índices nos padrões de acesso reais** — índice cobrindo as colunas do `WHERE`/`JOIN` mais frequentes; evitar N+1 com fetch em lote/join. Fonte: sqlfluff + pganalyze (ver D20).
5. **`Option<T>` para ausência, nunca sentinela** — `-1`/`""`/`0` como "vazio" é fonte de bug; `Option`/`enum` torna explícito. [training-data: rust idioms]

## Common pitfalls

- Booleans combinados (`is_a`, `is_b`, `is_c`) que permitem estados impossíveis.
- Primitive obsession (`String`/`u64` crus para conceitos de domínio distintos).
- Schema sem índices nos padrões de acesso → full scans / N+1.
- Sentinelas (`-1`, `""`) em vez de `Option`/`enum`.

## Remediation

1. `touring ast rust-semantic` → mapear structs/enums e estados representáveis.
2. Introduzir newtypes/enums; adicionar índices; normalizar via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <schema.rs>` (REGRA #2 canonical workflows — adicionar índice/migration; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C06 EDIT-MAJOR**
- Dims relacionadas: D20 (DB perf), D09 (API design), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust type-design + sqlfluff) — maintained by touring-quality_
