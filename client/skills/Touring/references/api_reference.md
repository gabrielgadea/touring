# Touring Library APIs (Public Surface)

> Public Rust APIs that callers can use in-process (no daemon round-trip). Consult when extending Touring crates or building consumers.

## touring_ast — Code Intelligence (pure library)

```rust
use touring_ast::{format_rust_code, WorkspaceInfo};
use touring_ast::rust_semantic::RustSemanticReport;
use touring_ast::{TracedAstError, AstResultExt};

let report    = RustSemanticReport::from_source(src)?;     // syn visit
let formatted = format_rust_code(src)?;                    // prettyplease
let ws        = WorkspaceInfo::load(".")?;                 // cargo metadata
let callers   = ws.dependents_of("touring-core");          // cross-crate
let with_feat = ws.packages_with_feature("simd");          // feature-aware
let result    = some_op().traced()?;                       // SpanTrace capture
```

## touring_analysis — Quality Bridges

```rust
use touring_analysis::quality::RustQualitySignals;
use touring_analysis::wiring::{WiringFinding, hypergraph_cycle_detection};
use touring_analysis::blast_radius::BlastWarning;

let signals = RustQualitySignals::from_report(&report);
let health  = signals.health_score();                      // RL-ready signal

let finding = WiringFinding::LowIntegration { /* ... */ };
finding.emit();                                            // RFC-100 W-101

let warning = BlastWarning::HighBlast { count, file };
warning.emit();                                            // RFC-100 B-300
```

## touring_core — Diagnostics + Health

```rust
use touring_core::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes};
use touring_core::diagnostic::read_source_snippet;
use touring_core::health::{compute_composite_health_score, compose_degraded_warning, DEGRADED_SCORE_THRESHOLD};

// Build a diagnostic with source snippet
let snippet = read_source_snippet("src/foo.rs", 4096);
let diag = Diagnostic::new(codes::Q_200_LOW_QUALITY, Severity::Warning, "msg".into())
    .with_file("src/foo.rs")
    .with_source_snippet(snippet)
    .try_attach_source_from_file("src/foo.rs", 4096);
let report = diag.to_miette_report();                      // fancy renderer

// Composite health (Wave 9 S8)
let score   = compute_composite_health_score(snapshot, ema_reward);
let warning = compose_degraded_warning(score);             // Some(...) when < 0.5
```

## touring_generator — Code Generation Pipeline

```rust
use touring_generator::core::context::GeneratorContext;
use touring_generator::executor::typestate::{Draft, Verified, Rendered, Speculated, Committed};
use touring_generator::error::{GenerateError, diagnostic_speculate_passed};

// Typestate pipeline: Draft → Verified → Rendered → Speculated → Committed
let plan: Draft = Draft::from_file(path)?;
let verified: Verified = plan.verify(&ctx)?;               // VGP: index find
let rendered: Rendered = verified.render(&ctx)?;           // template fill
let speculated: Speculated = rendered.speculate(&ctx)?;    // shadow validate
let committed: Committed = speculated.commit(&ctx)?;       // atomic apply

// Errors carry RFC-100 codes
match err.kind() {
    GenerateError::VgpFailed { .. } => {
        if let Some(d) = err.to_diagnostic_opt() {
            tracing::warn!(code = d.code, plan_id = ?id, missing = ?missing);
        }
    }
    _ => {}
}
```

## touring_hooks — Daemon-Side Helpers

```rust
use touring_hooks::shared::health_delta::{
    record_pre_signals, compute_signals_delta, HealthDelta, status_json,
};
use touring_hooks::shared::query_cache::{cache_query, invalidate_by_path};
use touring_hooks::memory_finding::MemoryFinding;
use touring_hooks::shared::gate_metrics::record_diagnostic_b302_emitted;
use touring_hooks::pre_write::emit_b302_if_low_confidence_expansion;
use touring_hooks::health_delta::PatchComplexityDelta;

// Health delta loop
record_pre_signals(file_path, pre_signals);                // before edit
let delta: HealthDelta = compute_signals_delta(file_path, post_signals);

// Query cache (5 hot paths use this)
let result = cache_query(key, ttl_secs, || fetch_from_db());
invalidate_by_path(file_path);                             // post-edit

// RFC-100 emission
let finding = MemoryFinding::RrfFusion { /* ... */ };
finding.emit();                                            // M-520 via tracing

// Wave 12 (2026-04-27) — B-302 PatchExpansion helper
// Gate: delta.is_expansion() AND delta.confidence < 0.7
#[cfg(feature = "mpatch-fuzzy")]
let opt_delta: Option<PatchComplexityDelta> =
    emit_b302_if_low_confidence_expansion(file_path, source, &preview);
// → emits B-302 via tracing::warn! + records gate_metrics counter
//   when both conditions are met; returns None otherwise.

// Direct counter increment (when emitting B-302 from a custom site)
record_diagnostic_b302_emitted();
```

