---
type: Audit
title: "Fechamento do DAG (03/10/2026): o que o `decompose` mentia e como ficou"
description: "Achado extra do Z3 (não estava na lista): o RPC `cli-workflow-stats`/`-status` lê `subtask_results` (`decompose_workflow.rs:104,393`), e nada em produção escrevia nessa tabela — consumidor sem produtor responde `null` e parece saudável. A lição está na memória `copia-morta-carregava-o-que-a-viva-perdeu`."
timestamp: 2026-10-04T00:38:36-03:00
---

# Fechamento do DAG (03/10/2026): o que o `decompose` mentia e como ficou

> Origem: o Gabriel aprovou ("recomendações aprovadas — corrija e ative tudo") os itens que
> restavam do ciclo r3: deduplicar a lista terminal de `lifecycle.rs`, os três defeitos achados e
> não consertados (cópia morta de `cli_decompose_update`, `INSERT OR REPLACE` do `add`,
> `decompose archive` só manual) e religar o emissor do modo sombra r3. Propagação para os projetos
> pinados e r5 seguem fechados (a própria lista dele os dá como fechados).
> DAG deste ciclo: `task_1791081574812268810` (Z1–Z9).

## 1. O que estava errado, e a prova de que estava

| # | Defeito | Causa | Prova de que existia (RED, antes do conserto) |
|---|---|---|---|
| Z2 | Terceira escrita do vocabulário terminal | `refresh_parent_status` mantinha `TERMINAL: [&str; 3]` própria, além da fundação (`is_terminal_task_status`) | Comportamento idêntico hoje; o guard percorre `TERMINAL_TASK_STATUSES` da fundação e a mutação "só entende `done`" o mata |
| Z3 | "Cópia morta" do `update` era a única que fazia 3 coisas | Fork de `cli/decompose.rs` perdeu `subtask_results` (Feature C), o laço de retry (S1.3) e `subtask_missing` | `a_subtask_that_runs_leaves_the_timing…`, `a_failed_subtask_with_a_retry_policy…`, falhando antes; `cli-workflow-stats` devolvia `avg_duration_ms: null` para sempre |
| Z4 | `INSERT OR REPLACE` do `add` | REPLACE apaga a linha e escreve outra: filha concluída voltava a `pending`, e `attempts`, `quality_score`, `retry_policy`, a reivindicação (`claimed_by`/`claim_expires_at`) e `created_at` voltavam ao default | `re_adding_a_closed_subtask_keeps_its_work`, `re_adding_keeps_what_the_row_accumulated`, falhando antes |
| Z5 | `decompose archive` só rodava na mão, e confiava no status do pai | Rotina sem chamador; predicado olhava só `status` do pai | `archive_leaves_a_closed_plan_with_open_children_alone` e `the_automatic_pass_archives_only_plans_closed_for_a_week`, falhando antes |

Achado extra do Z3 (não estava na lista): o RPC `cli-workflow-stats`/`-status` lê `subtask_results`
(`decompose_workflow.rs:104,393`), e **nada em produção escrevia nessa tabela** — consumidor sem
produtor responde `null` e parece saudável. A lição está na memória
`copia-morta-carregava-o-que-a-viva-perdeu`.

## 2. O que mudou

- **Dedup** (`lifecycle.rs`): `refresh_parent_status` usa `is_terminal_task_status`; some a cópia.
  O docstring de `reopen_parent_if_closed` deixa de registrar o "KNOWN GAP".
- **Update fundido** (`cli/decompose.rs` + `handlers/decompose.rs`): o `update` vivo grava a linha
  de execução (`record_subtask_transition`: início em `in_progress`, fim em `completed`/`failed`),
  decide o retry **antes** de derivar o pai e devolve `retry_scheduled` e `subtask_missing`.
  A cópia morta e `update_subtask_fields` (seu único chamador) foram removidas.
