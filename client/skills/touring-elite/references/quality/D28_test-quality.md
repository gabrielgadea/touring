# D28 — Test Quality (F3.2)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_2_test_quality`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/sourcefrog/cargo-mutants` · Stryker · PIT

## Definition

Avalia se os testes testam **comportamento** (contrato observável) e não **implementação** (detalhes internos), e se realmente detectariam um bug. **Mutation testing** é o gold-standard: muta o código e verifica se algum teste falha — testes que não pegam mutações são teatro.

## Why it matters

Cobertura (D27) mede o que é executado; qualidade mede o que é verificado. Um teste que executa a linha mas não asserta nada útil dá falsa confiança. Mutation score revela testes que passam mesmo com o código quebrado.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | asserts de comportamento, mutantes mortos |
| 0.5–0.8 | ⚠ Warn | testes acoplados a impl / mutantes sobrevivem |
| <0.5 | ❌ Fail | testes triviais (teatro) |

## MUST

```bash
touring-quality check --gate F3.2 --target <FILE>
touring-quality score <FILE> --dims F3.2 --format json
```

## SHOULD

```bash
cargo mutants --file <FILE>                              # mutation score — mutantes sobreviventes = gaps
cargo mutants --list                                    # mutações que seriam aplicadas
```

## MAY

```bash
touring memory recall "quality:F3.2"
```

## Elite best practices (context7 — `/sourcefrog/cargo-mutants`)

1. **Mutation testing para validar a suíte** — `cargo mutants` muta operadores/retornos; mutante SOBREVIVENTE = teste que não detectaria esse bug. Alvo: matar > 80%. Fonte: cargo-mutants.
2. **Testar comportamento via API pública, não internals** — assertar o contrato observável; testes acoplados a detalhes internos quebram em todo refactor (e não pegam bugs de contrato). [training-data: Stryker/test design].
3. **Asserts específicos e significativos** — `assert_eq!(result, expected_value)` não `assert!(result.is_ok())` quando o valor importa. [training-data].
4. **Um conceito por teste, nome descritivo** — `fn rejects_empty_input()` documenta a intenção; teste focado falha apontando a causa. [training-data: testing].
5. **`--in-diff` para mutar só o que mudou** — rodar mutation só no diff no CI (mais rápido, foco no novo). Fonte: cargo-mutants (--in-diff).

## Common pitfalls

- `assert!(x.is_ok())` quando o valor retornado é o que importa (mutante sobrevive).
- Testes acoplados a campos privados/ordem interna → quebram em refactor sem pegar bug real.
- Teste sem assert (só "não-panica") = teatro de cobertura.
- Mutation score baixo escondido por cobertura alta.

## Remediation

1. `cargo mutants --file <FILE>` → listar mutantes sobreviventes.
2. Adicionar asserts de comportamento que matem os mutantes via `Write tool + touring generate verify`/`Edit tool`.
3. `Write tool --path tests/<mutation>.rs --intent "<mutation test>" --kind RustTest` ou `cargo mutants` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + REGRA #0 (teste prova funcionalidade)
- Dims relacionadas: D27 (coverage), D30 (edge cases), D31 (test maint)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo-mutants) — maintained by touring-quality_
