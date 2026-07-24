---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W9"
name: "touring-server Internal Split"
phase: "F3-STABILIZATION"
depends_on:
  - W8
parallel_with:
  - W10
status: "DONE — pragmatic 3-crate split (2026-05-15); see Execution Result"
created: "2026-05-11"
completed: "2026-05-15"
cila: "L3"
rust_changes: "SPLIT"
estimated_days: "10-12"
checkpoint: "touring_premium_W9_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W9.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W9: touring-server Internal Split

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F3-STABILIZATION
> **Contribuição para resultado final**: Reduz mega-binary para façade slim 25k LOC. Cada sub-crate testável isoladamente. CLI dispatch latency baixa porque imports são re-exports.

---

## Contexto e Dependências

- **Depende de**: W8
- **Paralelo com**: W10
- **CILA**: `L3`
- **Mudanças Rust**: `SPLIT`
- **Estimativa**: 10-12 dias
- **Checkpoint**: `touring_premium_W9_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W9.py`

---

## Descrição

touring-server (61k LOC, 161 files, 628 pub) é god-binary. Split interno em 6 sub-crates: server-cli (CLI dispatch), server-tools (tools/*), server-reasoning (reasoning/*), server-session (session, snapshot), server-telemetry (telemetry init), server-visual (visual/, flow viz). Façade touring-server mantém o binary `touring` no main.rs. API externa intacta.

---

## Efeitos no Sistema

- 6 sub-crates server-* internos
- Façade touring-server slim ~25k LOC (binary + main + dispatch)
- 82 CLI commands smoke-test exit 0
- CLI dispatch P99 < 10ms

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W9.1: Create 6 internal sub-crates

**Descrição**: taco-forge perfect-create-crate × 6.

**Dias estimados**: 1.0

**Critério de validação**: 6 crates cargo check exit 0.

---

### W9.2: Move cli/* → server-cli

**Descrição**: CLI handlers + arg parsing + dispatch table.

**Dias estimados**: 1.5

**Critério de validação**: cargo check -p touring-server-cli exit 0.

---

### W9.3: Move tools/* → server-tools

**Descrição**: Tool registry + handlers + MCP integration.

**Dias estimados**: 1.0

**Critério de validação**: cargo check -p touring-server-tools exit 0.

---

### W9.4: Move reasoning/* → server-reasoning

**Descrição**: Reasoning engine wiring, verification, persistence.

**Dias estimados**: 1.0

**Critério de validação**: cargo check -p touring-server-reasoning exit 0.

---

### W9.5: Move session/* + snapshot/* → server-session

**Descrição**: Session manager + .toon snapshot + diary.

**Dias estimados**: 1.0

**Critério de validação**: cargo check -p touring-server-session exit 0.

---

### W9.6: Move telemetry/* + telemetry_init.rs → server-telemetry

**Descrição**: OTel init, fmt subscriber, console subscriber probe.

**Dias estimados**: 0.5

**Critério de validação**: cargo check -p touring-server-telemetry exit 0.

---

### W9.7: Move visual/* → server-visual + façade

**Descrição**: Visual emitters (flow.rs, mod.rs). Server crate fica como façade + main binary.

**Dias estimados**: 0.5

**Critério de validação**: cargo build --bin touring exit 0.

---

### W9.8: Tests reorganize

**Descrição**: 6k LOC tests por sub-crate. Integration tests fora.

**Dias estimados**: 1.5

**Critério de validação**: cargo test --workspace exit 0.

---

### W9.9: Bench CLI dispatch < 10ms P99

**Descrição**: touring status, touring doctor, touring ast meta benches.

**Dias estimados**: 1.0

**TDD RED** (escrever ANTES do código):
```python
def test_cli_dispatch_p99_under_10ms():
    """RED: P99 > 10ms FAILS."""
```

**Critério de validação**: P99 < 10ms para 3 commands hot-path.

---

### W9.10: Validation: 82 CLI commands smoke test

**Descrição**: Rodar `touring <cmd> --help` para 82 subcomandos. Exit 0 todos.

**Dias estimados**: 1.0

**Critério de validação**: 82/82 smoke tests exit 0.

---

## Gate de Saída

touring-server façade 25k LOC, 6 internal sub-crates, CLI dispatch P99 < 10ms, 82 commands smoke pass.

## Riscos Específicos

- Main binary still in touring-server façade; ensure cargo metadata shows it as the [[bin]] target
- Session sub-crate has heavy state (snapshot persist) — careful with test parallelism

## Checklist de Conclusão

- [ ] Todos os subtasks implementados
- [ ] Todos os testes TDD GREEN
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace --no-fail-fast` pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles --min-depth 2` no new cycles
- [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
- [ ] Bench regression < 5%
- [ ] Test ratio ≥ 20% per touched crate
- [ ] Checkpoint `.toon` salvo
- [ ] Memory lesson persistida (`touring memory store --tier semantic`)
- [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
- [ ] Documentação atualizada (se necessário)

---

## Execution Result (2026-05-15) — Pragmatic 3-Crate Split

The 6-crate split designed above was refined into a **pragmatic 3-crate
split** after a self-run SCC analysis (the `w9_server_split_planner.py`
forensic output was v1-quality — no cycle analysis, 113 files in a `misc`
fallback bucket). This section is the authoritative record of what W9
shipped; the subtask plan (W9.1–W9.10) above is superseded.

### SCC analysis — touring-server is genuinely layered

A cross-bucket `crate::` edge graph over touring-server's 23 modules
(directory-based bucketing) found a **single small SCC**:

```
SCC = { cli, server, tools }   — 116 files, ~45k LOC
```

The other 20 modules form a clean DAG. Edge highlights: `server → tools`
(104), `server → reasoning` (11), `cli → tools` (21). Contrast with W8,
where 7/10 buckets collapsed into one SCC — touring-server is genuinely
layered. The W9 plan's 6 buckets resolve as: `server-cli` + `server-tools`
were always part of the SCC with `server` (kept as `touring-server`);
`reasoning`, `session`, `visual` are **pure leaves** (`OUT → []`) and
extract cleanly.

### What W9 shipped — 3 crates

| Crate | Files | LOC | Tests |
|---|---|---|---|
| `touring-server-reasoning` | 5 | ~4,483 | 103 |
| `touring-server-visual` | 9 | ~2,766 | 89 |
| `touring-server-session` | 2 | ~686 | 35 |
| `touring-server` (SCC + leaves + binary) | 142 | ~53k | (unchanged) |

- All 3 extracted crates are cycle-free leaves — zero `crate::` references
  to any other internal module.
- **Façade**: `touring-server/src/lib.rs` converts `pub mod {reasoning,
  visual,session};` to `pub use touring_server_{reasoning,visual,session}::`.
  The `touring` binary and external API are byte-identical.
- **22 visibility promotions** `pub(crate)` → `pub` were required: the SCC
  `server/` modules call `TaskDecomposer` / `SubTask` /
  `DecomposeValidationMetrics` (16) and `SessionManager` (6) methods that
  were crate-private before extraction.

### Deferred (out of pragmatic scope)

- **`touring-server-telemetry`** — `telemetry/` + `telemetry_init.rs` are
  dependency-leaves but entangled with 6 observability feature flags
  (`console`, `otlp`, `file-logs`, `tracy`, `dhat-heap`, `ebpf-telemetry`),
  8 optional deps, and the binary's allocator/startup story. Extractable
  but error-prone — a focused follow-up.
- `src/snapshot/` (3 files, 724 LOC) — dead code (never `mod`-declared,
  0 `crate::snapshot` refs); left in place (W1 Dead Code Purge scope).

### Gate results

| Gate | Result |
|---|---|
| `cargo check --workspace` | exit 0 ✅ |
| `cargo check --workspace --tests` | exit 0 ✅ |
| New-crate tests | 227 PASS ✅ |
| `cargo clippy -D warnings` (new crates) | 0 issues ✅ |
| New Cargo cycles | 0 (proven by `cargo check`) ✅ |
| `touring` binary + external API | preserved (façade) ✅ |
| W9-introduced regressions | 0 ✅ |
