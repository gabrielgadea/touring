---
type: Audit
title: Cross-audit do r3 (Jev/Laya em modo sombra) — fidelidade ao propósito, 03/10/2026
description: Emissor Rust do shadow, 23 scripts r3/r4 e as afirmações de RESULTS/PROVA contra evidência executada. Veredito PASSA COM RESSALVAS; o S1b do cli-suggest não passa na releitura e há um guard de teste que passa por vácuo. Fase de conserto em pausa, esperando aprovação.
tags: [audit, r3, shadow, jev, laya, cross-audit]
timestamp: 2026-10-03T01:00:00-03:00
---

# Cross-audit do r3 — 03/10/2026

**Escopo.** Rust: `crates/touring-hook-runtime/src/r3_shadow.rs` (905 linhas), `crates/touring-cli/src/cli_suggester/r3_a2.rs` (237), `crates/touring-cli/src/cli/kpi/shadow.rs` (478) e os consumidores (`pre_bash.rs`, `cli_suggester.rs`, `cli_suggester_tests.rs`). Python: 23 scripts `r3_*`/`r4_*` e 20 arquivos de teste. Documentos: `RESULTS.md`, `PROVA-PRATICA.md`, `CRITERIOS-ACEITACAO.md`.
**Quem julgou.** O emissor Rust foi auditado por um `touring-auditor` independente (contexto limpo, só leitura, cego às minhas conclusões); eu rodei a evidência executável; o `touring-a4` (MiniMax) recontou os números do S1b com código próprio. O veredito não é autoavaliação.
**Limite do que isto prova.** A1 e A2 continuam `NAO_ADOTADO_PELOS_DADOS`. Este audit mede se a **entrega e a segurança do modo sombra** cumprem o propósito documentado, não se os modelos devem ser adotados. "Funciona de forma perfeita" **não** é uma conclusão que a evidência permita.

## Veredito: PASSA COM RESSALVAS

Os seis invariantes de desenho (I1–I6) se sustentam. Mas a condição de entrega do §5 do CRITERIOS ("ENTREGUE ⟺ S1–S6 passam") **não está satisfeita hoje**: o S1b do `cli-suggester` não passa na releitura com amostra grande, e o S3 só fecha às 15:10 de 03/10.

## Fase 1–2 — MAP e PURPOSE: os invariantes

| Invariante | Veredito | Evidência |
|---|---|---|
| I1 observe-only (retorno do hook idêntico com a flag ligada) | **Sustenta** | auditor: `ctx` entra por referência imutável (`r3_shadow.rs:207`); a decisão de produção é calculada antes e devolvida por valor (`cli_suggester.rs:4892-4901`); teste `a1_block_flag_on_ctx_byte_identical`. Executado: S4 passou em 3 rodadas (02/10), 0/200 divergências nos 4 hooks |
| I2 default OFF, custo ~zero | **Sustenta** | `r3_shadow.rs:96,103-118`; **S1a medido hoje (00:48, chip a 79 °C):** OFF p50 487 ns / p99 784 ns; emissão p50 11 µs / p99 14 µs |
| I3 fail-open | **Sustenta** | lock com recuperação de envenenamento (`r3_shadow.rs:111`, `r3_a2.rs:77,122`); erro de I/O só incrementa `DROPPED` (`:182-189`); teste `r3_emission_failure_never_changes_the_return` |
| I4 privacidade (S6) | **Sustenta** | `MAX_TEXT_CHARS = 80` e `exceeds_text_cap` recusam e contam; só `session_hash` é gravado. **Executado agora:** `r3_prove_delivery.py s6` nos 2 journals, 359 e 358 linhas, 0 inválidas, 0 violações |
| I5 entrega (uma linha por oferta fechada) | **Sustenta** | `MAX_PENDING` e `replaced` fecham 1 linha cada (`r3_a2.rs:76-103`). **Executado agora:** 361 eventos × 361 avaliações, um-para-um, 0 sem avaliação, 0 duplicadas, 0 órfãs |
| I6 honestidade do KPI | **Sustenta** | ausência vira `unmeasured`, nunca zero (`shadow.rs:142-181`); teste `empty_journal_is_unmeasured_not_zero` |

