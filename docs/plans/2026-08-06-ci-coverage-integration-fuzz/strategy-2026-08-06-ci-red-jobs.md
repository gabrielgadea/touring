---
okf_version: "0.1"
type: Strategy
title: "CI vermelho — coverage, integration e fuzz: por que, e o que já estava consertado em disco"
description: "Os três jobs vermelhos de 02/08 têm causas distintas. Duas já tinham correção no working tree, jamais commitada; a terceira (B-320) era drift de path que deixava o teste verde localmente e vermelho no CI sem exercitar nada."
plan_id: 2026-08-06-ci-coverage-integration-fuzz
tags: [ci, coverage, integration, fuzz, drift, regra-21]
timestamp: "2026-08-06T13:30:00-03:00"
---

# CI vermelho — diagnóstico e estratégia

Part of the [bundle](/index.md) · diagnóstico em [/diagnostics](/diagnostics/touring-20260806T131302.md) · histórico em [/log.md](/log.md).

## 1. A pergunta, respondida por execução

**Sim, o CI segue vermelho** — e exatamente nos três eixos citados. Última
execução na `main`: run `30757323428`, 02/08 16:44, 35m54s.

| Job | 02/08 |
|---|---|
| coverage (llvm-cov → lcov) | 🔴 failure |
| integration tests (nextest) | 🔴 failure |
| fuzz targets (build smoke) | 🔴 failure |
| check+clippy · doctests · cargo-deny · quality gates · MSRV | 🟢 success |

`HEAD == origin/main == d6a9a00`, zero commits pendentes: o vermelho descreve
fielmente o código publicado.

## 2. O achado que reordena tudo

**Duas das três correções já existiam em disco, não commitadas.** O CI rodou
`d6a9a00`; o working tree tinha 195 linhas de diferença em `ci.yml` e nos dois
`e2e_diagnostic_rfc100.rs`. Não era "CI quebrado sem conserto" — era **conserto
sem sincronização**.

| Job | Causa real (medida) | Estado antes desta sessão |
|---|---|---|
| coverage | `locate_binary(…).expect("touring binary not built — skipping")` **panicava** dizendo que pulava, nas linhas 86 e 126 do teste do `touring-generator`. `cargo llvm-cov` roda `cargo test --tests`, e `--tests` não constrói o `[[bin]]` de outro pacote — o binário não existia | corrigido em disco (`touring_bin_or_skip`), nunca commitado |
| fuzz | `cargo +nightly fuzz build` **sem `--target`**. O `taiki-e/install-action` entrega um cargo-fuzz **musl**, e o cargo-fuzz usa o próprio triplo como default → alvo musl, cuja std não existe no runner **e** cujo default é `crt-static`. Os dois erros do log (`E0463 can't find crate for core` e `sanitizer is incompatible with statically linked libc`) são o mesmo sintoma | corrigido em disco (`--target x86_64-unknown-linux-gnu`), nunca commitado |
| integration | drift de path no teste B-320 | **não corrigido** — ver §3 |

Reprodução local do fuzz com o comando idêntico ao do CI: `exit 0` em 54,59s.
Isso isola a falha no binário musl do runner, não na config nem na toolchain —
nenhum `crt-static` existe em `.cargo/config.toml`, `~/.cargo/config.toml` ou
`fuzz/`.

## 3. B-320 — o defeito que sobrevivia por ser verde no lugar errado

`crates/touring-hooks/tests/e2e_diagnostic_rfc100.rs` analisava
`crates/touring-hooks/src/pre_write.rs`. Esse arquivo **migrou** para
`crates/touring-hook-handlers/src/hooks/pre_write.rs` num split de crate; o path
fixo no teste não acompanhou.

O motivo de o drift sobreviver é o mais instrutivo: **o teste era inútil nas duas
direções**.

- Local: `touring` responde a falha de leitura como JSON no **stdout**, stderr
  vazio. O assert só olhava stderr → **passava**, analisando nada.
- CI: a mesma falha aflora como linha de tracing no **stderr**, e o assert
  tropeça no `"error"` minúsculo dentro de `"(os error 2)"` → **vermelho**.

Uma hipótese minha foi refutada no caminho: atribuí a divergência ao par
release/debug (`locate_binary` tenta release primeiro). Medido, os dois binários
se comportam igual localmente — a diferença real está em como o erro aflora
conforme o estado do daemon, não no perfil de build.

