# D04 — Clean Code / SOLID (F1.4)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_4_solid`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: Sourcery · DeepSource · `/rust-unofficial/patterns`

## Definition

Avalia aderência a Clean Code + SOLID adaptado a Rust: **S**ingle Responsibility (structs/módulos coesos), **O**pen/Closed (extensão via traits), **L**iskov (impls honram contrato do trait), **I**nterface Segregation (traits pequenos), **D**ependency Inversion (depender de traits, não de tipos concretos). Detecta god-structs e anti-patterns.

## Why it matters

Violações de SOLID criam acoplamento rígido: uma mudança força mudanças em cascata. Em Rust, traits bem-segregados + DI são o que permite testabilidade (mock via trait) e evolução sem quebrar consumidores.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | coeso, baixo acoplamento |
| 0.5–0.8 | ⚠ Warn | god-struct OU trait inchado |
| <0.5 | ❌ Fail | refatorar responsabilidades |

## MUST

```bash
touring-quality check --gate F1.4 --target <FILE>
touring-quality score <FILE> --dims F1.4 --format json
```

## SHOULD

```bash
touring ast overview <FILE> -j                          # pub surface + métodos por tipo (SRP/ISP)
touring wiring impact <symbol> --depth 2                # acoplamento de consumidores
Edit tool --path <FILE> --operation assist --assist-kind extract_function
```

## MAY

```bash
touring memory recall "quality:F1.4"
```

## Elite best practices (context7)

1. **SRP por struct/módulo** — se descrevê-lo exige "e", divida. [training-data: Clean Code]
2. **Open/Closed via trait + impl** — adicionar comportamento implementando um trait, não editando um `match` central. Fonte: `/rust-unofficial/patterns` (strategy/visitor).
3. **Interface Segregation: traits pequenos e focados** — `Read`/`Write` separados (std) > um trait gigante; consumidor pede só o que usa. Fonte: rust std design.
4. **Dependency Inversion: aceitar `impl Trait`/genérico** — `fn f(db: &impl Repo)` em vez de `&PostgresDb` concreto → testável e desacoplado. [training-data: rust DI patterns]
5. **Newtype para invariantes** — `struct Email(String)` com validação no construtor evita "primitive obsession" e espalhar checagem. Fonte: rust patterns (newtype).

## Common pitfalls

- God-struct com 30 campos + 40 métodos não-relacionados.
- Trait "kitchen-sink" que força impls a `unimplemented!()` metade dos métodos (ISP violado).
- Depender de tipo concreto onde um trait permitiria mock/troca (DI violado) → testes impossíveis.
- `impl` que viola o contrato semântico do trait (LSP) — panic onde o trait promete `Result`.

## Remediation

1. `touring ast overview` → mapear responsabilidades por tipo.
2. Dividir struct/trait; inverter dependência para `impl Trait`; newtype para invariantes via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <refactored.rs>` (REGRA #2 canonical workflows — separar responsabilidades; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + **C10 ARCHITECTURAL**
- Dims relacionadas: D07 (boundaries), D09 (API design), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust patterns + Sourcery) — maintained by touring-quality_
