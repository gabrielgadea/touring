# Touring + devrc Integration Master Plan

> **Date**: 2026-04-25 | **Status**: PLANNED | **Author**: TACO | **Version**: 1.0

---

## Executive Summary

This document defines the comprehensive integration plan between **Touring** (v30.3.0) and **devrc** (v0.6.0), combining Touring's context awareness, RL feedback loop, and Claude Code integration with devrc's mature YAML-first task definition UX, templating, environment handling, and plugin architecture.

**Outcome**: Touring becomes the ultimate Claude Code task orchestration layer with:
- Declarative YAML task definitions (familiar like GitHub Actions/Ansible)
- Tera template engine for dynamic task descriptions
- Environment file loading (.env)
- Include resolution (local + remote URLs with auth)
- Deno plugin sandbox runtime
- Full deadline/retry/review enforcement
- Complete event audit trail

---

## Part I: Ground Truth — Current State

### 1.1 Touring Task Database Schema

**Location**: `~/.claude/touring/knowledge.db` (143 MB, 38 active tasks, 267 total subtasks)

```sql
-- Core task table
CREATE TABLE task_decompositions (
    task_id TEXT PRIMARY KEY,           -- e.g., "task_1745612345678901234"
    task_type TEXT NOT NULL,            -- "intent" | "bug" | "refactor"
    description TEXT NOT NULL,           -- Human-readable description
    cila_level INTEGER NOT NULL DEFAULT 3,  -- CILA complexity 1-5
    created_at TEXT NOT NULL,           -- ISO8601
    updated_at TEXT NOT NULL,
    archived_at TEXT,                   -- NULL = active
    status TEXT NOT NULL DEFAULT 'active',  -- "active" | "finalized" | "archived"
    metrics TEXT,                       -- JSON metrics (NEVER POPULATED)
    origin TEXT NOT NULL DEFAULT 'claude-code',  -- Provenance
    mirrored_to_cc INTEGER NOT NULL DEFAULT 1   -- Bidirectional sync flag
);

-- Subtask table
CREATE TABLE decomposition_subtasks (
    subtask_id TEXT PRIMARY KEY,       -- e.g., "task_xxx::sub_1"
    task_id TEXT NOT NULL,
    description TEXT NOT NULL,
    depends_on TEXT NOT NULL DEFAULT '[]',  -- JSON array of subtask_ids
    priority INTEGER NOT NULL DEFAULT 255,   -- 1=highest, 255=lowest
    status TEXT NOT NULL,               -- "pending" | "in_progress" | "completed" | "failed" | "skipped"
    deadline TEXT,                     -- ISO8601 (UNUSED - no CLI flag, no enforcement)
    deadline_behavior TEXT DEFAULT 'Fail',  -- Only 'Fail' implemented
    review_required INTEGER NOT NULL DEFAULT 0,  -- UNUSED
    complexity_hint TEXT,              -- Produced but never consumed
    retry_policy TEXT,                 -- JSON (stored but not enforced)
    attempts INTEGER NOT NULL DEFAULT 0,  -- Incremented only in name
    quality_score REAL,                -- Written but not gated
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES task_decompositions(task_id)
);

-- Event audit trail (NEVER WRITTEN - 0 rows)
CREATE TABLE decomposition_events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    subtask_id TEXT,
    event_type TEXT NOT NULL,          -- "created" | "started" | "completed" | "failed"
    payload TEXT NOT NULL,             -- JSON event data
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Snapshot table (NEVER WRITTEN)
CREATE TABLE decomposition_snapshots (
    snapshot_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    subtasks_snapshot TEXT NOT NULL,   -- JSON
    metrics_snapshot TEXT NOT NULL,    -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Subtask execution results
CREATE TABLE subtask_results (
    id TEXT PRIMARY KEY,
    subtask_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_ms INTEGER,
    cache_hit INTEGER NOT NULL DEFAULT 0,
    output_json TEXT,
    error TEXT,
    FOREIGN KEY (subtask_id) REFERENCES decomposition_subtasks(subtask_id)
);
```

### 1.2 Touring CLI Handlers

