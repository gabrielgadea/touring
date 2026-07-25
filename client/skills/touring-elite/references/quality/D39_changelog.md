# D39 — Changelog / Migration (F3.13)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_13_changelog`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: Keep a Changelog · Conventional Commits · semantic-release

## Definition

Avalia documentação de mudanças: CHANGELOG mantido (Keep a Changelog format), breaking changes claramente marcados, versionamento semântico (SemVer), e guias de migração para mudanças incompatíveis. O changelog é o contrato de evolução com os consumidores.

## Why it matters

Consumidores precisam saber o que mudou e o que quebrou para atualizar com segurança. Breaking change não-documentado = upgrade que quebra silenciosamente em produção. SemVer + changelog tornam a evolução previsível e a confiança automatizável.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | CHANGELOG + SemVer + breaking marcado |
| 0.5–0.8 | ⚠ Warn | changelog incompleto |
| <0.5 | ❌ Fail | mudanças não-documentadas |

## MUST

```bash
touring-quality check --gate F3.13 --target <FILE>
touring-quality score <FILE> --dims F3.13 --format json
```

## SHOULD

```bash
# CHANGELOG.md é .md → Write/Edit permitido. Verificar seções [Unreleased] + Added/Changed/Fixed/Removed:
grep -iE '## \[(Unreleased|[0-9])' CHANGELOG.md
```

## MAY

```bash
touring memory recall "quality:F3.13"
```

## Elite best practices (context7)

1. **Keep a Changelog format** — seção `[Unreleased]` + por versão com `Added/Changed/Deprecated/Removed/Fixed/Security`; escrito para humanos, não dump de commits. Fonte: keepachangelog.com.
2. **SemVer rigoroso** — MAJOR = breaking, MINOR = feature compatível, PATCH = fix; o número comunica o risco do upgrade. Fonte: semver.org.
3. **Conventional Commits → changelog automático** — `feat:`/`fix:`/`feat!:` (breaking) permitem `semantic-release`/`git-cliff` gerar changelog + bump de versão automaticamente. Fonte: Conventional Commits + semantic-release.
4. **Breaking change com guia de migração** — `BREAKING CHANGE:` no commit + seção de migração no changelog (de X para Y, com exemplo). Fonte: Conventional Commits.
5. **Deprecar antes de remover** — `Deprecated` no changelog + `#[deprecated]` (D42) por um ciclo antes de `Removed`; dá tempo de migração. Fonte: Keep a Changelog + SemVer.

## Common pitfalls

- CHANGELOG = `git log` cru (ruído, não escrito para humanos).
- Breaking change sem bump MAJOR / sem guia de migração.
- Remover API sem ciclo de deprecação (ver D42).
- Esquecer de atualizar o changelog no PR (CI pode exigir entrada `[Unreleased]`).

## Remediation

1. `grep` seções → verificar formato Keep a Changelog.
2. Adicionar entradas por categoria, marcar breaking + migração, alinhar SemVer via Write.
3. `Edit tool --path CHANGELOG.md --operation free-form --content-from <changelog_entry.md>` (Keep a Changelog; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** (API change)
- Dims relacionadas: D42 (deprecated), D09 (API design), D44 (pkg mgmt)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: Keep a Changelog + Conventional Commits) — maintained by touring-quality_
