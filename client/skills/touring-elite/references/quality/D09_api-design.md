# D09 — API Design (F1.9)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_9_api_design`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-lang/api-guidelines` · OpenAPI/Spectral · Optic

## Definition

Avalia o design da API pública: naming consistente (C-* guidelines), ergonomia (builder/`From`/`Into`), contratos de erro explícitos (`Result` + erro tipado), versionamento e evolução sem breaking-change. Para APIs HTTP: schemas, status codes, contratos de erro.

## Why it matters

A API é o contrato mais caro de mudar — consumidores dependem dela. Design ruim (naming inconsistente, erro não-tipado, falta de versionamento) gera breaking changes e fricção. Boa API é "fácil de usar certo, difícil de usar errado".

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | naming consistente, erros tipados |
| 0.5–0.8 | ⚠ Warn | inconsistências / contratos vagos |
| <0.5 | ❌ Fail | redesenhar superfície |

## MUST

```bash
touring-quality check --gate F1.9 --target <FILE>
touring-quality score <FILE> --dims F1.9 --format json
```

## SHOULD

```bash
touring ast overview <FILE> -j                          # assinaturas públicas
touring ast rust-semantic <FILE>                        # generics/trait bounds/lifetimes nas assinaturas
touring index find <ApiSymbol>                          # consumidores antes de mudar contrato
```

## MAY

```bash
touring memory recall "quality:F1.9"
```

## Elite best practices (context7)

1. **Seguir o Rust API Guidelines checklist (C-*)** — naming (C-CONV, `as_/to_/into_` conversão por custo), `#[must_use]`, traits comuns (`Debug`/`Clone`/`Display`) derivados. Fonte: `/rust-lang/api-guidelines`.
2. **Builder para construção complexa** — `Foo::builder().a(x).b(y).build()` > construtor com 8 args. Fonte: rust-api-guidelines (C-BUILDER).
3. **Erro tipado no contrato, nunca `String`** — `Result<T, FooError>` com `thiserror`; `String` esconde variantes do consumidor. [training-data: Touring RBP-03]
4. **Aceitar genérico, retornar concreto** — `fn f(x: impl AsRef<str>)` na entrada (flexível); tipo concreto no retorno (previsível). Fonte: rust-api-guidelines (C-GENERIC).
5. **HTTP: contrato de erro consistente + versionamento** — schema de erro uniforme, status codes corretos, versão na rota/header; lint com Spectral. [training-data: OpenAPI/Spectral]

## Common pitfalls

- Naming inconsistente (`get_x` vs `x()` vs `fetch_x` na mesma API).
- Retornar `Result<_, String>` (consumidor não consegue casar variantes).
- Quebrar contrato sem bump de versão / sem `#[deprecated]` (ver D42).
- Construtor com muitos parâmetros posicionais (erro fácil de trocar args).

## Remediation

1. `touring ast overview`/`rust-semantic` → auditar superfície contra C-* checklist.
2. Introduzir builder, erro tipado, `#[must_use]` via `Edit tool`; `index find` para migrar consumidores.
3. `Edit tool --path <FILE> --operation free-form --content-from <api.rs>` ou `Write tool --path docs/api/openapi.yaml --intent "OpenAPI 3.1 spec"` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Patterns 2/7)

## Cross-references

- Decision matrix: **C07 NEW-SYMBOL** + **C10 ARCHITECTURAL**
- Dims relacionadas: D04 (SOLID), D35 (API docs), D42 (deprecated)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust-api-guidelines + Spectral) — maintained by touring-quality_
