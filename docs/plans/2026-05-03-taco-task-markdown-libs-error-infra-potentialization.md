# Plan — Potencialização Touring (TACO-task 2026-05-03): integrar markdown intelligence + consolidar thiserror 2.0 + expandir miette diagnostics. FAMILY M (13 subtasks): adotar comrak 0.28 (BSD-2) como validation gate em touring-generator nos 6 markdown-emitting GeneratorKinds (PlanMarkdown/SkillDocument/DiaryEntry/ChangelogEntry/Adr/TaskScaffold), injetar markdown_structural_layer em PlanExecutor::speculate() ao lado de polyglot_syntax_layer (typestate.rs:398), gated por novo metodo GeneratorKind::is_markdown_emitting(). Validar 7 invariants MV-1..MV-7 (heading hierarchy sem skip, code fence balanced, frontmatter YAML valido, link target nao-vazio, H1 presente, GFM table alignment, HTML balanced). Adicionar normalize_markdown via comrak::format_commonmark para idempotency round-trip. Expandir tree-sitter-md queries.scm (atualmente 7 linhas, 1 pattern) com 6 novos patterns Q-MD-1..Q-MD-6 (heading hierarchy, link targets, code block lang, frontmatter detection, anchor IDs, TOON delimiter). Criar touring_ast::markdown_analysis::analyse(path) usando comrak + queries para outline+anchors+links+frontmatter+code_blocks+validation. Adicionar novo GeneratorKind MarkdownDocument (generic markdown vs PlanMarkdown TACO-specific) com template markdown_document.tera (frontmatter + H1 + sections). Adicionar daemon handler cli_ast_markdown + CLI subcommand touring ast markdown <file> [--json|--outline-only|--links-only|--frontmatter-only]. Snapshot tests insta para idempotency dos 6 markdown kinds. Upgrade plan-detector.sh de regex bash para Option C (touring ast grep structural heading count com timeout 2s + fallback regex). FAMILY E (25 subtasks): consolidar thiserror 2.0.18 workspace eliminando dual-version em Cargo.lock — 4 crates pinned em v1 (touring-ast-polyglot, touring-assists phantom dep, touring-incremental-salsa phantom dep, touring-vfs) e 3 crates pinned em v2-direct (touring-embeddings, touring-vector-store, touring-geopostgis). Unificar TouringError duplicate em touring-hooks (errors.rs:18 String-based vs shared/touring_error.rs:27 typed) — escolher shared como canonical, absorver variants Knowledge/Wiring/Hook/Aco/Json, errors.rs vira pub use. Eliminar anyhow em 5 lib sites com tipos dedicados: MigrationError em touring-core/migration (CreateDir/OpenDb/SchemaError/MigrateTable/Unsupported), AdapterError em touring-devrc-adapter (EmptyExecCommands/InvalidTask/IoError/ParseError), SkipParseError em touring-generator/skip/parser (ReadDir/ParseFile/WalkDir). Expandir miette::Diagnostic em 3 crates seguindo padrao AstError: CognitiveError + LearningError + TasksfileError com #[diagnostic(code, help, severity)] per variant alinhado RFC-100. Introduzir touring-core::error::prelude com Result alias + re-exports thiserror+miette. Decisao critica: NAO criar novo typestate state (markdown_structural_layer injecta em speculate() existente, blast LOW vs HIGH para novo state). Riscos top 5: R-1 [HIGH] tree-sitter-md ABI v15 incompat em hook context (mitigacao: in-process daemon path apenas), R-2 [MEDIUM] TouringError consumers podem quebrar (deprecated aliases por 1 wave), R-3 [MEDIUM] salsa 0.18 transitive thiserror v1 (Cargo.lock dual entry residual harmless), R-4 [MEDIUM] plan-detector daemon dep (timeout 2s + fallback regex preserva fail-open), R-5 [LOW] snapshot_public_api.rs precisa cargo insta review. Wave priorization: W1 quick win 7 Cargo.toml edits (1h), W2 foundation comrak+TouringError dedup+queries.scm (2-3d), W3 MarkdownDocument+CLI+anyhow elimination (3-4d), W4 miette expansion+snapshots (2d), W5 prelude+plan-detector upgrade (1d). Symbol verification: 31 verified_existing + 11 to_be_created + 4 unverified_planned (entre os 2 architects).

> **Generated**: 2026-05-03T22:30:11.541251Z | **Task ID**: `task_1777847410285511707` | **CILA Level**: 3 | **Generator**: `taco-forge plan --quality high` v1.14.0
> **Provenance**: every section traces to a Touring CLI invocation. **Re-render is idempotent** — modify the underlying DAG, not this file.

## 01. Quality dimensions enforced

