# D38 — Documentation Accuracy (F3.12)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_12_doc_accuracy`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/errata-ai/vale` · codespell · `touring evolution drift`

## Definition

Avalia se a documentação **bate com a implementação**: sem doc desatualizada (drift), sem exemplos que não compilam, sem erros de prosa/ortografia, e sem referências quebradas. Doc errada é pior que doc ausente — induz ao erro com confiança.

## Why it matters

Doc que mente custa mais que doc faltante: o leitor confia e age errado. Drift entre doc e código é entropia inevitável sem enforcement. Touring tem `evolution drift` (USP) que detecta divergência doc↔código; doctests garantem exemplos corretos.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | doc == impl, exemplos compilam |
| 0.5–0.8 | ⚠ Warn | drift parcial / prosa com erros |
| <0.5 | ❌ Fail | doc desatualizada/enganosa |

## MUST

```bash
touring-quality check --gate F3.12 --target <FILE>
touring-quality score <FILE> --dims F3.12 --format json
```

## SHOULD

```bash
touring evolution drift -j                              # USP: divergência doc↔código (alert level)
cargo test --doc                                        # exemplos (doctests) compilam e passam
# Prosa: vale + codespell nos .md
```

## MAY

```bash
touring memory recall "quality:F3.12"
```

## Elite best practices (context7)

1. **`touring evolution drift` para detectar divergência** — sinal automático quando o código muda mas a doc não (none|degraded|structural). Fonte: Touring USP.
2. **Doctests: exemplos que não podem mentir** — todo exemplo no `///` compila e roda em `cargo test --doc`; se a API muda, o exemplo quebra o build (ver D34). Fonte: rustdoc.
3. **Vale para prosa-as-code** — linter de estilo/terminologia configurável (`.vale.ini`); consistência de voz e termos na doc. Fonte: `/errata-ai/vale`.
4. **codespell no CI** — pega typos em código+doc automaticamente. [training-data].
5. **Doc gerada do código quando possível** — `workspace-info`/`gen_reference` → doc derivada não diverge (vs prosa manual). Fonte: Touring sync_metrics/gen_reference.

## Common pitfalls

- Doc descrevendo comportamento antigo após refactor (drift silencioso).
- Exemplo em prosa (não doctest) que não compila mais.
- Referências/links quebrados (intra-doc links pegam isso — D34).
- Termos inconsistentes (mesma coisa, nomes diferentes na doc).

## Remediation

1. `touring evolution drift` + `cargo test --doc` → localizar divergência/exemplo quebrado.
2. Sincronizar doc com impl, converter exemplos para doctests, rodar vale/codespell via `Edit tool`/Write.
3. `Edit tool --path <FILE> --operation free-form --content-from <synced.md>` (codespell/vale; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C02 READ-COMPREHEND** + **C12 SYSTEM-HEALTH** (drift)
- Dims relacionadas: D34 (inline doc), D35 (API doc), D36 (arch doc)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned, USP drift)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /errata-ai/vale + Touring drift) — maintained by touring-quality_
