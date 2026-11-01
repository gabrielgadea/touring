---
type: Resume
title: "RETOMADA — impacto da injeção de contexto dos hooks do Touring + interruptor"
description: "Estado em disco para retomar sem depender de resumo de contexto."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# RETOMADA — onde está tudo

> Contexto desta sessão está em 56% e a autocompactação dispara aos 65%. **Este arquivo é
> o estado em disco.** Não dependa do resumo: leia os documentos abaixo.

## O pedido

Gabriel, 26/10/2026: *compreender o impacto que a injeção de contexto dos hooks do touring tem
no comportamento do llm que está utilizando-o; se ajuda de fato, se atrapalha, se melhora etc.
E precisamos de uma opção de ligar/desligar a injeção de contexto dos hooks.* — depois:
*"é preciso saber a adesão e o impacto de TODO contexto injetado na janela do llm, por TODOS os
hooks do Touring e seus eventos"* e *"aprofunde a Wave 2 nativa em Rust"*.

## Os documentos (leia nesta ordem)

| # | arquivo | o que tem |
|---|---|---|
| 1 | **`IMPACTO-POR-HOOK.md`** | **O documento da rodada 3/4.** A pergunta que faltava: *cada conteúdo injetado produz o efeito esperado, e de que forma?* 16 famílias, conformidade e forma do efeito, o confronto com o **contrato declarado pelo próprio harness** (`pre_read.rs:1-7` + o teste do "core invariant"), o **defeito do `similar-fail`** (alerta sem alvo) e a **correção do interruptor** (13,6% → 19,7%) |
| 2 | **`ANALISE-CONSOLIDADA.md`** | **O documento principal.** 10 seções: veredito, a janela, o mapa dos 134 injetores, **o comportamento medido por canal** (§4, com a tabela de adesão/impacto por hook e evento em §4.6), as 4 maneiras de atrapalhar, o interruptor, **os 3 defeitos da régua** (§7), a **Wave 2 nativa em Rust** (§8, com o esqueleto Rust, os 4 sítios, a telemetria, os 8 testes e a sequência de 9 passos), a réplica do baseline, o mapa de cobertura e **§11 — as correções da rodada 3/4** |
| 3 | **`IMPACTO-QUALITATIVO.md`** | **O complemento qualitativo** (210 linhas): os **cinco modos de influência** (INFORMAR · INSTRUIR · RESTRINGIR · DESVIAR · IMPRIMIR), o mapa hook a hook com **o que cada um melhora e o que cada um restringe**, a **conformidade medida por família** e a descoberta de que **o efeito do conteúdo é DISTAL** |
| 4 | `analysis-2026-10-26.md` | a RODADA 1 (volume, mapa, interruptor) — superado pelo consolidado |
| 5 | `strategy-2026-10-26.md` | o desenho do interruptor (3 níveis, cobertura, uso) |
| 6 | `evidence/injectors-corpus.txt` | inventário bruto das 134 combinações injetoras (21 dias) |
| 7 | `evidence/adesao-impacto-por-hook.txt` | a tabela de adesão/impacto por hook e evento |
| 8 | `evidence/conformidade-por-familia.txt` · `evidence/conformidade-por-familia-r3.txt` | a conformidade e a **forma do efeito** por família (rodada 2 e rodada 3/4) |
| 9 | `evidence/measure_injection_impact.py` · `evidence/measure_injection_effect.py` | os dois scripts de medição |

## O veredito, em uma linha

**A injeção atrapalha na proporção do que é sugestão e ajuda na proporção do que é fato ou
coerção.** (Rodada 3/4, 400 sessões, 28.295 blocos) — **FATO: 52% de conformidade** ·
**SERMÃO: ≈0,3%** · **ALERTA sem alvo: 0%** · **RUÍDO: 5,3%**. Coerção (gate): **1,000 por
construção** (reescreve o comando) + **13 negações** + **581,9 KB elidos**.

