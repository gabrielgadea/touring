# Plano: Cronflow-Inspired Touring Enhancements

**Data**: 24/04/2026 | **Autor**: TACO (Gabriel Gadea) | **Versão**: 1.0

## Objetivo

Implementar 4 features derivadas da análise de Cronflow (workflow automation engine TypeScript/Rust) para potencializar o Touring:

| ID | Feature | Prioridade | Impacto | Effort |
|----|---------|------------|---------|--------|
| **A** | HookContext Unification | 🔴 HIGH | Enables all below | M |
| **B** | Task Execution Visualization | 🔴 HIGH | Debugging, UX | M |
| **C** | Step-Level SQLite Tracking | 🟡 MEDIUM | Analytics | S |
| **D** | Schema Validation Layer | 🟡 MEDIUM | Robustness | L |

---

## Contexto — Lições de Cronflow

Cronflow (108 stars, v0.11.6) é um workflow automation engine TypeScript/Rust com padrões que o Touring pode absorver:

1. **Context propagation uniforme**: `ctx.payload/last/meta/services` — cada step recebe contexto consistente
2. **Chainable API**: `.onWebhook().step().action()` — declarative composition
3. **Execution visualization**: Timing, cache status, state persistence
4. **Schema validation**: Zod em boundaries para catch errors early

**Referência**: `~/.claude/rust/docs/2026-04-24-cronflow-analysis.md`

---

## FEATURE A — HookContext Unification

### Problema Atual

Cada hook (`pre_read`, `pre_edit`, `post_edit`, `pre_write`, etc.) recebe payload `&serde_json::Value` e parseia ad-hoc:

```rust
// current pattern - different per hook
fn pre_edit_handler(rt: &mut HookRuntime, payload: &Value) -> HookResponse {
    let file_path = payload.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let old_string = payload.get("old_string").and_then(|v| v.as_str());
    // ...
}
```

Não há uniformidade, não há `ctx.last` (output do hook anterior), não há metadados estruturados.

### Solução Proposta

Criar `HookContext` struct único passado a todos os hooks:

```rust
// crates/touring-hooks/src/shared/hook_context.rs

#[derive(Clone, Debug)]
pub struct HookMeta {
    pub timestamp: DateTime<Utc>,
    pub session_id: Uuid,
    pub file_path: Option<PathBuf>,
    pub tool_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HookServices<'a> {
    pub knowledge: &'a FileKnowledgeDB,
    pub symbol_index: Option<&'a SymbolIndex>,
    pub session_bus: &'a SessionBus,
}

#[derive(Clone, Debug)]
pub struct HookContext<'a> {
    /// Nome do hook: "pre_read", "pre_edit", etc
    pub hook_name: &'static str,
    /// Payload JSON de entrada (raw)
    pub payload: &'a Value,
    /// Output do hook anterior na chain (pre_edit→post_edit)
    pub last: Option<Value>,
    /// Metadados da execução
    pub meta: HookMeta,
    /// Serviços compartilhados (read-only)
    pub services: HookServices<'a>,
}

impl<'a> HookContext<'a> {
    /// Helper para extrair campo como &str
    pub fn get_str(&self, key: &str) -> Option<&'a str> {
        self.payload.get(key).and_then(|v| v.as_str())
    }
    
    /// Helper para extrair campo como u64
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.payload.get(key).and_then(|v| v.as_u64())
    }
    
    /// Helper para extrair campo como bool
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.payload.get(key).and_then(|v| v.as_bool())
    }
}
```

### Arquitetura de Hook Chaining

```
┌──────────────────────────────────────────────────────────────┐
│ Hook Chain: pre_read → pre_edit → post_edit                  │
├──────────────────────────────────────────────────────────────┤
│ pre_read(ctx, rt)                                            │
│   → output = ctx.last (None for first hook)                  │
│   → result = compute_context(ctx)                            │
│   → session_bus.add_hook_result("pre_read", result)         │
│                                                              │
│ pre_edit(ctx, rt)                                            │
│   → ctx.last = session_bus.get_last_hook_result("pre_read")  │
│   → ctx.payload includes pre_read output via ctx.last        │
│   → result = compute_blast_radius(ctx)                       │
│   → session_bus.add_hook_result("pre_edit", result)         │
│                                                              │
│ post_edit(ctx, rt)                                           │
│   → ctx.last = session_bus.get_last_hook_result("pre_edit")   │
│   → quality = assess_quality_delta(ctx)                       │
│   → inject RL reward for chain completion                     │
└──────────────────────────────────────────────────────────────┘
```

