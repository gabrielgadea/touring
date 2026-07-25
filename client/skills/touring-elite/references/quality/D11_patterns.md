# D11 — Design Patterns (F1.11)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_11_patterns`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-unofficial/patterns` · Sourcery · Sourcemonitor

## Definition

Avalia o uso adequado de padrões de design idiomáticos de Rust (não os GoF transplantados de OO) e a ausência de abstrações faltantes ou de over-engineering. Rust tem seus próprios idioms: newtype, typestate, builder, RAII guards, visitor via enum+match, strategy via trait object/generic.

## Why it matters

Padrão certo no lugar certo torna o código previsível e extensível; padrão errado (ou GoF forçado em Rust) adiciona indireção sem ganho. Over-engineering (factory de factory) é tão custoso quanto under-engineering. VGP do Touring verifica símbolos antes de gerar — pattern verificado, não alucinado.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | idioms adequados |
| 0.5–0.8 | ⚠ Warn | abstração faltante OU excessiva |
| <0.5 | ❌ Fail | revisar arquitetura local |

## MUST

```bash
touring-quality check --gate F1.11 --target <FILE>
touring-quality score <FILE> --dims F1.11 --format json
```

## SHOULD

```bash
touring ast overview <FILE> -j                          # estrutura de tipos/traits
touring wiring chains                                    # relacionamentos source→sink (padrões de fluxo)
touring index find <PatternType>                        # VGP — verificar símbolo antes de aplicar pattern
```

## MAY

```bash
touring memory recall "quality:F1.11"
```

## Elite best practices (context7 — `/rust-unofficial/patterns`)

1. **Typestate para máquinas de estado em tempo de compilação** — `Builder<Unset>` → `Builder<Set>`; o compilador impede chamar `.build()` antes de configurar. Fonte: rust patterns (typestate). Usado no próprio touring-generator (Draft→Verified→Rendered→Speculated→Committed).
2. **RAII guard para recursos** — `struct Guard` que libera no `Drop`; nunca cleanup manual propenso a esquecimento. Fonte: rust patterns (RAII).
3. **Strategy via trait object/generic, não herança** — `Box<dyn Strategy>` ou `<S: Strategy>` em vez de class hierarchy. Fonte: rust patterns.
4. **Visitor via `enum` + `match` exaustivo** — o compilador força tratar todos os casos; adicionar variante quebra o build até tratar. Fonte: rust patterns (enum dispatch).
5. **Evitar GoF transplantado** — não traduzir Singleton/AbstractFactory de Java; usar `OnceCell`/módulo functions/closures idiomáticas. Fonte: rust patterns (anti-patterns).

## Common pitfalls

- GoF de OO forçado em Rust (Singleton via static mut unsafe).
- Over-engineering: trait + factory + builder onde uma função bastava.
- Abstração faltante: `match` gigante repetido que pedia trait/enum dispatch.
- `Rc<RefCell<>>` por toda parte (sinal de modelo de ownership mal-pensado).

## Remediation

1. `touring ast overview` → identificar pattern inadequado/faltante.
2. Refatorar para idiom Rust (typestate/RAII/enum-dispatch) via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <refactored.rs>` (REGRA #2 canonical workflows — aplicar padrão idiomático; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Patterns 1/2)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C06 EDIT-MAJOR**
- Dims relacionadas: D04 (SOLID), D03 (duplication), D12 (arch consistency)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /rust-unofficial/patterns) — maintained by touring-quality_
