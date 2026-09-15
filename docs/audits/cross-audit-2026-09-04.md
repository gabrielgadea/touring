---
type: AuditReport
title: "Cross-audit 04/09/2026 — economia de contexto: a régua auditada pela própria disciplina"
description: "Auditoria cruzada de purpose-fidelity sobre tudo o que a sessão de 04/09/2026 produziu. Cinco achados, todos confirmados por execução; dois deles invalidavam números já publicados e um era regressão introduzida no mesmo turno."
plan: 2026-09-04-economia-de-contexto
plan_id: 2026-09-04-economia-de-contexto
tags: [cross-audit, purpose-fidelity, context-engineering, regua, classe-b]
timestamp: 2026-09-04
okf_version: 1
---

# Cross-audit — 04/09/2026

> **Escopo**: tudo que a sessão produziu — a extensão da régua `context_budget`
> (P5), o executor da classe B (S-4.3), o warm-up do daemon privado, o espelho
> `client/`, o bundle OKF e as memórias.
> **Veredito**: **5 achados, 5 corrigidos**, todos com red→green executado.
> Dois invalidavam números já escritos no plano; **um era regressão introduzida
> no próprio turno auditado**.

---

## VERDICT

A pergunta desta skill não é "quebra?", é **"cumpre o propósito que declara?"**.
Três artefatos declaravam um propósito que não cumpriam:

| artefato | propósito declarado | o que fazia |
|---|---|---|
| `ToolStats.reused_*` | *"piso do uso — linhas que REAPARECEM depois"* | contava co-ocorrência no turno, inclusive texto anterior ao resultado |
| `user_turns` | denominador de turnos **humanos** | contava saída de slash command, banner de caveat e resumo de compactação |
| `warm_heavy_path` | aquecer o daemon **fora** do orçamento dos testes | aquecia dentro do primeiro chamador — que era um teste de latência |

Nenhum dos três seria pego por teste unitário: todos passavam verdes. Foram
pegos por **perguntar o que o contrato promete e executar contra isso**.

---

## SCORECARD

| eixo | antes da auditoria | depois |
|---|---|---|
| suíte do workspace | 8.834 passando, **1 falhando** | ver §PROVA |
| `context_budget` | 15 testes | **17 testes** (+2 sondas de contrato) |
| hooks loop-engineering | 130 testes | **132 testes** |
| guards cruzados D8 no CI | 4 | **5** (`test_human_turn_predicate_parity.py`) |
| sonda viva | `#[ignore]`, **0 execuções automáticas** | roda no CI, pula honestamente sem corpus |
| 6 dims P0 BLOCK | 1.000 | 1.000 |
| 50-dim composite | Platinum 0,937 | Platinum 0,937 |
| órfãos | 1557 | 1557 (0 novos) |

---

## FINDINGS — todos confirmados por execução

### A-1 · A métrica de reuso não era o piso que declarava · **P0**

**Contrato**: *"as linhas do resultado que reaparecem no que o assistente
escreveu DEPOIS"* — declarado piso do uso, portanto teto do que um digest pode
descartar.

**Real**: a janela acumulava desde o início do turno, então um resultado que
chegava tarde era medido contra texto escrito **antes de ele existir**.

**Evidência** (`sonda_texto_anterior_ao_resultado_nao_pode_contar_como_uso`):

```
assertion `left == right` failed: texto que PRECEDE o resultado nao pode contar como uso dele
  left: 1
 right: 0
```

**Correção**: cada resultado guarda quanto da saída já existia quando chegou
(`PendingResult.after`), e o `flush` caminha a lista **de trás para frente**,
crescendo os conjuntos-sufixo — uma passada só sobre a janela (a forma ingênua,
um conjunto por resultado, é quadrática num turno com muitas chamadas, e esta
régua roda dentro de `touring kpi`).

**Impacto no número**: `reuse_ratio` **0,067 → 0,040** (−40,3%). Um limite que
erra na direção em que se diz seguro é pior que limite nenhum.

### A-2 · Regressão introduzida no próprio turno auditado · **P0**

**Contrato**: o aquecimento do daemon privado paga a montagem do índice **fora**
do orçamento de qualquer teste.

**Real**: `warm_heavy_path` foi posto dentro de `shared()`, que é inicialização
preguiçosa e portanto roda no **primeiro chamador, qualquer que seja**.
`predictive_wave_p99_guards` chama `private_daemon_env()` — um getter de
aparência barata — **dentro do laço que cronometra**.

