---
type: Archive
title: "Item 12 do CLAUDE.md — entrega F1-F6 do code-mode-sinal"
description: "Texto integral removido do CLAUDE.md do projeto na condensação de 2026-09-02, preservado sem edição."
tags: [claude-md, historico, arquivo]
timestamp: 2026-09-02T00:00:00-03:00
plan_id: 2026-08-31-code-mode-sinal
---

# Item 12 do CLAUDE.md — entrega F1-F6 do code-mode-sinal

> Removido do `CLAUDE.md` do projeto em 2026-09-02, na condensação que trouxe o arquivo de 332
> para perto das 200 linhas recomendadas pela documentação oficial do Claude Code. O
> invariante e os gotchas ficaram lá; o texto abaixo é a narrativa completa, preservada
> sem edição para quem precisar do racional.

12. **Code-mode-sinal F1-F6 entregue (2026-09-01)**: a **superfície SDK híbrida** do
    `--orchestrate` está wired fim-a-fim. **F2** S4 híbrida: 8 hooks canônicos
    hardcoded em `crates/touring-code/src/sdk.rs` + tipos derivados do
    `run_journal.jsonl` via `signal_report_from_journal` (Rust) + `scripts/gen_sdk.py`
    (CI sem toolchain). **F3** PostToolUse-sync: sink JSONL em
    `~/.claude/touring/sdk_signal_mirror.jsonl` via
    `crates/touring-code/src/sdk_signal_mirror.rs` (6 testes). **F4** SDK Python
    tipada: `record_hook_call` injetado no template in
    `crates/touring-server/src/cli/run.rs:298` + wrap `query()` que cronometra cada
    chamada. **F5** BestPracticesGate: 4ª regra `signal_use` em
    `crates/touring-quality/src/builtins/best_practices.rs` (mirror counts distinct
    hooks, threshold 6/8 = 75%, severity mantida em Warn-severo por design). **F6**
    6 critérios AND + 2 KPIs secundários: `code_mode_signal_use()` em
    `crates/touring-cli/src/cli/kpi.rs:478` + exemplo `kpi_f6_smoke.rs`. DAG
    `task_1788196388043002698` todo done; bundle completo em
    `docs/plans/2026-08-31-code-mode-sinal/` (5 phase reports, 5 typed abstracts,
    4 signal reports). **Lesson (2026-08-30, exercitada)**: rebuild parcial de
    `touring-cli` NÃO atualiza binário `touring`/`touring-daemon` (vêm de
    `touring-server`); daemon embute `touring-cli` via linkagem estática e carrega
    handlers uma vez no boot → após editar RPC handlers, sempre `cargo build -p
    touring-server --release` + `update-touring` (kill+restart). Toolchain
    `30.4.28` propagada nativamente em 01/09/2026 (commit `8d4cda0`); 3 projetos
    pinados (touring + analise + konverter) em 30.4.28 com `code_mode_signal_use`
    ativo em `touring kpi -j`. Próxima wave: PostToolUse wirar em satélites +
    measurement adoption (F6 secondary KPIs).

