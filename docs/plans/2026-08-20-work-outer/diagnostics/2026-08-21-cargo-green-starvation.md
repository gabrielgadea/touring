---
type: diagnostic
title: "cargo_green da W1: starvation do ator, três armadilhas de instrumento e o juiz que media o clima"
description: "Por que a cláusula cargo_green reprovou 5× na madrugada de 21/08, o que era defeito real e o que era ambiente — e os dois fixes aplicados (gerador proptest e isolamento de socket no juiz)."
plan_id: 2026-08-20-doc-rewriting
tags: [diagnostic, daemon, starvation, proptest, loop-converged, cargo-green]
timestamp: 2026-08-21T06:10:00-03:00
---

# cargo_green W1 — diagnóstico da madrugada de 21/08/2026

## Sintoma

`loop_converged.py --rust-full` reprovou `cargo_green` em 5 rodadas consecutivas
(03:34→06:05), com 3 assinaturas distintas: `graph_service_e2e::test_graph_svg_output`
(budget 15s estourado), timeout de 2400s da suíte inteira, e
`property_tests::class_def_produces_class_kind`.

## Mecanismo real (medido, não inferido)

1. **Starvation do ator global.** `cli-index-rebuild` do workspace (4,6k arquivos)
   roda 10-40 min monolítico no ator single-thread; qualquer op barata enfileirada
   atrás (viz 15s, doctor, index status) estoura budget. Sampler de CPU de 5 min
   provou trabalho contínuo (200-290 jiffies/3s) — **não** era deadlock.
2. **Testes e2e herdavam `TOURING_DAEMON_SOCKET` da sessão** e round-tripavam pelo
   daemon global vivo — o gate media o clima da máquina (mutation runs, rebuilds de
   outras origens), não o código.
3. **Defeito real e determinístico** encontrado no caminho: `py_class_name()` do
   `property_tests.rs` gerava `[A-Z][a-z]{1,10}`, que soletra exatamente `None`,
   `True`, `False` — `class None:` é erro de sintaxe e o extractor corretamente não
   emite símbolo. `py_func_name` sempre teve filtro de keyword; o gerador irmão não.

## Três armadilhas de instrumento (custaram ~2h)

- **`daemon.stderr.log` é compartilhado** por todos os daemons (global, per-project,
  privados de teste): os "trios de rebuild session=None" que persegui como sabotador
  externo eram, ao menos em parte, daemons efêmeros de teste rebuildando projetos tmp.
  Linha sem identidade de emissor não atribui culpa.
- **`cargo test` é fail-fast por binário**: "o teste X passou no gate N" só é válido
  com `--no-fail-fast`; duas conclusões minhas caíram por isso.
- **Janela curta de CPU (2-3s) não distingue pausa de bloqueio** — dois "0 jiffies"
  caíram em pausas legítimas do rebuild.

## Fixes aplicados

- `crates/touring-code/tests/property_tests.rs` — `prop_filter` de keywords em
  `py_class_name` (19/19 verde; espelho `client/` sincronizado).
- `loop_converged.py::clause_cargo` — cargo roda com daemon **privado por escopo**
  (`/tmp/touring-gate-<sha12>.sock`, spawn explícito + espera do socket, nome estável
  para reuso). Mesma lição que `e2e_diagnostic_rfc100.rs` codificou em 03/08/2026:
  "com socket próprio a contenção some por construção".

## Tickets abertos (produto)

- Peer pid/comm (SO_PEERCRED) na linha `heavy op:` do daemon; log por-daemon.
- Rebuild precisa ceder o ator (chunking/yield) ou fila com prioridade para reads.
- `daemon-ctl restart` deixa o daemon velho drenando 30+ min.
- Atores de `/tmp/cargo-mutants-*.tmp` persistem no estado do daemon.
- `cargo-mutants --jobs 24` órfão (linhagem 02:50→05:12, morto às 05:19) — spawner
  interno não identificado; assinatura de args casa `mutation_test.rs:296-301`.

## Adendo 07:55 — é uma CLASSE, não um bug (gates #3-#12)

Com o juiz isolado e daemon privado fresco por rodada, a suíte continuou falhando em
**pontos diferentes a cada rodada**: viz estrangulado (graph), sondas rfc100 com env
herdado, e às 07:43 uma assinatura nova — `touring-cli` lib `pensieve_*`/`cli_prove`
pendurados 24+ min com 0 CPU, sem filhos e sem write-lock (parente do gotcha
pipe-leak de 2026-06-27). Enumeração `nextest --no-fail-fast`: 15.513 testes,
15.496 verdes, 17 falhas — todas da classe. O `retries=3` do `[profile.ci]` do
nextest vinha mascarando a classe inteira em CI.

**Fix de classe (decisão de escopo para Gabriel):** refactor e2e-isolation — todo
teste daemon-touching possui seu `PrivateDaemon` (padrão `rfc100-b310`), retirar
retries do profile ci, e SO_PEERCRED na linha `heavy op:` para atribuição.

Memórias: `cargo-green-starvation-diagnosis-2026-08-21` ·
`suite-shared-state-flakiness-class-2026-08-21` ·
`daemon-actor-wedge-2026-08-21` (resolved) · `log-compartilhado-nao-atribui` (auto-memory).