**Evidência**: `D2 pre-tool-use CLI P99 = 10952ms, expected < 2_000ms` —
determinístico, reproduzido isolado.

**Correção**: `shared_warm()` separado; `shared()` volta a ser barato; o b310
opta pelo aquecimento no sítio que precisa dele.

**Prova**: `test result: ok. 2 passed` em **0,92s** (contra 10,97s).

**A lição é geral**: *custo escondido atrás de acessor preguiçoso é cobrado de
quem chegar primeiro — e quem chega primeiro pode ser um caminho medido.*

### A-3 · A sonda viva nunca rodava · **P1**

**Contrato**: a sonda contra transcripts reais é o instrumento que pega o que
teste sintético não pega — o log do bundle credita a ela a descoberta do erro
de 12× na própria régua.

**Real**: `--ignored` não aparecia em **nenhum** workflow nem script
(varredura de `.github/workflows/*` + `scripts/*.sh`: 0 ocorrências). E a
auditoria encontrou 3 asserções novas acrescentadas a ela nesta mesma sessão —
todas igualmente inertes.

**Correção**: a sonda pula com mensagem quando não há corpus (ambiente sem
transcript não é instrumento quebrado, é ambiente sem dado), e entra no CI.

**Prova, os dois caminhos**:
```
com corpus:  test ... live_probe ... ok            (1 passed, 0.72s)
sem corpus:  SKIP live_probe: sem corpus de transcript neste ambiente
             ("no transcript directory at …/fakehome/.claude/projects/…")
             test result: ok. 1 passed
```

### A-4 · O artefato classe B renascia a cada avaliação · **P1**

**Contrato**: um artefato em disco que o gate verifica.

**Real**: `glob: ""` ⇒ `_resolve_glob` devolve falsy ⇒ `hits` sempre vazio ⇒
materializa **toda** avaliação. Duas consequências: o diretório cresce sem
limite, e a cláusula fica **vacuamente verdadeira** — o artefato "aparece"
só por ter acabado de ser criado. Um gate que não pode reprovar não é gate.

**Correção**: identidade derivada de critério (`session_id` + `flow_armed_at`),
nunca de relógio — a disciplina da REGRA #17. Uma armação, um arquivo; a segunda
avaliação encontra o da primeira e não reescreve.

**Potencialização achada no caminho**: o materializador adivinhava o transcript
"mais recente do projeto". Com duas sessões CC no mesmo projeto — o cenário que
a REGRA #19 existe para tratar — o mais recente pode ser o da **outra** sessão,
e o registro sairia com trabalho alheio dentro. Agora prefere o
`transcript_path` que o harness **nomeia**: identidade, não heurística.

**Prova**: `test_a_segunda_avaliacao_reaproveita_o_registro_da_primeira` —
mesma armação, mesmo arquivo, conteúdo preservado, **1** arquivo em disco.

### A-5 · 20,3% dos "turnos humanos" não eram humanos · **P0**

**Contrato**: `user_turns` é o denominador de `injected_bytes_per_turn`, a
métrica-manchete de todo o plano.

**Real**: o Claude Code arquiva como record de usuário coisas escritas por
máquina — saída de slash command, banner de caveat, resumo de compactação.
Nenhuma carrega `tool_result`, então o predicado óbvio conta todas.

**Evidência** (5 transcripts, predicado estreito):

| classe | ocorrências |
|---|---:|
| `<local-command-caveat>` | 13 |
| `<local-command-stdout>` | 13 |
| resumo de compactação | 9 |
| **total** | **35 de 172 (20,3%)** |

**Este arquivo já tinha o defeito complementar registrado**: uma mensagem cujo
`content` é string crua não contava como turno, e a taxa lia **12× alto**. Um
denominador tem duas formas de errar e as duas estiveram vivas aqui. O teste
`so_a_fala_humana_conta_como_turno` fixa os dois lados de uma vez.

**Correção**: `NON_HUMAN_TURN_MARKERS` + `is_human_turn_text`, **uma fonte** em
Rust e a mesma declarada em Python, com guard cruzado D8
(`scripts/test_human_turn_predicate_parity.py`) que lê o executor Rust por regex
e a declaração Python por AST, e compara declaração **e** comportamento.

**Red→green por mutação**: remover **um** marcador do lado Python reprova
**dois** testes (paridade de literais E paridade de comportamento); restaurado,
3 passam.

**Impacto no número**: `injected_bytes_per_turn` **11.669 → 15.200** (+30,3%).
A régua subestimava em 30% exatamente o custo que existe para expor.

