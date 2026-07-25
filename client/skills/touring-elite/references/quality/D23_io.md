# D23 — I/O Bottlenecks (F2.10)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_10_io`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/tokio-rs/tokio` · strace · lsof

## Definition

Detecta gargalos de I/O: **chamada bloqueante em contexto async** (mata o executor), I/O não-bufferizado, payloads grandes sem streaming/paginação, e operações I/O síncronas no hot-path. Cobre o eixo de I/O da performance.

## Why it matters

Uma chamada bloqueante (`std::fs`, `std::net`, CPU pesado) dentro de uma task async **trava o worker thread**, impedindo o executor de progredir outras futures — degradação não-óbvia que só aparece sob carga. É o anti-pattern async mais comum e custoso.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | I/O async, sem blocking em task |
| 0.5–0.8 | ⚠ Warn | blocking/sync I/O em caminho async |
| <0.5 | ❌ Fail | bloqueio do executor |

## MUST

```bash
touring-quality check --gate F2.10 --target <FILE>
touring-quality score <FILE> --dims F2.10 --format json
```

## SHOULD

```bash
touring ast grep <FILE> '<std::fs|std::net|block_on em async>'   # blocking em contexto async
touring ast rust-semantic <FILE>                                 # contagem async/await
```

## MAY

```bash
touring memory recall "quality:F2.10"
```

## Elite best practices (context7 — `/tokio-rs/tokio`)

1. **`spawn_blocking` para blocking *bounded* curto** — operação que termina (CPU burst, lib sync) → `tokio::task::spawn_blocking`; ocupa thread do pool de blocking (default 512), não o worker async. Fonte: tokio `task::blocking` (use spawn_blocking for short-lived blocking).
2. **Thread dedicada para blocking *long-lived*** — worker/loop persistente → `std::thread::spawn`, não `spawn_blocking` (saturaria o pool). Fonte: tokio (rule of thumb: dedicated threads for persistent workloads).
3. **Nunca `block_on` dentro de runtime** — panic "Cannot start a runtime from within a runtime"; se inevitável, `task::block_in_place` primeiro. Fonte: tokio `rt_handle_block_on`.
4. **I/O async + bufferizado** — `tokio::fs`/`tokio::io::BufReader` em vez de `std::fs`; bufferizar reads/writes pequenos. Fonte: tokio io.
5. **Streaming/paginação para payloads grandes** — não carregar tudo em memória; `Stream`/chunked transfer; paginar respostas. [training-data: tokio streams].

## Common pitfalls

- ⚠ `std::fs::read`/`reqwest::blocking`/CPU pesado dentro de `async fn` → trava worker.
- `block_on` aninhado em runtime → panic.
- Read/write byte-a-byte sem buffer (syscall por byte).
- Carregar arquivo/resposta gigante inteiro em `Vec` (memória + latência).

## Remediation

1. `touring ast grep` → localizar blocking em async.
2. Mover para `spawn_blocking` (curto) ou thread dedicada (longo); trocar para `tokio::fs`+buffer via `Edit tool`.
3. `Edit tool --path <FILE> --operation free-form --content-from <async_io.rs>` (REGRA #2 canonical workflows — sync→async; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + **C06 EDIT-MAJOR**
- Dims relacionadas: D24 (concurrency), D21 (memory), D2 (db perf)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /tokio-rs/tokio) — maintained by touring-quality_