## touring_rkyv — Zero-Copy IPC

```rust
use touring_rkyv::ipc::{IpcRequest, IpcResponse};

// Daemon dispatch: peek byte 'R' → rkyv path, '{' → JSON
// Both directions framed (4-byte length prefix + body)
let request: IpcRequest = IpcRequest::new(method, params);
let bytes = rkyv::to_bytes::<_, 256>(&request)?;
// ... write to UnixStream ...
let response: IpcResponse = unsafe { rkyv::archived_root::<IpcResponse>(&buf) };
```

## ACP Protocol (feature `acp-protocol`)

```rust
use touring_hooks::protocol::acp::{
    Message, Response, Capabilities,
    PROTOCOL_VERSION, errors,
    detect_acp_payload, parse_message, serialize_response,
    success_response, error_response,
};

if detect_acp_payload(bytes) {
    let msg: Message = parse_message(json)?;
    let resp = match msg.method.as_str() {
        "wiring.impact" => success_response(msg.id, json!({...})),
        _ => error_response(msg.id, errors::E_METHOD_NOT_FOUND, "unknown method"),
    };
    serialize_response(&resp)?
}
```

## HyperGraph (touring-hooks::wiring::hypergraph)

```rust
use touring_hooks::wiring::hypergraph::{HyperGraph, FeatureGateHyperedge, MultiImportHyperedge};

let mut hg: HyperGraph<&str> = HyperGraph::new();
let a = hg.add_node("module_a");
let b = hg.add_node("module_b");
let edge = hg.add_hyperedge(&[a, b], "feature_gate");

hg.hyperedges_for(a)        // hyperedges containing 'a'
hg.members_of(edge)          // member nodes of hyperedge
hg.node_count()              // real nodes
hg.hyperedge_count()         // artificial nodes
hg.graph()                   // &DiGraph for petgraph algorithms

let gate = FeatureGateHyperedge::new(
    r#"all(feature = "simd", feature = "gpu")"#,
    "module_path",
);
// gate.features == vec!["simd", "gpu"]

let m = MultiImportHyperedge::new("use foo::{A, B, C}", "module_path");
// m.imported_symbols == vec!["A", "B", "C"]
```

## MCP Tools (87 tools)

See `references/mcp_tools.md` for the complete catalog. Most-used:

| Tool | Purpose |
|------|---------|
| `touring_minimal_context` | **First call** — entry point with task description (~100 tokens) |
| `touring_detect_changes` | Risk-scored change impact (blast + wiring + gotchas) |
| `touring_ast_overview` / `touring_ast_find` / `touring_ast_edit` | Navigation + surgical edits |
| `touring_memory_store` / `touring_memory_recall` | Lessons + patterns |
| `touring_suggest` / `touring_learn_pattern` / `touring_cluster_skills` | RL-backed guidance |
| `touring_speculate` | **Always** before Write/Edit on existing files |
| `touring_wiring` / `touring_wiring_audit` | Orphan detection + integration health |
| `touring_gotcha` | Pitfall lookup before editing |
| `touring_analysis_report` | Unified deep code health analysis |
| `touring_health_delta_status` / `touring_health_delta_reset` | W16 — health delta state |

## Token-Efficient MCP Workflow

1. **First call**: `touring_minimal_context` with task description (~100 tokens)
2. Use `detail_level='minimal'` on all tool calls unless minimal output is insufficient
3. Escalate to `'standard'` or `'full'` only for specific entities
4. Every response includes `_next_tools` suggestions — follow them for optimal workflow
5. For change review: `touring_detect_changes` → expand only high-risk items

## CLI vs MCP Selection

- **Read-only queries** (symbol lookup, memory search, wiring check): use CLI `touring index find <symbol>` or `touring memory recall '<query>'` (<10ms) instead of MCP (~200ms)
- **Write / complex ops** (store memory, start session, decompose, speculate): use MCP — CLI does not support write operations

## Critical Files (cross-reference)

- `crates/touring-core/src/diagnostic.rs` — `Diagnostic`, miette bridge
- `crates/touring-core/src/health.rs` — `compute_composite_health_score`
- `crates/touring-ast/src/rust_semantic.rs` — syn visitor
- `crates/touring-analysis/src/quality/rust_semantic.rs` — health bridge
- `crates/touring-hooks/src/shared/health_delta.rs` — Wave 5 health delta
- `crates/touring-hooks/src/shared/query_cache.rs` — moka cache
- `crates/touring-hooks/src/protocol/acp.rs` — ACP shim
- `crates/touring-hooks/src/wiring/hypergraph.rs` — HyperGraph wrapper
- `crates/touring-rkyv/src/ipc.rs` — rkyv IPC
- `crates/touring-generator/src/executor/typestate.rs` — generation pipeline
