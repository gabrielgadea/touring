---
type: Audit
title: Cross-audit de tudo o que foi implementado desde 30/09 (Jev/Laya r2–r4, XA, fechamento do decompose, workflow) — 04/10/2026
description: 107 commits e 137 arquivos de código auditados contra o propósito documentado, com prova executada pela CLI real, dois revisores independentes e o juiz de convergência. Veredito PASSA COM RESSALVAS — juiz CONVERGED (cargo do workspace verde, Platinum 0,926) e 16 achados, 3 de severidade média (um deles regressão de 02/10). Fase de conserto em pausa, esperando aprovação.
tags: [audit, cross-audit, decompose, workflow, r3, shadow, jev, laya, agent-harness]
timestamp: 2026-10-04T16:20:00-03:00
---

# Cross-audit — 04/10/2026

**Escopo, medido no git (não na memória).** `git log --since=2026-09-30`: 107 commits, 137 arquivos de código fora de `docs/`: 39 Rust em 14 crates, 88 Python, 3 shell, 2 do `agent-harness`, `Cargo.{toml,lock}`, `CLAUDE.md`. Isso cobre o r2 e a potencialização do Jev/Laya (30/09–01/10), o r3 e o r4 (02/10), o XA (03/10), o fechamento do DAG do decompose (04/10, madrugada) e as decisões do canvas de 04/10. O delta desde o último cross-audit (03/10 01:00, `66c30a9c..0e246ff4`) tem 19 arquivos, +1964/−357, e foi o alvo da revisão linha a linha.

**Quem julgou.** Eu rodei a evidência. Dois revisores independentes leram o delta sem as minhas conclusões: o `touring-a4` (MiniMax, outra família de modelo, lente de correção, só leitura) e um revisor Claude de contexto limpo (lente "teste que passa sem provar", SQL e caminhos de erro). Cada achado deles foi verificado por mim no código ou ao vivo antes de entrar aqui. O juiz de convergência (`loop_converged.py --rust-full`) roda o `cargo` do workspace inteiro.

**Limite do que isto prova.** A1 e A2 continuam `NAO_ADOTADO_PELOS_DADOS` e o r5 continua fechado. Este audit mede se o que foi construído faz o que diz. "Funciona de forma perfeita" **não** é uma conclusão que a evidência permita: há 16 achados abertos abaixo, e três coisas não foram reexecutadas (§7).

## Veredito: PASSA COM RESSALVAS

O núcleo entregue em 04/10 funciona como documentado, provado ponta a ponta pela CLI contra o daemon e conferido no banco a cada passo. São 21 afirmações; as 4 reprovações foram duas expectativas minhas erradas e dois achados (X1, X13). Também se confirmou ao vivo:

- os dois emissores do r3 gravam com trabalho real;
- a privacidade passa nos dois journals;
- órfãos, doctor e e2e estão na linha de base.

A auditoria também achou uma **regressão de 02/10** no `agent-harness` (X3), que só passou porque a suíte dele nunca roda. Achou ainda um comando da CLI que nunca alcançou o próprio handler (X1); eu o tinha documentado como proposital em 04/10, e estava errado. Por fim, quatro testes passam sem provar o que o nome diz (X9–X12).

## 1. MAP

| Medida | Valor | Como |
|---|---|---|
| Blast radius (top 3) | `decompose.rs` 345 · `rlm.rs` 175 · `lib.rs` 171 | `touring ast meta` nos 39 Rust, 0 sem meta |
| Binários × fonte | o daemon (09:22:48) e o `touring` (09:37:32) contêm todo o código do escopo; o único arquivo mais novo que o daemon é `touring-server/src/cli/workflow.rs` (09:30:29), que roda só no CLI | `binary_freshness.py` (mtime + git log) |
| Árvore de trabalho | nenhum arquivo de código sujo (só `memory.db` e KPIs) | `git status --porcelain -- crates scripts` |

## 2. PURPOSE — o que foi provado funcionando (executado nesta sessão)

**Sonda ao vivo do ciclo do plano** (`live_audit.py`, CLI real, daemon PID 1254933, cada passo conferido no `knowledge.db` do projeto antes do seguinte; planos `task_1791140164961140598` e `task_1791140246238066646`, ambos finalizados no fim):

| Afirmação | Resultado |
|---|---|
| `create` grava o plano e o passe de retenção roda | OK: 0 arquiváveis > 7 dias no projeto e no global |
| `add` grava filhas `pending`; `in_progress` abre 1 tentativa; `completed` fecha com `cache_hit=1`; `failed` fecha com 0 e o retry abre outra | OK. A tentativa em voo nasce com `cache_hit` 0 (default da coluna), não NULL; o produto só lê o valor depois de `completed_at`, então isso não afeta nada |
| `workflow stats` e `workflow status`: `attempt_completion_rate` == SQL == 0,5, com a tentativa em voo fora | OK, sem a palavra `cache_hit` no texto |
| `workflow slowest` emite `attempt_outcome` | OK `["completed","failed"]` |
| todas as filhas concluídas ⇒ o pai deriva `done`; `add` num pai fechado o reabre (XA-14) | OK `('done', None)` → `('in_progress', None)` |
| `add` sem a chave `error` fantasma; re-`add` (upsert) preserva status **e** `created_at` (decisão (a) do canvas) | OK; asserção travada contra o vácuo (`before is not None`) |
| `finalize` carimba e declara `archived` | OK `('finalized', <carimbo>)` |
| `archive --dry-run`: `held` == SQL, plano a plano | OK projeto 0 = 0 (candidatos 9); global 1 = 1 |
| `updated_at` comparável como texto ao cutoff | OK 411/411 e 10/10 em RFC3339 UTC |
| wiring: nenhum consumidor Markdown (XA-17); `kind_unknown` 0 | OK 0 e 0 |
| `touring workflow resume` alcança `cli-workflow-resume` | **FAIL**: responde `workflow_start` (X1) |
| nenhum símbolo palavra de escopo no wiring | **FAIL**: 31 linhas (X13) |

