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
  Um id que já pertence a **outra** task é recusado (`persisted: false`, o erro nomeia o dono);
  a resposta traz o status real e `existed`. O pai só é re-derivado quando a linha resultante não é
  terminal (terminal preservado não muda a derivação nem mexe no relógio do plano).
- **Retenção** (`archivable_plans_where`): o plano é arquivável se seu status é terminal, sem carimbo,
  antigo o bastante **e sem nenhuma subtarefa aberta** (a guarda que o touring-a4 pediu: o carimbo é
  praticamente permanente e a população antiga tem pais `done` sobre filhas pendentes). Vale para
  `decompose archive` e para o passe automático.
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
falhando (a medida RED, antes de qualquer conserto) para 31/31; `touring-cli --lib` 641;
`touring-dispatch --lib` 1342; `touring-hooks` inteiro 68 alvos / 1160 testes / 0 falhas;
`touring-hook-runtime --lib` 486; `clippy -D warnings` limpo nos quatro crates (o único aviso é o
future-incompat do `proc-macro-error2`, dependência).

**Mutação (`mut_z.py`, in-place sob `hw-slot`, sha256 conferido ao restaurar): 12/12 mortos.**
`subtask_missing` sempre `false` · passe ignora o store global · janela de retenção zero · sem a
guarda de filha aberta (rota manual e automática) · `add` sem recusa de dono · `add` voltando a zerar
status · `add` voltando a zerar `attempts`/`quality_score` · `update` sem a linha de execução ·
`update` sem retry · derivação que só entende `done` · hook `task-completed` sem o passe.
Cada alvo morreu pelo teste certo (a asserção `... FAILED` do próprio teste, não erro de compilação).

**O defeito que apareceu no caminho (achado H2, não estava na lista).** Ao editar
`touring-dispatch/src/lifecycle/tests.rs`, `touring doctor` foi a 8/9 com `kind_unknown=1`: a
passada de caminhos de módulo (`record_module_path_consumers`) gravava a **última palavra** do
caminho como nome do módulo, então `use super::*;` e o prefixo `super::super` de
`super::super::cli_handlers::run()` viravam consumo de um símbolo chamado `super`, sem produtor.
O rebuild apaga o fantasma no fim da varredura; o caminho de edição, que roda a mesma passada, não.
RED reproduzido com a linha exata do banco vivo; conserto = a regra que `record_import_consumers` já
aplicava aos `use` (palavra de escopo não é módulo). Depois do restart e de re-derivar o arquivo
pelo caminho de edição: `kind_unknown=0`.

**Build e restart.** `cargo build --release --workspace` verde (5m17s no primeiro). Foram **três**
restarts do daemon global (`update-touring --no-build`), não o "um único" planejado: o 2º por causa
do conserto de wiring acima, o 3º por causa do gatilho de criação de plano (seção 6). Ambiente do
daemon final conferido em `/proc/<pid>/environ`: sem `TOURING_CODE_MODE` nem `TOURING_R3_SHADOW`.
`touring doctor` 9/9.

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
- O gatilho de criação (3º restart) tem teste e RED, mas **não tem
  evento ao vivo próprio**: o censo já estava zerado (o `task-completed` real o esvaziou às 00:28:36,
  antes do 3º restart), então a sonda de criação (plano finalizado em seguida) não tinha o que varrer.
  A prova dele é o teste `creating_a_plan_retires_the_closed_ones_nobody_finalized`.

## 6. Limites declarados

- `INSERT OR REPLACE` continua em `cli_decompose_create`, na importação de tasksfile e em dois
  helpers de **teste** dos suggesters: criam linhas novas; nenhum é caminho de re-add de produção.
- O retry só é alcançável quando o payload do `update` traz `retry_policy`: nenhum caminho grava uma
  política válida na coluna (o importador grava `"{}"`, que não desserializa). Portado por paridade
  com o verbo original, sem mudar o padrão de ninguém.
- `cache_hit_rate` de `touring workflow stats` **não é uma medida**: a heurística original grava
  `cache_hit = (status == "completed")`, então toda subtarefa concluída conta como acerto e a taxa
  dá 1.0. Portada por paridade; não use o número. Candidata a remoção, decisão do Gabriel.
- O gatilho de criação tem RED e teste verde, mas a mutação "criar sem o passe" é o próprio RED
  anterior à implementação (não está no `mut_z.py`).
- A guarda de filha aberta usa o vocabulário **de subtarefa** (`failed`/`skipped`/`cancelled`
  encerram); um plano com filha `failed` e pai terminal é arquivável.
