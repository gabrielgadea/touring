---
type: Investigation
title: "F8 — Plano de fix para as 9 vulnerabilities de deps detectadas por cargo-audit"
description: "Mapeamento das 9 vulnerabilities detectadas (h2, quick-xml ×2 versions, wasmtime ×2 versions, webbrowser) + estratégia de mitigação (cargo update, constraint pin, acknowledge). Composite atual 0.8010 Gold mas 15_dependencies_advisories FAIL — promoção completa requer resolver."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
tags: [investigation, vulnerabilities, deps-audit, rustsec, F2.5]
timestamp: "2026-08-31T20:18:00-03:00"
---

# F8 — Plano de fix para as 9 vulnerabilities de deps

## TL;DR

`cargo-audit audit` (após `cargo install cargo-audit --locked && cargo-audit fetch`) detectou **9 vulnerabilities reais** em 5 crates: h2, quick-xml (2 versões), wasmtime (2 versões), webbrowser. O composite subiu para Gold 0.8010 (06_documentation fix), mas 15_dependencies_advisories FAIL persistiu porque o gate F2.5 agora detecta a dívida real.

## As 9 vulns (lista canônica)

| # | Crate | Version | RUSTSEC ID | Título |
|---|---|---|---|---|
| 1 | `h2` | 0.4.15 | RUSTSEC-2026-0258 | h2 unbounded empty DATA frames (GHSA-q83h-524g-xf6h) |
| 2 | `quick-xml` | 0.26.0 | RUSTSEC-2026-0194 | Quadratic runtime when checking start tag for duplicate attribute names |
| 3 | `quick-xml` | 0.26.0 | RUSTSEC-2026-0195 | Unbounded namespace-declaration allocation in `NsReader` enables memory-exhaustion DoS |
| 4 | `quick-xml` | 0.39.4 | RUSTSEC-2026-0194 | Quadratic runtime when checking start tag (same as #2) |
| 5 | `quick-xml` | 0.39.4 | RUSTSEC-2026-0195 | Unbounded namespace-declaration allocation (same as #3) |
| 6 | `wasmtime` | 36.0.13 | RUSTSEC-2026-0269 | Filesystem sandbox escape when paths or symlinks contain trailing slashes |
| 7 | `wasmtime` | 46.0.2 | RUSTSEC-2026-0268 | Guest controlled-size host heap allocation through WASIp3 streams |
| 8 | `wasmtime` | 46.0.2 | RUSTSEC-2026-0269 | Filesystem sandbox escape (same as #6) |
| 9 | `webbrowser` | 1.2.1 | RUSTSEC-2026-0257 | Unix `BROWSER` handling allows browser argument injection |

**Crates únicos flagged**: 5 (`h2`, `quick-xml`, `wasmtime`, `webbrowser`, + lru/chacha20 do output anterior — total ~9 únicas).

## Estratégia de mitigação (3 caminhos)

### Caminho 1 — `cargo update` (mais simples, se patches existem)

```bash
cargo update -p h2 --precise <patched_version>
cargo update -p quick-xml --precise <patched_version>
cargo update -p wasmtime --precise <patched_version>
cargo update -p webbrowser --precise <patched_version>
cargo audit                # re-verificar
```

**Prós**: zero código, só manifest update
**Contras**: requer patched version existir; pode quebrar transitive deps

### Caminho 2 — Pin de versão no workspace `Cargo.toml`

```toml
[workspace.dependencies]
h2 = "= 0.4.X"   # pin
quick-xml = "= 0.39.X"  # pin
```

**Prós**: controle fino
**Contras**: manutenção contínua

### Caminho 3 — Reconhecer como `acknowledged` (quando patch não existe ou é invasivo)

Adicionar ao `deny.toml` (se houver) ou ao `audit.toml`:

```toml
[advisories]
ignore = [
    "RUSTSEC-2026-0258",  # h2 — patch em 0.4.16+; update planejado F8.1
    # ...
]
```

**Prós**: gate fica PASS para escopo conhecido
**Contras**: aceita dívida (não ideal)

## Recomendação imediata

**Aplicar Caminho 1 primeiro** (`cargo update`) — se patches existem (a maioria tem), o upgrade é transparente. Se algum crate tem dep transitiva que bloqueia o update, fallback para Caminho 3 com acknowledge explícito + issue tracker.

## Estimativa

**Effort**: 30-60min para tentar `cargo update` em todas as 5 crates + re-rodar audit + composite.
**Risco**: médio — pode quebrar transitive deps se algum crate pinning for crítico. Testes E2E + cargo check workspace cobrem regressão.

## Próximo passo (gate humano)

Aguardando Gabriel aprovar claim F8-fix-9-deps-vulns para execução.

---

_v1.0 — 2026-08-31 20:18 BRT | 9 vulns em 5 crates | estratégia: cargo update (path 1) com fallback acknowledge (path 3) | effort: 30-60min | risco: médio_