**r3 em modo sombra, ao vivo:**
- **A1:** um `a1_block` do `pre-bash` às 15:58:52, no mesmo segundo em que o hook real injetou "⚡ last fail" num Bash meu.
- **A2:** um `a2_offer_closed` às ~15:49.
- **Flag:** `[shadow] r3 = true`; `touring kpi -j`: `shadow.status = measured`, 0 descartes, 0 linhas alheias, 0 ilegíveis.
- **S6 (privacidade), reexecutado:** `r3_events.jsonl` com 487 linhas, `r3_shadow.jsonl` com 400, 0 inválidas e 0 violações.
- **Sondas sintéticas:** as que mandei ao `pre-bash` não injetaram contexto e não gravaram evento, então o journal não foi contaminado.

**S-8 do r2 (gate semântico do `pre-bash`).** O aluno local (`127.0.0.1:8971`) não está escutando, e a unit nunca foi habilitada: é decisão sua pendente, registrada no FINAL-REPORT da potencialização. O fail-open está provado: comando desconhecido passa em ~20 ms, exit 0, sem advisory.

## 3. DEBT SCAN

`scan_debt` nos 137 arquivos acusou 19 marcadores, todos inofensivos:
- 13 são `#[ignore]`, `skipif` ou "todo" em português ("TODO módulo");
- 1 `# type: ignore` num import de disponibilidade (`g2_baselines.py:122`);
- o único `#[ignore]` real é o benchmark S1a, que agora roda pelo `scripts/r3-s1a-gate`.

Zero `unimplemented!`, `todo!()` ou WIP.

## 4. HARMONY

- **Órfãos:** 760 = a linha de base; 0 nos arquivos do escopo; `internal_only` 2.566.
- **Ciclos de wiring: 2.** Um de 2 módulos (`impl_vector_store` foundation × resilience) e um SCC de 1.242 módulos do grafo heurístico. Não tenho medição anterior para dizer se algum é novo: `UNVERIFIED` como delta.
- **Doctor:** 9/9 ok.
- **e2e:** 0,857 `pass` (mediana da linha de base 0,857). Por componente:
  - wiring 0,811 ≥ 0,809;
  - knowledge 0,900;
  - ast 0,776;
  - quality 1,000;
  - learning 0,886;
  - **index 0,847 < 0,852** (X14).
- **03/10, o que fechou:** os achados C, D, E, F e H de 03/10 estão fechados por commit (`1b7426cd`, `d46b3b1e`), e o H foi provado ao vivo de novo hoje.
- **03/10, o que segue aberto:** A e B seguem abertos por decisão sua (A/B adiado). O G (dependências) não foi remedido. O I **segue aberto** (X8).

## 5. Achados

Todos verificados por mim (leitura do código ou execução). "Fonte" diz quem apontou primeiro.

