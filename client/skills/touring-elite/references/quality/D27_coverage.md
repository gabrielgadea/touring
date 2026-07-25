# D27 — Test Coverage (F3.1)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_1_coverage`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/taiki-e/cargo-llvm-cov` · Codecov · diff-cover

## Definition

Mede cobertura dos caminhos críticos por testes — não a % bruta como fetiche, mas se a lógica que importa (caminhos de erro, branches de decisão, invariantes) é exercitada. Diff-coverage (cobrir o que mudou) > % global.

## Why it matters

Código não-testado é hipótese (princípio operacional 4). Caminhos críticos sem teste são bugs esperando para shippar. Cobertura guia onde faltam testes; o alvo é cobrir o que importa, não inflar a %.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | critical paths cobertos |
| 0.5–0.8 | ⚠ Warn | gaps em branches importantes |
| <0.5 | ❌ Fail | lógica crítica não-testada |

## MUST

```bash
touring-quality check --gate F3.1 --target <FILE>
touring-quality score <FILE> --dims F3.1 --format json
```

## SHOULD

```bash
cargo llvm-cov --workspace --summary-only               # cobertura por arquivo/região
cargo llvm-cov --html                                   # relatório navegável (linhas não-cobertas)
Write tool + touring generate verify --target <Symbol> --crate <C>       # gerar test module para símbolo
```

## MAY

```bash
touring memory recall "quality:F3.1"
```

## Elite best practices (context7 — `/taiki-e/cargo-llvm-cov`)

1. **Cobrir caminhos críticos e de erro, não buscar 100%** — branches de `Result::Err`, edge boundaries, invariantes; 100% com asserts triviais é falsa confiança (ver D28). Fonte: cargo-llvm-cov + Codecov philosophy.
2. **Diff-coverage no CI** — exigir que código NOVO/MUDADO seja coberto (diff-cover), em vez de travar na % global histórica. Fonte: diff-cover.
3. **`cargo llvm-cov` (region coverage)** — mais preciso que line coverage; mede regiões/branches via LLVM instrumentation. Fonte: cargo-llvm-cov.
4. **Cobertura como sinal, não meta** — alta cobertura com testes fracos (D28) é pior que média cobertura com testes de comportamento. [training-data: testing].
5. **Excluir do denominador o que não faz sentido testar** — código gerado, `#[cfg(test)]`, FFI trivial; foca a métrica no que importa. Fonte: cargo-llvm-cov exclusions.

## Common pitfalls

- Perseguir 100% com asserts triviais (alto número, baixo valor — ver D28).
- Ignorar caminhos de erro (só testar happy path).
- Cobertura global mascarando módulo crítico 0%.
- Contar código gerado/test no denominador (distorce).

## Remediation

1. `cargo llvm-cov --html` → identificar regiões críticas não-cobertas.
2. `Write tool + touring generate verify` para o símbolo; focar branches de erro/boundary.
3. `Write tool --path tests/<uncovered>.rs --intent "<gap test>" --kind RustTest` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** (test após mudança) + REGRA #0 (teste ausente → criar)
- Dims relacionadas: D28 (test quality), D30 (edge cases), D33 (perf tests)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo-llvm-cov + Codecov) — maintained by touring-quality_