## Fase 3 — DEBT SCAN

`scan_debt.py` no escopo (46 arquivos): **1 item**, `#[ignore]` em `r3_shadow.rs:841` → achado D1. Zero TODO, WIP, `unimplemented!` ou supressão.

## Fase 4 — HARMONY

- **Órfãos: 0 no escopo**, de 761 no projeto. A lista do CLI vem elidida (`_elided_array_len`), então a leitura foi pelo SDK de code mode (761 registros, igual ao `orphan_count`), com controle positivo do filtro (213 órfãos em `touring-intelligence`).
- **50-dim:** 6 gates P0 × 46 arquivos = 276 avaliações, **0 FAIL**. 9 `Warn` em F2.4 conferidos linha a linha: são vocabulário (`token`, `credential`, `secret` em padrões do varredor de privacidade e em strings de teste), sem valor atribuído. F2.5 nos 3 crates: **Pass, 0 CVEs**. F4.5: Warn 0,7 (114 crates duplicados no workspace inteiro, anterior ao r3; 13 avisos informativos de "sem manutenção").
- **Score do escopo: 0,926 (Platinum)**, sem bloqueadores, acima do piso Gold (0,80).

## Achados

| id | sev | achado | tipo |
|---|---|---|---|
| **A** | P1 | **S1b do `cli-suggest` NÃO passa** na releitura: p95 598 ms > envelope 583 ms (n = 3.862). Em 02/10 (n = 206) eu tinha registrado PASSA. Margem de 2,4 %, número em movimento (586 → 597 → 598 em três leituras) | FATO (recontado por `touring-a4`) |
| **B** | P1 | **O envelope do S1b do `post-tool-rl` é um artefato do instrumento.** O `lo` cai em 25/09 18:09 UTC, no meio de um dia, e a fatia parcial (n = 4.956, máx. 154 s) tem p95 8.294 ms e domina o "pior dia". Sobre dias completos o envelope é 874 ms e o veredito fica **indeterminado**: o ON agregado (913) e a fatia de 02/10 (927) ficam acima, a fatia de 03/10 até agora (792) fica abaixo (E14.7). A regra literal dá `passes: true`; ela é quase impossível de falhar | FATO (achado do `touring-a4`, confirmado por mim) |
| **C** | P2 | **Guard silencioso nos testes do A2.** `cli_suggester_tests.rs` tem 9 `if env_flag_is_set() { return; }` (linhas 4756 a 4981) e 0 `require_flag_unset`. **Medido:** com `TOURING_R3_SHADOW=1` forçada, os testes do A2 passam **11/11** (nove sem testar nada); os do A1 em `r3_shadow.rs` falham alto (**9 falhas**) e o de `pre_bash.rs:561` também. A minha correção de 02/10 ("9 testes no-op corrigidos") cobriu um arquivo só | FATO (executado) |
| **D** | P2 | **O S1a é `#[ignore]`** (`r3_shadow.rs:841`): um critério de entrega guardado por um teste que a suíte normal nunca roda. Rodado à mão hoje, passa | FATO |
| **E** | P3 | `with_events_path` (`r3_shadow.rs:143`) não restaura o thread-local se o closure der panic. Os 2 chamadores (`pre_bash.rs:602,645`) são de teste (depois do `#[cfg(test)]` da linha 535) | FATO |
| **F** | P3 | `cmd_heads` é truncado duas vezes (`r3_a2.rs:126` e `r3_shadow.rs:308`); a segunda é no-op e dá a impressão de que a entrada poderia passar de 80 caracteres | FATO |
| **G** | aviso | Dependências: 114 crates duplicados e 13 avisos de pacote sem manutenção, no workspace, anteriores ao r3 | FATO |