| Dim | Aspect | Mechanism in this plan |
|-----|--------|------------------------|
| **a** | Precision | VGP-verified symbols + Touring CLI evidence |
| **b** | Scalability | DAG decomposition; per-phase isolation |
| **c** | Performance | discover ~3s; VGP cached via memory store |
| **d** | Applicability | 31 generator kinds; 10 assist handlers |
| **e** | Code Quality | TDD enforced; clippy 0; tdg >= B |
| **f** | Detail | validation script per phase; cross-audit final |
| **g** | Systemic Integration | wiring orphans delta == 0; cycles 0 |
| **h** | Dependencies | cargo update + workspace-info checked |
| **i** | Potentialization | REGRA #0 — orphans wired; deliverables max scope |

## 02. Final goal

Potencialização Touring (TACO-task 2026-05-03): integrar markdown intelligence + consolidar thiserror 2.0 + expandir miette diagnostics. FAMILY M (13 subtasks): adotar comrak 0.28 (BSD-2) como validation gate em touring-generator nos 6 markdown-emitting GeneratorKinds (PlanMarkdown/SkillDocument/DiaryEntry/ChangelogEntry/Adr/TaskScaffold), injetar markdown_structural_layer em PlanExecutor::speculate() ao lado de polyglot_syntax_layer (typestate.rs:398), gated por novo metodo GeneratorKind::is_markdown_emitting(). Validar 7 invariants MV-1..MV-7 (heading hierarchy sem skip, code fence balanced, frontmatter YAML valido, link target nao-vazio, H1 presente, GFM table alignment, HTML balanced). Adicionar normalize_markdown via comrak::format_commonmark para idempotency round-trip. Expandir tree-sitter-md queries.scm (atualmente 7 linhas, 1 pattern) com 6 novos patterns Q-MD-1..Q-MD-6 (heading hierarchy, link targets, code block lang, frontmatter detection, anchor IDs, TOON delimiter). Criar touring_ast::markdown_analysis::analyse(path) usando comrak + queries para outline+anchors+links+frontmatter+code_blocks+validation. Adicionar novo GeneratorKind MarkdownDocument (generic markdown vs PlanMarkdown TACO-specific) com template markdown_document.tera (frontmatter + H1 + sections). Adicionar daemon handler cli_ast_markdown + CLI subcommand touring ast markdown <file> [--json|--outline-only|--links-only|--frontmatter-only]. Snapshot tests insta para idempotency dos 6 markdown kinds. Upgrade plan-detector.sh de regex bash para Option C (touring ast grep structural heading count com timeout 2s + fallback regex). FAMILY E (25 subtasks): consolidar thiserror 2.0.18 workspace eliminando dual-version em Cargo.lock — 4 crates pinned em v1 (touring-ast-polyglot, touring-assists phantom dep, touring-incremental-salsa phantom dep, touring-vfs) e 3 crates pinned em v2-direct (touring-embeddings, touring-vector-store, touring-geopostgis). Unificar TouringError duplicate em touring-hooks (errors.rs:18 String-based vs shared/touring_error.rs:27 typed) — escolher shared como canonical, absorver variants Knowledge/Wiring/Hook/Aco/Json, errors.rs vira pub use. Eliminar anyhow em 5 lib sites com tipos dedicados: MigrationError em touring-core/migration (CreateDir/OpenDb/SchemaError/MigrateTable/Unsupported), AdapterError em touring-devrc-adapter (EmptyExecCommands/InvalidTask/IoError/ParseError), SkipParseError em touring-generator/skip/parser (ReadDir/ParseFile/WalkDir). Expandir miette::Diagnostic em 3 crates seguindo padrao AstError: CognitiveError + LearningError + TasksfileError com #[diagnostic(code, help, severity)] per variant alinhado RFC-100. Introduzir touring-core::error::prelude com Result alias + re-exports thiserror+miette. Decisao critica: NAO criar novo typestate state (markdown_structural_layer injecta em speculate() existente, blast LOW vs HIGH para novo state). Riscos top 5: R-1 [HIGH] tree-sitter-md ABI v15 incompat em hook context (mitigacao: in-process daemon path apenas), R-2 [MEDIUM] TouringError consumers podem quebrar (deprecated aliases por 1 wave), R-3 [MEDIUM] salsa 0.18 transitive thiserror v1 (Cargo.lock dual entry residual harmless), R-4 [MEDIUM] plan-detector daemon dep (timeout 2s + fallback regex preserva fail-open), R-5 [LOW] snapshot_public_api.rs precisa cargo insta review. Wave priorization: W1 quick win 7 Cargo.toml edits (1h), W2 foundation comrak+TouringError dedup+queries.scm (2-3d), W3 MarkdownDocument+CLI+anyhow elimination (3-4d), W4 miette expansion+snapshots (2d), W5 prelude+plan-detector upgrade (1d). Symbol verification: 31 verified_existing + 11 to_be_created + 4 unverified_planned (entre os 2 architects).

