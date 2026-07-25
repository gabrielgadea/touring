# D46 — Build Configuration (F4.6)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_6_build_config`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/rust-lang/cargo` (profiles) · cargo-bloat · cargo-llvm-lines

## Definition

Avalia a configuração de build: profiles otimizados (dev vs release), tamanho de binário, tempo de compilação, e features bem-organizadas. Build mal-configurado custa tempo de iteração (dev) e tamanho/performance (release). Alinha com REGRA #12 (disk hygiene).

## Why it matters

Build dev lento mata produtividade (cada iteração espera o compilador); binário release inchado custa deploy/cold-start. Profiles corretos (debug fino em dev, LTO+strip em release) e features enxutas são alavancas diretas de DX e footprint. Touring usa `opt-level=s`, `lto=fat`, `strip=symbols`, `panic=abort` em release.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | profiles otimizados, features enxutas |
| 0.5–0.8 | ⚠ Warn | profile subótimo / bloat |
| <0.5 | ❌ Fail | build não-configurado |

## MUST

```bash
touring-quality check --gate F4.6 --target <FILE>
touring-quality score <FILE> --dims F4.6 --format json
```

## SHOULD

```bash
cargo bloat --release --crates                          # maiores contribuintes de tamanho
cargo build --timings                                   # gargalos de tempo de compilação
cargo tree -e features                                  # features arrastando deps pesadas
```

## MAY

```bash
touring memory recall "quality:F4.6"
```

## Elite best practices (context7 — `/rust-lang/cargo`)

1. **Profile dev rápido, release otimizado** — dev: `opt-level=0`, `debug="line-tables-only"`, `incremental` conforme sccache; release: `lto="fat"`, `codegen-units=1`, `strip="symbols"`, `panic="abort"`. Fonte: cargo profiles + Touring REGRA #12.
2. **`debug=false` para deps externas em dev** — `[profile.dev.package."*"] debug=false opt-level=2`: deps compiladas otimizadas+sem símbolos, seu código com debug fino. Fonte: cargo profile overrides + Touring disk-hygiene.
3. **`cargo bloat`/`cargo-llvm-lines` para tamanho** — identificar crates/genéricos que inflam o binário; mono-morphização excessiva é causa comum. Fonte: cargo-bloat.
4. **Features mínimas, `default-features=false`** — não arrastar features não-usadas de deps (`tokio` full → só `rt`+`net` necessários). Fonte: cargo features.
5. **sccache + mold para velocidade de build** — cache de compilação + linker rápido (REGRA #12); não é config do Cargo.toml mas do ambiente. Fonte: Touring disk-hygiene.

## Common pitfalls

- Profile release sem LTO/strip (binário 2-3× maior).
- `debug=true` para todas as deps em dev (target/ explode — REGRA #12).
- `tokio = { features = ["full"] }` quando só precisa de `rt`/`net`.
- Mono-morfização excessiva de genéricos inflando o binário.

## Remediation

1. `cargo bloat`/`build --timings`/`tree -e features` → identificar bloat/lentidão.
2. Ajustar profiles, podar features, `default-features=false` no Cargo.toml (.toml = Edit permitido).
3. `Edit tool --path Cargo.toml --operation ssr --pattern 'opt-level = 0' --replacement 'opt-level = 2'` (cargo-bloat; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C12 SYSTEM-HEALTH** + REGRA #12 (disk hygiene)
- Dims relacionadas: D25 (frontend/wasm size), D44 (pkg mgmt), D08 (deps)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /rust-lang/cargo + cargo-bloat) — maintained by touring-quality_
