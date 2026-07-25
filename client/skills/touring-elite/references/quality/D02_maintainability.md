# D02 — Maintainability (F1.2)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.9
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_2_maintainability`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: CodeClimate GPA · Sourcery · `/rust-lang/api-guidelines`

## Definition

Mede o custo cognitivo de **manter** o código: comprimento de função, qualidade de nomes (identificadores ≥ 3 chars, intenção-revelando), coesão de módulo, ausência de magic numbers. É a combinação de "fácil de ler" + "fácil de mudar com segurança".

## Why it matters

Código é lido ~10× mais do que escrito. Manutenibilidade baixa multiplica o tempo de onboarding e o risco de regressão a cada mudança. É o driver direto do GPA do CodeClimate e do throughput de um time.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.9+ | ✅ Pass | nomes claros, fns curtas |
| 0.5–0.9 | ⚠ Warn | fns longas OU nomes opacos |
| <0.5 | ❌ Fail | refatorar |

## MUST

```bash
touring-quality check --gate F1.2 --target <FILE>
touring-quality score <FILE> --dims F1.2 --format json
```

## SHOULD

```bash
touring ast rust-semantic <FILE>                        # análise profunda (assinaturas, naming)
touring ast meta <FILE> --depth summary -j              # quality_score / cognitive_score
touring ast grep --rewrite --from <x> --to <descriptive> --path <FILE>
```

## MAY

```bash
touring memory recall "quality:F1.2"
```

## Elite best practices (context7)

1. **Funções < 50 linhas, uma responsabilidade** — função longa = candidato a extract. [training-data: CodeClimate / SonarQube]
2. **Nomes intenção-revelando, sem abreviação críptica** — `elapsed_ms` não `e`; identificadores 1-2 chars só para índices triviais (`i`, `x` em closures). Fonte: `/rust-lang/api-guidelines` (naming C-WORD-ORDER, C-CASE).
3. **Magic numbers → `const` nomeado** — `const MAX_RETRIES: u8 = 3;` documenta intenção e centraliza mudança.
4. **Casing idiomático** — `snake_case` para fns/vars, `CamelCase` para tipos, `SCREAMING_SNAKE` para consts. Fonte: `/rust-lang/api-guidelines` (C-CASE). clippy enforça.
5. **Coesão > acoplamento** — agrupar dados+comportamento relacionados; evitar módulos "utils" grab-bag. [training-data: Sourcery]

## Common pitfalls

- Função de 200 linhas que "faz tudo".
- Identificadores 1-2 chars fora de loops triviais (código opaco).
- Magic numbers espalhados (mudar 1 valor exige caçar N callsites).
- Violações de casing snake/Camel (clippy warns).

## Remediation

1. `touring ast rust-semantic <FILE>` → identificar fns longas/nomes ruins.
2. Extrair helpers (`refactor-extract`), renomear (`refactor-rename`), nomear consts via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern '<short_id>' --replacement '<descriptive_name>'` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C02 READ-COMPREHEND** + **C06 EDIT-MAJOR**
- Dims relacionadas: D01 (complexity), D04 (SOLID), D34 (inline docs)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust-api-guidelines + CodeClimate) — maintained by touring-quality_