### A-6 · Flake sob carga no `migrate` — e a inferência errada que quase virou fix · **P1**

Achado ao rodar a suíte completa depois das correções: `cli::migrate::tests::
rlm_memory_skipped_gracefully_when_source_table_absent` reprovou com
`cmd_run must not crash on wrong source schema: database is locked` (erro 5 do
SQLite). Isolado, passa **6 de 6**; só falha sob a suíte inteira.

**Duas hipóteses testadas e REFUTADAS por medição, antes de qualquer correção:**

1. *"O teste escreve no banco REAL do operador"* — plausível: o tempdir não tem
   marcador `.touring`, e `normalize_project_root_inner` cai no fallback `$HOME`
   sem marcador; há até um guard no CI para essa família
   (`test_cli_tests_isolate_project_root.py`), que varre só
   `crates/*/tests/*.rs` e **não cobre** teste unitário em `src/`. Medido:
   mtime e tamanho de `~/.claude/touring/memory.db` **idênticos** antes e depois.
   Refutada.

2. *"Não há `busy_timeout` — `grep` devolve 0 no arquivo"* — verdadeiro como
   observação, **errado como inferência**. Ausência de DECLARAÇÃO não é ausência
   de COMPORTAMENTO: o rusqlite traz 5s por padrão. A mutação decidiu: remover
   a chamada explícita deixa o teste **verde**; zerar a constante o torna
   **vermelho com a mensagem de produção** (`database is locked, Error code 5`).

**O que sobrou, provado**: o mecanismo é contenção real de lock, e sob a carga
do workspace inteiro ela passou dos 5s do default. A correção declara 30s —
uma migração é idempotente, sem pressa, rodada uma vez; não há razão para
desistir em 5s, e 30s custam zero no caso normal.

**Prova**: `migracao_espera_o_lock_em_vez_de_desistir` segura o banco de destino
por 300 ms com `BEGIN IMMEDIATE` e roda a migração contra o lock. O teste prova
o **mecanismo**; o valor da constante é escolha declarada, e o comentário diz
exatamente isso em vez de fingir que o teste o defende.

**A lição meta**: eu quase enviei um `busy_timeout` "corretivo" cujo red→green
nunca fui buscar. A mutação que eu rodei por disciplina — não por dúvida — é a
única razão de o comentário não estar mentindo agora.

---

## FUSED RISK — o mecanismo comum

Os cinco são a mesma forma, que é a forma que o plano auditado diagnosticou:
**uma regra declarada num lugar e um executor decidindo por conta própria
noutro.**

| achado | onde a regra foi declarada | onde o executor decidiu diferente |
|---|---|---|
| A-1 | doc de `ToolStats`: "reaparecem depois" | `flush` sobre a janela do turno inteiro |
| A-2 | doc de `warm_heavy_path`: "fora do orçamento" | dentro do primeiro chamador |
| A-3 | doc: "a sonda é o instrumento que pega" | nenhum runner a invoca |
| A-4 | manifesto: "artefato que o gate verifica" | escrito na hora de verificar |
| A-5 | `user_turns`: "turnos de usuário" | todo record sem `tool_result` |
| A-6 | guard "teste em tmpdir marca o projeto" | varre só `tests/`, não `src/` |

E um sétimo, que um guard EXISTENTE pegou em mim: o teste de paridade importava
o espelho via `importlib` e deixava `__pycache__/*.pyc` dentro de `client/` —
`test_no_generated_artifact_sits_in_the_mirror` reprovou. O guard funcionou; o
defeito era meu. Remédio: `sys.dont_write_bytecode` em volta do import — um
teste não pode sujar a árvore que inspeciona.

