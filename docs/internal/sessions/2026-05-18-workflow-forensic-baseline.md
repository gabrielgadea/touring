# Workflow Forensic Baseline — CEG Pln2 P8.1

> **Wave**: CEG Pln2 FASE P8 (Workflow Intelligence Layer) | **Date**: 2026-05-18
> **Code**: `crates/touring-hooks/src/workflow/baseline.rs` — `WorkflowBaseline`, `ANTIPATTERN_BASELINE`
> **Source data**: `~/.claude/downloads/forense-resultado.json`

## Purpose

The Workflow Intelligence Layer (P8) needs a **measurable, deterministic baseline** of how
coding agents actually use tools — so antipattern conversion (P8.4) and the 30-day re-run
(P7.5 REINFORCED) are observed as a real delta, not a guess. This document records that
baseline and the method to reproduce it.

## Method (zero-LLM, deterministic)

A pure Python `os.walk` + JSON parse of every Claude Code transcript under
`~/.claude/projects/*/*.jsonl` — **no `claude -p`, no LLM CLI**. Tool-call lines are
classified by a deterministic rule set; bigrams/trigrams and antipattern markers are
counted. Re-runnable: the same transcripts always yield the same baseline.

- **Transcripts swept**: 3,058 `.jsonl` files (1 empty)
- **Tool-call lines parsed**: 575,821
- **Parse errors**: 7 (0.001%)

## Tool distribution

| Tool | Calls | Share |
|------|-------|-------|
| Bash | 113,214 | 52.9% |
| Read | 50,632 | 23.7% |
| Edit | 24,462 | 11.4% |
| Write | 6,279 | 2.9% |
| Grep | 3,751 | 1.8% |
| Glob | 1,645 | 0.8% |

## The 6 forensic findings (encoded by `WorkflowBaseline`)

| # | Finding | Count | Meaning |
|---|---------|-------|---------|
| 1 | `bash_grep_rg` — raw grep/rg in Bash | 35,975 | grep run as a shell command instead of the Grep tool |
| 2 | `bash_cat_head_tail` — file read via shell | 44,487 | cat/head/tail instead of Read offset/limit |
| 3 | `bash_find` — find in Bash | 3,494 | find instead of the Glob tool |
| 4 | `read_without_prior_search` | 46,307 | Read with no preceding search to locate the target |
| 5 | search→read ratio | 1:29.5 | search_then_read 1,570 vs read_without_search 46,307 (~3.4%) |
| 6 | Glob error rate | 26.2% | 431 Glob tool errors of 1,645 calls |

## Full antipattern table

| Antipattern | Count |
|-------------|-------|
| read_without_prior_search | 46,307 |
| bash_cat_head_tail | 44,487 |
| bash_grep_rg | 35,975 |
| bash_echo | 8,991 |
| bash_find | 3,494 |
| edit_without_read | 2,273 |
| bash_awk | 296 |
| bash_sed_inplace | 275 |
| claude_cli_in_bash | 5 |

The headline orchestration-antipattern total (grep-raw + cat/head/tail + find + edit-without-read)
is ~86,229; with read-without-locate it is ~132,536 — the volume the layer aims to convert.

## Good patterns (the conversion target)

| Good pattern | Count |
|--------------|-------|
| touring_cli_used | 16,558 |
| edit_then_bash | 10,541 |
| read_then_edit | 10,035 |
| search_then_read | 1,570 |

## Tool error rates (P8.6 — Glob diagnosis input)

| Tool | Error rate | Errors |
|------|-----------|--------|
| Glob | **26.2%** | 431 |
| WebFetch | 23.1% | 223 |
| Skill | 13.3% | 38 |
| Edit | 9.6% | 2,359 |
| Grep | 8.0% | 300 |
| Bash | 5.9% | 6,677 |

Glob's 26.2% is the standout: a quarter of all Glob calls error. P8.6 (`glob_diag.rs`,
`GlobErrorTaxonomy`) classifies the 431 failures by root cause and targets < 5%.

## How P8.1 encodes this

`crates/touring-hooks/src/workflow/baseline.rs`:
- `WorkflowBaseline` — struct holding the counts above (`jsonl_files`, `total_tool_calls`,
  `antipatterns`, `good_patterns`, `search_to_blind_read_ratio_pct`, `glob_error_rate_tenths_pct`).
- `ANTIPATTERN_BASELINE: OnceLock<WorkflowBaseline>` — process-global; `baseline()` initializes it.
- `WorkflowBaseline::from_json` — re-parses a fresh `forense-resultado.json` deterministically,
  so the 30-day re-run (P7.5 REINFORCED) measures the conversion delta.

## Re-running the baseline

```text
1. Deterministic re-mine (zero LLM): a Python os.walk over ~/.claude/projects/*/*.jsonl,
   classifying tool-call lines and counting antipattern markers → forense-resultado.json
2. WorkflowBaseline::from_json reloads it.
3. Compare against ANTIPATTERN_BASELINE — shrinking antipattern counts = the
   Workflow Intelligence Layer converting agents toward elite tool use.
```

---
_CEG Pln2 P8.1 — workflow forensic baseline. Source: 575,821 real tool-calls across 3,058 transcripts._
