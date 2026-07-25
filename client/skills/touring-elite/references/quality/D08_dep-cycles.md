# D08 — Dependency Management (F1.8)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_8_dep_cycles`
**Enforcement**: ADVISORY (ciclos → tratar como BLOCK)
**Elite reference (context7)**: Madge · dependency-cruiser · `/rust-lang/cargo`

## Definition

Avalia o grafo de dependências: **ciclos** (módulo/crate), direção de acoplamento (camadas devem fluir numa direção), e fan-in/fan-out. Ciclo de dependência = sintoma de fronteira mal-traçada. **Dim USP do Touring** (Tarjan SCC via `wiring cycles`).

## Why it matters

Ciclos impedem arquitetura em camadas, dificultam teste isolado e compilação incremental, e tornam o sistema rígido (não dá para extrair/reusar um nó sem arrastar o ciclo). Em Rust, ciclo entre crates é impossível (cargo proíbe) — mas ciclos entre módulos dentro do crate degradam a manutenibilidade.

## Thresholds

| Ciclos | Score | Status | Action |
|--------|-------|--------|--------|
| 0 | 1.0 | ✅ Pass | grafo acíclico |
| 1–2 (raso) | 0.5–0.7 | ⚠ Warn | quebrar ciclo |
| ≥3 / profundo | <0.4 | ❌ Fail (≈BLOCK) | re-arquitetar |

## MUST

```bash
touring-quality check --gate F1.8 --target <FILE>
touring-quality score <FILE> --dims F1.8 --format json
```

## SHOULD

```bash
touring wiring cycles --min-depth 2 --format json       # Tarjan SCC — ciclos reais
touring wiring impact <symbol> --depth 2                # direção de acoplamento
touring ast workspace-info                              # dependents_of cross-crate
```

## MAY

```bash
touring memory recall "quality:F1.8"
touring wiring chains --rebuild
```

## Elite best practices (context7)

1. **Zero ciclos — quebrar via abstração no kernel** — extrair o tipo/trait compartilhado para um módulo/crate ABAIXO de ambas as pontas (move-utils-down). [training-data: Touring playbook A5/W71 — kernel-home a shared abstraction below both ends]
2. **Dependência aponta para estabilidade (DIP)** — módulos voláteis dependem de abstrações estáveis, nunca o contrário. Fonte: dependency-cruiser `no-circular` + layer rules.
3. **`cargo` proíbe ciclo entre crates** — usá-lo como fronteira forte: dividir em crates impõe acíclico por construção. Fonte: `/rust-lang/cargo` (workspace).
4. **Re-export identity-preserving para desacoplar sem quebrar** — `pub use kernel::Type` mantém callers intactos enquanto remove a dep cíclica. [training-data: Touring A5 playbook]
5. **Monitorar fan-in/fan-out** — nós com fan-in alto são pontos de estabilidade (mude com cuidado); fan-out alto = god-module candidato a split. Fonte: Structure101/Madge.

## Common pitfalls

- Ciclo módulo↔módulo introduzido por "só preciso de uma função de lá".
- Crate fundacional dependendo de um consumidor (inversão de camada).
- Wiring DB stale reportando ciclo já resolvido — confirmar com `--rebuild` (VP-Scout Cadeia 7).

## Remediation

1. `touring wiring cycles --min-depth 2` → identificar SCC.
2. Quebrar: extrair abstração compartilhada para kernel abaixo de ambas as pontas; `pub use` re-export.
3. `touring wiring cycles --min-depth 2` para identificar; refactor via `Edit tool --path <FILE> --operation free-form` (REGRA #2 canonical workflows; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C11 DEPENDENCY-FLOW**
- Dims relacionadas: D07 (boundaries), D12 (arch consistency), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md` (USP — Tarjan SCC)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo + dependency-cruiser) — maintained by touring-quality_