| Handler | Location | Status | Notes |
|---------|----------|--------|-------|
| `cli_decompose_create` | cli_handlers.rs:2073 | ✅ Working | Generates task_id from unix_nanos |
| `cli_decompose_add` | cli_handlers.rs:2414 | ✅ Working | Scopes subtask_id as `task_id::subtask_id` |
| `cli_decompose_get` | cli_handlers.rs:2462 | ✅ Working | Returns task + subtasks array |
| `cli_decompose_update` | cli_handlers.rs:2518 | ✅ Working | FIX-6 applied: subtask_id alone triggers update |
| `cli_decompose_validate` | cli_handlers.rs:2584 | ✅ Working | DFS cycle detection |
| `cli_decompose_status` | cli_handlers.rs:2654 | ✅ Working | Aggregate counters |
| `cli_decompose_finalize` | cli_handlers.rs:2681 | ✅ Working | Fires RL reward 1.0 |
| `cli_decompose_ready` | cli_handlers.rs:2786 | ✅ Working | Dep resolution + priority sorting |
| `cli_decompose_event` | cli_handlers.rs:2893 | ⚠️ Defined but NEVER CALLED | Dead code |
| `cli_suggest_action` | cli_handlers.rs:2132 | ✅ Working | Pln3 bidirectional suggestions |

### 1.3 Claude Code Integration Hooks

| Hook | Purpose | Status |
|------|---------|--------|
| `pre_task_scout` | Enrichment before task execution | ✅ Working |
| `task_created` | Fire when task created | ✅ Working |
| `task_completed` | Fire when task completed | ✅ Working |
| `decompose_event` | Lifecycle events | ⚠️ Defined but never wired |
| `enter_plan_mode` | Plan mode entry | ✅ Working |
| `exit_plan_mode` | Plan mode exit | ✅ Working |
| `task-sync-create` | Bidirectional task sync | ✅ Working |
| `task-sync-update` | Bidirectional task sync | ✅ Working |
| `task-sync-list` | Bidirectional task sync | ✅ Working |
| `post_tool_rl` | RL reward computation | ✅ Working |
| `instructions-loaded` | Context injection | ✅ Working |

### 1.4 Touring Already Has (devrc-equivalent features)

| Feature | Location | Status |
|---------|----------|--------|
| Template Engine (Tera) | `touring-generator/src/template/` | ✅ EXISTS - needs wiring |
| WASM Plugins | `touring-wasm/` + `inferlets/` | ✅ EXISTS - needs expansion |
| Environment Handling | Partial in `HookRuntime` | ❌ INCOMPLETE |
| Include Resolution | None | ❌ MISSING |
| Parameter Schema | CLI flags partial | ❌ INCOMPLETE |

---

## Part II: The 14 Schema Gaps (Critical Fixes)

### Gap 1: `deadline` Column Unused
**Problem**: Column exists but `cli_decompose_add` accepts no `--deadline` flag.

**Fix**:
```rust
// cli_decompose_add: Add deadline parameter
struct DecomposeAddParams {
    task_id: String,
    subtask_id: String,
    description: String,
    depends_on: Option<String>,
    priority: Option<String>,
    deadline: Option<String>,      // NEW: ISO8601 timestamp
}
```

**Enforcement** (in `cli_decompose_ready` or new `cli_decompose_check_deadlines`):
```rust
fn check_deadlines(task_id: &str) -> Vec<SubtaskDeadlineBreach> {
    // For each subtask with deadline < now() AND status != completed:
    //   match deadline_behavior:
    //     "Fail"    => set status = "failed"
    //     "Skip"    => set status = "skipped"
    //     "Notify"  => emit notification, keep pending
    //     "Backburner" => lower priority
}
```

### Gap 2: `deadline_behavior` Only 'Fail'
**Problem**: Default is 'Fail' but no alternate behaviors implemented.

**Fix**: Implement all 4 behaviors (see Gap 1 above).

### Gap 3: `retry_policy` Stored But Not Enforced
**Problem**: JSON retry policy stored but no retry loop exists.

