# D07 — Component Boundaries (F1.7)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_7_boundaries`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: dependency-cruiser · Structure101 · `/rust-lang/api-guidelines`

## Definition

Mede separação de responsabilidades e força de encapsulamento: tamanho da superfície `pub`, uso de `pub(crate)`/`pub(super)` para detalhes internos, coesão de módulo. Superfície `pub` excessiva = abstração vazada (detalhes internos expostos viram contrato).

## Why it matters

Cada item `pub` é um contrato que você não pode mudar sem quebrar consumidores. Minimizar a superfície dá liberdade de refatorar o interior. É a dim onde Touring lidera (wiring graph mede consumidores reais por símbolo). **Dim primária do scouter** (mapear antes de tocar).

## Thresholds

| pub count (módulo) | Score | Status | Action |
|--------------------|-------|--------|--------|
| 0–10 | 0.8–1.0 | ✅ Pass | superfície enxuta |
| 11–30 | 0.5–0.8 | ⚠ Warn | revisar pub→pub(crate) |
| > 50 | <0.4 | ❌ Fail | re-encapsular |

## MUST

```bash
touring-quality check --gate F1.7 --target <FILE>
touring-quality score <FILE> --dims F1.7 --format json
```

## SHOULD

```bash
touring ast overview <FILE> -j                          # todos os itens pub do módulo
touring wiring impact <symbol> --depth 2                # quem realmente consome (pub com 0 consumer = candidato a privado)
Edit tool --path <FILE> --operation rewrite --pattern 'pub fn' --replacement 'pub(crate) fn'
```

## MAY

```bash
touring memory recall "quality:F1.7"
```

## Elite best practices (context7)

1. **`pub(crate)` por default para internos** — `pub` só no que é API real do crate; helpers internos → `pub(crate)`/privado. Fonte: `/rust-lang/api-guidelines` (C-STRUCT-PRIVATE).
2. **Pub com 0 consumidores externos = re-encapsular ou remover** — `touring wiring impact` revela; alinha REGRA #0. [training-data: dependency-cruiser orphan rule]
3. **Módulos coesos, fronteiras explícitas** — `mod` por conceito de domínio, não por camada técnica grab-bag. Fonte: Structure101 cohesion.
4. **Re-export curado no topo** (`pub use`) — expor a API pública num único ponto (`lib.rs`), escondendo a estrutura interna de módulos. [training-data: rust facade pattern]
5. **Sealed traits para extensão controlada** — trait com método privado impede impls externas indesejadas, preservando a fronteira. Fonte: rust API guidelines (C-SEALED).

## Common pitfalls

- `pub` em helper que deveria ser `pub(crate)` → vira contrato acidental.
- Struct com todos os campos `pub` (sem invariante protegida).
- Módulo "utils"/"helpers" grab-bag sem coesão.
- Vazar tipo interno na assinatura de uma API pública.

## Remediation

1. `touring ast overview` + `wiring impact` → pub sem consumidor externo.
2. Rebaixar para `pub(crate)`/privado; curar re-exports via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <pub_crate.rs>` (REGRA #2 canonical workflows — `pub` → `pub(crate)`; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C03 SYMBOL-LOOKUP** + **C10 ARCHITECTURAL**
- Dims relacionadas: D08 (dep cycles), D04 (SOLID), D09 (API design)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scouter-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust-api-guidelines + dependency-cruiser) — maintained by touring-quality_
