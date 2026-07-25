# D47 — CI/CD Pipeline (F4.7)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_7_cicd`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/actions/toolkit` (GitHub Actions) · `/rhysd/actionlint` · CircleCI

## Definition

Avalia o pipeline de CI/CD: gates de build/test/qualidade/segurança no PR, caching de dependências, paralelização, e automação de deploy. O CI é o enforcement automatizado de tudo que as outras dims definem — sem ele, qualidade é opcional.

## Why it matters

CI é onde os gates 50-dim viram obrigatórios (não confiança no dev). Um pipeline sem `clippy -D warnings`/testes/`cargo-deny`/quality-gate permite mergear regressões. Caching e paralelização mantêm o feedback rápido (CI lento = devs contornam). Touring já tem o gate `elite_aggregate.py` no CI.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | gates completos + cache + rápido |
| 0.5–0.8 | ⚠ Warn | gates parciais / lento |
| <0.5 | ❌ Fail | CI ausente/fraco |

## MUST

```bash
touring-quality check --gate F4.7 --target <FILE>
touring-quality score <FILE> --dims F4.7 --format json
```

## SHOULD

```bash
actionlint .github/workflows/*.yml                      # lint dos workflows (sintaxe, segurança)
# Gates esperados no CI: cargo check + clippy -D warnings + test + cargo-deny + elite_aggregate.py
```

## MAY

```bash
touring memory recall "quality:F4.7"
```

## Elite best practices (context7)

1. **Gates obrigatórios no PR** — `cargo check` + `clippy --all-targets -D warnings` + `cargo test` + `cargo deny check` + quality-gate (`elite_aggregate.py --check`); merge bloqueado se falhar. Fonte: Touring CI + GitHub Actions required checks.
2. **`actionlint` para os próprios workflows** — lint de sintaxe YAML, shell embutido (shellcheck), e expressões; pega erro de workflow antes de rodar. Fonte: `/rhysd/actionlint`.
3. **Caching de deps + sccache** — `actions/cache` para `~/.cargo` + `target/`; corta minutos de cada run. Fonte: GitHub Actions cache + Touring REGRA #12.
4. **Jobs paralelos + matrix** — lint/test/build em paralelo; matrix para múltiplas toolchains/OS quando aplicável. Fonte: GitHub Actions matrix.
5. **Least-privilege no CI** — `permissions:` mínimo por job, secrets via OIDC não long-lived tokens, pin de actions por SHA (supply-chain). Fonte: GitHub Actions security hardening.

## Common pitfalls

- CI sem `clippy -D warnings`/quality-gate (regressões passam).
- Sem caching → runs lentos → devs contornam o CI.
- Workflow com shell embutido não-lintado (actionlint pega).
- Actions de terceiros não-pinadas (supply-chain risk).

## Remediation

1. `actionlint` + revisar gates presentes.
2. Adicionar gates faltantes + cache + pin de actions no workflow (.yml = Edit permitido).
3. `Write tool --path .github/workflows/ci.yml --intent "ci: cargo build + test + clippy" --kind GitHubActions` (actionlint; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C12 SYSTEM-HEALTH**
- Dims relacionadas: D14 (CVEs/cargo-deny), D27 (coverage), D48 (deploy)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber/auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: GitHub Actions + actionlint) — maintained by touring-quality_
