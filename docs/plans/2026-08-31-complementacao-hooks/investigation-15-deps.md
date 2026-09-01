---
type: Investigation
title: "F2 — Root cause do 15_dependencies_advisories FAIL (composite 0.7798)"
description: "Investiga por que o gate 15_dependencies_advisories FAIL no elite_aggregate. Causa: F2.5 dim (dep CVEs) está UNVERIFIED — RustSec advisory DB offline (~/.cargo/advisory-db ausente). Fix: instalar cargo-audit + cargo audit fetch."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
tags: [investigation, root-cause, gate-15, dependencies, f2.5, cargo-audit]
timestamp: "2026-08-31T20:09:00-03:00"
---

# F2 — Root cause do 15_dependencies_advisories FAIL

## TL;DR

O gate `15_dependencies_advisories` no `docs/elite_aggregate.py` (linha 74) delega para `touring-quality check --gate F2.5 --target <workspace>`. A dimensão F2.5 (dependency CVEs) está **UNVERIFIED** porque RustSec advisory DB está offline (`~/.cargo/advisory-db` ausente). Sem cargo-audit instalado, o gate não pode confirmar que o dependency tree está livre de CVEs, então marca score 0.5 (Warn) — que para um gate `block` resulta em FAIL.

## Investigação (passo a passo, executado)

### 1. Localizar a regra do gate

```bash
$ grep -n "15_dependencies_advisories" docs/elite_aggregate.py
60:    # among 1839 zeroes F2.1/F2.4/F2.6 → 0.000). cargo-deny advisories in CI is the
74:    ("15_dependencies_advisories", "dims:F2.5", "block", "--check"),  # R0.5: real 50-dim dep-CVEs (cargo-deny still CI for bans)
```

### 2. Inspecionar o gate 15 isoladamente

```bash
$ touring-quality check --gate F2.5 --target /home/gabrielgadea/projects/touring --format json
{
  "scope_kind": "workspace",
  "root": "/home/gabrielgadea/projects/touring",
  "file_count": 2318,
  "total_loc": 819008,
  "dimensions": {
    "F2_5": {
      "value": 0.5,
      "status": "Warn",
      "evidence": "F2.5: UNVERIFIED — RustSec advisory DB offline (~/.cargo/advisory-db absent); cannot confirm the dependency tree is CVE-free. Fix: `cargo install cargo-audit && cargo audit fetch` (or clone rustsec/advisory-db). Not a clean pass."
    }
  },
  "composite": 0.49999997,
  "tier": "Unranked",
  "warnings": ["F2_5"]
}
```

### 3. Confirmar cargo-audit ausente

```bash
$ which cargo-deny
(nothing — não instalado)
$ which cargo-audit
(nothing — não instalado)
```

`cargo-deny` e `cargo-audit` (ferramentas de auditoria de deps) não estão no PATH. O gate F2.5 retorna fail-open 0.5 porque não pode confirmar.

## Root cause

**Falta do advisory DB** impede F2.5 de verificar CVEs nas dependências. O gate marca 0.5 (Warn, fail-open) mas para o composite `block` isso é insuficiente — o score do gate cai, e o 15_dependencies_advisories FAIL propaga.

## Plano de fix

**Ação concreta**: instalar cargo-audit + fetch advisory DB.

```bash
$ cargo install cargo-audit --locked
$ cargo audit fetch
# isso popula ~/.cargo/advisory-db/ com o índice RustSec

$ cargo audit            # roda scan no lockfile atual
# deve passar sem advisories críticos (a confirmar)
```

**Após o fix, re-rodar gate**:
```bash
$ touring-quality check --gate F2.5 --target /home/gabrielgadea/projects/touring --format json
# esperado: score 1.0, status PASS

$ python3 docs/elite_aggregate.py --check
# 15_dependencies_advisories: deve passar de FAIL (0.50) para PASS (1.0+)
```

**Bonus**: rodar `cargo install cargo-deny` e aplicar config `deny.toml` se existir, para verificar bans/licenses/source.

## Estimativa

**Effort**: 10-30min (install ~5min, fetch ~1min, audit ~5min, análise de advisories se houver).
**Risco**: baixo. `cargo audit fetch` é read-only no registry. `cargo install` só adiciona binário.

## Próximo passo

Após F1 + F2 done, **aplicar os 2 fixes** (regenerate docs + install cargo-audit) e re-rodar `elite_aggregate --check` para confirmar promoção a Gold.

---

_v1.0 — 2026-08-31 20:09 BRT | Root cause: cargo-audit ausente → RustSec DB offline → F2.5 UNVERIFIED → 15_dependencies_advisories FAIL | Fix: `cargo install cargo-audit && cargo audit fetch` | effort: 10-30min | risco: baixo_