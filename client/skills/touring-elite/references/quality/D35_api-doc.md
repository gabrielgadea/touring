# D35 — API Documentation (F3.9)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_9_api_doc`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/openapitools/openapi-generator` · Redoc · Stoplight

## Definition

Avalia documentação da API para consumidores: para crates, a doc rustdoc da superfície pública com exemplos; para APIs HTTP, spec OpenAPI/Swagger com endpoints, schemas de request/response, exemplos e códigos de erro. Doc de API ruim = API não-adotada.

## Why it matters

A API é tão boa quanto sua documentação para quem a consome. Endpoint sem exemplo, schema sem descrição, erro não-documentado → fricção e uso errado. Spec OpenAPI gera client SDKs, mock servers e docs interativas — multiplicador de valor.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | endpoints/schemas/exemplos documentados |
| 0.5–0.8 | ⚠ Warn | doc parcial / sem exemplos |
| <0.5 | ❌ Fail | API não-documentada |

## MUST

```bash
touring-quality check --gate F3.9 --target <FILE>
touring-quality score <FILE> --dims F3.9 --format json
```

## SHOULD

```bash
cargo doc --no-deps                                     # doc da superfície pública (crate API)
# Para HTTP: validar/gerar a partir do OpenAPI spec
# openapi-generator validate -i openapi.yaml
```

## MAY

```bash
touring memory recall "quality:F3.9"
```

## Elite best practices (context7 — `/openapitools/openapi-generator`)

1. **Spec OpenAPI como fonte da verdade** — gerar client SDKs, server stubs, mock e docs a partir do spec; um único contrato versionado. Fonte: `/openapitools/openapi-generator`.
2. **Exemplos de request/response por endpoint** — `examples` no spec; consumidor copia-cola e funciona. Fonte: OpenAPI examples.
3. **Documentar TODOS os códigos de erro** — cada status (4xx/5xx) com schema de erro e quando ocorre; consumidor trata corretamente. Fonte: OpenAPI responses.
4. **Docs interativas (Redoc/Swagger UI)** — renderizar o spec em UI navegável e testável. Fonte: Redoc/Stoplight.
5. **Para crates Rust: rustdoc com `# Examples`** — todo item público com exemplo executável (doctest — ver D34); `cargo doc` é a "OpenAPI" do crate. Fonte: rustdoc.

## Common pitfalls

- Endpoint sem exemplo (consumidor adivinha o formato).
- Erros não-documentados (consumidor não sabe tratar 4xx específico).
- Spec divergindo da implementação real (ver D38 — drift).
- Crate API pública sem exemplos no rustdoc.

## Remediation

1. `cargo doc`/OpenAPI validate → identificar endpoints/itens sem doc/exemplo.
2. Adicionar exemplos, schemas de erro, docs interativas via `Edit tool`.
3. `Write tool --path docs/api/openapi.yaml --intent "OpenAPI 3.1 spec" --kind OpenAPISpec` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C07 NEW-SYMBOL** + **C02 READ-COMPREHEND**
- Dims relacionadas: D34 (inline doc), D09 (API design), D38 (doc accuracy)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /openapitools/openapi-generator) — maintained by touring-quality_
