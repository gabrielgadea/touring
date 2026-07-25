# D05 — Technical Debt (F1.5)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_5_tech_debt`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: SonarQube SQALE · Stepsize · `/rust-unofficial/patterns`

## Definition

Quantifica débito técnico: marcadores `TODO`/`FIXME`/`HACK`/`XXX`, `allow(dead_code)`/`allow(unused)`, `unimplemented!()`/`todo!()`, e áreas caras de mudar (alto blast + baixo quality). Modelo SQALE: débito = custo estimado de remediação.

## Why it matters

Débito composto: cada atalho não-pago aumenta o custo de toda mudança futura naquela área. Quantificar (não só sentir) permite priorizar pagamento vs. juros. Alinha com REGRA #0 (potencializar): `allow(dead_code)` = código morto a integrar ou remover.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | débito baixo/rastreado |
| 0.5–0.8 | ⚠ Warn | marcadores acumulando |
| <0.5 | ❌ Fail | pagar débito |

## MUST

```bash
touring-quality check --gate F1.5 --target <FILE>
touring-quality score <FILE> --dims F1.5 --format json
```

## SHOULD

```bash
touring ast tdg <FILE>                                   # grade global (proxy de débito)
grep -rnE 'TODO|FIXME|HACK|XXX|unimplemented!|todo!|allow\(dead_code|allow\(unused' <FILE>
touring wiring orphans -j                                # REGRA #0: pub symbols sem consumidor = débito
```

## MAY

```bash
touring memory recall "quality:F1.5"
```

## Elite best practices (context7)

1. **Marcador rastreável, nunca solto** — `// TODO(issue#123): ...` com referência; TODO sem dono é débito invisível. [training-data: Stepsize/SonarQube]
2. **Quantificar com SQALE** — estimar custo de remediação por item; priorizar por juros (frequência de mudança × risco). Fonte: SonarQube SQALE model.
3. **Zero `allow(dead_code)`/`allow(unused)` permanente** — integrar ao consumidor OU remover (REGRA #0). `unimplemented!()` em prod = débito P1.
4. **Boy-scout rule** — deixar o arquivo um pouco melhor que encontrou; pagar débito incremental no caminho de mudanças relacionadas. [training-data: Clean Code]
5. **Débito = decisão consciente, documentada** — atalho aceitável só com ADR/comentário do "por quê" e condição de pagamento. Fonte: tech-debt quadrant (Fowler).

## Common pitfalls

- TODO/FIXME que viram fósseis (anos sem dono).
- `allow(dead_code)` para silenciar warning em vez de wirar/remover.
- `todo!()`/`unimplemented!()` chegando a produção (panic).
- Débito não-medido → priorização por "gut feeling".

## Remediation

1. Listar marcadores + orphans; classificar por juros.
2. Pagar: wirar dead-code (REGRA #0), implementar `todo!()`, resolver FIXME via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <refactored.rs>` (REGRA #2 canonical workflows — resolver TODOs; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + REGRA #0 (potencializar)
- Dims relacionadas: D01 (complexity), D06 (error handling), D08 (dep cycles)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: SonarQube SQALE) — maintained by touring-quality_
