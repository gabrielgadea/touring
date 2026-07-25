# D43 — Modernization (F4.4)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_4_modernization`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-lang/rust` (editions) · `/rust-lang/cargo` (cargo fix) · jscodeshift

## Definition

Avalia adoção de features modernas da linguagem: edition atual (2021/2024), construções recentes (`let-else`, `if let` chains, const generics, `async fn` in traits, GATs onde aplicável), e migração de padrões legados. Modernizar reduz código boilerplate e melhora clareza/performance.

## Why it matters

Features novas frequentemente eliminam boilerplate e pegadinhas (ex.: `let-else` substitui o padrão `match { None => return }`). Ficar em edition/idioms antigos acumula débito e perde otimizações do compilador. Modernização incremental mantém o código no estado da arte.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | edition atual, features modernas |
| 0.5–0.8 | ⚠ Warn | padrões legados substituíveis |
| <0.5 | ❌ Fail | edition antiga / código datado |

## MUST

```bash
touring-quality check --gate F4.4 --target <FILE>
touring-quality score <FILE> --dims F4.4 --format json
```

## SHOULD

```bash
cargo fix --edition                                     # migração mecânica de edition
cargo clippy                                            # clippy sugere modernizações (uninlined_format_args, etc.)
touring ast rust-semantic <FILE>                        # features em uso
```

## MAY

```bash
touring memory recall "quality:F4.4"
```

## Elite best practices (context7 — `/rust-lang/cargo`)

1. **`cargo fix --edition` para migração mecânica** — ferramenta oficial migra a edition automaticamente e de forma auditável; rodar por edition (2018→2021→2024). Fonte: `/rust-lang/cargo` (edition migration).
2. **`let-else` para early-return** — `let Some(x) = opt else { return Err(...) };` substitui `match`/`if let` aninhado. Fonte: Rust 1.65+.
3. **Inline format args** — `format!("{x}")` em vez de `format!("{}", x)`; clippy `uninlined_format_args`. Fonte: clippy + Rust 2021.
4. **`async fn` em traits (Rust 1.75+)** — sem `#[async_trait]` macro onde a feature nativa serve; menos overhead. Fonte: rust async traits.
5. **Modernizar incrementalmente, validado** — cada modernização é uma mudança pequena testada (não big-bang); clippy + testes garantem equivalência. [training-data: Touring incremental refactor].

## Common pitfalls

- Ficar em edition 2015/2018 sem migrar (perde `cargo fix --edition`).
- `match opt { Some(x) => ..., None => return }` onde `let-else` é mais claro.
- `format!("{}", x)` verboso (clippy modernizaria).
- `#[async_trait]` onde async-fn-in-trait nativo serve.

## Remediation

1. `cargo fix --edition` + `cargo clippy` → modernizações sugeridas.
2. Aplicar `let-else`/inline-args/async-trait nativo via `Edit tool`; validar com testes.
3. `Edit tool --path <FILE> --operation ssr --pattern '<old_api>' --replacement '<modern_api>'` (jscodeshift; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C05/C06 EDIT**
- Dims relacionadas: D40 (idioms), D42 (deprecated), D03 (duplication)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust editions + cargo fix) — maintained by touring-quality_