**A variável que decide não é o volume nem a prominência: é o conteúdo carregar um fato
que o modelo não tinha.** O harness declara esse critério em código (`pre_read.rs:1-7`,
"SILENCE IS THE DEFAULT / So what?") e tem **teste** para ele — e **~34% do contexto
injetado viola os três testes que ele declara**.

## O interruptor — entregue e instalado no live

```bash
TOURING_HOOK_INJECTION = full | no-advisory | off
```

- `full` (padrão): **byte-idêntico** ao comportamento atual (o shim faz `exec` direto).
- `no-advisory`: **recomendado** — corta **19,7% dos bytes** do contexto do Touring
  (medido no corpus: 2.888.894 de 14.644.672). O que corta é exatamente o que tem
  **efeito medido zero**: o sermão (incluindo o **sem cabeçalho**, que era o furo de
  13,6%), o alerta sem alvo e o ruído. O que fica é o canal de fato — **52% de
  conformidade** — e **todos** os gates.
- `off`: corta todo o contexto; os hooks **continuam bloqueando**.

**Para ativar**: acrescentar `"TOURING_HOOK_INJECTION": "no-advisory"` ao bloco `env` de
`~/.claude/settings.json` — **não foi feito** (decisão do Gabriel).

## Arquivos de código

| arquivo | papel |
|---|---|
| `client/hooks/injection_policy.py` | **fonte única** da política (níveis, taxonomia de famílias, filtro) — **revisada na rodada 3/4**: 10 famílias novas, taxonomia por **conteúdo** e não por cabeçalho |
| `scripts/hooks/touring-hook-shim.sh` | filtra a saída dos hooks Rust no caminho do daemon (~95% dos bytes) |
| `scripts/guard_injection_policy.py` | guarda: 152/152 registros cobertos **e nomes de família validados** |
| `scripts/test_injection_policy.py` | **70 asserções** (24 novas na rodada 3/4) |
| `scripts/measure_injection_impact.py` | medição de adesão/impacto por hook e evento |
| `scripts/measure_injection_effect.py` | **reescrito na rodada 3/4**: taxonomia por conteúdo, sinais por família, forma do efeito |
| 10 hooks Python/shell | consultam a política antes de emitir |

## Estado verificado (26/10/2026, rodada 3/4)

`python3 scripts/test_injection_policy.py` → **70/70 PASS** ·
`python3 scripts/guard_injection_policy.py` → **152/152, 0 lacunas, 0 nomes obsoletos** ·
`python3 scripts/measure_injection_effect.py --max-sessions 400` → tabela de 16 famílias ·
medição de bytes do interruptor → **19,7% cortado** ·
`bash -n scripts/hooks/touring-hook-shim.sh` → OK.

## Pendências

1. **A4** (subagente) não entregou: as entradas restantes da tabela `FAMILIES` Rust (o
   enriquecimento), a função `permite_injecao` com testes, e os contadores de telemetria.
2. **A Wave 2 nativa em Rust não foi implementada** — só o desenho (§8). O caminho:
   `crates/touring-foundation/src/injection_policy.rs` + re-export em `touring-hooks-shared`.
   **A taxonomia nativa precisa nascer da taxonomia por conteúdo** (§1, D1 da rodada 3/4),
   e não da de cabeçalho — senão a Wave 2 nasce com o mesmo furo.
3. **O A/B de corpus** (`crates/touring-hooks-shared/tests/s8_corpus_ab.rs`) é o passo que
   falta para medir **resultado** em vez de adesão — e decide se o `no-advisory` pode ir além.
4. **O defeito do `similar-fail`** (§5 de `IMPACTO-POR-HOOK.md`) — **não corrigido**: é
   Rust. `Pensieve::check_known_failure` devolve `Option<f64>` e descarta o `FailedPath`
   que tem o `reason` (`pensieve.rs:117,139`); o produtor tem o comando
   (`post_bash.rs:199-206`) e a mensagem não mostra (`pre_bash.rs:236`). A correção é
   de **interface**: devolver a `FailedPath` casada. Três consumidores do mesmo defeito:
   `pre_bash.rs:236`, `pre_edit.rs:436`, `pretask.rs:326`.