| id | sev | achado | evidência | fonte |
|---|---|---|---|---|
| **X1** | médio | `touring workflow resume <task>` vai para `cli-workflow-run` com `resume: true`, e `cli_workflow_run` nunca lê essa chave (só `task_id` e `color`). O handler `cli-workflow-resume`, registrado e com `next_subtask`/`attempt_outcome`, não é alcançável pela CLI nem pelo `touring.query` do SDK (que só abre RPCs de leitura). A rota é anterior a 04/10; em `3cd11992` eu a documentei como proposital, e isso estava errado | ao vivo: resposta `event: workflow_start`; `decompose_workflow.rs:265-321` | touring-a4 (A2) |
| **X2** | médio | `cli_workflow_resume` conta TENTATIVAS como filhas: o `LEFT JOIN subtask_results` dá uma linha por tentativa, e `total`/`completed`/`completion_pct` (`:383-396`) usam `subtasks.len()`. Com retry, diverge do `status` (ex.: 66,7 contra 50). Latente porque X1 o esconde; consertar X1 sozinho o expõe | leitura `:334-396`; o e2e só olha `attempt_outcome` | revisor cego (R1) |
| **X3** | médio | **Regressão de 02/10 (`74cd6871`)** no `agent-harness`: a whitelist do F2.1 virou `{"legacy": "file", "current": "file_path"}[schema]`, mas o detector devolve `legacy` ou **`v5`**, então `cli-anything-touring index status` quebra com `KeyError: 'v5'`. Antes, todo esquema que não fosse `legacy` caía em `file_path` | A/B no mesmo banco: o módulo anterior responde e o atual quebra; a suíte foi de 5 para 7 falhas (`index_status_json` e `json_sort_keys`) | esta auditoria |
| **X4** | baixo | `in_progress` repetido grava duas tentativas, porque `record_subtask_started` sempre insere e o fechamento fecha todas as abertas. A métrica `attempt_completion_rate` de 04/10 depende de uma linha = uma tentativa | ao vivo: `in_progress`, `in_progress`, `completed` → `1:1 1:1` | revisor cego (R5, 0,6) |
| **X5** | baixo | `record_subtask_started`/`_completed` engolem falha de escrita (`let _ =`, `decompose.rs:427,461`): a tentativa some sem rastro, enquanto os leitores da mesma onda aprenderam a reportar falha (`held_error`) | leitura | touring-a4 (A3) |
| **X6** | baixo | `r3-s1a-gate:50` diz "o custo do emissor r3 passou do limite" para QUALQUER rc ≠ 0: erro de compilação, flag inválida, `hw-slot` 70 (machine check) ou 75 (sem slot) | **executado:** `S1A_CARGO="cargo --broken-flag" scripts/r3-s1a-gate` → rc 1, `error: unexpected argument '--broken-flag'` duas vezes, e a última linha afirma que o custo do emissor passou do limite, sem que o custo tenha sido medido | touring-a4 (A1) + revisor |
| **X7** | baixo | `max_attempts: N` permite N+1 execuções: `attempts` conta retries e a guarda é `attempts >= max_attempts`. O docstring diz "máximo de tentativas antes de desistir"; o e2e que escrevi em 04/10 **fixa** as 3 falhas para `max_attempts: 2`. A semântica veio da cópia morta fundida em `b9b9b20b` | leitura `decompose.rs:377-397` e e2e `:966-985` | revisor cego (0,5) |
| **X8** | baixo | `decompose ready` lista filhas **concluídas** em `blocked_subtasks` (achado I de 03/10, nunca resolvido) | ao vivo: plano finalizado com 4 concluídas → 4 "bloqueadas" | 03/10 |
| **X9** | baixo (teste) | `test_family_table_parity_with_context_budget_kpi` reprovou por **corrida de população**: o porte e o `touring kpi` escolhem "os 3 transcripts mais recentes" em instantes diferentes, e o `touring-a4` escreveu no dele no meio. Prova aritmética exata: o KPI leu o transcript do par (14.633 B de `code-mode-deny`) e o meu (36.459), e 36.459 + 14.633 = 51.092, o número do KPI. O porte está certo; o teste compara populações diferentes sem saber. E o touring-a4 observou que não é só o teste: o KPI `context_budget` soma blocos dos transcripts de TODAS as sessões do projeto, então a população é parte da unidade do número; antes de ler a razão de uma família é preciso declarar de quem são os blocos contados | `parity_probe.py` (3/3 iguais depois), `parity_split.py` | esta auditoria |
| **X10** | — (reclassificado: não é defeito) | `the_parent_derivation_closes_on_every_status_the_foundation_calls_terminal` percorre `TERMINAL_TASK_STATUSES`, os mesmos 3 membros da lista privada removida: reverter a deduplicação o deixa verde. Verificado, mas a reversão é **comportamentalmente equivalente** (mesmos 3 status), e nenhum teste de comportamento a distingue. O teste guarda a divergência FUTURA (a foundation ganha um status e a cópia privada não), que é o propósito da deduplicação. Fica como limite declarado | leitura e2e `:838` | revisor cego (R3) |
| **X11** | baixo (teste) | a última asserção de `test_the_release_runs_the_gate_in_gate_1_and_announces_it_in_dry_run` (`"S1a" in body and "die" in body`) é vácua: apagar o `\|\| die "S1a …"` a deixa verde | **mutação executada** (`mut_x11.py`): sem o `\|\| die "S1a…"`, o teste sai rc 0 (o mutante sobrevive); arquivo restaurado com sha256 idêntico (`efacf913…`), `git status` limpo | revisor cego (R4) |
| **X12** | baixo (doc) | `ast_bridge.rs:197` ainda diz "XA-17 stays open", e o docstring de `:224` ("The REAL file…") se lê como reprodução, mas o teste não falha contra o código antigo. O XA-17 foi resolvido em `4c49ab1b`, em outra função | leitura | revisor cego (R2) |
| **X13** | baixo (estado) | 31 linhas de wiring com símbolo `super` (kind `extern`, `rust_import`, todas de 01/10 23:43–23:47, consumidores em `touring-analysis`) sobrevivem no banco do projeto. O código atual não as recria: reindexar um dos consumidores levou 1 → 0. Mas `wiring repair --purge-phantom` só remove kind `unknown`, então nenhuma rota de reparo as alcança; só reindex ou rebuild | ao vivo (`touring index ingest` em `equality.rs`) | esta auditoria |
| **X14** | baixo (métrica) | o `index` do e2e caiu de 0,852 para 0,847 por diluição: 82 relatórios de turno em `docs/plans/*-work-outer/turns/` são gravados por um hook fora do caminho que reindexa (`index why` → `eligible_not_indexed`) e entram no denominador `disk_files` | `touring index why`, contagem por diretório | esta auditoria |
| **X15** | baixo (pré-existente) | a suíte do `agent-harness` nunca roda: o executável `cli-anything-touring` não está instalado (19/19 `FileNotFoundError`). Com um wrapper temporário, há 5 falhas anteriores a 02/10: o harness lê tabelas que o Touring atual não tem (`learning_wilson`, `learning_drift`) e caminhos de banco antigos | wrapper no scratchpad, nenhuma instalação feita | esta auditoria |
| **X16** | baixo (memória) | 3 memórias não são alcançáveis a partir do índice por link nenhum (`ausencia-de-declaracao-nao-e-comportamento`, `retomar-autoresearch-rl-intelligence`, `skill-structured-reasoning-ledger`). Já estavam fora antes da fusão de 04/10 (ausentes do backup) | BFS sobre links e `[[wikilinks]]`: 205/208 | esta auditoria |