### Estrutura de Arquivos

```
crates/touring-hooks/src/
  shared/
    hook_context.rs     # NOVO - HookContext, HookMeta, HookServices
    session_bus.rs     # EXISTENTE - SessionBus para inter-hook comm
```

### Dependências

- Nenhuma crate nova necessária
- Usa `chrono::DateTime<Utc>`, `uuid::Uuid`, `serde_json::Value` (já presentes)

---

## FEATURE B — Task Execution Visualization

### Problema Atual

`touring decompose status` mostra estado estático (pending/completed):

```
task_12345: "implement feature X"
  sub_1: completed
  sub_2: in_progress  
  sub_3: pending
  sub_4: pending
```

Não há timeline, não há timing, não há cache status, não há resume capability.

### Solução Proposta

Novo comando `touring workflow run <task_id>` com output streaming:

```
[▶] task_12345 "implement feature X"
├── [✓] sub_1 "research" — 23ms (cached)
├── [▶] sub_2 "implement" — running 1.2s
├── [ ] sub_3 "test" — pending (dep: sub_2)
└── [ ] sub_4 "docs" — pending (dep: sub_3)
```

Formato: JSON events para stdout (parseable por tools externos):

```json
{"event":"task_start","task_id":"task_12345","timestamp":"2026-04-24T10:30:00Z"}
{"event":"subtask_start","subtask_id":"sub_1","started_at":"2026-04-24T10:30:00.023Z"}
{"event":"subtask_complete","subtask_id":"sub_1","duration_ms":23,"cache_hit":true}
{"event":"subtask_start","subtask_id":"sub_2","started_at":"2026-04-24T10:30:00.050Z"}
```

### Arquitetura

```
cli_handlers_decompose.rs
  cli_workflow_run()     # NOVO - main entry point
  cli_workflow_status()  # NOVO - real-time status polling
  cli_workflow_resume()  # NOVO - resume after crash/interrupt

subtask_results table    # FEATURE C - execution tracking
```

### Comando CLI

```bash
# Run task with real-time visualization
touring workflow run task_12345

# Run with JSON output (for scripting)
touring workflow run task_12345 --json

# Watch mode (continuous)
touring workflow run task_12345 --watch

# Resume after interrupt
touring workflow resume task_12345

# Analytics
touring workflow analytics task_12345
```

---

## FEATURE C — Step-Level SQLite Tracking

### Problema Atual

`decomposition_subtasks` table tem `status` mas não tem execution metadata:

```sql
current schema:
subtask_id, task_id, description, depends_on, status, priority,
deadline, deadline_behavior, review_required, complexity_hint,
retry_policy, attempts, quality_score, created_at, updated_at
```

Falta: started_at, completed_at, duration_ms, cache_hit, output_json, error.

### Solução Proposta

Nova tabela `subtask_results` (clean separation, não modifica schema existing):

```sql
CREATE TABLE subtask_results (
    id TEXT PRIMARY KEY,
    subtask_id TEXT NOT NULL REFERENCES decomposition_subtasks(subtask_id),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_ms INTEGER,
    cache_hit BOOLEAN DEFAULT FALSE,
    output_json TEXT,
    error TEXT
);

CREATE INDEX idx_results_subtask ON subtask_results(subtask_id);
CREATE INDEX idx_results_started ON subtask_results(started_at);
```

### Instrumentação

```rust
// Em cada transição de status em cli_handlers_decompose.rs:

fn update_subtask_status(subtask_id: &str, new_status: &str) {
    match new_status {
        "in_progress" => {
            // Insert started_at
            conn.execute(
                "INSERT INTO subtask_results (id, subtask_id, started_at) VALUES (?1, ?2, ?3)",
                params![uuid(), subtask_id, now],
            );
        }
        "completed" | "failed" => {
            // Update completed_at + duration_ms
            let started: String = conn.query_row(
                "SELECT started_at FROM subtask_results WHERE subtask_id = ?1 AND completed_at IS NULL",
                params![subtask_id],
                |row| row.get(0),
            ).unwrap_or_default();
            let duration = (now_ts - parse_rfc3339(&started).unwrap_or(0)) as i64;
            
            conn.execute(
                "UPDATE subtask_results SET completed_at = ?1, duration_ms = ?2 WHERE subtask_id = ?3 AND completed_at IS NULL",
                params![now, duration, subtask_id],
            );
        }
    }
}
```