**Atribuição do achado A: inconclusiva.** A mediana do `cli-suggest` é 2–3 ms em todas as fases e o pior dia sem shadow (01/10) é exatamente o envelope (583). Mas no `post-tool-rl` a **mediana subiu** de 33–35 ms (3 dias sem shadow) para 39–43 ms (2 dias com shadow; no mesmo dia 02/10, 34 → 39). E há um confundidor que não controlei: a release 30.4.91 e a flag entraram no mesmo instante (binário trocado às 15:09:21,34; daemon nasceu às 15:09:21,37), e a máquina esteve a 90–96 °C. Que houvesse uma 30.4.90 antes é hipótese (não verificável por leitura). Só um A/B de latência com o **mesmo binário**, flag ligada e desligada, as mesmas entradas e execução simultânea separaria flag, release e carga. Isso não foi feito nem aprovado.

## Fase 6 — E2E PROOF (executado nesta sessão)

| O que | Comando | Resultado |
|---|---|---|
| Testes Rust do escopo | `hw-slot --class rust -- cargo test …` | `hook-runtime` r3: 18 · `cli` r3: 11 · `cli` shadow: 20 · `handlers` r3: 5 · **0 falhas**. Mais `pre_bash::tests`: 13, **0 falhas** (inclui `a1_block_emitted_at_context_injection`, cujo nome não casava com o filtro `r3`) |
| Contraste com a flag forçada | `TOURING_R3_SHADOW=1 cargo test …` | A2 `cli`: 11 passam (**vácuo**); A1 `hook-runtime`: 9 falham (alto); A1 `handlers`: falha em `pre_bash.rs:561` (alto) |
| Suíte Python r3/r4 | `hw-slot -- pytest test_r3_*.py test_r4_*.py` | **367 passam, 0 falham**; dos 114 artefatos reais vigiados (hash + mtime), **0 alterados pela suíte** |
| S1a (ignorado) | `cargo test … -- --ignored r3_emission_cost_is_bounded` | passa; OFF p99 784 ns, emissão p99 14 µs |
| S1, S1b, S2, S6 | `r3_prove_delivery.py s1\|s1b\|s2\|s6` sobre `hook_trace.jsonl` | S1b `cli-suggest` **não passa** (598 > 583); S1b `post-tool-rl` passa só pela letra; S2 0,0 pp (passa); S6 passa |
| Doctor | `touring doctor -j` (Bash comum, raiz do projeto) | **9/9 ok** |

**Nota de método.** Dentro do sandbox do `touring run`, o `doctor` mostra 3 checks "missing" e o `hw-slot --status` quebra; os dois são artefatos do sandbox (sem acesso ao banco do projeto e ao socket), e por Bash comum ficam normais. Quase li esses sinais como defeitos.

## O que NÃO foi feito

- **Nenhum conserto de código** (a skill exige pausa humana antes da fase 5).
- **S4 não foi reexecutado** (3 rodadas em 02/10); **S3 aguarda as 15:10 de 03/10**; o juiz de convergência (`loop_converged.py`) não foi reexecutado hoje; `cargo check --workspace` e `touring e2e -j` não foram rodados.
- **A/B de latência não feito.** `touring-auditor` não rodou `cargo` (UNVERIFIED para compilação por ele; os testes acima são meus).

## Pausa humana — conserto proposto, na ordem

1. **C (P2)** trocar os 9 guards silenciosos por `require_flag_unset()` em `mod r3_shadow_a2`. RED já medido (11/11 passam com a flag forçada); GREEN esperado: falham alto com a flag forçada, passam com ela limpa. Só arquivo de teste.
2. **E (P3)** guard RAII no `with_events_path`; **F (P3)** comentário sobre o truncamento duplo.
3. **D (P2)** decisão sua: manter o S1a `#[ignore]` (sensível a carga) mas **ligá-lo ao gate de release** (`propagate-release.sh`, sob `hw-slot`), ou trocar por um limite relativo ao caminho OFF.
4. **A e B** decisão sua: aprovar o A/B de latência (mesmo binário, flag on/off, simultâneo) e se o S1b ganha uma leitura adicional sobre **dias completos**. Nenhum limiar se move.

Registro completo, com a ordem pelo git: Errata 14 (E14.1–E14.6) em `CRITERIOS-ACEITACAO.md`.
