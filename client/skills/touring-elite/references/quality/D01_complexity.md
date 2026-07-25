# D01 — Cyclomatic/Cognitive Complexity (F1.1)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.9
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_1_complexity`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: SonarQube · CodeClimate · `/rust-lang/rust-clippy` (cognitive_complexity lint)

## Definition

**Cyclomatic complexity (CC)** = nº de caminhos linearmente independentes (conta `if/else if/match arm/for/while/loop/&&/||/?`). **Cognitive complexity** penaliza aninhamento e quebra de fluxo linear (mais pesado que CC para legibilidade humana).

## Why it matters

Pesquisa empírica: ~80% dos defeitos concentram-se nos ~20% de funções mais complexas. CC alto correlaciona com bug-density e custo de manutenção. Reduzir complexidade no momento da escrita é a alavanca de qualidade mais barata.

## Thresholds

| CC | Score | Status | Action |
|----|-------|--------|--------|
| ≤ 5 | 1.0 | ✅ Pass | ideal |
| ≤ 10 | 0.8 | ✅ Pass | aceitável |
| 11–20 | 0.5 | ⚠ Warn | extrair sub-funções |
| > 20 | <0.4 | ❌ Fail | refatorar (obrigatório) |

## MUST

```bash
touring-quality check --gate F1.1 --target <FILE>
touring-quality score <FILE> --dims F1.1 --format json
```

## SHOULD

```bash
touring ast tdg <FILE>                                  # grade A+..F (6 dims, inclui complexity)
touring ast rust-semantic <FILE>                        # semantic_complexity ∈ [0,1]
# Remediação — extrair função do range complexo:
touring assist apply extract_function --file <FILE> --range L1:L2 --name <helper>
```

## MAY

```bash
touring memory recall "quality:F1.1"
```

## Elite best practices (context7)

1. **Achatar aninhamento com early-return / `let-else`** — `if let Some(x) = y else { return };` em vez de pirâmide de `if`. Reduz cognitive complexity sem mudar CC. [training-data: clippy `needless_nested_if`]
2. **Combinar guardas: `if a && b`** conta menos cognitive que `if a { if b }` aninhado. Fonte: clippy `cognitive_complexity` rationale.
3. **Extract method para CC > 10** — uma função = uma responsabilidade testável. [training-data: SonarQube CC threshold 10/15]
4. **Substituir cadeias `match`/`if-else` longas por table-dispatch** (`const TABLE: [...]` ou `HashMap`) — colapsa CC linearmente (padrão usado no próprio touring-quality META_TABLE[50]: CC 52→1).
5. **Iterator chains > loops manuais** — `.filter().map().collect()` elimina branches/mutação acumulada. [training-data: rust idioms]

## Common pitfalls

- Funções "deus" com CC 30+ que ninguém ousa tocar.
- Aninhamento profundo (`if/for/match` 4+ níveis) — cognitive dispara mesmo com CC moderado.
- Mover complexidade para um helper sem reduzi-la (só renomear o problema).

## Remediation

1. `touring ast tdg <FILE>` → identificar função grade D/F.
2. `touring assist apply extract_function` no range mais aninhado; achatar com early-return/`?`.
3. Re-score `touring-quality check --gate F1.1` → alvo ≥ 0.8.
4. `Edit tool --path <FILE> --operation assist --assist-kind extract_function --line <N>` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C04 PRE-EDIT-TRIAGE** (ast tdg STOP em D/F) + **C06 EDIT-MAJOR**
- Dims relacionadas: D02 (maintainability), D04 (SOLID), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: clippy + SonarQube) — maintained by touring-quality_