**Schema** (already supports):
```json
{
  "max_attempts": 3,
  "backoff_ms": 1000,
  "backoff_multiplier": 2.0,
  "retry_on": ["failed", "timeout"]
}
```

**Fix**:
```rust
// In cli_decompose_update, after status = "failed":
fn evaluate_retry_policy(subtask: &mut Subtask, policy: &RetryPolicy) -> bool {
    if subtask.attempts >= policy.max_attempts {
        return false; // Give up
    }
    // Otherwise: increment attempts, reset status to pending, schedule backoff
    subtask.attempts += 1;
    subtask.status = "pending";
    true
}
```

### Gap 4: `review_required` Never Checked
**Problem**: Column exists but no handler enforces review gate.

**Fix** (in `cli_decompose_finalize`):
```rust
// Before marking task as finalized:
for subtask in &subtasks {
    if subtask.review_required && subtask.quality_score.is_none() {
        return Err("Subtask requires review before completion");
    }
}
```

### Gap 5: `complexity_hint` Produced But Not Consumed
**Problem**: TACO generates complexity hints but decompose CLI doesn't accept them.

**Fix**:
```rust
// Add to cli_decompose_add:
let complexity_hint = params.complexity_hint
    .or_else(|| estimate_complexity(&description));

// Add --complexity-hint flag to CLI
```

### Gap 6: `decomposition_events` Never Written (0 rows)
**Problem**: Table exists with correct schema but no code writes to it.

**Fix** (wire `cli_decompose_event` into all handlers):
```rust
fn log_event(conn: &Connection, task_id: &str, subtask_id: Option<&str>, event_type: &str, payload: &serde_json::Value) {
    conn.execute(
        "INSERT INTO decomposition_events (task_id, subtask_id, event_type, payload) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![task_id, subtask_id, event_type, serde_json::to_string(payload).unwrap()],
    ).ok(); // Best-effort
}

// Wire at:
log_event(conn, task_id, None, "task_created", &payload);
log_event(conn, task_id, Some(subtask_id), "subtask_added", &payload);
log_event(conn, task_id, Some(subtask_id), "subtask_started", &payload);
log_event(conn, task_id, Some(subtask_id), "subtask_completed", &payload);
log_event(conn, task_id, None, "task_finalized", &payload);
```

### Gap 7: `decomposition_snapshots` Never Written
**Problem**: Pre-compact checkpoint not implemented.

**Fix** (in `hook_decompose_bridge.rs`):
```rust
// bridge_precompact_checkpoint - documented but NOT implemented
pub fn bridge_precompact_checkpoint(rt: &HookRuntime, task_id: &str) {
    let subtasks = get_subtasks(rt, task_id);
    let metrics = compute_metrics(rt, task_id);
    let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4());
    
    conn.execute(
        "INSERT INTO decomposition_snapshots (snapshot_id, task_id, subtasks_snapshot, metrics_snapshot) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![snapshot_id, task_id, serde_json::to_string(&subtasks).unwrap(), serde_json::to_string(&metrics).unwrap()],
    ).ok();
}
```

### Gap 8: `template` Param Not Supported
**Problem**: No `--template` flag to create task from predefined template.

**Fix**:
```yaml
# templates/devrc-standard.yml
---
templates:
  standard:
    desc: "Standard development workflow"
    tasks:
      - build: "cargo build --release"
      - test: "cargo test"
      - deploy: "deploy.sh"
```

### Gap 9: `env_file` Param Not Supported
**Problem**: No way to load .env files for task context.

**Fix**:
```rust
// env_file_loader module
pub fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)?;
    parse_env_content(&content)  // Handle KEY=value, "KEY"="value", export KEY=value
}

// Wire into task execution context
```

### Gap 10: `include` Param Not Supported
**Problem**: No include resolution (local files or URLs).