- **`add` como upsert** (`upsert_subtask`): `ON CONFLICT(subtask_id) DO UPDATE` nomeia só as colunas
  de definição (descrição, arestas, prioridade, prazo, grupo paralelo, revisão). O que a linha ganha
  por viver — status, `attempts`, `quality_score`, `retry_policy`, reivindicação, `created_at` — fica.
  Um id que já pertence a **outra** task é recusado (`persisted: false`, o erro nomeia o dono); a
  posse é o `WHERE` do próprio conflito, então recusa e escrita são **uma instrução atômica** (zero
  linhas alteradas = recusa; a primeira versão lia o dono antes e escrevia depois, com janela). A
  resposta traz o status real e `existed`, e a chave `error` **só existe na falha**. O pai só é
  re-derivado quando a linha resultante não é terminal.
- **Retenção** (`archivable_plans_where`): o plano é arquivável se seu status é terminal, sem carimbo,
  antigo o bastante **e nenhuma subtarefa fora de `completed`/`done`/`finalized`** — o vocabulário de
  PLANO, o mesmo que a derivação usa para fechar (a primeira versão usava o vocabulário de subtarefa,
  mais largo, e carimbava um pai fechado à mão sobre filha `failed`). Vale para `decompose archive` e
  para o passe automático. O carimbo é **levantado pela própria derivação** (`refresh_parent_status`)
  sempre que ela deixa o plano aberto, tanto no `add` quanto no `update`.
- **Passe automático** (`auto_archive_pass`, janela `AUTO_ARCHIVE_AFTER_SECS` = 7 dias): chamado pelo
  hook `task-completed` (`hook_registry.rs`), no store do projeto **e** no global
  (`open_global_store`, agora o único lugar que escreve o caminho). Idempotente; um erro de escrita é
  logado, nunca vira "0 arquivados". Nos testes unitários do `touring-dispatch` só o store do projeto
  é varrido (`#[cfg(test)]`), porque eles despacham contra o `$HOME` real; o braço global tem e2e
  próprio com HOME privado.
- **Segundo gatilho** (`cli_decompose_create`): criar um plano também roda o passe, só no store do
  projeto. Veio de um dado: o `task-completed` real dispara poucas vezes por dia (censo do
  `hook_trace`: 2 eventos em 03/10 e 0 depois do restart), raro demais para a rotina contar como
  automática. Iniciar um plano é o começo do trabalho; um `UPDATE` condicional por plano criado.

## 3. Decisões de desenho (para o Gabriel revisar)

1. **7 dias**: tempo para a sessão que fechou o plano (e a seguinte) ainda o ver nas vistas vivas.
   Constante única; mudar é editar uma linha.
