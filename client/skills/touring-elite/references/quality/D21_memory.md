# D21 — Memory Management (F2.8)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_8_memory`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: Valgrind · heaptrack · `/rust-lang/rust` (ownership) · `/tokio-rs/tokio`

## Definition

Avalia gestão de memória: vazamentos (raros em Rust safe, possíveis via `Rc` cycles/`mem::forget`/`Box::leak`), crescimento ilimitado (coleções sem bound), alocações grandes/desnecessárias, e overhead de `clone`/`Arc`. Rust dá segurança de memória, mas não impede ineficiência ou unbounded growth.

## Why it matters

Vazamento/crescimento ilimitado derruba serviços long-running (OOM). Clones desnecessários e alocações no hot-path matam performance. Em Rust, ownership elimina use-after-free, mas o design de ownership ruim (`Rc<RefCell>` por toda parte, clone defensivo) é débito de performance e clareza.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | bounded, poucos clones no hot-path |
| 0.5–0.8 | ⚠ Warn | clone/alloc evitável / coleção unbounded |
| <0.5 | ❌ Fail | leak / unbounded growth |

## MUST

```bash
touring-quality check --gate F2.8 --target <FILE>
touring-quality score <FILE> --dims F2.8 --format json
```

## SHOULD

```bash
touring ast rust-semantic <FILE>                        # clone/Arc/Box usage no contexto
touring profile heap-dump                               # touring-core::profile RAII instrumentation
touring ast grep <FILE> '.clone()'                      # clones no hot-path
```

## MAY

```bash
touring memory recall "quality:F2.8"
```

## Elite best practices (context7)

1. **Bound em toda coleção que cresce com input** — cache/fila/buffer com capacidade máxima (LRU, `bounded channel`); unbounded = vetor de OOM/DoS. Fonte: `/tokio-rs/tokio` (bounded channels p/ backpressure).
2. **Emprestar (`&`), não clonar** — passar `&T`/`&str`/`&[T]`; `clone()` só quando ownership é genuinamente necessária. [training-data: rust ownership].
3. **`Cow<str>` para clone condicional** — evita alocação quando não há mutação. [training-data: rust idioms].
4. **Evitar `Rc<RefCell>` cycles** — ciclo de `Rc` vaza (refcount nunca zera); usar `Weak` para back-references. Fonte: Rust Book (Rc/Weak).
5. **Medir antes de otimizar** — `heaptrack`/`touring profile heap-dump` para achar o alocador real, não adivinhar. Fonte: heaptrack (princípio operacional 8).

## Common pitfalls

- Cache/`Vec`/`HashMap` que cresce sem limite com requests → OOM.
- `clone()` defensivo no hot-path (cópia desnecessária).
- Ciclo de `Rc` (vazamento silencioso).
- `Box::leak`/`mem::forget` sem justificativa (leak intencional não-documentado).

## Remediation

1. `touring ast rust-semantic` + `profile heap-dump` → identificar clone/unbounded.
2. Adicionar bound (LRU/bounded channel), trocar clone por borrow/`Cow` via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <bounded_alloc.rs>` (REGRA #2 canonical workflows — bounded capacity; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + **C06 EDIT-MAJOR**
- Dims relacionadas: D23 (I/O), D24 (concurrency), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust ownership + tokio) — maintained by touring-quality_