**Fix**:
```rust
pub struct IncludeSpec {
    file: Option<PathBuf>,      // Local file
    url: Option<String>,         // Remote URL
    path_resolve: String,       // "relative" | "absolute"
    auth: Option<NetrcAuth>,    // .netrc credentials
}

pub fn resolve_include(spec: &IncludeSpec) -> Result<String> {
    match &spec.file {
        Some(path) => Ok(std::fs::read_to_string(path)?),
        None => {
            let content = fetch_url(&spec.url)?;
            if let Some(auth) = &spec.auth {
                apply_netrc_auth(&mut reqwest_client, auth)?;
            }
            Ok(content)
        }
    }
}
```

### Gap 11: `parallel_groups` Not Exposed in CLI
**Problem**: `parallel_groups_with_profile()` exists in plan_generator but no CLI flag.

**Fix**:
```bash
# New CLI flag
touring decompose add task_xxx sub_1 "step1" --parallel-group=group_a
touring decompose add task_xxx sub_2 "step2" --parallel-group=group_a
# sub_1 and sub_2 can run concurrently

# CLI output enhancement
touring decompose ready task_xxx --parallel-groups
# Returns:
# {
#   "ready_subtasks": [...],
#   "parallel_groups": [
#     {"group": "group_a", "subtasks": ["sub_1", "sub_2"], "can_run_concurrent": true}
#   ]
# }
```

### Gap 12: `quality_score` Written But Not Gated
**Problem**: Quality scores accumulate but no minimum threshold enforced.

**Fix** (in `cli_decompose_finalize`):
```rust
// Add quality_threshold parameter
pub fn decompose_finalize(task_id: &str, quality_threshold: Option<f64>) -> Result<()> {
    let min_quality = quality_threshold.unwrap_or(0.0);
    
    for subtask in &subtasks {
        if let Some(score) = subtask.quality_score {
            if score < min_quality {
                return Err(format!("Subtask {} quality {} below threshold {}", 
                    subtask.subtask_id, score, min_quality));
            }
        }
    }
}
```

### Gap 13: `metrics` Column Never Populated
**Problem**: JSON metrics column exists but no handler writes to it.

**Fix**:
```rust
fn compute_task_metrics(task_id: &str) -> TaskMetrics {
    TaskMetrics {
        total_subtasks: count(&subtasks),
        completed: count_status(&subtasks, "completed"),
        failed: count_status(&subtasks, "failed"),
        pending: count_status(&subtasks, "pending"),
        in_progress: count_status(&subtasks, "in_progress"),
        avg_quality: average_quality(&subtasks),
        total_duration_ms: sum_duration(&results),
        completion_pct: completed as f64 / total_subtasks as f64,
    }
}

// Wire into decompose_finalize
conn.execute(
    "UPDATE task_decompositions SET metrics = ?1 WHERE task_id = ?2",
    rusqlite::params![serde_json::to_string(&metrics).unwrap(), task_id],
)?;
```

### Gap 14: `attempts` Column Incremented Only in Name
**Problem**: Column defaults to 0, conceptually tied to retry_policy but not incremented.

**Fix**: See Gap 3 (retry loop implementation).

---

## Part III: devrc Integration Architecture

### 3.1 What devrc Brings

| Feature | devrc Implementation | Touring Equivalent | Gap |
|---------|----------------------|-------------------|-----|
| YAML Devrcfile | 29-module Rust crate | Internal decompose | **MISSING: YAML format** |
| Jinja2/Tera templates | `devrc::template` (Tera) | `touring-generator/template/` | **EXISTS but not wired to tasks** |
| Environment files | `devrc::env_file` | None | **MISSING** |
| Include (local) | `devrc::include` | None | **MISSING** |
| Include (URL) | `devrc::include` + `.netrc` | None | **MISSING** |
| Parameters | `params:` field with defaults | CLI flags only | **INCOMPLETE** |
| Deps | `deps: [task1, task2]` | `depends_on` JSON array | ✅ Working |
| Hooks | `before_task`, `after_task` | `pre_task_scout`, `task_completed` | ✅ Working |
| Deno runtime | `devrc_config.plugins.deno-runtime` | WASM plugins | **WASM exists, Deno missing** |
| Netrc auth | `devrc::netrc` | None | **MISSING** |
| Cache TTL | `devrc_config.cache_ttl` | Moka cache | ✅ Working |

