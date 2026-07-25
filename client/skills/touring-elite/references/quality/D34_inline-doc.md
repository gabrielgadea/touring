# D34 — Inline Documentation (F3.8)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_8_inline_doc`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-lang/rust` (rustdoc) · Doxygen · pydocstyle

## Definition

Avalia documentação inline: doc comments (`///`/`//!`) em itens públicos, explicação de algoritmos/lógica de negócio não-óbvia, exemplos executáveis (doctests), e o princípio "comentar o porquê, não o quê". Dim do scriber.

## Why it matters

Doc inline é o contrato e a memória do código. Itens `pub` sem doc forçam o consumidor a ler a implementação. Comentários "o quê" apodrecem (o código já diz o quê); comentários "porquê" capturam a intenção que o código não expressa. Doctests são documentação que não pode mentir (compilam e rodam).

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | pub docs + porquês + doctests |
| 0.5–0.8 | ⚠ Warn | docs parciais / só "o quê" |
| <0.5 | ❌ Fail | pub sem doc |

## MUST

```bash
touring-quality check --gate F3.8 --target <FILE>
touring-quality score <FILE> --dims F3.8 --format json
```

## SHOULD

```bash
cargo doc --no-deps                                     # gerar e revisar; #![deny(missing_docs)] força cobertura
touring file-knowledge extended <FILE>                  # 23 campos, inclui doc coverage
cargo test --doc                                        # doctests compilam e passam
```

## MAY

```bash
touring memory recall "quality:F3.8"
```

## Elite best practices (context7 — `/rust-lang/rust`)

1. **`#![deny(missing_docs)]` no crate** — força doc em todo item público; o build falha sem doc. Fonte: rustdoc lints (usado no próprio Touring DOC-06).
2. **Doctests como exemplos vivos** — ` ``` ` no doc comment compila+roda em `cargo test --doc`; documentação que não pode divergir da API. Fonte: rustdoc doctests.
3. **Comentar o PORQUÊ, não o quê** — `// why: bound evita stampede (D22)` não `// incrementa i`. Código auto-explicativo dispensa "o quê". [training-data: Clean Code].
4. **Intra-doc links** — `[crate::Foo]` cria navegação cruzada na doc gerada; mantém referências corretas (quebram no build se erradas). Fonte: rustdoc intra-doc links.
5. **`# Errors`/`# Panics`/`# Safety` sections** — documentar quando retorna `Err`, quando pode `panic`, e invariantes de `unsafe`. Fonte: rust-api-guidelines (C-FAILURE).

## Common pitfalls

- Itens `pub` sem `///` (consumidor lê a impl).
- Comentários "o quê" redundantes que apodrecem.
- Doc desatualizada (ver D38) — doctests previnem.
- `unsafe` sem `# Safety` documentando a invariante.

## Remediation

1. `cargo doc` + `#![deny(missing_docs)]` → listar gaps.
2. Adicionar `///` com porquê + doctests + `# Errors/Panics/Safety` via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <documented.rs>` (rustdoc; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C02 READ-COMPREHEND** + **C07 NEW-SYMBOL**
- Dims relacionadas: D35 (API docs), D38 (doc accuracy), D09 (API design)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /rust-lang/rust rustdoc) — maintained by touring-quality_