5. **A régua** (`context_budget.rs`) tem o mesmo furo de taxonomia: 19% do contexto
   do Touring cai em "outros". Corrigir junto com a Wave 2.

## Pendências (rodada 3/4 — substituem a lista anterior)

1. **A4** (subagente) não entregou: as entradas restantes da tabela `FAMILIES` Rust (o
   enriquecimento), a função `permite_injecao` com testes, e os contadores de telemetria.
2. **A Wave 2 nativa em Rust não foi implementada** — só o desenho (§8). O caminho:
   `crates/touring-foundation/src/injection_policy.rs` + re-export em `touring-hooks-shared`.
3. **O A/B de corpus** (`crates/touring-hooks-shared/tests/s8_corpus_ab.rs`) é o passo que
   falta para medir **resultado** em vez de adesão — e decide se o `no-advisory` pode ir além.
4. **Premissa já verificada**: `systemMessage` é canal de **modelo** (`context_budget.rs:369`
   conta-o como injeção; `types.rs:145` diz "for context injection"). A análise foi corrigida.
5. **O defeito do `similar-fail`** (§5 de `IMPACTO-POR-HOOK.md`) — **não corrigido**: é
   Rust. `Pensieve::check_known_failure` devolve `Option<f64>` e descarta o `FailedPath`
   que tem o `reason` (`pensieve.rs:117,139`); o produtor tem o comando
   (`post_bash.rs:199-206`) e a mensagem não mostra (`pre_bash.rs:236`). A correção é
   de **interface**: devolver a `FailedPath` casada. Três consumidores do mesmo defeito:
   `pre_bash.rs:236`, `pre_edit.rs:436`, `pretask.rs:326`.
6. **A régua** (`context_budget.rs`) tem o mesmo furo de taxonomia: 19% do contexto
   do Touring cai em "outros". Corrigir junto com a Wave 2.

## Lições desta sessão (graves, para quem retomar)

1. **Nome de família escrito como literal em N call sites divergiu** (`session-start` vs
   `session-start-contract`) e o sintoma foi **silencioso**: nome inválido caía em "permitir",
   os dashboards seguiam sendo injetados e a guarda passava verde. ⇒ Agora `allows()` falha
   **fechado** para família desconhecida e a guarda **valida os nomes**.
2. **`defaultdict` + `+=` em chave ausente** executa a aritmética sobre o valor da fábrica
   (`dict + int` ⇒ TypeError). Use `.get`.
3. **A4 e a revisão de segurança corrigiram duas inferências minhas** — `systemMessage` é
   contexto de modelo, e `session-start` designava duas coisas em lados diferentes.
4. **Classificar por cabeçalho esconde a família que mais importa.** O caminho de
   *orçamento de turno esgotado* emite o sermão sem cabeçalho: 5.630 blocos caíam em
   "outros" e a régua media 906. ⇒ **classifique por conteúdo**, e valide a cobertura
   da taxonomia contra o corpus (nomes + bytes), não contra a lista de marcadores.
5. **O instrumento precisa ter o sinal do FATO, não o do sermão.** O efeito do conteúdo
   é **distal**: o modelo age em cima do bloco (75,6%) sem nunca citá-lo (1,1%). Um
   instrumento de adesão proximal mede o inverso do que importa.
6. **O `no-advisory` promises mais do que a taxonomia cobra.** A assimetria
   conservadora ("não descartar o que não se reconhece") transforma *taxonomia
   incompleta* em *switch que não corta*. Medir o switch contra o corpus — não contra
   a intenção — foi o que revelou 13,6% → 19,7%.

---

## RODADA 4 (26/10/2026) — qualitativa, com fontes externas

> **DAG**: `task_1793033502719494636` (6 subtasks, validado, sem ciclos).
> **Bundle**: `docs/plans/2026-10-26-injecao-contexto-hooks/round4/`.