**Correção (REGRA #0, potencializa)**: o teste passa a apontar para o arquivo
real, a exigir que ele exista (mensagem acionável se mover de novo) e a asseverar
o sinal determinístico do stdout — `gated_item_count > 0` —, que é a
pré-condição que o `cli_ast_blast_cross_feature` checa antes de emitir B-320
(`cli/ast.rs:615`). O arquivo real tem 2 itens sob `cfg(feature = "mpatch-fuzzy")`,
então o teste passa a provar o que promete.

Prova negativa executada: com o path obsoleto o teste **FALHA** com
`B-320 fixture is missing: …`; com o path real, passa.

## 4. Falso verde evitado

Com `touring_bin_or_skip()` no lugar do panic, o job de coverage passaria a
**pular** os testes E2E — verde oco, exatamente o que os comentários do próprio
arquivo condenam. Por isso o job ganhou um passo que constrói o binário
(`cargo build -p touring-server --bin touring`), para que os testes **rodem** em
vez de pular.

## 5. Drift adjacente corrigido

O comentário do job `msrv` ainda descrevia `rust-version = "1.85"` (herdado de
1.80) enquanto `Cargo.toml:151` declara **1.95**, o job se chama "MSRV (rust 1.95)"
e executa `cargo +1.95 check`. Comment drift justamente no único job cuja razão
de existir é a verdade sobre versão.

## 6. O que a suíte completa revelou — 3 tripwires do registry

Rodar `cargo nextest run --workspace --profile ci` (15.102 testes) expôs **3
falhas que o CI de 02/08 nem chegou a mostrar**, porque o job de integração
morreu antes no B-320. Todas eram contagem do hook registry, erradas por
exatamente 1:

| Arquivo | Esperado | Real |
|---|---|---|
| `stringzilla_e2e.rs:439` (`ALL_DAEMON_HOOK_NAMES`) | 218 | 219 |
| `stringzilla_e2e.rs:450/452` (`EXPECTED_NAMES`) | 224/222 | 225/223 |
| `wave2_4_e2e.rs:268/270` | 224/222 | 225/223 |
| `wave_c_e2e.rs:99/101` | 224/222 | 225/223 |

**Origem: eu mesmo, nesta sessão.** Ao registrar o hook `cli-memory-credit`
atualizei as duas contagens de `touring-dispatch/src/hook_registry_tests.rs` e
considerei o trabalho fechado. A mesma invariante está travada em **cinco**
lugares, e atualizar um subconjunto é a assimetria C08 que esta sessão já
cometeu duas vezes.

Uma armadilha no caminho: no `stringzilla_e2e.rs` o assert da linha 439 estoura
antes do par 450/452, que também estava velho — consertar só o primeiro faria o
segundo falhar na rodada seguinte. Por isso o mapeamento completo dos call sites
veio **antes** da primeira edição.

Não removi os tripwires (isso reduziria proteção): o doc do teste em
`stringzilla_e2e.rs` passa a listar os cinco sites e a exigir atualização
conjunta, e os outros dois arquivos ganham ponteiro para essa lista.
`potentialization_comprehensive_e2e.rs` e `e2e_touring_hooks_integration.rs`
asseveram apenas limite inferior (`>= 130`, `> 100`) — não derivam.

## 7. Gates executados

| Gate | Resultado |
|---|---|
| `cargo nextest run --workspace --profile ci` | **15.102/15.102**, 0 falhas, 34 skipped (282s) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **0 erros** |
| Os 3 tripwires, perfil default **e** `acp-protocol` | 3/3 verdes nos dois |
| `cargo +nightly fuzz build --target x86_64-unknown-linux-gnu` | exit 0 (54,59s) |
| B-320 — prova negativa | path velho → FAILED · path real → ok |
| 6 dims P0 BLOCK (arquivos tocados) | Pass/NotApplicable, 1.0 |
| `touring-quality score --fail-below 0.80` | exit 0 |
| Sintaxe `ci.yml` + 11 scripts referenciados | válida, todos existem |

## 8. O que fica com o humano

As correções estão **em disco, não no CI**. Sincronizar exige commit + push —
gate humano por REGRA #11 e por ser ação externa irreversível. Enquanto não
houver push, o CI seguirá vermelho por descrever `d6a9a00`, não o working tree.
