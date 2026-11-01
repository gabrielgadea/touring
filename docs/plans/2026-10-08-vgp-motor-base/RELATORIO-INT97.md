---
type: Report
title: INT97 — integração sem commit de VL0 + L2 + F21 + P0 + gates-97 + CLI não-UTF-8 + E2EHERM sobre a árvore implantada da 30.4.96
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-17T14:00:00Z
autor: sessão efe418c2 (retomada; o agente original `aint97` parou no limite de créditos em 10/10)
tags: ["#kind:report", "#status:done", "#domain:vgp", "#lang:python"]
---

# RELATÓRIO INT97

Brief: [briefs/2026-10-10-int97.md](briefs/2026-10-10-int97.md). B = este bundle.

## 1. Identidade

- Worktree `~/projects/touring-wt-int97`, branch `int/2026-10-10-int97`, HEAD c118aa52, **sem commit**.
- 1º passe (10/10, agente `aint97`): componentes 1–5 (VL0, L2, F21, P0 portado para `lang/python*`, gates-97) e o patch do CLI
  não-UTF-8. O log desse passe (`scratchpad/int97/logs/final.log`) morreu com o /tmp no boot de 16/10; o binário `b30d107c`
  daquele passe é ANTERIOR ao patch do CLI e não é o final.
- 2º passe (17/10, sessão efe418c2): E2EHERM + gates completos + release reproduzível + juízes por id.
  Scripts: `diagnostics/int97/scripts/{pipeline,gates,release}_int97.sh`. Saídas: `diagnostics/int97/resultados-2026-10-17/`.

## 2. O que foi feito, com evidência

- `diagnostics/e2eherm/e2eherm.patch` (sha256 8b2cfcc3…) aplicado com `git apply --3way`: 2 arquivos de teste
  (`touring-hooks/tests/common/private_daemon.rs`, `wave25_task_enrichment_e2e.rs`), **0 conflito**. Originais em
  `diagnostics/int97/backup-pre-e2eherm-2026-10-17/` (iguais ao `safety/2026-10-10/int97`).
- Prova de conteúdo: o int97 difere do snapshot `safety/2026-10-10/int97` SÓ nesses 2 arquivos; o `e2eherm.patch` reverte
  limpo (contido); o patch do CLI (`diagnostics/vhtml/cli-nao-utf8.patch`) tem as 179 linhas `+` presentes NA ORDEM em
  `cli/vgp.rs` — o reverso não aplica só porque edições posteriores mudaram o contexto (o único `lang: "python".into()`
  restante é o helper de teste `out_base`, L292; o sítio do patch tem `lang: recusa.lang`).
- Conflito por conflito: nenhum neste passe. Os do 1º passe não têm registro (log perdido); a consistência deles é atestada
  pelos gates e juízes abaixo.

## 3. Estado por item do método

| item | resultado |
|---|---|
| 4. fmt `--all -- --check` | rc 0 |
| 4. clippy `--workspace --all-targets -D warnings` | rc 0 (321 s) |
| 4. testes (`--no-fail-fast`, 0 falhas) | code 1.201 · server 1.810 · hooks 1.184 · cli 690 · hook-handlers 752 (`pre-hooks,post-hooks`) · generator 433 · integration-tests 208 · pyright 175 · lsp 13 — **0 failed** em todos; generator e integration-tests entram porque incluem o helper do E2EHERM |
| 4. falha conhecida do E2EHERM | **resolvida**: `wave25_task_enrichment_e2e` 6/6 (eram as 2 falhas do brief) |
| 4. rustdoc `-D warnings` touring-code | rc 0 |
| 5. binário | **`8fd0efb0`** (`~/.claude/touring/vgp/binarios/8fd0efb0/touring`, sha256 8fd0efb0e62e64a8…), **reproduzível**: touch em 5 fontes do VGP → build 2 → `cmp` rc 0 |
| 6. juízes por id contra o P0 final `4501e1e5` | ver §3.1 |

