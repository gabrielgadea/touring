# D24 — Concurrency (F2.11)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_11_concurrency`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/tokio-rs/tokio` · `/tokio-rs/loom` · ThreadSanitizer

## Definition

Avalia segurança de concorrência: data races (impossíveis em Rust safe, mas lógica de sincronização pode estar errada), deadlocks (lock ordering), `Send`/`Sync` corretos, e o anti-pattern de **segurar lock síncrono através de `.await`**. Rust previne data race de memória; não previne deadlock nem lógica concorrente errada.

## Why it matters

Bugs de concorrência são Heisenbugs: não-determinísticos, difíceis de reproduzir, catastróficos em prod. Deadlock trava o serviço; segurar `std::Mutex` através de `await` pode deadlockar o runtime async. Rust elimina a classe de data-race de memória — o resto é design.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | sync correta, sem lock-across-await |
| 0.5–0.8 | ⚠ Warn | lock ordering frágil / lock em await |
| <0.5 | ❌ Fail | deadlock/race lógico |

## MUST

```bash
touring-quality check --gate F2.11 --target <FILE>
touring-quality score <FILE> --dims F2.11 --format json
```

## SHOULD

```bash
touring ast rust-semantic <FILE>                        # contagem async/unsafe, contexto de locks
touring ast grep <FILE> '<MutexGuard mantido através de await>'
cargo test                                              # rodar suíte (loom feature se presente)
```

## MAY

```bash
touring memory recall "quality:F2.11"
```

## Elite best practices (context7)

1. **Nunca segurar `std::sync::Mutex` através de `.await`** — o guard não é `Send` e pode deadlockar; usar `tokio::sync::Mutex` para locks que cruzam await, OU liberar o guard antes do await (escopo `{}`). Fonte: tokio (Shared state / async Mutex guidance).
2. **Lock ordering consistente** — sempre adquirir múltiplos locks na mesma ordem global; ordem inconsistente = deadlock clássico. [training-data: concurrency].
3. **Preferir message-passing a estado compartilhado** — `tokio::sync::mpsc` (bounded, backpressure) em vez de `Arc<Mutex<>>` quando possível. Fonte: tokio channels.
4. **`loom` para testar lógica concorrente** — model-checker explora interleavings; pega bugs que testes normais não pegam. Fonte: `/tokio-rs/loom`.
5. **Atomics para contadores simples** — `AtomicU64` (como os counters do Touring gate-metrics) em vez de `Mutex<u64>`; lock-free para o caso simples. [training-data: rust atomics].

## Common pitfalls

- ⚠ `let g = mutex.lock().unwrap(); foo().await;` (guard vivo no await → deadlock/`!Send`).
- Lock ordering inconsistente entre code-paths → deadlock.
- `Arc<Mutex<>>` onde um canal mpsc seria mais simples e sem contenção.
- NaN-panic em `sort_by(partial_cmp().unwrap())` sob concorrência (ver D06).

## Remediation

1. `touring ast grep`/`rust-semantic` → localizar lock-across-await / ordering.
2. Trocar para `tokio::sync::Mutex`/canal, escopar guards, padronizar ordem via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern '<unsafe_share>' --replacement '<arc_mutex>'` (REGRA #2 canonical workflows — race-free; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + **C06 EDIT-MAJOR**
- Dims relacionadas: D23 (I/O), D06 (error handling), D21 (memory)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: tokio + loom) — maintained by touring-quality_