### Queries Analíticas

```bash
# Estatísticas de task
touring workflow stats task_12345
# {"total_subtasks": 4, "completed": 1, "failed": 0, "avg_duration_ms": 847, "cache_hit_rate": 0.67}

# Slowest steps
touring workflow slowest task_12345 --top 5
# [{"subtask_id": "sub_3", "duration_ms": 4521, "cache_hit": false}, ...]

# Compare tasks
touring workflow compare task_12345 task_67890
```

---

## FEATURE D — Schema Validation Layer

### Problema Atual

Hooks parse payload ad-hoc sem validação:

```rust
// current - no validation, silent failures
let file_path = payload.get("file_path")
    .and_then(|v| v.as_str())
    .unwrap_or("");  // empty string on missing - bad!
```

### Solução Proposta

Usar crate `validator` (Rust ecosystem standard) para derive-based validation:

```toml
# crates/touring-hooks/Cargo.toml
[dependencies]
validator = { version = "0.18", features = ["derive"] }
```

#### Hook Payload Schemas

```rust
// crates/touring-hooks/src/schemas/hook_payloads.rs

use validator::Validate;

#[derive(Validate, Debug)]
pub struct PreEditPayload {
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,
    
    #[validate(length(max = 1000000, message = "old_string exceeds size limit"))]
    pub old_string: Option<String>,
    
    #[validate(length(max = 1000000, message = "new_string exceeds size limit"))]
    pub new_string: Option<String>,
    
    pub cursor_position: Option<u32>,
    pub selection: Option<Selection>,
}

#[derive(Validate, Debug)]
pub struct PreReadPayload {
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,
    
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Validate, Debug)]
pub struct PreWritePayload {
    #[validate(length(min = 1, message = "file_path cannot be empty"))]
    pub file_path: String,
    
    pub content: Option<String>,
}

#[derive(Validate, Debug)]
pub struct PostEditPayload {
    #[validate(length(min = 1))]
    pub file_path: String,
    
    #[validate(length(min = 1))]
    pub old_string: String,
    
    #[validate(length(min = 1))]
    pub new_string: String,
    
    pub cursor_position: Option<u32>,
}
```

#### Validation Middleware

```rust
// crates/touring-hooks/src/schemas/validation.rs

pub fn validate_payload<T: Validate>(
    payload: &serde_json::Value
) -> Result<T, Vec<ValidationError>> {
    let typed: T = serde_json::from_value(payload.clone())
        .map_err(|e| vec![ValidationError { 
            field: "unknown".to_string(), 
            message: e.to_string() 
        }])?;
    
    typed.validate()
        .map_err(|e| e.field_errors().iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |e| ValidationError {
                    field: field.to_string(),
                    message: e.message.unwrap_or(&Cow::Owned("invalid".to_string())).to_string(),
                })
            })
            .collect());
    
    Ok(typed)
}

// Hook handler wrapper
macro_rules! with_validation {
    ($handler:ident, $payload_type:ident) => {
        fn $handler(rt: &mut HookRuntime, payload: &serde_json::Value) -> HookResponse {
            match validate_payload::<$payload_type>(payload) {
                Ok(validated) => actual_handler(rt, validated),
                Err(errors) => {
                    let msg = errors.iter()
                        .map(|e| format!("{}: {}", e.field, e.message))
                        .join("; ");
                    HookResponse::Deny { 
                        reason: format!("validation_failed: {}", msg),
                        context: None,
                        event_name: Some(stringify!($handler).to_string()),
                    }
                }
            }
        }
    };
}
```

#### MCP Tool Parameter Schemas

```rust
// crates/touring-hooks/src/schemas/mcp_params.rs

#[derive(Validate, Debug)]
pub struct DecomposeCreateParams {
    #[validate(length(min = 1, max = 100))]
    pub task_type: String,
    
    #[validate(length(min = 1, max = 1000))]
    pub description: String,
    
    pub origin: Option<String>,
    pub cila_level: Option<u8>,
}

#[derive(Validate, Debug)]
pub struct DecomposeAddParams {
    #[validate(length(min = 1))]
    pub task_id: String,
    
    #[validate(length(min = 1))]
    pub subtask_id: String,
    
    #[validate(length(min = 1, max = 1000))]
    pub description: String,
    
    pub depends_on: Option<Vec<String>>,
    pub priority: Option<u8>,
}

#[derive(Validate, Debug)]
pub struct MemoryStoreParams {
    #[validate(length(min = 1))]
    pub key: String,
    
    #[validate(length(min = 1))]
    pub value: String,
    
    pub tier: Option<String>,  // "semantic" | "local"
    pub memory_type: Option<String>,  // "lesson" | "pattern" | "insight" | "gotcha"
}
```

