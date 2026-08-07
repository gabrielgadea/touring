---
type: Strategy
title: Excelência touring-cli — corrigir tudo com evidência medida
description: Estratégia do programa que ataca F3.1 (via root-cause do llvm-cov), F1.3 residual, F4.5/F4.7 e o defeito do devrcfile export.
plan_id: 2026-08-02-excelencia-touring-cli
tags: [loop, strategy, quality, coverage, supply-chain]
timestamp: 2026-08-02T21:45:00-03:00
okf_version: "0.1"
---

# Estratégia — excelência em `touring-cli`

Parte do [bundle](/index.md). Diagnóstico: [diagnostics/touring-20260802T213834.md](/diagnostics/touring-20260802T213834.md).

## Baseline medido (não estimado)

| Escopo | composite | tier | blockers |
| --- | --- | --- | --- |
| workspace | 0.93268 | Silver | `F1_3` |
| `crates/touring-cli` | 0.9209 | Silver | `F3_1` |

Warnings comuns: `F1_1`, `F1_2`, `F1_7`, `F4_5`, `F4_7`. Orphans 4894.

OUTER determinístico: 3 rodadas de `explore` (12 → 0 → 0 findings), ledger CCE
`converged: true` após a lente `external` ser visitada com fonte real.

## O achado que reordena o programa

`F3.1` **não estava medindo cobertura**. O verificador consome um artefato LCOV
do `cargo llvm-cov` e, na ausência dele, cai num **proxy de presença**
(`#[test]` fns ÷ fns públicas), rotulado como proxy. Não havia artefato:

```
$ cargo llvm-cov --lcov ... -p touring-cli
error: could not execute process ` /home/.../bin/rustc -vV` (never executed):
       No such file or directory (os error 2)
                                 ↑ espaço à esquerda = nome de programa vazio
```

**Causa-raiz**: `.cargo/config.toml` tinha `build.rustc-wrapper = ""`. O valor
existe para anular o `rustc-wrapper = "sccache"` **global** — sccache aqui é um
*correctness hazard* provado (26/06/2026: objeto stale, binário ≠ fonte, exit 0).
Mas `cargo llvm-cov` monta `$WRAPPER $RUSTC -vV`, e com wrapper vazio isso vira
`"" rustc -vV`, que não existe. `cargo build` tolerava; llvm-cov não.

**Consequência medida**: nenhum artefato LCOV jamais existiu → o `F3.1` de
**todo o workspace** reportava proxy, não cobertura. E o proxy é trivialmente
gameável (10 testes vazios "consertam" um arquivo com 10 fns públicas) — o que
faria o harness virar o "validador estéril e frouxo" que a autoridade do
projeto rejeitou explicitamente.

**Correção aplicada** — wrapper identidade, provado nas duas metades:

```toml
rustc-wrapper = "/usr/bin/env"   # exec transparente; NÃO é sccache
```

| Metade | Prova |
| --- | --- |
| sccache continua fora | `Running \`/usr/bin/env …\`` + `Compile requests` 6336 → 6336 |
| llvm-cov volta a funcionar | artefato de 6086 bytes, exit 0, **sem prefixo de env** |

Duas tentativas anteriores foram medidas e **descartadas**: `[env] RUSTC_WRAPPER
= ""` reintroduziu o sccache (`Running \`sccache rustc\``), e um `replace` amplo
corrompeu o TOML. Ambas revertidas por backup antes de seguir.

## Ordem de ataque (por alavancagem medida, não por gosto)

| # | Frente | Por quê agora |
| --- | --- | --- |
| **P1** | Artefato LCOV do workspace + reavaliar `F3.1` | Único blocker do crate. Faz a dimensão medir verdade; só depois faz sentido escrever teste |
| **P2** | Testes reais onde a cobertura REAL for baixa | Guiado pelo LCOV, não pelo proxy — sem teste-token |
| **P3** | `F1.3` residual: `semantics.rs` (A1) + 6 arquivos (A2) | Crate já saiu de Fail→Warn; fechar o resto |
| **P4** | `F4.7`: pinar 32 refs de action por SHA + `dependabot.yml` | Supply-chain real (OpenSSF/SLSA) |
| **P5** | `F4.5`: 113 crates com versão duplicada | Medir quantas são acionáveis (diretas) vs transitivas |
| **P6** | `devrcfile export` emite Tasksfile | Defeito de correção; exige serializer ou aposentar o comando |

## Decisão de escopo em P4 (vinda da lente externa)

Pinar por SHA só é sustentável com tooling que mantenha as pins vivas.
[Changelog do GitHub (31/10/2022)](https://github.blog/changelog/2022-10-31-dependabot-now-updates-comments-in-github-actions-workflows-referencing-action-versions/):
o Dependabot atualiza actions pinadas por SHA **e** mantém o comentário de
versão em sincronia — logo `@<sha> # v4` é o padrão correto, e o
`dependabot.yml` entra **junto** com as pins, nunca depois.

Mas [dependabot-core#14716](https://github.com/dependabot/dependabot-core/issues/14716):
quando o SHA pinado **não** tem tag direta, o Dependabot avança para o HEAD do
branch e deixa o comentário obsoleto. `dtolnay/rust-toolchain@stable|nightly|1.95`
são **branches**, não tags — e o ref carrega a semântica da toolchain. Portanto:
pinar as **32** refs baseadas em tag; **não** pinar as **11** do dtolnay.
Isso é redução deliberada e justificada de escopo, registrada aqui.

## Contrato de convergência

`loop_converged.py` exit 0 é o único "pronto". Cláusulas aplicáveis: DAG vazio,
tier ≥ Gold, 0 dims P0 BLOCK em Fail, orphans ≤ baseline (4894), cargo
check+test+clippy verdes, cross-audit limpo.

**Previsão honesta**: `F1_3` do workspace pode não virar Pass — o programa de
03/07/2026 mediu piso estrutural de ~8-13% em crates scaffold-dominated, e o
`touring-cli` só saiu dele (7,5%) porque a duplicação *intra-arquivo* nunca
tinha sido medida. Os demais crates não foram tocados. Se o tier não atingir
Gold, o relatório dirá isso com número, não com narrativa.

## ██ GATE HUMANO ██

Aprovação necessária antes de P1-P6. Itens que são decisão do Gabriel:

1. **P6** — implementar um serializer Devrcfile de verdade (feature, exige
   decisão de schema: `DevrcfileRoot`/`Task` hoje derivam só `Deserialize`)
   **ou** aposentar o subcomando `devrcfile export`, que não faz o que o nome diz.
2. **P2** — quanto de teste escrever: cobertura real guia o alvo, mas o volume
   (42 arquivos, 14,3k LOC) é decisão de orçamento.
3. **P4** — mexer em `.github/workflows/` é mudança outward-facing num repo público.