### O que entrou

| arquivo | o que é |
|---|---|
| `round4/CERTIFICACAO-FONTES.md` | **a certificação que o Gabriel pediu**: há DOIS notebooks "Claude Code"; o dele é `65c3ecb1-…b469` (91 fontes, 25/09/2026) — o com **espaço final** no título. O de 42 fontes (2025-06-27) é homônimo antigo e sequestra a busca por título |
| `round4/fonte-notebooklm-claude-code.md` | extrato direto: "What to Delete" (6 categorias), as 12 regras do Opus 5.5, `fast-jev-compaction` (evicção, não resumo), `pond-db` (staleness) |
| `round4/scout-context7.md` | a doc oficial **não publica critério editorial** (65 docs, negativo); teto de 10.000 chars; `systemMessage` contraditório entre 2 fontes first-party |
| `round4/scout-harness2.md` | **a ablação existe** (paper arXiv 2610.14224, Table 6): remover prompt\* custa **−1,5**, skills −5,3, scripts −3,7 ⇒ always-on prompt é a categoria de **menor valor por token** |
| `round4/scout-notebooklm-github-repos.md` | hook ~98% de contexto economizado vs arquivo-de-instrução ~60%; push-vs-pull; forma vencedora do payload; cadência de re-emissão |
| `round4/scout-corpus-qualitativo.md` | a leitura qualitativa dos payloads verbatim |
| `round4/evidence-v1-v2.md` | **minhas** verificações: 114 blocos > 10.000 chars = **10,2% dos bytes inertes por mecanismo**; o Touring usa **só** `additionalContext` ⇒ o risco do scout de Context7 está **disprovado** |
| `round4/evidence-janela-e-redundancia.md` | a janela real (**12,0%**, não 27,8%) e **69,8% dos bytes são repetição** |
| `round4/MATRIZ-DIAGNOSTICO.md` | **a matriz** (FASE 2, architect) |

### Os achados que mudam o diagnóstico

1. **A doc oficial NÃO tem critério editorial** ⇒ o "So what?" do Touring
   (`pre_read.rs:1-7`) é **invenção local**, não doutrina. (Correção a mim.)
2. ⚡ **A ORDEM DE LEITURA (medido: 77.794 de 77.794 = 100%)** — o
   `additionalContext` do `PreToolUse` chega **DEPOIS** do `tool_use` que o
   modelo já emitiu. ⇒ **a ordem chega tarde demais para influenciar a
   decisão que ela queria orientar.** É o **mecanismo** do veredito: o
   `touring-suggest` tem a **melhor prosa** do corpus (79,2% ordem completa) e
   a **pior adesão (0,3%)**; o `pre-edit-prevention` (98,8%) chega 680 de 700
   vezes **depois** do Edit aplicado. Só o gate age no momento da decisão
   (reescreve). ⇒ Para **influenciar**, um hook precisa **reescrever/negar**
   ou ir para um evento que precede a decisão.
3. ⚡ **58,0% das ordens MUST do `code-mode-contract` chegam sintaticamente
   INVÁLIDAS** (1.076 de 1.854; 1.070 aspas ímpares, 513 heredoc sem
   terminador — após normalizar o ideoma `'\''`). ⇒ os "0% de adesão" dessa
   família **não são desobediência: é ordem quebrada**. Emissor:
   `turn_budget.rs:118-133` (`directives_only`).
4. **69,8% dos bytes injetados são repetição** — e o dedup **já existe**
   (`injection_dedup.rs:271`; call sites de produção **agora 3**: daemon + standalone + dedup-stdin nos hooks Python — S-4p).
5. **O melhor bloco do corpus (`[G2 …→ REESCRITO]`) está na família que a
   taxonomia não nomeia** (`outros`) — junto com o pior (`[generic]`).
   *Achado de instrumento, não de conteúdo.*
