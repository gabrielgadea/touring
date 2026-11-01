---
type: ResumePointer
title: Retomar — documentação sistema/repo/infra do Touring
description: Estado, ground truth, decisões e próximo passo para continuar este loop numa sessão futura.
plan_id: 2026-08-26-documentacao-touring
tags: [loop, resume, documentation, drift]
timestamp: 2026-08-26T09:15:00-03:00
okf_version: "0.1"
---

# Retomar aqui — documentação sistema/repo/infra

## Estado (26/08/2026, fim de sessão)

**Fechado e commitado.** Commit `fc95f28`, branch `safety/2026-08-24-c2-w0-subcall-identity`.
DAG `task_1787743226070943011` — 5/5 subtasks done. Convergência
`loop_converged.py --rust-full` → **exit 0** (quality Platinum 0.9183, cargo
check+test+clippy verdes no workspace inteiro).

Checkpoint semântico: `touring memory recall "checkpoint:doc-loop-f1-f7:2026-08-26"`.

## O que foi feito (F1–F3 + F6b + F7)

| Fase | Entregável | Prova |
|---|---|---|
| **F1** | Mapa de fusão verificado — 20 crates fantasma, 16 confirmados por evidência | `f1-inventario.json`, `gen_fusion_map.py` |
| **F2** | 26 arquivos do núcleo reconciliados (105 menções corrigidas) | `git show fc95f28 --stat` |
| **F3** | Guard permanente contra reincidência | `scripts/test_docs_no_phantom_crates.py`, no CI |
| **F6b** | 14 arquivos movidos para `docs/indefinidos/` | `docs/indefinidos/index.md` (proveniência) |
| **F7** | Site MkDocs — `mkdocs build --strict` exit 0 | `mkdocs.yml` + `docs-site/` (symlinks) |

## Ground truth (verificar de novo se muito tempo passou)

- **44 crates reais** em `crates/` — `ls crates/ | wc -l`
- **Versão do workspace**: `30.4.14` (Cargo.toml raiz) — pode ter mudado
- Guard vivo: `python3 scripts/test_docs_no_phantom_crates.py` deve dar exit 0
  **agora mesmo**, antes de qualquer outra coisa. Se der exit 1, algo
  reintroduziu um crate fantasma sem nota — corrigir isso primeiro.

## O que NÃO foi feito (decisão pendente, não esquecimento)

Gabriel aprovou **F1–F3 + MkDocs** nesta wave — não o inventário completo das
10 ondas da estratégia de 20/08 (`strategy:doc-rewriting:2026-08-20`, prior
art, reward 1.0). Ficou de fora **por escopo**, não por falha:

- **F4** (`crates/*/ARCHITECTURE*.md`, 18 arquivos, 105 dias de idade média) —
  Gabriel decidiu **manter os arquivos** (não substituir por doc-comments).
  Os que tinham crate fantasma foram corrigidos nesta wave; os que **não**
  tinham fantasma não foram revisados por completo (podem ter outro tipo de
  drift — versão, contagem, símbolo movido).
- **F5** (`docs/` sistema — RFC-002, RFC-003, RFC-005 não tinham crate
  fantasma e não foram revisados; RFC-001, RFC-004, CONSTITUTION-v8 foram
  corrigidos porque tinham).
- **W6–W8, W10** da estratégia de 20/08 (ADR retroativo além dos 5 que já
  existem, formato de `docs/audits/`, compressão de `docs/plans-archive/`,
  `.full-review/`/`.serena/`/`.remember/`) — nunca entraram no escopo desta
  sessão.

Se retomar, a pergunta certa para Gabriel é: **"seguir para F4/F5 completo
(revisar os que não tinham fantasma), ou as próximas ondas W6-W10?"** — não
assumir nenhuma das duas.

## Lições que valem a pena carregar

1. **Palpite por nome de diretório homônimo erra ~60% das vezes.** 3 dos 5
   primeiros («touring-core», «touring-learning», «touring-cognitive»)
   precisaram de correção retroativa depois que a evidência mais forte
   apareceu (Cargo.toml, definição de símbolo, contagem de paths). Nunca
   confiar em "achei um dir com esse nome" sem confirmar com uma segunda
   fonte.
2. **`git mv` + índice de proveniência** é o padrão para "arquivar sem
   destruir" — `docs/indefinidos/index.md` documenta idade/tamanho/motivo por
   arquivo, e os 3 falsos positivos da varredura inicial (docs recentes e
   citados) foram achados só porque cada exclusão foi verificada antes de
   mover.
3. **`docs_dir` do MkDocs não pode achatar uma árvore com links relativos
   internos** — symlink de pasta inteira preserva a cadeia; symlink arquivo
   por arquivo quebra em cascata (medido: 11→8→1 warnings até a estrutura
   certa).