---

## Deliverables

### Feature A — HookContext Unification

| # | Deliverable | Arquivo | Effort |
|---|-------------|---------|--------|
| A1 | `HookMeta` struct | `shared/hook_context.rs` | S |
| A2 | `HookServices` struct | `shared/hook_context.rs` | S |
| A3 | `HookContext` struct + helpers | `shared/hook_context.rs` | M |
| A4 | `HookContext::from_payload()` constructor | `shared/hook_context.rs` | M |
| A5 | Refactor pre_read_handler (pilot) | `hooks/pre_read.rs` | M |
| A6 | Refactor pre_edit_handler | `hooks/pre_edit.rs` | M |
| A7 | Refactor post_edit_handler | `hooks/post_edit.rs` | M |
| A8 | session_bus hook_result storage | `shared/session_bus.rs` | M | ✅ DONE — `add_hook_result`/`get_last_hook_result` + test |
| A9 | Hook chaining via ctx.last | all hooks | L |
| A10 | RL reward per chain | `post_tool_rl.rs` | M |
| A11 | E2E tests for chaining | `tests/hook_chaining.rs` | M |

### Feature B — Task Execution Visualization

| # | Deliverable | Arquivo | Effort |
|---|-------------|---------|--------|
| B1 | `subtask_results` table schema | `cli_handlers_decompose.rs` | S |
| B2 | `cli_workflow_run()` handler | `cli_handlers_decompose.rs` | M |
| B3 | JSON event streaming to stdout | `cli_handlers_decompose.rs` | M |
| B4 | `cli_workflow_status()` polling | `cli_handlers_decompose.rs` | S |
| B5 | `cli_workflow_resume()` after crash | `cli_handlers_decompose.rs` | M |
| B6 | ANSI colored terminal output | `cli_handlers_decompose.rs` | S |
| B7 | `--watch` mode continuous | `cli_handlers_decompose.rs` | M |
| B8 | `cli_workflow_analytics()` | `cli_handlers_decompose.rs` | M |
| B9 | E2E tests | `tests/workflow_e2e.rs` | M |

### Feature C — Step-Level SQLite Tracking

| # | Deliverable | Arquivo | Effort |
|---|-------------|---------|--------|
| C1 | `subtask_results` CREATE TABLE | `knowledge.rs` | S |
| C2 | Instrument status transitions | `cli_handlers_decompose.rs` | M |
| C3 | Track cache_hit on cached results | `cli_handlers_decompose.rs` | S |
| C4 | `touring workflow stats` command | `cli_handlers_decompose.rs` | M |
| C5 | `touring workflow slowest` command | `cli_handlers_decompose.rs` | M |
| C6 | `touring workflow compare` command | `cli_handlers_decompose.rs` | M |
| C7 | Analytics E2E tests | `tests/analytics.rs` | M |

### Feature D — Schema Validation Layer

| # | Deliverable | Arquivo | Effort |
|---|-------------|---------|--------|
| D1 | Add validator dependency | `Cargo.toml` | S |
| D2 | PreEditPayload schema | `schemas/hook_payloads.rs` | M |
| D3 | PreReadPayload schema | `schemas/hook_payloads.rs` | S |
| D4 | PreWritePayload schema | `schemas/hook_payloads.rs` | S |
| D5 | PostEditPayload schema | `schemas/hook_payloads.rs` | S |
| D6 | PostToolFailurePayload schema | `schemas/hook_payloads.rs` | S |
| D7 | Validation middleware `validate_payload()` | `schemas/validation.rs` | M |
| D8 | `with_validation!` macro | `schemas/validation.rs` | M |
| D9 | Wire validation into all hook handlers | `hooks/*.rs` | L |
| D10 | DecomposeCreateParams schema | `schemas/mcp_params.rs` | S |
| D11 | DecomposeAddParams schema | `schemas/mcp_params.rs` | S |
| D12 | MemoryStoreParams schema | `schemas/mcp_params.rs` | S |
| D13 | Validation E2E tests | `tests/validation.rs` | M |

---

## Timeline

### Sprint 1 (Dias 1-5) — Feature A Phase 1 + Feature C

