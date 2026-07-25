# D40 — Language Idioms (F4.1)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.9
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_1_idioms`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/rust-lang/rust-clippy` · ruff · ESLint

## Definition

Avalia se o código é idiomático: usa as construções nativas da linguagem (iterators, `?`, pattern matching, `Option`/`Result` combinators) em vez de traduzir idioms de outras linguagens. Em Rust, clippy é o oráculo de idiomaticidade. Alvo: `cargo clippy -- -D warnings` limpo.

## Why it matters

Código idiomático é mais legível para quem conhece a linguagem, frequentemente mais correto (idioms evitam pegadinhas) e mais performático (iterators otimizam melhor que loops manuais com índice). Clippy codifica anos de sabedoria coletiva da comunidade Rust.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.9+ | ✅ Pass | clippy clean, idiomático |
| 0.5–0.9 | ⚠ Warn | warnings de clippy / não-idiomático |
| <0.5 | ❌ Fail | C-style/anti-idioms |

## MUST

```bash
touring-quality check --gate F4.1 --target <FILE>
touring-quality score <FILE> --dims F4.1 --format json
```

## SHOULD

```bash
cargo clippy --workspace --all-targets -- -D warnings    # oráculo de idiomaticidade
touring ast rust-semantic <FILE>                         # construções usadas
Edit tool --path <FILE> --operation rewrite # aplicar sugestão do clippy
```

## MAY

```bash
touring memory recall "quality:F4.1"
```

## Elite best practices (context7 — `/rust-lang/rust-clippy`)

1. **`cargo clippy -- -D warnings` no CI** — zero warnings; clippy pega centenas de não-idiomatismos (needless_clone, redundant_closure, etc.). Use `--all-targets` (pega tests/benches também — lição Touring RBP). Fonte: clippy.
2. **Iterator chains > loop manual** — `.iter().filter().map().sum()` em vez de `for` com mutação+índice; mais claro e otimiza melhor. Fonte: clippy `needless_range_loop`.
3. **Combinators de `Option`/`Result`** — `.map()`/`.and_then()`/`.unwrap_or_else()`/`?` em vez de `match` verboso. Fonte: clippy `option_if_let_else`.
4. **`if let`/`let-else`/`matches!`** — pattern matching idiomático para extração; `let-else` para early-return. Fonte: clippy + Rust 2021+.
5. **Não silenciar clippy com `#[allow]` por reflexo** — corrigir; `#[allow]` só com justificativa documentada (idioma falso-positivo raro). [training-data: Touring RBP-01].

## Common pitfalls

- C-style `for i in 0..len { v[i] }` em vez de `for x in &v`.
- `match opt { Some(x) => ..., None => ... }` onde `.map_or()` bastaria.
- `.clone()` desnecessário (clippy `redundant_clone`).
- `#[allow(clippy::...)]` espalhado para silenciar em vez de corrigir.

## Remediation

1. `cargo clippy --all-targets -- -D warnings` → lista de não-idiomatismos.
2. Aplicar as sugestões (clippy frequentemente dá o fix exato) via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern '<unidiomatic>' --replacement '<idiomatic>'` (clippy --fix; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C05/C06 EDIT** (clippy pós-edit)
- Dims relacionadas: D43 (modernization), D01 (complexity), D06 (error handling)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /rust-lang/rust-clippy) — maintained by touring-quality_
