# C5 — Active Output Summarizer no CEG — Design N3 (implementation-ready)

> **Data**: 2026-06-27 | **Autor**: TACO (Opus 4.8 1M) p/ Gabriel Gadea
> **Backlog**: `2026-06-26-coupling-backlog.md` C5 (Construção · M · alto ROI · **dep = design N3**)
> **Status**: design fechado + VGP nos entry points → a sessão dedicada de C5 é **pura execução**.
> **Desbloqueia**: C9 (Class-D detector consome o `exit_code` + assinaturas preservadas pelo summary).

---

## 1. Problema (e por que importa)

O CEG hoje, ao capturar stdout de um sandbox-run, trunca em **1 MB** e devolve **apenas
`content_hash`** (BLAKE3) + um path em disco. A LLM recebe um *ref*, não o conteúdo —
e (Codex pathology, *"Is Grep All You Need?"*) **não relê** o arquivo: a acurácia caiu
**93%→55%** quando o agente dependia de file-based em vez de inline. Pior: um run que
**falhou** (exit≠0) pode chegar à LLM como um hash opaco — a falha fica **mascarada**.

**Tese C5**: devolver um **summary inline, metadata-first** (<200 tokens) que **preserva
exit-code + assinaturas de erro + refs `file:line`**, mantendo o full em disco **opcional
sob demanda** — nunca só o ref. Densificação (não indução): muda `C(tokens)` reinjetados.

## 2. Estado atual `[FACT]` (VGP 2026-06-27)

| Símbolo | Local | Campos relevantes |
|---|---|---|
| `SandboxOutcome` | `crates/touring-ceg/src/gateway/sandbox_stage.rs:43-56` | `exit_code:i32`, `output_bytes:u64`, `was_truncated:bool`, `timed_out:bool`, `content_hash:String`, `capability_profile:String` |
| `SandboxResult` | `crates/touring-ceg/src/gateway/sandbox_executor.rs:55` | `bytes`, `was_truncated`, `content_hash` |
| `spawn_and_capture` | `…/sandbox_executor.rs:324` | captura → trunca em `max_output_bytes=1_000_000` (L47) → `hash_output` (L385) → `store_output` (L386); tee p/ exit≠0 (L390) |

**Blast (grounded por grep, corrige o medo do scout 2026-06-26)**: os consumidores de
`SandboxResult`/`SandboxOutcome` estão **todos dentro de `touring-ceg`** (`fast_path`,
`pre_exec`, `supervised`, `speculative`, `dry_run_cache`, `sandbox_stage`, `decision`,
`typestate`, `mod`). O acoplamento a `touring-hooks-core` (`sandbox_output_store.rs`,
`tantivy_index.rs`) é via **`content_hash`/store serializado**, **não** via a struct →
**adicionar um campo `summary` é aditivo e contido ao crate** (downstream ignora até C9
consumir). Risco de blast **menor** que o backlog estimava — mas ainda Construção no CEG
crítico (gateway X0-X9), por isso sessão dedicada para o **código** (este doc fecha o design).

## 3. Design

### 3.1 Data model — `OutputSummary` (novo, em `touring-ceg`)

```rust
/// Inline, metadata-first digest of a (possibly truncated) sandbox capture.
/// Token budget: < 200 tokens when rendered. NEVER masks a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSummary {
    /// Subprocess exit code, verbatim (-1 timeout/spawn-fail, -2 dry-run-skip).
    pub exit_code: i32,
    /// Total stdout bytes BEFORE truncation (so the LLM knows the real size).
    pub total_bytes: u64,
    /// First N error-signature lines (`^error`, `error:`, `panic`, `FAILED`, …), verbatim.
    pub error_lines: Vec<String>,
    /// `file:line(:col)` references extracted from the output (deduped, capped).
    pub file_refs: Vec<String>,
    /// Counts by class: {"error": n, "warning": n, "test_failed": n, …}.
    pub counts: BTreeMap<String, u32>,
    /// First + last K lines of output (head/tail retention) when no errors dominate.
    pub head_tail: Vec<String>,
    /// `true` when the full output was clipped (full available on-demand by content_hash).
    pub truncated: bool,
}
```

### 3.2 Pure core — `summarize_output` (stateless, unit-testable)

```rust
/// Build an OutputSummary from raw stdout + the real exit code. PURE: no I/O,
/// deterministic, the whole testable heart of C5 (mirrors `run_audit` in tools_workflow).
pub fn summarize_output(output: &str, exit_code: i32, truncated: bool) -> OutputSummary
```