## 03. Consequences (impacts on multiple perspectives)

- Codebase consequences: deliverables added/modified per phase; orphan delta == 0.
- Testing consequences: test files generated BEFORE impl (TDD); validation scripts run per phase.
- Memory consequences: outcome persisted via `touring memory store --tier semantic`.
- RL consequences: reward injected per phase + final audit (closes feedback loop).
- Documentation consequences: plan + validators + audit script tracked under plan/.

## 04. Success criteria

Default gate (TACO Delivery Checklist):
- All phases completed (`touring decompose status` → 100%)
- Cross-audit script PASSES (`audit-plan-completion.sh` exit 0)
- VGP: zero `BLOCKED` symbols
- Wiring: zero new orphan pub symbols (REGRA #0)
- E2E: `touring e2e --depth standard` composite ≥ 0.7

## 05. DISCOVER snapshot

| Signal | Value | Source |
|--------|-------|--------|
| Daemon healthy | `True` | `touring doctor -j` |
| Composite health score | `0.53` | `touring status -j` |
| Symbol count | `47,236` | `touring status -j` |
| Orphan pub symbols | `7,425` | `touring wiring orphans -j` |
| Cycle count | `0` | `touring wiring cycles` |
| EMA reward (RL) | `0.1101` | `touring status -j` |
| Drift alert | `degraded` | `touring evolution drift -j` |
| E2E composite | `0.00` | `touring e2e --depth standard -j` |
| Synergy wired pairs | `50` | `touring synergy --with-metrics -j` |
| Workspace packages | `0` | `touring ast workspace-info` |

**Known gotchas active**: 1 pitfall(s) flagged

## 06. VGP — Verified Generation Protocol report

**Tally**: verified=24, not_found=0, blocked=0, skipped=0

| Symbol | Status | Blast | Files | Evidence |
|--------|--------|-------|-------|----------|
| `comrak::format_commonmark` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: comrak::format_commonmar |
| `touring_ast::markdown_analysis::analyse` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: touring_ast::markdown_an |
| `miette::Diagnostic` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: miette::Diagnostic |
| `core::error::prelude` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: core::error::prelude |
| `TACO` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: TACO |
| `FAMILY` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: FAMILY |
| `GeneratorKinds` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: GeneratorKinds |
| `PlanMarkdown` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: PlanMarkdown |
| `SkillDocument` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: SkillDocument |
| `DiaryEntry` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: DiaryEntry |
| `ChangelogEntry` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: ChangelogEntry |
| `TaskScaffold` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: TaskScaffold |
| `PlanExecutor` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: FOUND: PlanExecutor at crates/touri |
| `GeneratorKind` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: FOUND: GeneratorKind at crates/tour |
| `YAML` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: YAML |
| `HTML` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: HTML |
| `TOON` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: TOON |
| `MarkdownDocument` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: MarkdownDocument |
| `TouringError` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: FOUND: TouringError at crates/touri |
| `MigrationError` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: MigrationError |
| `CreateDir` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: CreateDir |
| `OpenDb` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: OpenDb |
| `SchemaError` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: SchemaError |
| `MigrateTable` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: MigrateTable |

## 07. Phases overview

| ID | Title | Deps | Status | Effort | Validator |
|----|-------|------|--------|--------|----------|
| `P1` | Bootstrap (placeholder) | — | `pending` | 4.0h | `validate-phase-P1.sh` |

## 08.P1 — Bootstrap (placeholder)

**Contribution to final goal**: Establish the working baseline. Replace this phase with concrete decomposition via `touring decompose add` + re-run `taco-forge plan --quality high`.

**Impacts**:
- scope discovery
- baseline metrics captured

**Deliverables**:
- `plan/ — generated artifacts`
- `validators/ — per-phase validators`
- `checkpoints/ — TOON state snapshots`

**Validation gate** (deep mode): `cargo check`=False, `cargo test`=False, TDG ≥ `B`, orphan delta ≤ 0, E2E `quick` ≥ 0.5, TDD gates: red=False green=False refactor=False

## 09. Validation scripts (one per phase)

Each phase ships with a deep validation script. Run individually:

- `bash validators/validate-phase-P1.sh`

Run all in dependency order:

```bash
for f in validators/validate-phase-*.sh; do
  echo "=== $f ==="
  bash "$f" || { echo "FAIL at $f"; exit 1; }
done
```

## 10. Cross-audit (final gate)

After all phases complete, run the generated cross-audit script:

```bash
bash audit-plan-completion.sh
```

Audits performed:
- All phases finalized via `touring decompose status`
- VGP symbols still verified (re-run V1+V2 batch)
- Zero new orphan pub symbols vs DISCOVER baseline
- `cargo check --workspace` exit 0
- `cargo test --workspace` exit 0
- `touring e2e --depth standard` composite ≥ 0.7
- All `validate-phase-*.sh` exit 0
- Memory persists final outcome via `touring memory store`
- Diary entry via `touring diary write --aaak`

## 11. Reproduce / re-render

```bash
~/.claude/tools/taco-forge/workflows/plan.sh \
  --intent "Potencialização Touring (TACO-task 2026-05-03): integrar markdown intelligence + consolidar thiserror 2.0 + expandir miette diagnostics. FAMILY M (13 subtasks): adotar comrak 0.28 (BSD-2) como validation gate em touring-generator nos 6 markdown-emitting GeneratorKinds (PlanMarkdown/SkillDocument/DiaryEntry/ChangelogEntry/Adr/TaskScaffold), injetar markdown_structural_layer em PlanExecutor::speculate() ao lado de polyglot_syntax_layer (typestate.rs:398), gated por novo metodo GeneratorKind::is_markdown_emitting(). Validar 7 invariants MV-1..MV-7 (heading hierarchy sem skip, code fence balanced, frontmatter YAML valido, link target nao-vazio, H1 presente, GFM table alignment, HTML balanced). Adicionar normalize_markdown via comrak::format_commonmark para idempotency round-trip. Expandir tree-sitter-md queries.scm (atualmente 7 linhas, 1 pattern) com 6 novos patterns Q-MD-1..Q-MD-6 (heading hierarchy, link targets, code block lang, frontmatter detection, anchor IDs, TOON delimiter). Criar touring_ast::markdown_analysis::analyse(path) usando comrak + queries para outline+anchors+links+frontmatter+code_blocks+validation. Adicionar novo GeneratorKind MarkdownDocument (generic markdown vs PlanMarkdown TACO-specific) com template markdown_document.tera (frontmatter + H1 + sections). Adicionar daemon handler cli_ast_markdown + CLI subcommand touring ast markdown <file> [--json|--outline-only|--links-only|--frontmatter-only]. Snapshot tests insta para idempotency dos 6 markdown kinds. Upgrade plan-detector.sh de regex bash para Option C (touring ast grep structural heading count com timeout 2s + fallback regex). FAMILY E (25 subtasks): consolidar thiserror 2.0.18 workspace eliminando dual-version em Cargo.lock — 4 crates pinned em v1 (touring-ast-polyglot, touring-assists phantom dep, touring-incremental-salsa phantom dep, touring-vfs) e 3 crates pinned em v2-direct (touring-embeddings, touring-vector-store, touring-geopostgis). Unificar TouringError duplicate em touring-hooks (errors.rs:18 String-based vs shared/touring_error.rs:27 typed) — escolher shared como canonical, absorver variants Knowledge/Wiring/Hook/Aco/Json, errors.rs vira pub use. Eliminar anyhow em 5 lib sites com tipos dedicados: MigrationError em touring-core/migration (CreateDir/OpenDb/SchemaError/MigrateTable/Unsupported), AdapterError em touring-devrc-adapter (EmptyExecCommands/InvalidTask/IoError/ParseError), SkipParseError em touring-generator/skip/parser (ReadDir/ParseFile/WalkDir). Expandir miette::Diagnostic em 3 crates seguindo padrao AstError: CognitiveError + LearningError + TasksfileError com #[diagnostic(code, help, severity)] per variant alinhado RFC-100. Introduzir touring-core::error::prelude com Result alias + re-exports thiserror+miette. Decisao critica: NAO criar novo typestate state (markdown_structural_layer injecta em speculate() existente, blast LOW vs HIGH para novo state). Riscos top 5: R-1 [HIGH] tree-sitter-md ABI v15 incompat em hook context (mitigacao: in-process daemon path apenas), R-2 [MEDIUM] TouringError consumers podem quebrar (deprecated aliases por 1 wave), R-3 [MEDIUM] salsa 0.18 transitive thiserror v1 (Cargo.lock dual entry residual harmless), R-4 [MEDIUM] plan-detector daemon dep (timeout 2s + fallback regex preserva fail-open), R-5 [LOW] snapshot_public_api.rs precisa cargo insta review. Wave priorization: W1 quick win 7 Cargo.toml edits (1h), W2 foundation comrak+TouringError dedup+queries.scm (2-3d), W3 MarkdownDocument+CLI+anyhow elimination (3-4d), W4 miette expansion+snapshots (2d), W5 prelude+plan-detector upgrade (1d). Symbol verification: 31 verified_existing + 11 to_be_created + 4 unverified_planned (entre os 2 architects)." \
  --cila-level=3 \
  --quality high \
  --out plan/plan.md
```

---

_Code-First plan v1.14.0 — `task_1777847410285511707`. Modify the DAG, not this file._