### 3.2 Devrcfile Schema (Reference)

```yaml
# Devrcfile - devrc task definition format
---
# Global configuration
devrc_config:
  shell: /bin/bash
  log_level: info
  cache_ttl: 3600  # seconds
  plugins:
    deno-runtime: ./plugin.dylib
  interpreter:
    runtime: deno-runtime
    permissions:
      allow-net: api.github.com
      allow-env: GITHUB_TOKEN

# Global variables (template context)
variables:
  project_name: myapp
  rust_version: "1.75"

# Environment files to load
env_file:
  - .env.local
  - .env.production

# Global environment variables
environment:
  RUST_BACKTRACE: "1"
  CARGO_HOME: "{{ env.CARGO_HOME }}"

# Hooks
before_script:
  - echo "Starting..."
after_script:
  - echo "Done!"

before_task:
  - echo "Task starting: {{ task_name }}"
after_task:
  - echo "Task completed: {{ task_name }}"

# Include other files
include:
  - file: ./shared-tasks.yml
    path_resolve: relative
  - url: "https://raw.githubusercontent.com/org/repo/main/tasks.yml"
    auth:
      machine: api.github.com
      type: bearer
      token: "{{ secrets.GH_TOKEN }}"

# Task definitions
tasks:
  build:
    desc: "Build the project"
    params:
      profile:
        required: false
        default: release
      target:
        required: false
        default: x86_64-unknown-linux-gnu
    environment:
      BUILD_PROFILE: "{{ params.profile }}"
    exec:
      - cargo build --{{ params.profile }} --target {{ params.target }}
    tags:
      - ci
      - fast

  test:
    desc: "Run tests with coverage"
    deps: [build]
    exec:
      - cargo test --coverage
    timeout: 300s
    tags:
      - ci
      - quality

  deploy:
    desc: "Deploy to {{ environment }}"
    params:
      environment:
        required: true
        options: [staging, production]
      service:
        required: true
    deps: [test]
    environment:
      DEPLOY_ENV: "{{ params.environment }}"
    exec:
      - ./deploy.sh {{ params.service }} {{ params.environment }}
    hooks:
      before_task:
        - echo "Deploying {{ params.service }} to {{ params.environment }}"
    tags:
      - deploy

  lint:
    desc: "Run linters"
    exec:
      - cargo clippy -- -D warnings
      - cargo fmt --check
    tags:
      - ci
```

### 3.3 Tasksfile Format (Touring-Native YAML)

```yaml
# Tasksfile.yml - Touring-native task definition
---
version: "1.0"
metadata:
  name: myproject
  description: My project tasks

# Task templates
templates:
  ci_job:
    timeout: 300s
    tags: [ci]
    retry_policy:
      max_attempts: 2
      backoff_ms: 1000

tasks:
  build:
    desc: "Build with {{ profile }} profile"
    template: true
    command: cargo build --{{ profile }}
    env:
      RUST_BACKTRACE: "1"
    params:
      profile:
        default: release
        options: [debug, release]
    tags: [ci, fast]
    complexity_hint: "medium"

  test:
    desc: "Run tests with coverage"
    deps: [build]
    command: cargo test --coverage
    timeout: 600s
    retry_policy:
      max_attempts: 3
      backoff_ms: 2000
    tags: [ci, quality]

  lint:
    desc: "Run linters"
    command: |
      cargo clippy -- -D warnings
      cargo fmt --check
    tags: [ci]

  deploy:
    desc: "Deploy {{ service }} to {{ environment }}"
    params:
      service:
        required: true
      environment:
        required: true
        default: staging
        options: [staging, production]
    deps: [test]
    env_file:
      - .env.{{ params.environment }}
    command: ./deploy.sh {{ params.service }} {{ params.environment }}
    deadline: "{{ env.DEPLOY_DEADLINE }}"
    deadline_behavior: Fail
    review_required: true
    tags: [deploy]

  full_ci:
    desc: "Full CI pipeline"
    deps: [lint, test]
    command: echo "CI complete"
    tags: [ci]

# Include external tasks
includes:
  - file: ./shared-tasks.yml
  - file: ./database-tasks.yml
  - url: "https://example.com/tasks.yml"
    auth:
      machine: example.com
      type: basic
      username: "{{ secrets.EXAMPLE_USER }}"
      password: "{{ secrets.EXAMPLE_PASS }}"

# Global hooks
hooks:
  before_all:
    - echo "Starting {{ total_tasks }} tasks"
  after_all:
    - echo "Completed all tasks"
  on_failure:
    - echo "Task {{ failed_task }} failed!"
```

