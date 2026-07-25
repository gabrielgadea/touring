# Code Generator Reference

> touring-generator pipeline, **31 GeneratorKind** (canonical list via `touring generate list-kinds`), template variables, and error handling.
>
> **Consumers**: `Touring-native tooling` (`~/.claude/skills/Touring-native tooling/`) is the bash-orchestrated wrapper that drives the full typestate pipeline (Draft → Verified → Rendered → Speculated → Committed) for Rust, Python, and TypeScript scaffolding. See `~/.claude/skills/Touring-native tooling/references/touring-integration.md` for the Tier × Stage mapping and `references/pipeline-stages.md` for the 16-stage I/O contracts.

## Generator Workflow (VGP Pipeline)

```
DRAFT → VERIFIED → RENDERED → SPECULATED → COMMITTED
```

**Step 1**: Schema — `touring generate schema-dump -j`

**Step 2**: Build Plan JSON:
```json
{
  "plan_id": "gen-<name>-<timestamp>",
  "kind": "<kind>",
  "intent": "<what this artifact should do>",
  "target_path": "<path/to/file>",
  "symbols_to_verify": ["SymbolA", "SymbolB"],
  "template_variables": {...},
  "assembly": {"append_mode": false, "dry_run": false},
  "capacity": {"estimated_files": 1, "priority": "normal"}
}
```

**Step 3**: Submit — `touring generate plan-submit --file <plan.json>`

**Step 4**: Verify — `touring generate plan-status --file <plan.json>`

**Dry Run**: `"dry_run": true` — executa pipeline completo sem escrever arquivos.

## 31 GeneratorKind (canonical)

The authoritative list is emitted by `touring generate list-kinds` (verified 2026-05-01). Do not duplicate hard-coded snapshots in this file — they drift. Run the command to get the current list.

```
$ touring generate list-kinds
31 GeneratorKind variants:
  Rust Module                     template: rust_module.tera
  CLI Handler                     template: cli_handler.tera
  MCP Tool                        template: mcp_tool.tera
  Hook Handler                    template: hook_handler.tera
  Test                            template: test.tera
  Benchmark Suite                 template: benchmark.tera
  Fuzz Target                     template: fuzz_target.tera
  Derive Macro                    template: derive_macro.tera
  Attribute Macro                 template: derive_macro.tera
  Function Macro                  template: derive_macro.tera
  Error Catalog                   template: error_catalog.tera
  Incremental Patch               template: incremental_patch.tera
  FFI Binding                     template: ffi_binding.tera
  JSON Schema                     template: schema.tera
  Migration Script                template: migration.tera
  ProtoBuf Schema                 template: protobuf_schema.tera
  OpenAPI Spec                    template: openapi_spec.tera
  AsyncAPI Spec                   template: asyncapi_spec.tera
  Plan (Markdown)                 template: plan.md.tera
  Skill Document                  template: skill_document.tera
  Diary Entry                     template: diary_entry.tera
  Changelog Entry                 template: changelog_entry.tera
  Architecture Decision Record    template: adr.tera
  Shell Completion                template: shell_completion.tera
  Man Page                        template: man_page.tera
  Python Script                   template: python_script.tera
  TypeScript Module               template: typescript_module.tera
  Dockerfile                      template: dockerfile.tera
  Kubernetes Manifest             template: k8s_manifest.tera
  Terraform Module                template: terraform_module.tera
  CI Workflow                     template: ci_workflow.tera
```

JSON shape: `touring generate list-kinds -j`. Each entry has `name`, `kind` (PascalCase canonical), `template`, and `description`. The `kind` field is the value placed in `plan.json::kind`.

## Template Variables by Kind

| Kind | Variables |
|------|-----------|
| `rust_module` | `module_name`, `description`, `pub_structs`, `pub_fns`, `imports` |
| `mcp_tool` | `tool_name`, `description`, `params_struct`, `return_type`, `handler_body` |
| `hook_handler` | `handler_name`, `hook_phase`, `description`, `module_name` |
| `test` | `test_module`, `target_module`, `test_functions` |
| `plan_markdown` | `plan_name`, `version`, `phases`, `tasks` |

## Error Handling

