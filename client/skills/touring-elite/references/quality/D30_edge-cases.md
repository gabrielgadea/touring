# D30 — Edge Cases (F3.4)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_4_edge_cases`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/proptest-rs/proptest` · Hypothesis · `/rust-fuzz/cargo-fuzz`

## Definition

Avalia cobertura de casos de borda: limites (0, 1, MAX, vazio, overflow), caminhos de erro, entradas concorrentes, e inputs inesperados. Property-based testing e fuzzing exploram o espaço de input automaticamente, achando edge cases que humanos não imaginam.

## Why it matters

Bugs vivem nas bordas: off-by-one, vazio, overflow, Unicode, concorrência. Testes baseados em exemplos só cobrem o que o autor pensou. Property-based/fuzz geram milhares de inputs e encontram o contra-exemplo — "production finds the edge cases you didn't."

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | boundaries + property/fuzz |
| 0.5–0.8 | ⚠ Warn | só happy-path + alguns edge |
| <0.5 | ❌ Fail | edge cases não-testados |

## MUST

```bash
touring-quality check --gate F3.4 --target <FILE>
touring-quality score <FILE> --dims F3.4 --format json
```

## SHOULD

```bash
cargo test                                              # rodar proptest suites
cargo fuzz run <target>                                 # fuzzing (rust-fuzz) para parsers/decoders
Write tool + touring generate verify --target <Symbol> --crate <C>       # adicionar boundary tests
```

## MAY

```bash
touring memory recall "quality:F3.4"
```

## Elite best practices (context7 — `/proptest-rs/proptest`)

1. **Property-based para invariantes** — `proptest! { fn roundtrip(x in any::<T>()) { assert_eq!(decode(encode(x)), x) } }`; o framework gera casos e **shrink** ao contra-exemplo mínimo. Fonte: proptest.
2. **Testar boundaries explícitos** — 0, 1, N-1, N, MAX, vazio, negativo, Unicode multi-byte; cada `if`/limite tem um teste na borda. [training-data: boundary testing].
3. **Fuzzing para parsers/decoders/entrada não-confiável** — `cargo fuzz` (libFuzzer) acha panics/UB em código que processa bytes externos. Fonte: `/rust-fuzz/cargo-fuzz`.
4. **Testar caminhos de erro, não só sucesso** — cada `Err`/`None`/timeout tem um teste que verifica o comportamento. [training-data].
5. **Shrinking dá o caso mínimo reproduzível** — quando proptest falha, ele reduz ao menor input que ainda falha → debug trivial. Fonte: proptest shrinking.

## Common pitfalls

- Só testar o happy path (meio do range).
- Esquecer vazio/zero/overflow/Unicode.
- Não fuzzear código que processa input externo (parser → panic em prod).
- Ignorar concorrência (testar só single-thread — ver D24).

## Remediation

1. Adicionar property tests (proptest) para invariantes + boundary tests explícitos.
2. `cargo fuzz` em parsers via `Write tool + touring generate verify`.
3. `Write tool --path tests/<edge>.rs --intent "<property test>" --kind PropertyTest` (proptest; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + REGRA #0 (edge cases provam robustez)
- Dims relacionadas: D27 (coverage), D28 (test quality), D06 (error handling)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: proptest + cargo-fuzz) — maintained by touring-quality_