**Limites confirmados, não defeitos.**
- Os predicados `archivable`/`held` são complementares por construção, e um plano não pode ser carimbado e reportado como retido na mesma chamada. Resta um limite só de relatório: `candidates` é lido antes do UPDATE e pode contar duas vezes um plano que ganhe filha aberta no meio da chamada (touring-a4 P1).
- `add` não zera status de filha nem reabre `finalized` (P3).
- Nenhuma passada de wiring ficou de fora entre rebuild e edição (P4).
- `reply_failed` do bridge, a rota do `workflow status`, o RAII do `with_events_path` e os 9 guards `require_flag_unset` estão corretos (revisor cego).

## 6. E2E PROOF

| O que | Comando | Resultado |
|---|---|---|
| Suíte Python do escopo (44 arquivos) | `hw-slot -- pytest` | **646 passam, 1 falha** (X9, corrida); reexecutado depois: 3/3 iguais |
| Sonda ao vivo do ciclo do plano | `live_audit.py` | 21/25; 2 reprovações eram expectativa minha (`cache_hit` em voo), 2 são X1 e X13 |
| Instrumento da sonda | — | a 1ª rodada tinha 4 defeitos meus: id sem escopo, caminho errado do journal, regex que casou um comentário do toml e duas asserções que passavam em vazio (`None == None`, `{} >= {}`). Corrigidos e travados contra o vácuo antes da 2ª rodada |
| R5 ao vivo | `in_progress`×2 + `completed` | 2 tentativas fechadas (X4) |
| r3 S6 | `r3_prove_delivery.py s6` | passa, 487 e 400 linhas, 0/0 |
| `agent-harness` | pytest com wrapper | anterior a 02/10: 5 falhas; atual: 7 (X3, X15) |
| doctor / e2e / órfãos | `touring doctor -j`, `touring e2e -j`, `touring wiring orphans -j` | 9/9 · 0,857 · 760 |
| Juiz de convergência | `loop_converged.py --task task_1791115477937244883 --scope ~/projects/touring --rust-full` | ver §6.1 |

### 6.1 Juiz

`loop_converged.py --task task_1791115477937244883 --scope ~/projects/touring --rust-full --json` → **exit 0, CONVERGED**. Rodou sob `hw-slot --class rust`: 813 s, rc 0, sem machine check, chip a 84–97 °C.

| cláusula | resultado | evidência |
|---|---|---|
| `judge_intact` | PASS | juiz de registro intacto, 8 cláusulas |
| `dag_done` | PASS | 9/9 (o DAG do ciclo de 04/10) |
| `quality_gold` | PASS | Platinum 0,926 |
| `no_p0_fail` | PASS | nenhum P0 |
| `measured_whole_scope` | PASS | nenhuma dimensão truncada |
| `orphans_base` | PASS | 760 ≤ 2.481 |
| `cargo_green` | PASS | `cargo check` + `test` + `clippy` do workspace inteiro verdes |
| `cross_audit` | **N/A** | não existe `audit-plan-completion.sh`. N/A não é aprovação: o juiz não consome este relatório |

O que o juiz NÃO vê: os 16 achados acima passam todos pelo juiz. Ele mede compilação, testes, nota e órfãos, não a fidelidade ao propósito. É por isso que esta auditoria existe.

**Nota de instrumento.** A 1ª rodada (sem `--json`) também saiu com rc 0 em 892 s, segundo o `hw-slot.jsonl`, mas o stdout redirecionado ficou vazio. Provei que o `hw-slot` repassa o stdout do filho (teste com `print`), e a 2ª rodada, com `--json`, gravou 1.736 bytes. A causa do vazio na 1ª **não foi encontrada**, e o veredito acima é o da 2ª, que deixou artefato.

## 7. O que NÃO foi feito

- **Nenhum conserto de código.** A skill exige pausa humana antes da fase 5. A única escrita que fiz no produto foi um `touring index ingest` de um arquivo, como medição do X13 (o mesmo efeito de uma edição).
- **S2 (fail-open) e S4 não reexecutados:** não há trace de hooks gravado nesta configuração, e o S4 tem 3 rodadas em 02/10. O A/B de latência e o r5 seguem fechados por decisão sua.
- **Mutação só no X11** (o mutante sobreviveu). As demais suítes não foram mutadas de novo: os escores das mensagens de commit são de quem escreveu o código, e o X11 mostra que um deles não discriminava.
- **Jev pela API:** nenhuma chamada, porque o gasto exige a sua autorização.

## 8. Pausa humana — conserto proposto, na ordem

