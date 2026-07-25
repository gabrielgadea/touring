# D29 — Test Pyramid (F3.3)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_3_test_pyramid`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/microsoft/playwright` · Cypress · JUnit XML analysis

## Definition

Avalia a proporção unit/integration/E2E. A pirâmide saudável: muitos testes unitários (rápidos, isolados), menos de integração, poucos E2E (lentos, frágeis). O anti-pattern "ice-cream cone" (invertida: muitos E2E, poucos unit) gera suítes lentas e flaky.

## Why it matters

Pirâmide invertida = CI lento + flaky + feedback tardio. Unit tests dão feedback em ms e isolam a causa; E2E dão confiança de integração mas são caros e instáveis. O balanço errado degrada a velocidade e a confiança do time.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | base unit larga, topo E2E estreito |
| 0.5–0.8 | ⚠ Warn | desbalanceado |
| <0.5 | ❌ Fail | ice-cream cone (E2E-heavy) |

## MUST

```bash
touring-quality check --gate F3.3 --target <FILE>
touring-quality score <FILE> --dims F3.3 --format json
```

## SHOULD

```bash
# Rust: unit em #[cfg(test)] mod tests; integração em tests/; contar a razão
grep -rc '#\[test\]' src/ tests/                        # distribuição unit vs integração
Write tool + touring generate verify --target <Symbol> --crate <C>       # adicionar unit na base
```

## MAY

```bash
touring memory recall "quality:F3.3"
```

## Elite best practices (context7)

1. **Base unit larga, isolada e rápida** — a maioria dos casos cobertos por unit tests determinísticos (`#[cfg(test)] mod tests`), sem I/O. [training-data: test pyramid].
2. **Integração para contratos entre componentes** — `tests/` para fronteiras (DB, API), não para lógica que um unit cobriria. [training-data].
3. **E2E mínimo, só fluxos críticos** — Playwright/Cypress para os caminhos de usuário que importam; E2E são lentos e flaky, mantenha poucos e estáveis. Fonte: `/microsoft/playwright` (test isolation, auto-wait reduz flakiness).
4. **E2E hermético com auto-wait** — Playwright auto-espera elementos (sem `sleep` arbitrário) → menos flaky; isolar estado entre testes. Fonte: Playwright best practices.
5. **Evitar ice-cream cone** — se a suíte é dominada por E2E lentos, migrar lógica testável para unit. [training-data: test anti-patterns].

## Common pitfalls

- Ice-cream cone: testar tudo via E2E (lento, flaky, feedback tardio).
- E2E com `sleep(n)` em vez de auto-wait → flaky.
- Unit tests que na verdade fazem I/O (não são unit).
- Sem integração nas fronteiras reais (gap de contrato).

## Remediation

1. Medir a razão unit:integração:E2E.
2. Migrar lógica de E2E para unit; manter E2E só nos fluxos críticos (auto-wait) via `Write tool + touring generate verify`.
3. `Write tool --path tests/e2e/<flow>.spec.ts --intent "<E2E test>" --kind PlaywrightTest` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR**
- Dims relacionadas: D27 (coverage), D31 (test maint), D33 (perf tests)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /microsoft/playwright) — maintained by touring-quality_
