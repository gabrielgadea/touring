# D37 — README Completeness (F3.11)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_11_readme`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-lang/api-guidelines` (C-README) · common-readme · README-lint

## Definition

Avalia a completude do README: o que é, por que existe, como instalar/rodar, exemplo de uso mínimo, como contribuir/testar, licença, e badges (CI/coverage/version). O README é a primeira impressão e a porta de entrada do projeto.

## Why it matters

README é o primeiro (e às vezes único) ponto de contato. Um README incompleto custa adoção e gera issues repetidas de "como rodo isso?". Boa primeira impressão = confiança. Para crates, o `//!` do `lib.rs` vira a doc do crate.no docs.rs.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | setup + uso + exemplo + licença |
| 0.5–0.8 | ⚠ Warn | seções faltando |
| <0.5 | ❌ Fail | README ausente/mínimo |

## MUST

```bash
touring-quality check --gate F3.11 --target <FILE>
touring-quality score <FILE> --dims F3.11 --format json
```

## SHOULD

```bash
# README é .md → Write/Edit permitido. Validar seções essenciais presentes:
grep -iE '^#+ (install|usage|example|license|contribut|test)' README.md
```

## MAY

```bash
touring memory recall "quality:F3.11"
```

## Elite best practices (context7 — `/rust-lang/api-guidelines`)

1. **Seções essenciais** — título+tagline, instalação, exemplo de uso mínimo runnable, features, licença, contribuição. Fonte: common-readme structure.
2. **Crate: `#![doc = include_str!("../README.md")]`** — incluir o README como doc do crate (C-README); uma fonte para README + docs.rs, sem divergência. Fonte: `/rust-lang/api-guidelines`.
3. **Exemplo "hello world" copy-paste** — o primeiro exemplo deve funcionar colado, sem setup escondido; idealmente um doctest (não pode mentir). [training-data].
4. **Badges informativos** — CI status, versão crates.io, docs.rs, coverage, license; sinal rápido de saúde do projeto. Fonte: common-readme.
5. **Quickstart antes de detalhes** — "rodar em 30s" no topo; aprofundamento depois. Reduz fricção de avaliação. [training-data].

## Common pitfalls

- README só com o título (sem como rodar).
- Exemplo que não funciona (setup escondido / desatualizado — ver D38).
- README divergindo da doc do crate (duplicação que apodrece — usar `include_str!`).
- Sem licença (bloqueia adoção corporativa).

## Remediation

1. `grep` seções essenciais → identificar faltantes.
2. Adicionar setup/uso/exemplo/licença/badges via Write (README é .md).
3. `Edit tool --path README.md --operation free-form --content-from <new_readme.md>` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C02 READ-COMPREHEND**
- Dims relacionadas: D34 (inline doc), D35 (API doc), D39 (changelog)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust-api-guidelines + common-readme) — maintained by touring-quality_