### 3.1 Juízes (forma de `diagnostics/p0b/scripts/juizes_p0b.sh`; réguas `diagnostics/p0b/scripts/comparar_porid2.py`)

Referência: as saídas por id do `4501e1e5` já gravadas em `diagnostics/p0-atributo-2026-10-09/resultados/juizes_dc_{ev,bak}`
(mesmo corpus: vivo entradas 28d93bcb, bak 7b819af2).

| corpus | juiz | ids | R1 | R2 | deny novo / perdido | pronto (novo × ref) |
|---|---|---|---|---|---|---|
| vivo | v2 | 738 | 0 | 0 | 0 / 0 | sim × sim (p95 87,5 ms) |
| vivo | V3 | 738 | 2 | 43 (`camada2`) | 0 / 0 | não × não — p95 1.061,2 × 1.037,7 ms > 1.000 |
| bak | v2 | 762 | 0 | 0 | 0 / 0 | sim × sim (p95 137,7 ms) |
| bak | V3 | 762 | 0 | 58 (`camada2`) | 0 / 0 | não × não — p95 1.055,2 × 1.055,9 ms > 1.000 |

**Toda divergência explicada**: as 2 de R1 (vivo, ids `76d4d4a88939` e `eae0c2b76058`) são `warn` de membro na referência
que viraram `allow` porque a camada 2 estourou o prazo de 1.000 ms esperando o `publishDiagnostics` do pyright
(`unknown` → vale a camada 1, fail-open). As de R2 são todas no campo `camada2` e todas timeouts (vivo: 30 novos, 9 que
deixaram de ser, 4 com outro texto). É latência da camada 2, não mudança do motor; a camada 1 é idêntica por id nos dois
corpora. O V3 reprova por p95 na referência também — limite pré-existente, não regressão da INT97.

## 4. Processos

Tudo pelo `scripts/hw-slot` (rust `--wait 7200`; juízes na classe heavy), sequencial. A máquina passou de 95 °C (o hw-slot
segurou jobs acima de 88 °C). O 2º teste do P0B dividiu a vaga com o início do V3 vivo por < 1 min (terminou 10:32:39; o V3
começou 10:31:51). Nenhum `kill`, nenhum `update-touring`.

## 5. Dados com sha

- `e2eherm.patch` 8b2cfcc3d22fe60c · `cli-nao-utf8.patch` 2e245078127d · binário 8fd0efb0e62e64a8 · corpora vivo 28d93bcba17a7993,
  bak 7b819af27bccb0db · referência 4501e1e56294a81a.

## 6. Decisões e riscos

- `CARGO_TARGET_DIR=…/target/wt-int97` foi recriado a frio: o diretório do 1º passe sumiu de `touring/target/` (causa não
  identificada; o safe-clean semanal só toca `incremental/`).
- Risco aberto: o p95 do V3 está ~5% acima de 1 s em AMBOS os binários — critério do V3 depende do ambiente (pyright na
  camada 2). Não bloqueia o R97 (o J13 é o juiz do release), mas fica registrado.
- Próximo: P0B e FRAG entram no int97 (simulação: P0B aplica limpo; FRAG depois dele conflita só na linha `use` do
  `sombra.rs`), e o int97 resultante vira a base do J13 e do `touring-wt-adapt`.

## 7. Git

`touring-wt-int97`: sem commit; 46 modificados + 13 não rastreados (inclui os 2 staged do `--3way`). Worktrees de origem não
foram tocados neste passe. Snapshots: `safety/2026-10-10/int97` (antes do E2EHERM) — o estado atual difere dele só nos 2
arquivos do patch, que está no bundle.

## 8. O que NÃO foi verificado

- Os gates e a reprodutibilidade do `b30d107c` (1º passe): evidência perdida com o /tmp; superados por este passe.
- O raciocínio dos conflitos do 1º passe (log perdido).
- Held-outs (`corpus_holdout_*`): fora do escopo da INT97 — são o J13.