---

## Part IV: Implementation Phases

### Phase 1: Schema Completeness (Foundation) — XL

**Effort**: 5-7 days | **Impact**: CRITICAL | **Risk**: LOW

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| S1.1 | Implement `deadline` CLI flag + enforcement | `cli_handlers.rs:2414` | 8 |
| S1.2 | Implement `deadline_behavior` (Fail/Skip/Notify/Backburner) | `cli_handlers.rs` | 6 |
| S1.3 | Implement retry loop with `attempts` tracking | `cli_handlers.rs:2518` | 10 |
| S1.4 | Wire `decomposition_events` logging | `cli_handlers.rs` + `hook_decompose_bridge.rs` | 8 |
| S1.5 | Implement `review_required` gate in finalize | `cli_handlers.rs:2681` | 4 |
| S1.6 | Wire `decomposition_snapshots` for pre-compact | `hook_decompose_bridge.rs` | 6 |
| S1.7 | Populate `metrics` column on finalize | `cli_handlers.rs:2681` | 4 |
| S1.8 | Expose `parallel_groups` in CLI | `cli_decompose_add` + `cli_decompose_ready` | 8 |

**Subtotal**: 54 new tests

### Phase 2: Tasksfile YAML Parser — L

**Effort**: 3-4 days | **Impact**: HIGH | **Risk**: MEDIUM

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| T2.1 | Create `touring-tasksfile-parser` crate | `crates/touring-tasksfile/` | 20 |
| T2.2 | YAML schema validation (serde_yaml) | `parser/schema.rs` | 15 |
| T2.3 | Convert Tasksfile → Decompose DAG | `parser/compiler.rs` | 15 |
| T2.4 | Add `touring tasksfile import` CLI | `cli/tasksfile.rs` | 10 |
| T2.5 | Add `touring tasksfile export` CLI | `cli/tasksfile.rs` | 8 |
| T2.6 | Add `touring tasksfile validate` CLI | `cli/tasksfile.rs` | 8 |

**Subtotal**: 76 new tests

### Phase 3: Template Engine Wiring (Tera) — M

**Effort**: 1-2 days | **Impact**: MEDIUM | **Risk**: LOW

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| T3.1 | Create `TaskTemplater` using existing Tera | `touring-generator/src/template/` | 10 |
| T3.2 | Wire template rendering into task execution | `runner.rs` | 8 |
| T3.3 | Support `{{ params.* }}` substitution | `TaskTemplater` | 6 |
| T3.4 | Support `{{ env.* }}` substitution | `TaskTemplater` | 6 |
| T3.5 | Support `{{ secrets.* }}` with secret masking | `TaskTemplater` | 4 |

**Subtotal**: 34 new tests

### Phase 4: Environment File Loading — M

**Effort**: 1-2 days | **Impact**: MEDIUM | **Risk**: LOW

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| E4.1 | Create `env_file_loader` module | `touring-hooks/src/env_file.rs` | 12 |
| E4.2 | Parse `.env` format (KEY=value, "KEY"="value") | `env_file/parser.rs` | 10 |
| E4.3 | Hierarchy: .env.local > .env > .env.production | `env_file/loader.rs` | 8 |
| E4.4 | Wire into task execution context | `runner.rs` | 6 |

**Subtotal**: 36 new tests

### Phase 5: Include Resolution — L