6. **A janela real é 12,0%, não 27,8%** — divergência metodológica **a
   reconciliar**, não a substituir em silêncio. (O stdout cru do Touring são
   **695 bytes** em 400 sessões ⇒ a hipótese de inflação por stdout cru está
   **refutada**.)
7. **A contradição da doc sobre `systemMessage` não atinge o Touring** (uso
   exclusivo de `additionalContext`; o ramo `systemMessage` da política nativa
   é **código morto hoje**).
8. **Duas taxonomias em duas linguagens divergiam em 9 famílias** (a Rust
   dizia `past-lessons`, a Python `similar-fail`) ⇒ **resolvido nesta rodada**
   (as duas passaram a ter os mesmos 27 nomes).
9. **`s8_corpus_ab.rs` NÃO invoca modelo** — mede **bytes**, não desfecho. ⇒ o
   A/B de resultado exige harness **novo** (com modelo no circuito).
10. **O "10,2% de blocos gigantes" da minha primeira medição estava ERRADO**
    (contava terceiros) ⇒ a fatia do **Touring** é **1,83%**.

### Estado do DAG (26/10/2026, 17:20)

- `S-1` (matriz) — ✅ **fechada** (`round4/MATRIZ-DIAGNOSTICO.md`, 1.162 linhas, 9 correções C1–C9).
- `S-2` (similar-fail nomeia o comando) — ✅ **fechada** (`FailedPath.subject` + `check_known_failure_detail` + `command_notice()` + teste).
- `S-3` (taxonomia por conteúdo) — ✅ **fechada** (catch-all 77,2% → **11,5%**; teste de paridade `the_python_policy_mirrors_this_taxonomy`).
- `S-4p` (alcance do dedup) — ✅ **fechada** (dedup-stdin nos hooks Python + rota standalone + systemMessage; 6.173 B/resumo medidos; DEPLOY PENDENTE até `update-touring`).
- `S-7` (ordem MUST executável) — ✅ **fechada** (4 testes novos; 494+20+29 PASS).
- `S-4` (política nativa Rust) — ✅ **fechada** (`injection_family.rs` + `injection_policy.rs` em `touring-foundation`; 568+711+34 testes; Diamond 0,95). **Falta aplicar nos 4 sítios** (forward_daemon_output, hook_response.rs, hook_runtime.rs, verdict.rs) — passo para o plano.
- **4 engineers mortos por 429** (rate-limit) — o S-2 foi recuperado do working tree (cargo check + clippy + testes verdes) e marcado completo; os outros (S-3, S-4p, S-7) estavam incompletos quando morreram.
- **1 clippy FAIL pré-existente** (`touring-quality/f3_1_py_imports.rs:130,131,198,261,387` — commit `e1bcd5a9`, fora do escopo dos engineers; registrado).
- **2 P0 workspace** (não da rodada 4): `F2_1` (`build.rs` 0.2) e `F2_5` (34 CVEs em `ctutils` e `quinn-proto`) — pré-existentes, registrados, não resolvidos nesta rodada.
- **1 achado de segurança** (`post_bash.rs` loga o comando cru — secrets/PII) — pré-existente, fora do escopo dos engineers.

### Gates verificados

`cargo check --workspace` exit 0 · `cargo clippy -p touring-intelligence -p touring-hook-handlers -p touring-cli --all-targets` exit 0 · `cargo test -p touring-intelligence -p touring-hook-handlers` exit 0 (711 passed) · `python3 scripts/test_injection_policy.py` **89 PASS / 0 FAIL** · `python3 scripts/guard_injection_policy.py` **152/152, 0 lacunas, 0 obsoletos** · `touring e2e -j` **0,853** (baseline 0,857).

### Correções de citação feitas nesta rodada

- `context_budget.rs` vive em **`crates/touring-cli/src/cli/context_budget.rs`**
  (não `touring-foundation/`). Os **números de linha estavam certos**
  (`injection_family` L200, `attributed_family` L298, L369 itera
  `additionalContext`/`systemMessage`); o **caminho** estava errado em
  `ANALISE-CONSOLIDADA.md` §8.3.2 e no comentário de
  `client/hooks/injection_policy.py`.