**Uma correção de medição do próprio processo**: `cargo test` para no primeiro
BINÁRIO que falha. Os totais que relatei em rodadas anteriores ("8.834
passando") vinham de execuções truncadas — a suíte nem chegou aos crates
seguintes. A verificação final usa `--no-fail-fast`.

Dois deles foram cometidos **nesta sessão, pela mesma disciplina que os
diagnostica**. Isso é o dado mais útil do relatório: a regra não protege quem a
escreve; só o executor protege — e por isso cada correção saiu com um guard que
falha, provado por mutação.

---

## ROOT-CAUSE — o que teria pego mais cedo

| achado | o que teria pego antes |
|---|---|
| A-1 | escrever a sonda de contrato ANTES do algoritmo: "texto anterior não conta" é uma frase do contrato, não uma ideia posterior |
| A-2 | perguntar *quem mais chama isto* antes de pôr efeito em inicialização preguiçosa — eram 3 binários, e um cronometrava |
| A-3 | `grep --ignored` no CI no dia em que a primeira sonda nasceu |
| A-4 | perguntar o que acontece na SEGUNDA avaliação, não só na primeira |
| A-5 | medir a população do predicado (`quantos records casam?`) antes de confiar nele |

Quatro dos cinco são a mesma pergunta: **"e na segunda vez? e para o outro
chamador?"**.

---

## PROVENANCE — comandos executados

| evidência | comando |
|---|---|
| A-1 red→green | `cargo test -p touring-cli --lib context_budget::tests::sonda` |
| A-2 red | `cargo test -p touring-hooks --test predictive_wave_p99_guards test_d2` → `P99 = 10952ms` |
| A-2 green | idem → `2 passed` em 0,92s |
| A-3 dois caminhos | `cargo test … live_probe -- --ignored` + o mesmo binário com `HOME` sem corpus |
| A-4 | `pytest test_outer_gate_require.py` → 132 passed |
| A-5 mutação | remover 1 marcador → 2 failed; restaurar → 3 passed |
| re-medição | `cargo test … live_probe -- --ignored --nocapture` |
| sonda de decis | validada contra a régua: 0,042 vs 0,040 |
| 6 dims P0 | `touring-quality check --gate F2.{1,4,5,6} F4.{3,5}` → 1.000 |
| 50-dim | `touring-quality score --workspace --fail-below 0.80` → Platinum 0,937 |
| órfãos | `touring wiring orphans -j` → 1557, 0 novos |

---

## O EFEITO NOS NÚMEROS PUBLICADOS

Ordem de Gabriel: *re-medir e corrigir, preservando o valor antigo e a causa.*

| grandeza | publicado (contaminado) | corrigido | delta | causa |
|---|---:|---:|---:|---|
| `user_turns` (denominador) | 165 | **133** | −19,4% | A-5 |
| `injected_bytes_per_turn` | 11.669 | **15.200** | **+30,3%** | A-5 |
| `tool_output.reuse_ratio` | 0,067 | **0,040** | **−40,3%** | A-1 |
| zero-reuso Bash | 5,5% | **14,8%** | +169% | A-1 |
| zero-reuso Read | 2,0% | **7,2%** | +260% | A-1 |

### A conclusão da P5 sobrevive — e sai mais forte

A refutação do digest por truncagem descansava em duas medidas, ambas
contaminadas. Re-medidas com a semântica correta:

| decil | antes | **corrigido** |
|---|---:|---:|
| d1 | 9,55% | **6,15%** |
| d2–d9 | 5,7–7,4% | **5,54–7,10%** |
| d10 | 5,86% | **5,36%** |

O pico do primeiro decil era **artefato da contaminação** — texto escrito antes
do resultado casava com o cabeçalho dele. Sem ele, a distribuição é ainda mais
plana. E cabeça+cauda(40) cobre ≥90% do reuso em **17%** dos resultados grandes
(2 de 12), contra os 45% relatados.

**Cortar por posição perde mais do que eu havia dito.** A conclusão original
(digest derivado do conteúdo — `--brief` — e spill com `retrieval_hint`) fica
confirmada por medida melhor, não por sorte.

**O que MUDA**: existe uma população descartável, e ela é maior do que eu
relatei — 14,8% dos resultados do Bash e 7,2% dos do Read não são observavelmente
usados por nada. Antes eu havia escrito "não existe população descartável";
com o instrumento corrigido isso é falso. É um alvo real para trabalho futuro,
e ele nasce desta auditoria, não do plano.

---

## ACTIONS — o que fica aberto

| # | item | por quê não agora |
|---|---|---|
| 1 | os 14,8% de resultados Bash sem reuso algum | alvo novo, revelado pela correção; exige desenho próprio, não cabe em correção de auditoria |
| 2 | deploy (`update-touring`) | muda o comportamento de toda sessão CC — aguarda ordem de Gabriel. Enquanto o daemon carregar o binário antigo, `touring kpi -j` não mostra `tool_output` |
| 3 | `aggregate_lines` com CC=33 | a função cresceu com a régua; refatorar durante auditoria misturaria correção com reforma |

---

_Relatório da FASE 7. O veredito de convergência é o exit code de
`loop_converged.py`, não esta prosa._