**Effort**: 2-3 days | **Impact**: MEDIUM | **Risk**: MEDIUM

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| I5.1 | Create `include_resolver` module | `touring-hooks/src/include.rs` | 10 |
| I5.2 | Local file resolution with `path_resolve` | `include/local.rs` | 8 |
| I5.3 | URL fetching with HTTP client | `include/remote.rs` | 10 |
| I5.4 | `.netrc` authentication integration | `include/netrc.rs` | 8 |
| I5.5 | Circular include detection | `include/resolver.rs` | 6 |

**Subtotal**: 42 new tests

### Phase 6: Devrcfile Adapter — M

**Effort**: 2-3 days | **Impact**: HIGH | **Risk**: LOW

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| D6.1 | Create `touring-devrc-adapter` crate | `crates/touring-devrc-adapter/` | 15 |
| D6.2 | Parse devrc `Devrcfile` format | `adapter/parser.rs` | 12 |
| D6.3 | Convert Devrcfile → Touring format | `adapter/converter.rs` | 12 |
| D6.4 | Add `touring devrcfile import` CLI | `cli/devrcfile.rs` | 8 |
| D6.5 | Add `touring devrcfile export` CLI | `cli/devrcfile.rs` | 8 |

**Subtotal**: 55 new tests

### Phase 7: Deno Plugin Runtime — XL

**Effort**: 10-14 days | **Impact**: VERY HIGH | **Risk**: HIGH

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| P7.1 | Create `touring-deno-runtime` crate | `crates/touring-deno-runtime/` | 20 |
| P7.2 | Deno isolate execution sandbox | `deno/isolate.rs` | 15 |
| P7.3 | Permission system (allow-net, allow-env, allow-read) | `deno/permissions.rs` | 20 |
| P7.4 | Plugin manifest schema | `deno/manifest.rs` | 8 |
| P7.5 | Plugin registry and lifecycle | `deno/registry.rs` | 12 |
| P7.6 | Wire into touring hooks system | `hook_registry.rs` | 10 |
| P7.7 | Example: GitHub API plugin | `plugins/github.rs` | 8 |

**Subtotal**: 93 new tests

### Phase 8: Claude Code Deep Integration — L

**Effort**: 3-4 days | **Impact**: HIGH | **Risk**: MEDIUM

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| C8.1 | Task context propagation to subagents | `hook_pre_task_scout.rs` | 10 |
| C8.2 | RL task outcome signals (beyond tool-level) | `hook_post_task_rl.rs` | 8 |
| C8.3 | Subagent visibility/tracking | `hook_task_sync.rs` | 12 |
| C8.4 | Task queue broker hooks | `hook_task_broker.rs` | 8 |

**Subtotal**: 38 new tests

---

## Part V: Grand Totals

| Phase | Effort | New Tests | Risk |
|-------|--------|----------|------|
| Phase 1: Schema Completeness | XL (5-7d) | 54 | LOW |
| Phase 2: Tasksfile YAML Parser | L (3-4d) | 76 | MEDIUM |
| Phase 3: Template Engine Wiring | M (1-2d) | 34 | LOW |
| Phase 4: Environment File Loading | M (1-2d) | 36 | LOW |
| Phase 5: Include Resolution | L (2-3d) | 42 | MEDIUM |
| Phase 6: Devrcfile Adapter | M (2-3d) | 55 | LOW |
| Phase 7: Deno Plugin Runtime | XL (10-14d) | 93 | HIGH |
| Phase 8: Claude Code Integration | L (3-4d) | 38 | MEDIUM |
| **TOTAL** | **~30-40 days** | **428 tests** | |

---

## Part VI: Dependency Graph

```
Phase 1 (Schema)
    │
    ├── Phase 2 (Tasksfile) depends on Phase 1
    │       │
    │       ├── Phase 3 (Templates) depends on Phase 2
    │       │       │
    │       │       └── Phase 4 (Env Files) can run parallel
    │       │
    │       └── Phase 5 (Include) depends on Phase 2
    │
    ├── Phase 6 (Devrcfile Adapter) depends on Phase 1
    │
    └── Phase 7 (Deno) can run parallel to all
            │
            └── Phase 8 (Claude Code) depends on Phase 1 + Phase 7
```