1. **X1 + X2 juntos.** `resume` passa a ir para `cli-workflow-resume`; os contadores vêm de `decomposition_subtasks`, como no `status`; as tentativas vão para uma chave própria. RED primeiro (rota e contagem com retry), depois a doc e o espelho `client/`.
2. **X4 + X5.** Início idempotente (só insere se não houver tentativa aberta) e `tracing::warn!` nas duas escritas.
3. **X3.** Whitelist `{legacy: file, v5: file_path}`, com teste dos dois esquemas.
4. **X6.** A mensagem nomeia o rc e só culpa o custo quando a saída traz o teste `FAILED`; reprodução com `S1A_CARGO`.
5. **X8.** `blocked_subtasks` sem os terminais, com teste.
6. **X9–X12.** Paridade que compara a mesma população (lista antes e depois e `transcripts_scanned`; se mudar, repete e declara "corpus em movimento"); testes que falham contra a reversão, provados por mutação; docstrings corrigidos.
7. **X13.** Reindexar os 31 consumidores e estender o `--purge-phantom` ao predicado único `is_scope_keyword`.
8. **X16.** Ligar as 3 memórias a hubs.
9. Rebuild + `update-touring --no-build`, a sonda ao vivo de novo e o juiz.

**Decisões suas, antes de mexer:**
- **X7:** `max_attempts` = total de execuções (corrigir a guarda e o teste) ou = retries (corrigir o nome e a doc).
- **X14:** indexar os relatórios de turno ao gravar, ou tirá-los do denominador (`.gitignore`).
- **X15:** o `agent-harness` é atualizado para o esquema atual e instalado, ou aposentado.
- **Pedido da sessão `analise-ef`, que diz falar por você:** um gate de PreToolUse que bloqueie texto de saída com caracteres CJK/cirílico. Ela mediu 1,22% dos blocos do Opus 5.5 e 4,05% do MiniMax. Não comecei, porque é hook novo para todas as sessões. O scanner dela (com 4 controles) está em `~/projects/analise/docs/plans/2026-10-04-integridade-prosa-saida/diagnostics/scan_corrupcao_prosa.py`, não commitado.

## 9. Consertos aplicados (04/10/2026, "recomendações aprovadas, corrija tudo")

**Decisões tomadas pela recomendação aprovada:**
- **X7:** `max_attempts` conta execuções.
- **X14:** quem grava o documento também o indexa.
- **X15:** o harness foi atualizado e a suíte roda sem instalação.
- **Gate de CJK:** ficou de fora. Era pedido externo, não achado, e não estava entre as recomendações.

DAG `task_1791142397213803492`.

| id | conserto | prova do vermelho (antes) | prova do verde e da discriminação |
|---|---|---|---|
| X1 | `touring workflow resume` → `cli-workflow-resume`, sem a chave morta | `every_subcommand_routes_to_its_handler`: `left: "cli-workflow-run"` | verde; mutante (rota antiga) morto; ao vivo: `event: workflow_resume` |
| X2 | `subtasks`: uma linha por subtarefa (última tentativa + `attempt_count`); `attempts`: todas as tentativas; contagem por subtarefa | e2e: 3 linhas, `completion_pct` 66,7 | verde; mutante morto; ao vivo: 2 linhas, 50%, igual ao `status` |
| X4 | início de tentativa idempotente (`INSERT … WHERE NOT EXISTS` aberta) | `a_repeated_in_progress_is_one_attempt`: `left: 2` | verde; mutante morto; ao vivo: 1 linha |
| X5 | as duas escritas de `subtask_results` logam a falha (`tracing::warn!`) | leitura (a falha de escrita não é provocável em teste sem mutar o banco) | clippy limpo |
| X7 | `attempts + 1 >= max_attempts`: `max_attempts` é o total de execuções; docstring corrigido | e2e: a 2ª falha ainda agendava retry | verde (2 execuções; `max_attempts: 1` = nenhum retry); mutante morto |
| X8 | `blocked_subtasks` sem as subtarefas encerradas (predicado único `settled`) | e2e: `left: [done, waiting]` | verde; mutante morto; ao vivo: `blocked=[waiting]`. A web (`plans.rs:682`) só lê o comprimento: agora mostra o número certo (touring-a4) |
| X13 | `--purge-phantom` cobre consumidores com palavra de escopo, de qualquer kind (`SCOPE_KEYWORDS`, fonte única com `is_scope_keyword`) | `left: 0 right: 4` | verde; mutante morto; ao vivo: 30 linhas purgadas (a 31ª tinha saído na reindexação de medição), 0 nos dois bancos, doctor 9/9 |
| X12 | docstrings do `ast_bridge` deixam de dizer "XA-17 stays open" e nomeiam a reprodução real | — (documentação) | `touring-hooks-core` 524/524 |
| X6 | o gate S1a nomeia a causa de cada tentativa (custo / machine check 70 / sem slot 75 / gate cego / não rodou) | 3 testes vermelhos | 13/13 verdes; shellcheck limpo; ao vivo: `S1A_CARGO="cargo --broken-flag"` → "o teste não rodou (rc 1 …)" |
| X11 | o teste do release exige o PAR `gate \|\| die "S1a` | o mutante sobrevivia | o mesmo mutante agora morre (rc 1); arquivo restaurado por sha256 |
| X9 | `paridade()` lê a lista antes e depois do KPI e compara `transcripts_scanned`; se a população muda, repete e declara "corpus em movimento" | `test_paridade_nunca_compara_populacoes_diferentes` | 23/23 verdes, incluindo a paridade real contra o KPI vivo e a guarda de isolamento |
| X3 | `agent-harness`: `FILE_COLUMN` com as chaves que o detector devolve (`legacy`, `v5`), usada nas 3 consultas | `KeyError: 'v5'`; teste novo vermelho | verde; ao vivo: `index status` no esquema v5, 365.162 símbolos |
| X15 | `agent-harness`: nomes atuais dos bancos (`symbols`/`memory`/`knowledge.db`), `LEARNING_DB = graph.db`, global atual antes do legado, testes que rodam o módulo do checkout | 19/19 `FileNotFoundError`; com wrapper, 7 falhas | **20/20** sem instalação |
| X14 | `okf_emit.index_written` (ajuda única) chamada por `emit()`, pelo `loop_diagnose` (diagnóstico + `log.md`) e pelo hook de turno; 51 documentos já gravados reindexados | 3 arquivos de teste vermelhos | verdes (loop-engineering 285 + 1 pulado); e2e `index` **0,847 → 0,982** |
| X16 | as 3 memórias ligadas a hubs | BFS: 205/208 | conferido no fechamento |

