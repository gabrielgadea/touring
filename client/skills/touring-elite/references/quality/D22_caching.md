# D22 — Caching (F2.9)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_9_caching`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: Redis · `/moka-rs/moka` (Rust in-process cache) · Varnish

## Definition

Avalia estratégia de cache: presença onde compensa, invalidação correta (a parte difícil), prevenção de cache stampede, e política de TTL/eviction. "There are only two hard things: cache invalidation and naming."

## Why it matters

Cache mal-feito é pior que sem cache: dados stale causam bugs sutis; stampede (mil requests recalculando ao expirar) derruba o backend; cache unbounded vira leak (D21). Cache bem-feito é a maior alavanca de latência/throughput.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | invalidação + bound + TTL corretos |
| 0.5–0.8 | ⚠ Warn | invalidação frágil / sem bound |
| <0.5 | ❌ Fail | risco de stale/stampede |

## MUST

```bash
touring-quality check --gate F2.9 --target <FILE>
touring-quality score <FILE> --dims F2.9 --format json
```

## SHOULD

```bash
touring ast grep <FILE> '<cache get/insert>'            # localizar pontos de cache e invalidação
touring gate-metrics -j | jq '.query_cache_hit_ratio'   # exemplo real: moka cache do próprio Touring
```

## MAY

```bash
touring memory recall "quality:F2.9"
```

## Elite best practices (context7)

1. **Invalidação por evento, não só TTL** — invalidar no write (`invalidate_by_path` como o Touring faz em post_edit/post_write); TTL é rede de segurança, não a estratégia primária. Fonte: `/moka-rs/moka` (invalidate) + Touring W18.
2. **Bounded cache com eviction (LRU/LFU)** — capacidade máxima + política de eviction; nunca cache unbounded (vira leak — D21). Fonte: moka `Cache::builder().max_capacity()`.
3. **Anti-stampede: single-flight / jittered TTL** — coalescer recálculos concorrentes da mesma chave (moka `get_with`); TTL com jitter para não expirar tudo junto. Fonte: moka `get_with` + Redis cache stampede patterns.
4. **TTL alinhado à volatilidade do dado** — dado estável: TTL longo; volátil: curto ou event-invalidation. [training-data: cache design].
5. **Medir hit-ratio** — cache sem observabilidade de hit-rate é fé, não engenharia; alvo típico ≥ 0.6 (Touring `query_cache_hit_ratio` ~0.58+). Fonte: Touring gate-metrics.

## Common pitfalls

- Cache sem invalidação no write → dados stale.
- Cache stampede ao expirar chave quente (thundering herd).
- Cache unbounded (leak — D21).
- Cachear dado de baixo hit-rate (overhead > ganho).

## Remediation

1. `touring ast grep` → mapear get/insert/invalidate.
2. Adicionar bound+eviction, invalidação por evento, single-flight via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <cached.rs>` (REGRA #2 canonical workflows — bound + eviction + invalidation; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D21 (memory), D20 (DB perf), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: moka + Redis) — maintained by touring-quality_