**Parallel Execution Strategy**:
- Phase 1: MUST run first (foundation)
- Phase 2, 3, 4, 5, 6: After Phase 1, can run in parallel
- Phase 7: Independent, can run parallel after Phase 1
- Phase 8: Requires Phase 1 + Phase 7

---

## Part VII: Success Metrics

| Metric | Before | After |
|--------|--------|-------|
| CLI commands to create task | 5+ (`decompose create` + multiple `add`) | 1 (`tasksfile import`) |
| Task definition format | Internal only | YAML (Devrcfile + Tasksfile) |
| Template support | None | Tera (Jinja2) |
| Environment handling | Partial | Full .env loading |
| Include resolution | None | Local + URL |
| Deadline enforcement | None | 4 behaviors |
| Retry with backoff | None | Configurable |
| Event audit trail | None | Full logging |
| Plugin runtime | WASM only | WASM + Deno |
| Claude Code integration | 153 hooks | 157+ hooks |

---

## Part VIII: Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Phase 7 (Deno) complexity | HIGH | HIGH | Use existing `devrc` Deno implementation as reference; sandbox isolation critical |
| Schema migration breaking existing tasks | LOW | HIGH | Add migration with backward compatibility; test with existing 38 active tasks |
| YAML parsing performance | MEDIUM | LOW | Cache parsed Tasksfiles; lazy loading |
| Circular include detection failure | MEDIUM | MEDIUM | Depth limit (max 10 includes); cycle detection algorithm |
| Deno permissions escalation | MEDIUM | CRITICAL | Start with NO permissions; explicit grant only; audit logging |

---

## Part IX: File Structure

```
crates/
├── touring-hooks/
│   ├── src/
│   │   ├── cli_handlers.rs           # Modify: add deadline, parallel_groups
│   │   ├── hook_decompose_bridge.rs # Modify: wire events, snapshots
│   │   ├── env_file.rs              # NEW: Environment file loader
│   │   ├── include.rs               # NEW: Include resolver
│   │   └── tasksfile.rs             # NEW: Tasksfile CLI commands
│   └── tests/
│       └── test_decompose_*.rs      # Add: deadline, retry, events tests
│
├── touring-tasksfile/                # NEW: Tasksfile parser crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── parser.rs
│       ├── schema.rs
│       ├── compiler.rs
│       └── error.rs
│
├── touring-devrc-adapter/             # NEW: Devrcfile adapter crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── parser.rs
│       ├── converter.rs
│       └── error.rs
│
├── touring-deno-runtime/              # NEW: Deno plugin runtime
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── isolate.rs
│       ├── permissions.rs
│       ├── manifest.rs
│       ├── registry.rs
│       └── plugins/
│           └── github.rs
│
└── touring-generator/
    └── src/
        └── template/
            └── task_templater.rs    # Modify: wire to task execution
```

---

## Appendix A: Reference Crates

| Crate | Purpose | Location |
|-------|---------|----------|
| `devrc` v0.6.0 | Reference implementation | `https://github.com/devrc-hub/devrc` |
| `serde_yaml` | YAML parsing | crates.io/crates/serde_yaml |
| `tera` | Template engine (already in workspace) | `touring-generator/src/template/` |
| `deno_runtime` | Deno isolate | `https://deno.land/` |
| `netrc` | .netrc parsing | crates.io/crates/netrc |

---

## Appendix B: Schema Migration

```sql
-- Migration: Add missing columns to decomposition_subtasks
ALTER TABLE decomposition_subtasks ADD COLUMN deadline_enforced INTEGER DEFAULT 0;
ALTER TABLE decomposition_subtasks ADD COLUMN last_retry_at TEXT;

-- Migration: Add index for deadline queries
CREATE INDEX IF NOT EXISTS idx_subtasks_deadline ON decomposition_subtasks(deadline) WHERE deadline IS NOT NULL;

-- Migration: Populate metrics column for existing tasks
UPDATE task_decompositions SET metrics = '{}' WHERE metrics IS NULL;
```

---

*Document generated by TACO v6.2 — 2026-04-25*
*Next action: Await Gabriel authorization to begin Phase 1*
