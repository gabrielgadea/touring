# Quality Rules — 50-Dimension Elite Reference Index

> **Auto-load** | **Version**: v2.0 (consolidated + enriched) | **Date**: 2026-06-20
> **Keystone**: `~/.claude/rules/elite-50-quality.md` (catálogo, comandos reais, reflexos, tiers)
> **Engine**: `touring-quality` binário (50 verifiers) · **Release gate**: `touring-elite` (`elite_aggregate.py`)

Cada arquivo `D{nn}.md` é a **referência de elite** de uma dimensão: definição, thresholds, comandos REAIS, best practices de context7 (com fonte), pitfalls e remediação. **50 dims = 50 rules** (duplicatas D18/D45 removidas em 2026-06-20).

## Comandos reais (NÃO há `touring quality` subcommand nem `generator de qualidade dedicado (inexistente)`)

```bash
touring-quality score <TARGET> [--workspace] [--dims F1.1,F2.5] [--fail-below 0.80] [--format json]
touring-quality check --gate F2.1 --target <TARGET> [--format json]
touring-quality list
python3 ~/projects/touring/docs/elite_aggregate.py --check     # composite release gate (13 gates)
```

## Mapa F-dim → D-rule

| Fase | Dims | D-rules |
|------|------|---------|
| **F1** Code Quality & Architecture | F1.1–F1.12 | D01–D12 |
| **F2** Security & Performance | F2.1, F2.2, F2.3, F2.4, F2.5, F2.6, F2.7–F2.13 | D13, D15, D16, D17, D14, D19, D20–D26 |
| **F3** Testing & Documentation | F3.1–F3.13 | D27–D39 |
| **F4** Best Practices & CI/CD | F4.1–F4.12 | D40, D41, D42, D43, D44, D46, D47, D48, D49, D50, D51, D52 |

## 6 BLOCK dims (P0 — fail-closed pré-Write)

| Dim | D-rule | Verifier |
|-----|--------|----------|
| F2.1 OWASP | D13_owasp | f2_1_owasp |
| F2.4 Secrets/Crypto | D17_secrets | f2_4_secrets |
| F2.5 Dep CVEs | D14_dep_cves | f2_5_dep_cves |
| F2.6 Config security | D19_config | f2_6_config |
| F4.3 Deprecated APIs | D42_deprecated | f4_3_deprecated |
| F4.5 Package mgmt | D44_pkg_mgmt | f4_5_pkg_mgmt |

## Tier-alvo por contexto

| Contexto | Tier mínimo |
|----------|-------------|
| Entrega TACO padrão | 🥈 Gold (0.80) |
| Release / nova API pública | 💎 Diamond (0.95) |
| 6 BLOCK dims (P0) | sempre PASS (score ≥ 0.5) |

## Cross-references

- `~/.claude/rules/elite-50-quality.md` — keystone (catálogo completo + reflexos 10-12 + dim→agent)
- `~/.claude/rules/touring-elite.md` — release composite (13 gates)
- `~/.claude/rules/touring-decision-matrix.md` — C01-C12 task→cmd