**Regras de extração** (regex compilados via `OnceLock`, multi-linguagem):
| Sinal | Padrão (exemplos) | Destino |
|---|---|---|
| error line | `^error`, `error[E\d+]`, `error:`, `panic`, `FAILED`, `Traceback`, `^\s*E\s` (pytest) | `error_lines` (cap 8, verbatim) |
| file:line | `([\w./-]+):(\d+)(:\d+)?`, `--> path:line:col` (rustc) | `file_refs` (dedup, cap 12) |
| counts | `\d+ (passed\|failed\|error\|warning)`, `error: aborting due to N` | `counts` |
| head/tail | primeiras K=3 + últimas K=3 linhas | `head_tail` (só se `error_lines` vazio) |

### 3.3 Invariante anti-mascaramento (N3 ↔ I4) — **NÃO-NEGOCIÁVEL**

```
exit_code != 0  ⟹  error_lines não-vazio OU head_tail contém as últimas linhas
                   E  exit_code aparece literal no summary renderizado.
```

Se um run falha e o output não casa nenhum padrão de erro, o summary **força** as últimas
K linhas + o exit-code (a falha **nunca** vira um hash silencioso). Teste dedicado prova
`masked_failure_rate == 0`.

### 3.4 Wiring (sessão dedicada — execução)

1. `summarize_output` + `OutputSummary` em novo módulo `gateway/summarize.rs` (touring-ceg).
2. `SandboxResult` (sandbox_executor.rs:55) ganha `pub summary: OutputSummary`; populado em
   `spawn_and_capture` (após L385, antes do return L398) com o `output_bytes` já capturado
   (reusa o buffer **antes** do hash — sem re-ler disco).
3. `SandboxOutcome` (sandbox_stage.rs:43) ganha `pub summary: OutputSummary` (propagado de
   `SandboxResult`). Aditivo: os 10 consumidores in-crate compilam sem mudança (campo novo).
4. Render: `apply_detail_level`-style — o summary entra inline no `GatewayOutcome`; o full
   permanece em `store_output` (on-demand por `content_hash`). `gate-metrics`: counter
   `ceg_summary_tokens_reinjected`.

## 4. Test plan (gate da sessão dedicada)

| Teste | Prova |
|---|---|
| `summarize_clean_output` | exit 0 + sem erros → `error_lines` vazio, `head_tail` populado |
| `summarize_rustc_error` | `error[E0382]... --> src/x.rs:10:5` → error_line + file_ref `src/x.rs:10` |
| `summarize_pytest_failure` | `FAILED test_x` + `3 failed` → counts{test_failed} + error_line |
| `summarize_preserves_exit_code` | exit≠0 sempre no summary |
| **`failure_never_masked`** | exit≠0 + output sem padrão → head_tail forçado, exit no summary (**rate 0**) |
| `summary_under_budget` | render < 200 tokens em output de 1 MB |
| `truncated_flag_propagates` | output > 1 MB → `truncated=true` + full recuperável por hash |

## 5. Aceitação + medição (do backlog)

- **Aceitação**: `summary` < 200 tok preserva **exit-code + assinaturas de erro**; **nunca
  mascara falha** (N3↔I4). ✅ coberto por §3.3 + teste `failure_never_masked`.
- **Medição**: tokens reinjetados por execução (counter novo); `masked_failure_rate` = 0.

## 6. Gate de qualidade (50-dim)

`summarize.rs` deve sair **≥ Gold (0.80)**, mira Diamond: `touring-quality score` + 6 P0
BLOCK Pass. CC por função ≤ 15 (extração fatorada em helpers por classe de sinal). Sem
`unwrap` em prod (regex via `OnceLock` + `.expect("static regex")`).

## 7. Desbloqueio de C9 (Class-D detector)

C9 (CEG X9, `learn.rs`) cruza a narrativa-do-turno da LLM com o **outcome real**. O
`OutputSummary.exit_code` + `error_lines` são exatamente a fonte-de-verdade que X9 compara
contra o claimed-outcome → gotcha automático + reward negativo quando divergem. **C5 entrega
o sinal; C9 o consome.** Sem C5, C9 só tem o `content_hash` opaco.

## 8. Estimativa de execução (sessão dedicada)

| Fase | Esforço |
|---|---|
| `summarize.rs` (struct + fn + 8 helpers + 7 testes) | ~2-3h |
| Wiring `SandboxResult`/`SandboxOutcome` + populate em `spawn_and_capture` | ~1h |
| Counter `gate-metrics` + render inline no `GatewayOutcome` | ~1h |
| Gate (cargo test touring-ceg + clippy + touring-quality + update-touring) | ~1h |

**Total ~M (1 sessão dedicada)** — agora **pura execução** (design fechado, VGP feito, blast mapeado).