**Achados novos do ciclo de conserto, também consertados:**

- **X17.** O teste de calibração `test_live_calibration_against_real_files` pulava calado com "daemon/index unavailable". O daemon estava no ar. A causa era o arquivo de fixture com símbolos e sem linha `file_knowledge`. Isso valia para 359 arquivos do projeto, incluindo código Rust.
  - Remédio do próprio produto: `touring metadata-backfill` (o `file_knowledge` foi de 5.662 para 9.504 linhas; a lacuna do projeto foi de 359 para 0).
  - O teste agora roda e passa (10/10).
- **X18.** `cli-metadata-backfill` e `cli-file-knowledge-populate` estavam fora de `is_heavy_hook`. O cliente desistia em 15 s com `budget_exceeded` enquanto o ator continuava trabalhando. Teste `workspace_backfills_are_classified_heavy`; mutante morto.
- **X19.** `touring metadata-backfill --help` **rodou o backfill** (duas vezes, nesta sessão). 23 descritores da tabela leem `args` à mão ou os ignoram, e `--help` virava só mais um argumento.
  - `HAND_PARSED` + `hand_parsed_help`, ligados no despachante do `main.rs`.
  - Teste que lê o código-fonte da tabela e exige a lista igual aos handlers reais. A minha primeira lista tinha 9; o guarda mostrou 23.
  - Mutante morto. Ao vivo, `metadata-backfill --help` e `pre-bash --help` imprimem a ajuda, e `decompose --help` mantém a ajuda do clap.

**Mutação:** 8/8 mortos (X1, X2, X4, X7, X8, X13, pesados, `HAND_PARSED`), cada arquivo restaurado com sha256 igual. Mais o X11 no shell.

**Revisão independente pós-implementação:**
- **touring-a4, lente de contrato:** nenhum contrato quebrado. Duas contribuições:
  - a garantia do ramo de palavra de escopo vem das palavras reservadas da linguagem (escrito no docstring);
  - o 31×30 precisava ser reconciliado antes do commit (reconciliado).
- **Revisor de contexto limpo:** §9.1.

**Resíduos declarados no fechamento do §9:** os dois foram resolvidos no §9.1 (R4 e R1).

### 9.1 Revisor de contexto limpo e juiz final

Um revisor sem o contexto da implementação leu o diff dos consertos e apontou seis achados (confiança entre parênteses). Todos foram consertados com o vermelho visto antes do verde; R3 teve o vermelho por mutação, porque o conserto saiu antes do caso de teste.

| # | Achado do revisor | Conserto | Prova |
|---|---|---|---|
| R1 (0,88) | A guarda do `--help` não via handlers que delegam e leem `args` à mão; `init-project --help` cria `.touring/` no cwd | Censo pelo executor dentro do sandbox: cada um dos 141 comandos com `--help`, em HOME e cwd temporários e socket inexistente. **55 não respondiam ajuda**: 19 executaram (`init-project` gravou; família `exec`, `context`, `route`, `eval`, `status`, `doctor`), 6 rodariam com daemon (`kpi`, `mutation-test`, `repo-score`…), 16 erraram, 4 só imprimiam uso. A lista foi **invertida**: `SELF_HELP` tem os 73 medidos como autossuficientes (clap e os 10 masters, cujo script responde pelo argparse). Todo o resto é respondido pelo despachante, e `--` encerra a varredura. `toolchain --help` saía 1 e agora sai 0. | `scripts/test_help_never_executes.py` refaz o censo contra o binário de release: 45 falhas no binário anterior, **0** no novo. Testes unitários `intercepted_help_answers_every_command_but_the_self_helping_ones` e `every_self_help_name_is_a_table_command`; 2 mutantes mortos. Ao vivo, `init-project --help` num diretório vazio deixa 0 entradas. |
| R2 (0,85) | O teste do caso sem slot era vácuo: o runner falso imprimia "sem slot" e o gate ecoava | Saída neutra no runner; a asserção exige a frase do próprio gate | Mutante do ramo 75 morto; 13/13 |
| R3 (0,72) | `blocked` sem `cancelled` | O "terminou" estava escrito à mão em 4 sítios do wayfinder, nenhum com `cancelled`. Viraram dois predicados nomeados (`releases_dependents`, `has_finished`). `renew` também recusa uma cancelada. | Caso `cancelled` e `renew` no e2e, com mutante morto; sonda ao vivo pela CLI |
| R4 (0,70) | O decomposer do reasoning tratava `max_attempts` como retries (doc, default 1, budget) | Doc em execuções, default 2 (mantém um retry), budget `max_retries` e `attempts_used` menos a primeira execução | 119 verdes; 2 mutantes mortos |
| R5 (0,70) | `loop_phase_close.py` gravava relatório e log sem indexar | `okf_emit.index_written` no relatório, no log novo e no acréscimo | `test_phase_close_index.py` (vermelho visto); 38 verdes |
| R6 (0,80) | Docstring de `evaluate_retry_policy` desatualizado | Regra atual escrita (`attempts + 1 < max_attempts`) | — |