---

## RODADA 5 + REVISÃO DO PLANO (27/10/2026) — retome por aqui

**Pedidos de Gabriel**: "faça uma revisão completa do PLANO.md" → "preciso de uma
avaliação completa de [os 5 tickets]" → "corrija e atualize tudo" →
"recomendações dos tickets aprovadas".

**Estado (por evidência)**:
- Plano **v2**: `docs/plans/2026-10-26-touring-janela-eficacia/PLANO.md`.
  `plan_validator.py --strict` → **OK, 0 erros, 0 warnings**; `gap_detector
  --fail-on=P0` → exit 0; `dimension_scorer` (heurística léxica, sem ground truth)
  → composite **4,44**. Correção: o "0 warnings" reportado para a v1 em 26/10 lia
  um campo que não existe (`valid`); o veredito do validador é `status`, e a v1
  dava **FAIL** (6 erros de formato).
- Revisão: `.../REVISAO-2026-10-27.md` (R1–R12, aplicada). Decisões:
  `.../AVALIACAO-DECISOES-2026-10-27.md` (5 recomendações **aprovadas**).
- **C0 concluído**: `scripts/measure_tool_trio.py` + `scripts/test_measure_tool_trio.py`
  (9 PASS, ruff limpo). Os números de trio da rodada 5 foram **retratados**
  (§13 do consolidado reescrita e movida, porque estava enxertada no meio da
  §12.1; banners em `round4/MODUS-OPERANDI-FERRAMENTAS.md` e
  `round4/ESTRATEGIA-FERRAMENTAS.md`).
- Medição corrigida (200 sessões, 27/10): retry byte-idêntico **1,5%**, negado
  por hook **4,2%** (Edit 9,6%, Write 8,8%), Read = 39,6% dos bytes de resultado.
- T-C1, FACT nativo: `cargo tree -i quinn-proto --target all -e all` → nada;
  `cargo tree -i ctutils` → sem match. O F2.5 mede o **lockfile**.
- **DAG `task_1793043745985215009`**: 20 subtasks, valid, sem ciclos. Concluídos:
  C0, B3 (decisão: fora do alcance), T-C1/T-W0/T-A5/T-A3 (decisões fechadas).
  `frontier` → 0 decisões abertas; prontos: **A1, C1, C2, C3, C4, C5, C6**. O W0
  depende de A1, A2, C1, C4 e C5; o momento do restart é confirmado por Gabriel.

**Próximo comando**: `touring decompose claim task_1793043745985215009 --owner <id>
--subtask A1` (a Wave 1 roda em paralelo; nada pesado fora do `hw-slot`).

### Atualização 27/10 (tarde) — plano v3, pendências fechadas

- **Correção grave**: os 27 testes vermelhos de `client/hooks/tests/` são
  **regressão desta wave**, não "edição de outra sessão". O guard shell
  `if ! python3 "$HOME/.claude/hooks/injection_policy.py" --allows …; then exit 0`
  (`check_context.sh:3`, `doc-coverage-gate.sh:53`) sai mudo quando o arquivo de
  política falta. Correção planejada no S-8 (sair só com rc 1).
- F2.1 = falso positivo (`build.rs:42`, diretiva `cargo:rerun-if-changed`; detector
  em `touring-analysis/src/quality/security.rs:245`). CVEs: 38 advisories em 11
  pacotes, 6 com patch compatível (S-4) e 5 com upgrade maior (S-4b, fora do R1).
  O log cru é latente (`TOURING_LOG` off). Os órfãos são pré-existentes.
- Fontes consultadas de novo (evidência em
  `docs/plans/2026-10-26-touring-janela-eficacia/fontes-2026-10-27/`). O
  **T-B3 foi reapresentado** como T-B3b: os mods do CC (`session.compact`)
  alcançam a evicção.
