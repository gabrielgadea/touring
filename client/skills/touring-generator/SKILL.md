---
name: touring-generator
description: Generate code artifacts via touring-generator Rust pipeline (30 kinds, VGP verification, typestate safety). Use when creating new Rust modules, MCP tools, hook handlers, CLI commands, templates, tests, or any structured code artifact in the touring workspace.
---

# touring-generator — Code Artifact Generator

**CLI RANKED GUIDE**: `~/.claude/skills/Touring/SKILL.md` — CLI COMMAND RANKS v5.0 (TIER 1-9, best practices, ~120 commands)

## When to Use

Use this skill when the user asks to:
- Create a new Rust module, struct, trait, or function
- Add a new MCP tool to touring-server
- Add a new hook handler
- Add a new CLI subcommand
- Create tests, benchmarks, or fuzz targets
- Generate schemas, migrations, or API specs
- Create documentation artifacts (plans, ADRs, changelogs, skills)
- Generate infrastructure files (Dockerfile, K8s manifest, Terraform, CI)

## Available Generator Kinds (30)

| Kind | Use When |
|------|----------|
| `rust_module` | New Rust module with pub API |
| `cli_handler` | New touring CLI subcommand |
| `mcp_tool` | New MCP tool in touring-server |
| `hook_handler` | New pre/post hook handler |
| `test` | Test module for existing code |
| `benchmark` | Criterion benchmark |
| `fuzz_target` | Fuzz testing target |
| `error_catalog` | Error type hierarchy |
| `schema` | JSON Schema definition |
| `plan_markdown` | Pln2 plan document |
| `skill_document` | Claude Code skill |
| `changelog_entry` | CHANGELOG entry |
| `adr` | Architecture Decision Record |
| `python_script` | Python script with touring integration |

## Workflow

### Step 1: Get the Schema

```bash
touring generate schema-dump -j
```

This returns the GeneratorPlan JSON Schema. The plan has these required fields:
- `plan_id`: Unique identifier (e.g., "gen-hook-pre-edit-2026")
- `kind`: One of the 30 GeneratorKind variants (snake_case)
- `intent`: What the artifact should do
- `target_path`: Where to write the output file
- `symbols_to_verify`: List of symbols that must exist (VGP checks these)
- `template_variables`: Key-value pairs passed to the Tera template
- `assembly`: Output config (append_mode, dry_run, encoding)
- `capacity`: Hints (estimated_files, priority)

### Step 2: Build the Plan

Construct a GeneratorPlan JSON. Example for a new hook handler:

```json
{
  "plan_id": "gen-hook-pre-edit-enrichment",
  "kind": "hook_handler",
  "intent": "Pre-edit enrichment handler that injects context from touring memory",
  "target_path": "crates/touring-hooks/src/pre_edit_enrichment.rs",
  "symbols_to_verify": ["HookRuntime", "FileKnowledgeDb", "MemoryStore"],
  "template_variables": {
    "module_name": "pre_edit_enrichment",
    "handler_name": "PreEditEnrichmentHandler",
    "hook_phase": "PreEdit",
    "description": "Enriches pre-edit context with touring memory recall"
  },
  "assembly": {
    "append_mode": false,
    "diff_only": false,
    "dry_run": false,
    "encoding": "utf-8"
  },
  "capacity": {
    "estimated_files": 1,
    "estimated_symbols_to_verify": 3,
    "estimated_output_bytes": 2000,
    "estimated_llm_tokens": 0,
    "priority": "normal"
  }
}
```

### Step 3: Submit via MCP Tool

Use the `touring_generator_submit_plan` MCP tool to execute the full pipeline:

```
touring_generator_submit_plan(plan=<json>)
```

This executes the typestate pipeline:
1. **Draft** — Plan parsed and validated
2. **Verified** — VGP checks all symbols_to_verify exist in the touring index
3. **Rendered** — Tera template rendered with template_variables
4. **Speculated** — Shadow validation checks output quality
5. **Committed** — File written to target_path

### Step 4: Verify

```bash
touring generate plan-status
```

## CLI Alternative

For quick generation without MCP:

```bash
# Verify symbols exist
touring generate verify --symbols "HookRuntime,FileKnowledgeDb"

# Render a template
touring generate render --kind hook_handler --var module_name=my_handler

# Full pipeline
touring generate plan-submit --file plan.json

# Validate a plan
touring generate plan-validate --file plan.json
```

## Template Variables by Kind

### rust_module
- `module_name`, `description`, `pub_structs`, `pub_fns`, `imports`

### mcp_tool
- `tool_name`, `description`, `params_struct`, `return_type`, `handler_body`

### hook_handler
- `handler_name`, `hook_phase`, `description`, `module_name`

### test
- `test_module`, `target_module`, `test_functions`

### plan_markdown
- `plan_name`, `version`, `phases`, `tasks`

## Dry Run Mode

Always test with dry_run first:

```json
{
  "assembly": { "dry_run": true, ... }
}
```

This runs the full pipeline (VGP + render + speculate) without writing files.

## Error Handling

If VGP verification fails (symbol not found):
1. Check `touring index find <symbol>` to verify the symbol exists
2. The symbol may have been renamed or moved — use `touring index search <prefix>`
3. If the symbol is new (not yet created), remove it from `symbols_to_verify`

If template rendering fails:
1. Check `touring generate template-list` for available templates
2. Verify template_variables match the template's expected variables
3. Use `touring generate template-validate --kind <kind>` to check

## Integration with Touring Task System

When generating as part of a TACO task:
1. The generator auto-creates subtasks in `touring decompose` for each pipeline stage
2. Status updates flow: Draft→InProgress, Verified→InProgress, etc.
3. RL rewards are injected per successful transition
4. Session checkpoints track generation progress

## Rust-Specific Deep Analysis (2026-04-18)

The `touring-ast` crate now exposes three Rust-specific pre-generation
helpers, available both as library APIs and CLI subcommands. Use them
during generation to make Rust artifacts more precise:

### 1. `touring ast rust-semantic <file.rs>` — semantic depth via `syn`

Returns generics, trait bounds, lifetimes, derives, where clauses,
unsafe + async counts, and a `semantic_complexity` score ∈ [0, 1].
Use this when generating code that extends an existing Rust module:
feed the report into your generator's template_variables so the new
code matches the surrounding type parameter / lifetime conventions.

```bash
touring ast rust-semantic crates/touring-ast/src/rust_semantic.rs
# Returns: {"generics": [...], "trait_impls": [...], "lifetimes": [...], ...}
```

**Library:** `touring_ast::rust_semantic::RustSemanticReport::from_source`

### 2. `touring ast format-rust <file.rs>` — rustfmt-clean output via `prettyplease`

Formats Rust source without invoking the external `rustfmt` binary.
Use this as a post-generation step to ensure emitted code is visually
consistent with human-written code in the same crate.

```bash
touring ast format-rust generated_module.rs > clean.rs
```

**Library:** `touring_ast::format_rust_code` (or
`format_rust_code_best_effort` for fallible input).

### 3. `touring ast workspace-info [<dir>]` — workspace metadata via `cargo_metadata`

Dumps the whole Cargo workspace as JSON: packages, features,
dependencies, workspace membership. Use this to answer
**"which crate should this artifact live in?"** or **"does the target
crate already re-export the feature my template needs?"** before
committing a plan.

```bash
touring ast workspace-info .
# Returns: {"workspace_root": "...", "packages": [{"name": "...", "features": {...}, ...}], ...}
```

**Library:** `touring_ast::WorkspaceInfo::load(manifest_dir)` —
also offers `.dependents_of("crate-name")` and
`.packages_with_feature("feature-name")` query helpers for
cross-crate blast-radius reasoning from generator plans.

### When to use which

| Goal | Helper |
|------|--------|
| Match existing generics when adding a new method | `rust-semantic` |
| Emit final file with consistent formatting | `format-rust` |
| Decide which crate to add new code into | `workspace-info` |
| Check if a feature flag already exists | `workspace-info` + `packages_with_feature` |
| Measure cross-crate blast radius | `workspace-info` + `dependents_of` |