2. **`created_at` é preservado** no re-add. O touring-a4 sugeriu atualizá-lo ("re-adicionar é
   recomeçar"), mas o `add` agora preserva status/`attempts`, e recomeçar a idade com o trabalho
   mantido seria incoerente. Quem quer refazer o passo usa `update --status pending`.
3. **Sem timer/unit**: os gatilhos são hooks que já existem (`task-completed` e `decompose create`);
   o cwd do daemon já resolve o store certo, e um timer teria de descobrir qual store varrer.

## 4. Medido no DAG vivo antes do passe (censo somente leitura)

| store | planos | fechados sem carimbo | …com mais de 7 d | …desses com filha aberta | o passe guardado carimbaria |
|---|---|---|---|---|---|
| projeto | 406 | 18 | 12 | 0 | 12 |
| global | 10 | 3 | 3 | 1 | 2 |

O plano global com filha aberta é o caso concreto que a guarda poupa de um carimbo permanente.

## 5. Provas

**Testes (RED visto antes, GREEN depois).** `dag_lifecycle_closure_e2e` foi de 20 passando + 7
falhando (a medida RED, antes de qualquer conserto) para 34/34 (31 + 3 dos achados da revisão,
cada um visto falhar antes); `touring-cli --lib` 641; `touring-dispatch --lib` 1342;
`touring-hooks` inteiro 68 alvos / 1163 testes / 0 falhas; `touring-hook-runtime --lib` 487;
`touring-hooks-core --lib` 524; `clippy -D warnings` limpo nos cinco crates (o único aviso é o
future-incompat do `proc-macro-error2`, dependência).

**Mutação (`mut_z.py` + `mut_z2.py`, in-place sob `hw-slot`, sha256 conferido ao restaurar): 12/12
na primeira rodada e 17/17 na final.** `subtask_missing` sempre `false` · passe ignora o store global ·
janela de retenção zero · sem a guarda de filha aberta (rota manual e automática) · posse atômica do
`add` removida · `add` voltando a zerar status · `add` voltando a zerar `attempts`/`quality_score` ·
`update` sem a linha de execução · `update` sem retry · derivação que só entende `done` · hook
`task-completed` sem o passe · `add` emitindo `error` em sucesso · derivação sem levantar o carimbo ·
guarda de volta ao vocabulário de subtarefa · predicado de palavra de escopo sem `super` · criar
plano sem o passe. Cada alvo morreu pelo teste certo (a asserção `... FAILED` do próprio teste, não
erro de compilação).

## 5b. Revisão independente (04/10/2026) — o que ela achou e o que fiz

Duas revisões somente leitura do range `4c49ab1b..77fda791`: a do touring-a4 (sessão paralela) e a de
um revisor de contexto limpo, sem Bash e sem acesso ao meu raciocínio. Eu já tinha dado o ciclo por
fechado antes delas; elas acharam coisas que meus testes não alcançavam.

| Achado | Quem | Veredito | O que fiz |
|---|---|---|---|
| `add` bem-sucedido devolvia `"error":null` e o bridge trata a presença da chave como falha: 3 avisos falsos por tarefa espelhada | revisor independente | **procede, regressão minha**; confirmado ao vivo | chave só na falha; teste `a_successful_add_carries_no_error_key`; mutante morto |
| Reabrir por `update --status pending` (o caminho que este relatório indica) deixava o plano `in_progress` e arquivado | revisor independente | procede | o carimbo é levantado em `refresh_parent_status`; teste e mutante |
| A guarda usava o vocabulário de subtarefa; pai fechado à mão sobre filha `failed` recebia carimbo quase permanente | touring-a4 | procede | guarda no vocabulário de plano; teste `archive_leaves_a_plan_with_a_failed_child_alone`; mutante |
| Posse do `add` era leitura-depois-escrita (TOCTOU) | touring-a4 | procede, exposição baixa (handles distintos) | posse no `WHERE` do conflito, uma instrução atômica; mutante |
| Cinco listas de palavra de escopo escritas à mão | touring-a4 | procede para 3 sítios; **2 são intencionais** (sem `crate`: `crate::x` tem resolução própria) | `is_scope_keyword` único nos 3; mutante; não criei o teste estrutural que varre o código por listas (frágil em arquivos cheios de prosa) |
| `scoped_id` com prefixo de outra task grava a linha na task errada; `update` escreve sem `task_id` no predicado | touring-a4 | endurecimento, **sem falha observada** | **não mudei**: o predicado do `update` tem ~40 chamadores e não vejo caso real; fica nos limites |
| `use crate;` sozinho perde uma aresta | touring-a4 | procede, forma rara | não reabri |
| `record_subtask_transition` ignora `done` (o status do `loop_phase_close`); re-adicionar filha `skipped` reabre plano fechado à mão | revisor independente | baixa confiança, paridade com a cópia antiga | não mudei; fica nos limites |
| Comparação de `updated_at` mistura `T` e espaço; `Local::now()` em `record_task_metrics` | os dois | sem defeito hoje: todo escritor de `task_decompositions.updated_at` grava UTC RFC 3339 | nota latente, sem mudança |

**O defeito que apareceu no caminho (achado H2, não estava na lista).** Ao editar
`touring-dispatch/src/lifecycle/tests.rs`, `touring doctor` foi a 8/9 com `kind_unknown=1`: a
passada de caminhos de módulo (`record_module_path_consumers`) gravava a **última palavra** do
caminho como nome do módulo, então `use super::*;` e o prefixo `super::super` de
`super::super::cli_handlers::run()` viravam consumo de um símbolo chamado `super`, sem produtor.
O rebuild apaga o fantasma no fim da varredura; o caminho de edição, que roda a mesma passada, não.
RED reproduzido com a linha exata do banco vivo; conserto = a regra que `record_import_consumers` já
aplicava aos `use` (palavra de escopo não é módulo). Depois do restart e de re-derivar o arquivo
pelo caminho de edição: `kind_unknown=0`.

**Build e restart.** `cargo build --release --workspace` verde (5m17s no primeiro). Foram **cinco**
restarts do daemon global (`update-touring --no-build`), não o "um único" planejado: o 2º por causa
do conserto de wiring acima, o 3º por causa do gatilho de criação de plano, o 4º por causa dos
achados da revisão independente (seção 5b), o 5º por causa do `reply_failed` (commit `77c09fd9`): o
touring-a4 achou, depois de tudo isto, o MESMO idioma frágil (`contains("\"error\"")`) nos dois únicos
sítios de produção do bridge (`:135` e `:173`, varredura com controle positivo); o helper lê o valor
do JSON em vez da presença da palavra. Binário final: PID 263464; juiz `--rust-full` exit 0 nele.
Ambiente do daemon do 4º restart (PID 40670) conferido em
`/proc/<pid>/environ`: sem `TOURING_CODE_MODE` nem `TOURING_R3_SHADOW`. `touring doctor` 9/9 e
`kind_unknown=0`; um `add` bem-sucedido ao vivo já não traz a chave `error`.

**Emissor r3 religado (`[shadow] r3 = true`, restart).** Prova ao vivo, não suposta: o journal
`r3_events.jsonl` ficou parado em 431 linhas desde 03/10 21:12 e cresceu para 432 e depois 439 só
com a flag ligada e o daemon novo; cada linha nova é `a1_block` do `pre-bash` com cabeçalho ≤ 80
caracteres, hash de sessão e sha — sem texto livre (S6). Gate S1a (`scripts/r3-s1a-gate`):
`1 passed`, exit 0. A flag não decide nada: o emissor é observe-only e fail-open.

**Ao vivo no DAG com o binário novo.**
- `update` de uma subtarefa: `subtask_missing:false`; de um id inexistente: `subtask_updated:false,
  subtask_missing:true`.
- `add` repetido sobre `Z1` já concluída: `persisted:true, existed:true, status:"completed"`.
- `touring workflow stats` deste DAG: `avg_duration_ms: 172381` (Z7 em `in_progress` 00:20:11 →
  `completed` 00:23:03). Antes do conserto esse campo era `null` para sempre.
- **Retenção, com evento real.** Censo somente leitura antes: projeto 12 planos fechados e antigos
  (0 com filha aberta), global 3 (1 com filha aberta). Às 00:28:36 BRT, num par `task-created` /
  `task-completed` genuíno de outra sessão, o daemon (binário #2, o braço de release do
  `task-completed`) carimbou **12 do projeto e 2 do global no mesmo segundo e poupou o global com
  filha aberta**. O censo seguinte confirma: projeto 7 fechados sem carimbo e 0 antigos; global 1
  (o poupado). Exatamente o previsto antes do passe.
- O gatilho de criação (3º restart) tem teste, RED e mutante morto, mas **não tem
  evento ao vivo próprio**: o censo já estava zerado (o `task-completed` real o esvaziou às 00:28:36,
  antes do 3º restart), então a sonda de criação (plano finalizado em seguida) não tinha o que varrer.
  A prova dele é o teste `creating_a_plan_retires_the_closed_ones_nobody_finalized`.

## 6. Limites declarados

- `INSERT OR REPLACE` continua em `cli_decompose_create`, na importação de tasksfile e em dois
  helpers de **teste** dos suggesters: criam linhas novas; nenhum é caminho de re-add de produção.
- O retry só é alcançável quando o payload do `update` traz `retry_policy`: nenhum caminho grava uma
  política válida na coluna (o importador grava `"{}"`, que não desserializa). Portado por paridade
  com o verbo original, sem mudar o padrão de ninguém.
- ~~`cache_hit_rate` de `touring workflow stats` não é uma medida~~ — **substituído por
  `attempt_completion_rate` em 04/10 (§7)**. A leitura acima também estava incompleta: o denominador
  contava as tentativas ainda abertas, então a taxa caía conforme o trabalho começava.
- A guarda de filha aberta usa o vocabulário de **plano**: um plano fechado à mão sobre filha
  `failed`, `skipped` ou `cancelled` **nunca é arquivado** pela retenção. A regra ficou (§7); o que
  mudou é que `decompose archive` passou a nomear esses planos (`held`) e a rota de saída, o
  `finalize`.
- O `update` escreve por `WHERE subtask_id = ?` sem `task_id` no predicado, e o `add` aceita um
  `subtask_id` com `::` de outra task quando a linha ainda não existe (a linha fica na task errada).
  Sem falha observada; endurecer toca ~40 chamadores. Apontado pelo touring-a4.
- `record_subtask_transition` não registra o fim de uma subtarefa fechada com `done` (o status do
  `loop_phase_close.py`): a linha de execução aberta no `in_progress` fica sem fim. Paridade com a
  cópia antiga.
- `use crate;` sozinho (a raiz do crate) perde uma aresta com o guard de palavra de escopo; forma rara.
- A comparação `updated_at < corte` é lexicográfica: segura enquanto todo escritor grava UTC
  RFC 3339 (hoje todos gravam). Se alguém escrever com `Local::now()`, a janela desloca pelo offset
  sem erro nenhum.

## 7. Decisões do canvas de 04/10/2026, implementadas

O Gabriel pediu o canvas das quatro decisões que este relatório deixou abertas; as recomendações do
§8 do canvas foram implementadas sob o goal dele ("prossiga com a implementação completa"), cada uma
num commit próprio para poder ser revertida sozinha. A investigação de `subtask_results` e duas
correções de desenho vieram do touring-a4 (somente leitura, `file:line` por afirmação).

| Item | Decisão | O que mudou |
|---|---|---|
| (a) janela de 7 dias e `created_at` no re-add | manter | nada: três leitores ordenam por `priority, created_at`, e o `updated_at` já registra a reescrita |
| (b) plano fechado à mão sobre filha aberta | manter a guarda e torná-la visível | `decompose archive` devolve `held` (contagem completa), `held_plans` (os 20 mais antigos, com `open_statuses`) e `held_remedy`; falha na consulta vira `held: null` + `held_error`, nunca 0. Os dois predicados (`archivable_plans_where`, `held_plans_where`) são montados das mesmas duas metades. O passe automático não mudou: não tem resposta onde carregar o relatório |
| (c) `cache_hit_rate` | renomear para o que mede | `attempt_completion_rate` = tentativas `completed` ÷ tentativas **terminadas** (`sr.completed_at IS NOT NULL`), em `stats`, `status` (`summary`) e `compare`. Uma linha de `subtask_results` é uma tentativa: o retry abre linha nova. Por linha, `slowest` e `resume` trazem `attempt_outcome` (`completed`/`failed`/`null`), lido de `completed_at` e nunca deduzido da duração. A coluna `cache_hit` fica (sem migração), com o invariante escrito no escritor: só `completed` e `failed` fecham linha |
| (d) índice de memória com 161 linhas | juntar entradas em hubs | 161 → 137 linhas (18.544 → 16.240 bytes): 4 arquivos-hub (retomadas, defeitos resolvidos, lições do r3, code mode) com 27 membros e o par estado/execução do r3 numa linha só. Nenhum arquivo de memória apagado; 0 ponteiro quebrado |

Por que o rename e não a remoção: medido, nenhuma das três respostas tinha outra taxa de desfecho, e
a remoção apagaria a única. E por que por tentativa e não por subtarefa: por subtarefa as respostas
já trazem `completed` e `failed`; o que a tabela acrescenta é o custo das tentativas.

Consumidores procurados antes de mudar o contrato: em todo o `~/projects`, só a doc da skill, este
relatório, documentos de sessão e as cópias nos worktrees `touring-wt-q50` e `touring-wt-wiring`
citam a chave junto do comando. A doc viva (`~/.claude/skills/Touring/references/touring-cli-tasks.md`)
foi atualizada e o espelho `client/` sincronizado.

Revisão adversarial do touring-a4 (somente leitura, depois de implementar), quatro ajustes aplicados:
o relatório `held` passou a ser lido DEPOIS do `UPDATE` no caminho real (antes, uma filha que fechasse
no meio da chamada podia sair carimbada e listada como retida); `slowest` lê `completed_at` em vez de
deduzir o desfecho da duração (um campo nulável que hoje só o fechador escreve é o que o `cache_hit`
era); o `held_remedy` diz que só filha `completed`/`done` deixa a derivação fechar o plano; e um teste
novo fixa `attempt_completion_rate = null` com uma tentativa em voo (o agregado antigo dava 0/1 = 0).

A prova ao vivo achou dois drifts antigos entre a doc e a CLI. `touring workflow status` estava
documentado e respondia "unrecognized subcommand": o handler `cli-workflow-status` existia e só um teste
o chamava. Foi ligado à CLI por um `route(cmd) -> (handler, payload)` único, testado subcomando a
subcomando (`every_subcommand_routes_to_its_handler`), o que também baixou o CC do `run` de 15 para o
máximo de 8 no arquivo. E `touring workflow resume` usa o handler de `run` com `resume: true`, por
escolha documentada no código; a doc dizia `cli-workflow-resume` e foi corrigida.

Provas (04/10/2026):

- RED antes de cada mudança: `held` ausente (`left: Null`), `attempt_completion_rate` ausente,
  `no variant named Status`.
- `dag_lifecycle_closure_e2e`: 37/37. `touring-cli --lib` + `touring-hooks` inteiro: 1.807 passaram,
  0 falharam. `touring-server --lib`: 1.641/0. Clippy `-D warnings` limpo em `touring-cli`,
  `touring-hooks` e `touring-server`.
- Mutação in-place com restauração por sha256: **11/11 mortos** (predicado dos retidos, relatório
  ausente no dry-run e no caminho real, rota de saída ausente, janela ignorada, taxa contando
  tentativas abertas, desfecho sempre `completed`, tentativa aberta virando `failed`, e a chave antiga
  voltando em cada uma das três respostas). Dois mutantes ficaram de fora por serem equivalentes com
  os dados de hoje, com o motivo escrito no harness.
- Ao vivo, daemon PID 1254933 (release desta árvore, ambiente sem `TOURING_CODE_MODE` nem
  `TOURING_R3_SHADOW`): uma sonda real (`task_1791116724800024594`: uma falha, um retry concluído, uma
  tentativa aberta, um passo pendente) responde `attempt_completion_rate = 0,5` em `stats` e `status`,
  igual ao SQL (a definição antiga daria 0,333), sem `cache_hit` em resposta nenhuma; `held` bate com
  o SQL plano a plano nos dois bancos reais: 0 no projeto, e 1 no global, que é o plano
  `task_1787627051162160519` (`finalized` em 25/08 sobre filha `pending`). `candidates` no projeto
  ficou igual ao do binário antigo (8). A sonda foi encerrada por `finalize` e saiu carimbada.
- Não provado ao vivo: um plano RECÉM-fechado à mão aparecendo em `held`. A CLI `decompose update`
  exige `<SUBTASK_ID>`, então a rota que fecha o pai sem subtarefa só existe no RPC (hooks/MCP), e o
  SDK do `--orchestrate` só alcança RPCs de leitura. Esse caminho está provado pelo e2e e pela mutação;
  o que a prova ao vivo mostra é o plano real que essa rota deixou no banco global.

Limites novos, declarados e não corrigidos (sem falha observada):

- Uma subtarefa fechada com `done` não fecha sua tentativa (§6, `record_subtask_transition`): a
  linha fica aberta para sempre. Antes ela puxava a taxa para baixo; agora fica fora do denominador,
  mas essa tentativa também nunca conta como concluída.
- O `UPDATE` que fecha a tentativa não prende a linha mais recente: duas execuções sobrepostas da
  mesma filha fecham uma linha arbitrária (touring-a4).
- `subtask_results` não tem coluna `status`: a equivalência "linha fechada com `cache_hit = 0` é
  falha" vale porque só `completed` e `failed` chegam ao escritor. Um braço novo nesse `match` a
  quebra em silêncio.