```
Dia 1-2: Feature C (Step-Level SQLite Tracking)
  - C1: subtask_results table
  - C2-C3: Instrument status transitions
  - Result: SQL tracking infrastructure for B

Dia 3-5: Feature A Phase 1 (HookContext struct)
  - A1-A4: Create HookContext, HookMeta, HookServices
  - Result: Foundation for all hook refactoring
```

### Sprint 2 (Dias 6-12) — Feature A Phase 2 + Feature B

```
Dia 6-8: Feature A Phase 2 (Hook handler refactor)
  - A5-A7: Refactor pre_read, pre_edit, post_edit
  - A8: session_bus hook result storage
  - Result: All hooks can use HookContext

Dia 9-12: Feature B (Task Execution Visualization)
  - B1-B4: workflow run + streaming
  - B5-B7: resume + watch mode
  - B8-B9: analytics + tests
  - Result: Visual task execution with timing
```

### Sprint 3 (Dias 13-18) — Feature A Phase 3-4 + Feature D

```
Dia 13-15: Feature A Phase 3-4 (Hook chaining + RL)
  - A9: Hook chaining via ctx.last
  - A10: RL reward per chain
  - A11: E2E tests
  - Result: Hooks compose with shared context

Dia 16-18: Feature D (Schema Validation)
  - D1-D8: Hook payload schemas + middleware
  - D9-D13: Wire into handlers + MCP params + tests
  - Result: Type-safe payloads with clear errors
```

---

## Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **Breaking change to CLI handlers** | HIGH | HIGH | Comprehensive test suite before merge; flag day for API change |
| **Performance overhead** (validation adds ~1ms) | LOW | LOW | Benchmark before/after; validation is opt-in for slow paths |
| **Terminal buffering** (streaming JSON) | MEDIUM | MEDIUM | Use `std::io::Write::flush()` after each event; `--json` mode for scripting |
| **session_bus deadlock** under heavy hook load | MEDIUM | HIGH | Use `try_lock()` for session_bus access; fallback to direct DB on lock failure |
| **Schema migration** on existing DB | LOW | HIGH | Use `CREATE TABLE IF NOT EXISTS` idempotent pattern; never DROP columns |

---

## Validation

### Feature A — HookContext Unification

```bash
# Test hook chaining
touring pre-read '{"file_path": "src/main.rs"}'
# Should output: ctx.last = None for first hook

touring pre-edit '{"file_path": "src/main.rs", "old_string": "foo", "new_string": "bar"}'
# Should have ctx.last = pre_read output (via session_bus)
```

### Feature B — Task Execution Visualization

```bash
# Run task and check streaming output
timeout 5 touring workflow run task_12345 || true
# Should show animated progress with timing

# Resume after Ctrl+C
touring workflow resume task_12345
# Should continue from where it left off
```

### Feature C — Step-Level SQLite Tracking

```bash
# Check analytics
touring workflow stats task_12345
# {"avg_duration_ms": 234, "cache_hit_rate": 0.72}

# Slowest steps
touring workflow slowest task_12345 --top 3
# Should show step name + duration
```

### Feature D — Schema Validation

```bash
# Test validation error
touring pre-edit '{"file_path": ""}'
# Should return: validation_failed: file_path cannot be empty

# Test valid payload
touring pre-edit '{"file_path": "src/main.rs", "old_string": "foo", "new_string": "bar"}'
# Should proceed normally
```

---

## T-Shirt Sizing

| Feature | Complexity | Notes |
|---------|------------|-------|
| A — HookContext | M | 11 deliverables, mostly mechanical refactor |
| B — Task Visualization | M | 9 deliverables, CLI streaming is tricky |
| C — SQLite Tracking | S | 7 deliverables, mostly additive schema |
| D — Schema Validation | L | 13 deliverables, validation across many hooks |

**Total**: 40 deliverables across 4 features, ~18 sprints (90 days) at 2-3 deliverables/day.

---

## Dependencies Entre Features

```
Feature C (SQLite tracking) ──→ Feature B (Visualization)
     │                                │
     └──── Feature A (HookContext) ──┘
                  │
                  └──── Feature D (Validation)
                  
Order: C → B → A → D (S before M before L)
```

**Rationale**: 
- C (S) is quick win, gives SQL infrastructure for B
- B (M) needs C's tracking data  
- A (M) is foundational but risky - do middle
- D (L) is extensive validation across hooks - do last when architecture is stable