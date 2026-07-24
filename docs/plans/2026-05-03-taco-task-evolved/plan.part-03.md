# Potencialização Touring (TACO-task 2026-05-03) — Part 3 of 3

> **Nav**: [← Part 2](plan.part-02.md) | [↑ Index](plan.md) | _(last)_

---

## 11. Reproduce / re-render

```bash
~/.claude/tools/taco-forge/workflows/plan.sh \
  --intent "Potencialização Touring (TACO-task 2026-05-03): integrar markdown intelligence + consolidar thiserror 2.0 + expandir miette diagnostics. FAMILY M (13 subtasks): adotar comrak 0.28 (BSD-2) como validation gate em touring-generator nos 6 markdown-emitting GeneratorKinds (PlanMarkdown/SkillDocument/DiaryEntry/ChangelogEntry/Adr/TaskScaffold), injetar markdown_structural_layer em PlanExecutor::speculate() ao lado de polyglot_syntax_layer (typestate.rs:398), gated por novo metodo GeneratorKind::is_markdown_emitting(). Validar 7 invariants MV-1..MV-7 (heading hierarchy sem skip, code fence balanced, frontmatter YAML valido, link target nao-vazio, H1 presente, GFM table alignment, HTML balanced). Adicionar normalize_markdown via comrak::format_commonmark para idempotency round-trip. Expandir tree-sitter-md queries.scm com 6 novos patterns Q-MD-1..Q-MD-6 (heading hierarchy, link targets, code block lang, frontmatter detection, anchor IDs, TOON delimiter). Criar touring_ast::markdown_analysis::analyse(path) usando comrak para outline+anchors+links+frontmatter+code_blocks+validation. Adicionar novo GeneratorKind MarkdownDocument com template markdown_document.tera (frontmatter + H1 + sections). Adicionar daemon handler cli_ast_markdown + CLI subcommand touring ast markdown. Snapshot tests insta para idempotency dos 6 markdown kinds. Upgrade plan-detector.sh de regex bash para Option C (touring ast grep structural heading count com timeout 2s + fallback regex). FAMILY E (25 subtasks): consolidar thiserror 2.0.18 workspace eliminando dual-version em Cargo.lock. Unificar TouringError duplicate em touring-hooks (errors.rs:18 String-based vs shared/touring_error.rs:27 typed). Eliminar anyhow em 5 lib sites com tipos dedicados: MigrationError em touring-core/migration, AdapterError em touring-devrc-adapter, SkipParseError em touring-generator/skip/parser. Expandir miette::Diagnostic em 3 crates seguindo padrao AstError: CognitiveError + LearningError + TasksfileError com diagnostic codes alinhado RFC-100. Introduzir touring-core::error::prelude com Result alias + re-exports thiserror+miette. Wave priorization: W1 quick win 7 Cargo.toml edits (1h), W2 foundation comrak+TouringError dedup+queries.scm (2-3d), W3 MarkdownDocument+CLI+anyhow elimination (3-4d), W4 miette expansion+snapshots (2d), W5 prelude+plan-detector upgrade (1d)." \
  --cila-level=3 \
  --quality high \
  --out plan/plan.md
```

---

_Code-First plan v1.15.0 — `task_1777851082497360181`. Modify the DAG, not this file._

---

> **Nav**: [← Part 2](plan.part-02.md) | [↑ Index](plan.md) | _(last)_
