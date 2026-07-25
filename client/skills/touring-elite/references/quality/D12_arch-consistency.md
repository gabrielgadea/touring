# D12 — Architectural Consistency (F1.12)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_12_arch_consistency`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: ArchUnit · dependency-cruiser · `/rust-unofficial/patterns`

## Definition

Verifica se o código segue os padrões arquiteturais estabelecidos do workspace: camadas respeitadas (ex.: foundation < intelligence < code), convenções de cross-crate, naming de módulos, tratamento uniforme de cross-cutting concerns (logging, erro, config). **Dim USP** — `wiring audit` mede consistência real cross-crate (nenhum agent de mercado faz isso).

## Why it matters

Inconsistência arquitetural é entropia: cada exceção local torna o sistema mais difícil de raciocinar como um todo e abre brecha para violação de camada (ex.: crate fundacional importando um consumidor → ciclo). Consistência é o que permite a um novo dev (ou LLM) prever onde algo vive.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | camadas/convenções respeitadas |
| 0.5–0.8 | ⚠ Warn | desvio local de padrão |
| <0.5 | ❌ Fail | re-alinhar arquitetura |

## MUST

```bash
touring-quality check --gate F1.12 --target <FILE>
touring-quality score <FILE> --dims F1.12 --format json
```

## SHOULD

```bash
touring wiring audit -j                                  # orphans + módulos com score < 1.0 (consistência real)
touring ast workspace-info                              # camadas/dependents cross-crate
touring wiring cycles --min-depth 2                     # violação de camada = ciclo
```

## MAY

```bash
touring memory recall "quality:F1.12"
touring synergy report -j
```

## Elite best practices (context7)

1. **Regras de camada explícitas e enforçadas** — definir "quem pode depender de quem" e checar em CI; em Rust, a estrutura de crates impõe acíclico. Fonte: ArchUnit/dependency-cruiser layer rules.
2. **Cross-cutting uniforme** — logging via `tracing` (não `println!` ad-hoc), erro via padrão único (thiserror/anyhow), config centralizada. [training-data: rust observability]
3. **Convenção de nomes consistente entre módulos** — mesma operação tem o mesmo nome em todo o workspace (`new`/`from`/`build` com semântica fixa). Fonte: rust-api-guidelines.
4. **Foundation-first dependency flow** — tipos compartilhados no kernel mais baixo; consumidores acima. Violação = ciclo (ver D08). [training-data: Touring layered crates A2/A5]
5. **Detectar drift cedo** — `wiring audit` + `evolution drift` flagam quando o código diverge do padrão documentado antes de virar débito. Fonte: Touring USP.

## Common pitfalls

- Crate fundacional importando um consumidor (inversão de camada → ciclo).
- Logging/erro/config tratados de N formas diferentes pelo workspace.
- Mesmo conceito com nomes divergentes em módulos diferentes.
- Padrão documentado (ARCHITECTURE.md) divergindo do código real (drift).

## Remediation

1. `touring wiring audit -j` + `workspace-info` → identificar desvio/violação de camada.
2. Re-alinhar (mover tipo para kernel, unificar cross-cutting) via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <aligned.rs>` (REGRA #2 canonical workflows — alinhar com padrões arquiteturais; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C11 DEPENDENCY-FLOW** + **C12 SYSTEM-HEALTH**
- Dims relacionadas: D07 (boundaries), D08 (dep cycles), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned, USP)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: ArchUnit + dependency-cruiser) — maintained by touring-quality_