| Erro | Solução |
|------|---------|
| VGP fail (symbol not found) | `touring index find <symbol>` — verificar se existe; se novo, remover de `symbols_to_verify` |
| Template rendering fail | `touring generate template-list` — verificar variáveis disponíveis |
| Plan validation fail | `touring generate plan-validate --file <plan.json>` — verificar schema |

## 7-Layer Validation Pipeline (ESAA §7-layer)

Formalized in S6 (touring-generator v8). Each layer is independently testable,
emits a `validate.layer.<n>` event, and rejects with a layer-specific error_code.

| # | Layer | Validates | Error Code | Context |
|---|-------|-----------|------------|--------|
| L1 | `JsonParse` | plan JSON is valid UTF-8 + parseable | `JSON_PARSE_FAIL` | `ValidationLayer::L1_JsonParse` |
| L2 | `SchemaValidation` | plan fields conform to schema | `SCHEMA_VIOLATION` | `ValidationLayer::L2_SchemaValidation` |
| L3 | `VocabularyAllowed` | `kind` is in the allowed set | `VOCABULARY_DENIED` | `ValidationLayer::L3_VocabularyAllowed` |
| L4 | `StateMachine` | plan status transitions are legal | `INVALID_TRANSITION` | `ValidationLayer::L4_StateMachine` |
| L5 | `PathBoundary` | artifact paths respect `Contracts.path_boundaries` | `BOUNDARY_VIOLATION` | `ValidationLayer::L5_PathBoundary` |
| L6 | `Immutability` | committed artifacts are not modified | `IMMUTABILITY_VIOLATION` | `ValidationLayer::L6_Immutability` |
| L7 | `VerificationGate` | composite health score ≥ 0.85 | `VERIFICATION_GATE_FAILED` | `ValidationLayer::L7_VerificationGate` |

### Pipeline Flow

```
GeneratorPlan → validate_plan(plan, ctx)
                    │
         ┌──────────▼──────────┐
         │  L1 JsonParse      │ → LayerResult { name: "l1_json_parse", passed, score, issues, elapsed_ms }
         ├─────────────────────┤
         │  L2 SchemaValid     │ → LayerResult { name: "l2_schema", ... }
         ├─────────────────────┤
         │  L3 VocabAllowed   │ → LayerResult { name: "l3_vocabulary", ... }
         ├─────────────────────┤
         │  L4 StateMachine    │ → LayerResult { name: "l4_state_machine", ... }
         ├─────────────────────┤
         │  L5 PathBoundary    │ → LayerResult { name: "l5_path_boundary", ... }
         ├─────────────────────┤
         │  L6 Immutability    │ → LayerResult { name: "l6_immutability", ... }
         ├─────────────────────┤
         │  L7 Verification    │ → LayerResult { name: "l7_verification_gate", ... }
         └─────────────────────┘
                    ↓
         ValidationReport { layers_passed, all_passed, layer_results[], layer_durations_ms{} }
```

### ValidationContext Builder

```rust
let ctx = ValidationContext::new()
    .with_allowed_kinds(vec!["RustModule".into()])
    .with_contracts(contracts)
    .with_composite_health(0.91)
    .with_layer_observer(|layer, result| {
        // S1 activity log wiring point
        println!("{:?} → passed={}", layer, result.passed);
    });
let report = validate_plan(&plan, &ctx);
```

### Error Taxonomy

```rust
pub enum ValidationError {
    L1JsonParse(String),                                          // JSON parse failed
    L2Schema(String),                                            // Schema field violation
    L3VocabularyNotAllowed { kind: String },                     // kind not in allowed_kinds
    L4StateMachine(String),                                      // invalid state transition
    L5BoundaryViolation { file: String, kind: String },         // path boundary breach
    L6ImmutabilityViolation { path: String },                   // committed path touched
    L7VerificationFailed { score: f64 },                         // composite_health < 0.85
}
```

### Key Generate Commands

```bash
touring generate list-kinds -j              # Lista 30 GeneratorKind
touring generate schema-dump -j             # JSON Schema do GeneratorPlan
touring generate template-list [-j]         # Lista 29 templates Tera
touring generate verify --symbol <name>     # VGP symbol verification
touring generate verify --symbols "SymA,SymB"  # VGP batch verification
touring generate plan-submit --file <path>  # Pipeline completo
touring generate plan-validate --file <path>  # Valida plan JSON
```
