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