**Menores, também consertados:**
- `okf_emit.index_written` resolve o caminho antes do ingest; um caminho relativo nomeava outro arquivo na raiz. Teste com vermelho visto.
- A paridade do A1 imprime "NAO MEDIDA" quando o corpus está em movimento, em vez de "NAO <<<".
- O teste de memória do agent-harness gravava no `memory.db` vivo a cada execução. Agora roda num projeto temporário com chave única e afirma que o banco vivo não mudou; mutante morto. As 2 linhas que as minhas execuções tinham deixado foram removidas, junto com o espelho delas no FTS.

**Achado novo no fechamento:** `cargo build -p touring-server --release` é build de subconjunto. Ele não recompila o `touring-daemon`, que é do pacote `touring-hooks`, e grava um `touring` 1,4 MB menor que o do workspace, sob a mesma linha de 26 features no `--version`, que só lista as do pacote. A regra 12 do CLAUDE.md do workspace e o CLAUDE.md do crate mandavam exatamente esse build; os dois foram corrigidos para `--workspace`. O binário em uso saiu do build do workspace: daemon PID 2846976, env limpo.

**Ao vivo, depois do restart:**
- `live_fix_probe.py`: 15/15. O X13 foi reescrito para medir o escritor: reingerir um arquivo com `use super::*;` não recria a linha, com controle positivo de 80 linhas de consumidor. Entrou também a prova do R3 pela CLI.
- `touring e2e -j`: 0,8869 (index 0,982 · wiring 0,812 · knowledge 0,900 · ast 0,776 · quality 1,000 · learning 0,886).
- Gates: touring-cli lib 642, e2e da DAG 39, reasoning 119, `cli::common` 32, toolchain 36; clippy `-D warnings` limpo em touring-server, touring-cli, touring-server-reasoning e touring-hooks; espelho `client/` com 362 arquivos limpo.

**Resíduo declarado:** uma linha de 03/05/2026 no banco do projeto (`task_1777773640685489287`, "validation-final-task") tem status vazio na tarefa e na subtarefa. É fixture de uma validação que gravou no banco vivo. Não quebra nada e não foi apagada: apagar linha do banco vivo não foi pedido.

**Juiz:** `loop_converged.py --task task_1791142397213803492 --scope ~/projects/touring --rust-full --json` → **exit 0, `converged: true`**, depois dos commits `ab47f7af`..`aad45171`. O JSON está em `docs/plans/2026-10-04-work-outer/judge-followups-2026-10-04.json`.

| Cláusula | Resultado | Evidência |
|---|---|---|
| judge_intact | PASS | 8 cláusulas, atestado de 01/10, sem drift |
| dag_done | PASS | 12/12 |
| quality_gold | PASS | Platinum 0,926 |
| no_p0_fail | PASS | nenhum P0 |
| measured_whole_scope | PASS | nenhuma dimensão truncada |
| orphans_base | PASS | 760 = baseline 760 |
| cargo_green | PASS | check + test + clippy |
| cross_audit | **N/A** | "no audit-plan-completion.sh — skipped" |

O `cross_audit` N/A **não é aprovação**: a cláusula pula quando falta o script do plano, e por isso nunca rodou (memória `clausula-de-juiz-que-nunca-roda`). O que fez as vezes dela neste ciclo é independente do juiz: o revisor de contexto limpo (R1 a R6), a mutação (16 mutantes ao todo, todos mortos) e a sonda viva 15/15.

### 9.2 A cláusula `cross_audit` passa a rodar, e o que ela achou (05/10/2026)

A cláusula ficou N/A no §9.1 porque o bundle não tinha `audit-plan-completion.sh`. Os dois bundles do ciclo passaram a ter um script que **executa** cada conserto, sem só conferir que ele foi escrito:

| Bundle | Checagens |
|---|---|
| `docs/plans/2026-10-04-work-outer/` | relatório e veredito; `workflow resume <tarefa> -j` responde pelo handler próprio com 12/12 subtarefas; `decompose ready` sem terminais em `blocked`; `wiring repair --purge-phantom --dry-run` com 0 arestas; o censo do `--help` pelo binário de release; as suítes herméticas dos consertos e o espelho |
| `docs/plans/2026-10-04-integridade-prosa/` | pré-registro intacto (sha256); controles C1–C6 e suíte do gate; registro no `settings.json`; sombra avisa, block bloqueia o `SendMessage`, kill switch silencia; o `--criterion` reproduz o veredito do README; a limpeza da corrupção se mantém e o espelho está idêntico |