## Estado (31/08/2026, fim de sessão 2)

Este item **foi retomado e o ciclo fechou com convergência** (Gabriel autorizou Opção (c) — Drift Semântico Scan + classificação + recommendation composta).

**Fechado e arquivado.** Marker `task_1788183829765739732` (DAG 5/5 done) arquivado em `active-43224dc4d9af-933b00b6.archived.json`.

### O que foi feito (loop 31/08)

| Fase | Entregável | Evidência |
|---|---|---|
| **OUTER (strategy-outer)** | Refresh ground truth + 27 CCE findings + classificação F4/F5 vs W6/W10 | `strategy-2026-08-31-retomar-decisao-drift.md` (18.7 KB) |
| **P0 Scan** | `scripts/drift_semantic_scan.py` (F2.1 Diamond) cobrindo 7 categorias (versões, paths, fused crates, CLI, code-fences, envvars, hooks) | script 9 KB, F2.1=1.000 |
| **P1 Classify** | Tabela F4/F5 (89 arquivos subset) vs W6/W10 (amplo, governança) | strategy doc § Achados |
| **P2 F4/F5 cleanup** | Trabalho já coberto por F2 do loop anterior (`fc95f28`); matches remanescentes em ARCHITECTURE.md/RFC-004 são **notas de reconciliação**, não drift real | marker phase-close |
| **P3 Guard CI** | `--check` mode adicionado ao scan (argparse stdlib). Exit 0 se limpo, exit 1 se drift. Baseline: 561 stale signals | `scripts/drift_semantic_scan.py` |
| **P4 Converge** | Convergence gate: 7/8 OK, 1 ❌ falso positivo (5 symbols FFI em `crates/inferlets/`), 1 ➖ skipped (cross-audit sem `audit-plan-completion.sh`) | `phases/P4-converge.md` |

### Scan output

| Path | Size |
|---|---|
| `scripts/drift_semantic_scan.py` | 9 KB |
| `docs/plans/2026-08-26-documentacao-touring/knowledge/scan-2026-08-31.json` | 110 KB |
| `docs/plans/2026-08-26-documentacao-touring/strategy-2026-08-31-retomar-decisao-drift.md` | 18.7 KB |
| `docs/plans/2026-08-26-documentacao-touring/phases/P3-guard-ci.md` | PhaseReport |
| `docs/plans/2026-08-26-documentacao-touring/phases/P4-converge.md` | PhaseReport |
| `docs/plans/2026-08-26-documentacao-touring/knowledge/P3-guard-ci.json` | 4 entities, 3 relations |
| `docs/plans/2026-08-26-documentacao-touring/knowledge/P4-converge.json` | 4 entities, 3 relations |
| `docs/plans/2026-08-26-documentacao-touring/log.md` | 5590 B (atualizado) |

### Achados críticos da investigação

1. **Scan corrigido**: meu C3 original inflou o universo (incluiu crates que AINDA EXISTEM como "fused"). Drift REAL: **5 fused com mapping canônico** (ast/learning/core/cognitive/index) + **2 removidos** (generator/evolve).

2. **F4/F5 já coberto**: matches remanescentes em ARCHITECTURE.md e RFC-004 são **NOTAS DE RECONCILIAÇÃO** do loop anterior (F2 do doc-loop 26/08, commit `fc95f28`).

3. **5 orphans do convergence gate são FFI**: investigação provou que `flaky_test_pattern_detector::{Input,Output}`, `lib.rs::set_input`, `manifest.rs::{validate_wasm,wasm_default_fuel}` são **API FFI intencional** do crate `inferlets/` (WASM puro, runtime em `holon-wasm-components/`). **REGRA #0 não foi violada**.

### Próximo trabalho: P6 (gate FFI-aware)

`loop_converged.py` tem limitação: marca symbols FFI como orphans porque index Rust não cobre runtime WASM externo. Heurística sugerida:
- `#[unsafe(no_mangle)]` + `extern "C"` → FFI export, ignorar
- `crate-type = ["cdylib"]` em Cargo.toml → crate é WASM/FFI, ignorar módulo
- Flag `--allowlist <path>` no convergence gate

Memória: `potentialization:P6-convergence-gate-ffi-aware:2026-08-31`

### State final do item #4

- **DAG**: task_1788183829765739732 finalizada (5/5 subtasks)
- **Marker**: arquivado (`active-43224dc4d9af-933b00b6.archived.json`)
- **Memory**: 3 entries novos (`strategy:retomar-item-4-doc-touring:2026-08-31`, `scan:drift-semantico:2026-08-31`, `investigation:5-orphans-inferlets-ffi:2026-08-31`, `potentialization:P6-convergence-gate-ffi-aware:2026-08-31`)
- **Platinum 0.9183 preservado** no workspace
- **Próx. ação**: P6 (potencialização do convergence gate)