- DAG: 22 subtasks, valid. Fronteira **A1, C1, C2, C3, C4, C5, C6**; o W0 depende
  de A1, A2, C1, C3, C4 e C5. `plan_validator --strict` OK.
- **T-B3b decidida (27/10)**: opção (2), mod `session.compact` determinístico,
  condicionado ao D1 → subtask B3m (depende de D1, T-B3b, C0; o D2 depende dela).
  CVEs: 38 − 4 aceitas no `deny.toml` = 34 do F2.5 (mesmo db). wasmtime 46 sem
  patch na linha (≥ 48.0.7), dependência direta do workspace (`Cargo.toml:339`).

---

## EXECUÇÃO DO PLANO v3 (27/10/2026, tarde) — estado por evidência

DAG `task_1793043745985215009`. Concluídos e verificados: **C0, C1, C3, C5, C6**,
mais T-* (decisões) e B3 (decisão). Concluídos pelo engineer E2 (falta a
verificação integrada): **C2** (falso positivo do F2.1 corrigido no detector,
`touring-analysis/src/quality/security.rs`; build.rs 0,2 → 1,0) e **C4** (4
"órfãos": 1 removido, 3 eram falsos do índice). Em curso: **E1** (A1 + A2) e
**E3** (D1, trio na emissão). D1 agora entra no R1 (D1 ← A2; W0 ← D1).

- C1: `cargo update` só no lock para ctutils, quinn-proto, rustls, rustls-webpki,
  webpki-roots e wasmtime@36. F2.5 34 → **18** (sobram wasmtime 46, z3, zip = S-4b).
  Backup do lock antes: scratchpad `Cargo.lock.pre-C1`.
- C3: **1.389 passed ×2**. Três regressões desta frente corrigidas: guard shell
  fail-closed (`check_context.sh`, `doc-coverage-gate.sh`; teste
  `test_injection_guard_shell.py`), a sentinela `"unknown"` na dedup do
  `session_startup_intelligence.py`, e testes vazando estado da dedup via
  `CLAUDE_PROJECT_DIR` (fixture autouse em `client/hooks/tests/conftest.py`).
- C5: `log_safe_command` (redact + 60 B) + `test_post_bash_log_has_no_raw_command`.
- C6: `docs/plans/2026-10-26-touring-janela-eficacia/audit-plan-completion.sh`
  (uma verificação por fase, fail-closed).
- D1a: `scripts/measure_tool_trio.py --extras --until` (14 fixtures). Ponto 0 da
  régua em `regua-2026-10-27-pre-deploy.{md,json}`: G6 repete 87,5% depois do
  deny; G3 é negado de novo em 25,4%; Read > 10 KB carrega 78,9% dos bytes de
  Read; 16,8% dos bytes de Read são superados.
- **Deploy não controlado**: às 10:19 a sessão `touring-a1` rodou `update-touring`
  (SEG-3); o daemon reiniciou às 10:29:02 e a wave S-\* (sem commit) entrou no ar.
- **Gate novo (pedido do Gabriel)**: `client/hooks/injection_policy_gate.py`
  (symlink em `~/.claude/hooks/`, registrado no `settings.json` PreToolUse
  `Write|Edit|MultiEdit`) **nega** gravar ou registrar um hook que injeta
  contexto sem consultar a política. Predicado único:
  `guard_injection_policy.uncovered_injector`. Testes:
  `client/hooks/tests/test_injection_policy_gate.py` (11). A guarda está em
  154/154 e agora é fail-closed quando não lê o `settings.json` (exit 2).
  Backup do settings: scratchpad `settings.json.bak-gate`.

**Próximo**: quando E1 e E3 voltarem → gates integrados (cargo test/clippy dos
crates tocados, fmt dos arquivos da wave, suites Python, e2e, audit) → commit
**só dos arquivos da wave** numa branch → `scripts/propagate-release.sh <v>`
(Gabriel autorizou: "resolva … R1") → verify → juiz.