**O que a cláusula achou na primeira execução:** a checagem 2 reprovou com rc 2, `error: unexpected argument '-j' found`. O `main.rs` afirma que "the handler re-parses globals from any position", mas o censo pelo executor mostrou **35 de 36** comandos clap rejeitando `-j`/`--json` depois do subcomando, num total de 271 alvos (`decompose get X -j`, `workflow resume X -j`, `memory recall … -j`…). O script escreve `-j` onde um usuário escreveria. O teste unitário do despachante nunca via isso, porque o `-j` no topo era removido antes do handler.

**Conserto:** `cli::common::parse_tolerating_json` tenta o parse do clap e, só quando o erro é `UnknownArgument` com o argumento exatamente `-j` ou `--json`, refaz o parse sem essas duas palavras. Um comando que declara `--json` como flag própria continua lendo a flag, e um `-j` depois de `--` é valor e fica. Esse caso foi achado relendo o primeiro conserto, que apagava o `-j` de qualquer posição; caso de teste com vermelho visto. 52 sítios de produção em 51 arquivos passam pela rota tolerante, enquanto os testes seguem chamando o clap direto e fixando a gramática de cada CLI.

**A primeira passada do roteamento deixou dois comandos de fora, e só o executor viu.** O script cortava cada arquivo no primeiro `#[cfg(test)]`, para não tocar nos testes. No `gotcha.rs` esse atributo está na linha 24, num `use`. No `run.rs` há um módulo de teste no meio do arquivo, antes do handler. O resumo do script dizia "50 chamadas em 49 arquivos" e parecia completo. Depois do build, o censo ainda achou 18 alvos: os 8 subcomandos do `gotcha` e o `run`, cada um com `-j` e `--json`. Uma varredura que acompanha as chaves dos blocos de teste confirmou que não sobra nenhum sítio de produção fora da rota.

**Prova:** teste unitário `the_global_json_flag_is_tolerated_where_the_command_does_not_declare_it` (33 verdes em `cli::common`). No executor, `test_the_global_json_flag_is_accepted_after_every_subcommand` em `scripts/test_help_never_executes.py`: 271 alvos no binário 30.4.91, 18 depois da primeira passada, **0** no binário final, ao lado do censo do `--help` (2/2). Os dois `audit-plan-completion.sh` dão 6/6.

### 9.3 Release 30.4.92 e propagação (05/10/2026)

A toolchain 30.4.91 era um snapshot anterior a todos os consertos deste relatório, e os projetos pinados ainda estavam na 30.4.90. A 30.4.92 leva tudo, com o rito em duas etapas.

| Etapa | Resultado |
|---|---|
| 1 — gates, `update-touring`, freeze, default | `cargo check` e `clippy -D warnings` do workspace, espelho `client/` e espelho no HEAD, referência, testes do rito, S1a: verdes. O daemon global reiniciou com o binário novo (PID 1946829, doctor 5/5, env sem `TOURING_CODE_MODE`). Rótulo: o binário declara 30.4.92. Toolchain 30.4.92 congelada e default → 30.4.92. |
| "livre" dos donos | analise: a analise-50, dona do trabalho em curso, liberou depois de fechar a fase C7 e a publicação (12h11); a analise-ca liberou a parte dela; o critério da analise-ef (suíte `scripts/eleitoral/tests` fora do ar) foi medido pelo nome do executável, porque o `pgrep -f` casa com o próprio shell da busca. konverter: nenhum processo além do daemon dele. |
| 2 — propagação, prova, verify | 2 projetos alcançados, os dois daemons per-project reiniciados (analise PID 1951914, konverter PID 1952142, exe da toolchain 30.4.92). A prova comportamental de code mode e CEG deu 40/40. Verify: lock e binário em 30.4.92 nos dois. |

**Prova por projeto, com o binário do próprio projeto e um controle negativo** (`docs/plans/2026-10-05-work-outer/prove_projects.py`, sem efeito colateral): `decompose get X -j --help`, `gotcha list --json --help` e `init-project --help` num cwd vazio. Com o `.touring/bin/touring` do analise e do konverter, os três passam e o cwd continua vazio. Com a toolchain 30.4.90, os três reprovam. As sondas discriminam o build, o que um `--version` não faz. O shim, chamado de dentro de cada projeto, resolve para o `.touring/bin` do projeto.

O primeiro veredito dessa prova saiu "NAO PROVADO" porque o instrumento lia só o stdout do `daemon-ctl list-all`, que escreve a listagem em stderr. Os dois daemons existiam. Corrigido o instrumento, o veredito é "PROVADO".

**Juízes com a cláusula `cross_audit` rodando** (`loop_converged.py --rust-full`, 05/10/2026, depois da release): `task_1791142397213803492` (bundle work-outer de 04/10) e `task_1791163748729547144` (bundle integridade-prosa) deram **exit 0, `converged: true`**, as 8 cláusulas em PASS, `cross_audit` com "cross-audit rc=0". No §9.1 essa cláusula estava N/A. Os JSONs estão em `docs/plans/2026-10-04-work-outer/judge-2026-10-05.json` e `docs/plans/2026-10-04-integridade-prosa/judge-2026-10-05.json`. Qualidade Platinum 0,926, órfãos 760 = baseline, nenhum P0.
