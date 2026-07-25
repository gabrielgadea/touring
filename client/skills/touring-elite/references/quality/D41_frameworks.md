# D41 — Framework Patterns (F4.2)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_2_frameworks`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/tokio-rs/tokio` · `/tokio-rs/axum` · framework-specific linters

## Definition

Avalia uso idiomático do(s) framework(s) adotado(s): runtime async (Tokio), web (axum/actix), ORM (sqlx/diesel), etc. Cada framework tem patterns corretos e anti-patterns conhecidos; usá-los contra o grão gera bugs sutis e perda de performance.

## Why it matters

Frameworks codificam decisões; lutar contra elas (ex.: bloquear o executor Tokio, criar runtime aninhado, segurar conexão fora do pool) reintroduz exatamente os problemas que o framework resolve. Uso idiomático = aproveitar o trabalho da comunidade.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | uso idiomático do framework |
| 0.5–0.8 | ⚠ Warn | anti-pattern de framework |
| <0.5 | ❌ Fail | luta contra o framework |

## MUST

```bash
touring-quality check --gate F4.2 --target <FILE>
touring-quality score <FILE> --dims F4.2 --format json
```

## SHOULD

```bash
touring ast rust-semantic <FILE>                        # async/trait usage no contexto do framework
cargo clippy -- -D warnings                             # alguns anti-patterns viram lint
```

## MAY

```bash
touring memory recall "quality:F4.2"
```

## Elite best practices (context7 — `/tokio-rs/tokio`)

1. **Tokio: `#[tokio::main]`/único runtime, sem aninhar** — um runtime por processo; não criar `Runtime` dentro de async (panic — ver D23). Fonte: tokio runtime.
2. **Axum: extractors + State, handlers async puros** — usar extractors tipados (`Json<T>`, `State<S>`) em vez de parsing manual; handler não bloqueia (ver D23). Fonte: `/tokio-rs/axum`.
3. **`#[tokio::test]` para testes async** — não `block_on` manual em teste; o atributo provê o runtime. Fonte: tokio.
4. **ORM: pool + queries parametrizadas** — usar o pool do framework (sqlx `PgPool`), nunca conexão global; parametrizar (segurança D13 + perf D20). [training-data: sqlx].
5. **Respeitar o lifecycle do framework** — graceful shutdown (tokio signal), middleware na ordem certa (tower layers), spawn de tasks com `tokio::spawn` não `std::thread` para trabalho async. Fonte: tokio/axum.

## Common pitfalls

- Runtime Tokio aninhado / `block_on` em handler (ver D23).
- Parsing manual de request em vez de extractor tipado.
- Conexão de DB global em vez de pool do framework.
- Ignorar graceful shutdown (perde requests em deploy).

## Remediation

1. `touring ast rust-semantic` + clippy → identificar anti-pattern de framework.
2. Refatorar para o idiom do framework via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <aligned.rs>` (framework linter; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C05/C06 EDIT**
- Dims relacionadas: D23 (I/O), D24 (concurrency), D40 (idioms)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: tokio + axum) — maintained by touring-quality_
