# Cross-audit 14/09/2026 — rodadas 8 e 9 e o wrapper de qualidade

> **Estado: fases 1-4 concluídas; PAUSA HUMANA antes da fase 5.** Nenhuma correção foi aplicada. DAG `task_1789406779265539522`.

## Escopo

Tudo o que esta sessão implementou:
- **Rodada 8 (S1-S19):**
  - patch do tree-sitter-md;
  - cessão cooperativa do ator;
  - produtores de wiring com fonte única;
  - gate de raiz no pre_edit;
  - escopo do rebuild e selo de geração;
  - `worktree_rebuild_command`;
  - `is_wireable_source`;
  - skip único no touring-quality;
  - exit 75;
  - `startup_identity`.
- **Rodada 9:** 1-A (companheiras fora do wiring), 2-A (daemon em scope systemd), 3-A (`index status` fora do ator).
- **Wrapper `touring-quality-score`,** agora versionado.

São 39 arquivos: 28 rastreados (+3.831 −687 linhas) e 11 novos. Lista em `target/audit-2026-09-14/` (cópia da lista do scratchpad).

## Método

As fases 1-4 foram executadas pelo orquestrador em paralelo com quatro críticos independentes, em sessões novas, só leitura e proibidos de rodar `cargo`. Cada crítico tinha uma lente:
- concorrência e contrato do ator;
- wiring e integridade do índice;
- processo, spawn e scripts;
- parser C, touring-quality, hooks e drift de documentação.


## Achados verificados até agora

> **Status (atualizado na rodada 2):** as tabelas abaixo descrevem o estado **antes** das correções, com linhas daquela data. O que foi corrigido está na seção "Fase 5 — correções aplicadas"; a verificação independente dessas correções e os achados novos estão em `docs/audits/cross-audit-2026-09-14-r2.md`.

Legenda de verificação:
- **Conferido:** o orquestrador leu o código ou reproduziu o problema.
- **Reproduzido:** o comportamento foi executado por comando.
- **Crítico:** a evidência veio só do crítico, com experimento dele.

### Orquestrador (fases 1-4)

| ID | Sev | Achado | Evidência |
|---|---|---|---|
| O1 | P1 | Uma leitura muta o wiring. O `post_read` apaga todas as arestas de consumidor do arquivo (`post_read.rs:271`, `clear_consumer_entries`) e grava palpites por nome de método como `ast_resolved` (passe F9 de 11/05, `post_read.rs:~120`). O rebuild e a edição já usam `record_inferred_consumers`; a leitura não. | Reproduzido: `dep_health.rs` foi de 79 inferidas + 1 declarada para 0 + 68 depois de um único Read. Sete órfãos "novos" apareceram no escopo durante a auditoria, gravados às 17:13-17:15 UTC pelas leituras dos críticos. |
| O2 | P1 | O F2.5 reprova `touring-quality` por `crates/touring-quality/Cargo.lock` morto, de 24/07. O crate é membro do workspace e o lock da raiz tem as versões corrigidas. O localizador pega o lock mais próximo. | Conferido: `touring-quality check --gate F2.5 --target crates/touring-quality` lista 8 CVEs; versões na raiz: crossbeam-epoch 0.9.20, h2 0.4.19, quinn-proto 0.11.15, rkyv 0.8.18, wasmtime 36.0.14. |
| O3 | P2 | `daemon_tests.rs:132` tem `#[ignore]` com justificativa falsa ("spawna processo real"): o teste usa threads e caminhos fixos em /tmp. | Conferido. |
| O4 | P3 | Supressões mortas ou imprecisas: `store.rs:777` (`allow(dead_code)` num campo `pub` usado, com comentário falso), `pre_edit.rs:21` (`allow(unused_imports)` com os traits usados 7×), `touring-hooks/src/main.rs:219` (`allow(unused_mut)` que deveria ser `cfg_attr`). | Conferido. |
| O5 | P3 | `systemic_diag_v2.py` ignora `--help` e dispara o diagnóstico completo. | Reproduzido. |
| O6 | P3 | `shellcheck` não está instalado, então dois testes de lint de shell nunca rodam localmente. Instalar exige sudo: decisão humana. | Conferido. |

### Crítico A — concorrência e contrato do ator

| ID | Sev | Achado | Verificação |
|---|---|---|---|
| A1 | P1 | A fila `deferred` da cessão não tem limite. Cada `try_recv` libera uma vaga do canal de 128 e o backpressure some durante o heavy. Os comandos adiados rodam em série depois, com os clientes já desistidos (2-3 s). Introduzido na rodada 8. | Conferido: `touring-dispatch/src/daemon.rs:596-631`, `touring-hooks/src/main.rs:685/757`. |
| A2 | P2 | `rt.ctx.set_cmd_tx(cmd_tx.clone())` sem nenhum chamador de `.cmd_tx()`: o canal nunca fecha, então um ator despejado pelo LRU nunca termina, e o comentário da evicção é falso. Anterior a esta sessão. | Conferido: `touring-dispatch/src/daemon.rs:177`, `:1938-1940`. |
| A3 | P2 | O piso de leitura do cliente é igual ao orçamento heavy do servidor e começa antes, então o `budget_exceeded` tipado não chega. A mensagem de timeout manda "repita com orçamento maior", o que dispara um segundo walk. O teste `heavy_op_budget_is_never_below_the_client_floor` é tautológico. | Conferido: `server/cli/index.rs:122`, `daemon_client.rs:50-55`. |
| A4 | P2 | Nenhum consumidor lê `retryable`; todo `budget_exceeded` vira exit 75 ("repita"), heavy incluído. | Crítico: `daemon_client.rs:218-231`. |
| A5 | P2 | Só o rebuild cede a vez; os outros 11 hooks heavy instalam a função e nunca a chamam. `actor_yield.rs` promete o contrário. | Conferido: `yield_now` só em `index.rs:902/1210/1240/1327`. |
| A6 | P3 | Corrida no cache de status fora do ator: a invalidação final vem antes do selo (`index.rs:1422` antes de `:1451`), e um leitor concorrente pode deixar `building` cacheado por 60 s. | Conferido. |
| A7 | P3 | `index_status_from_disk` não propaga erro do `knowledge.db` (sem `?`): responde `state: unknown` com sucesso e ainda cacheia. | Conferido: `index.rs:124-131`. |
| A8 | P3 | O caminho fora do ator não chama `record_hook_dispatch_named`, então os dois contadores por hook divergem. | Conferido: a única chamada é `touring-dispatch/src/daemon.rs:498`. |
| A9 | P3 | Testes que sobrevivem à remoção do que guardam: o de panic do yield reinstala antes de afirmar; o de yields só conta (não afirma autocommit); nada cobre `MAX_SERVED_PER_YIELD` nem a ordem FIFO dos adiados. | Conferido: `actor_yield_tests.rs:71-85`. |
| A10 | P3 | O auto-commit do tantivy a cada 500 upserts publica o delete de um arquivo antes das suas adições; um `memory recall` servido no yield vê o arquivo pela metade. O comentário "um commit para o walk" é falso. | Crítico: `tantivy_index.rs:1708`. |
| A11 | P3 | `cli-hook-memory-*` fica fora da allowlist e é reordenado contra `cli-memory-*`. | Conferido: `hook_registry.rs:244-245`. |

### Crítico C — processos, spawn e scripts

| ID | Sev | Achado | Verificação |
|---|---|---|---|
| C1 | P1 | A chave do cache do wrapper ignora os flags: num acerto de cache o `--fail-below` vira no-op, e `--dims`, `--format` e `--output` recebem o relatório de outra invocação. | Reproduzido: composite 0,85 com `--fail-below 0.95` saiu rc=0; `--dims F2.1 --format text` recebeu o JSON completo. |
| C2 | P1 | Arquivo não rastreado não entra na chave (`git ls-files` só vê o index), mas o motor o pontua: a nota velha é servida. | Reproduzido: engine calls=1 depois de criar o arquivo. |
| C3 | P1 | `daemon-ctl restart` manda SIGKILL para TODOS os `touring-daemon` quando o dreno estoura 10 s (cascading kill, REGRA #19). Anterior a esta sessão. | Conferido: `daemon_ctl.rs:400-405` (`all_daemon_pids()`). |
| C4 | P2 | O fallback direto do `update-touring` dispara por "socket ausente em 5 s", não por falha do launcher: o daemon direto pode vencer o flock e voltar ao cgroup do chamador. `-S` aceita socket stale. | Crítico. |
| C5 | P2 | O teste novo do `update-touring` só confere conteúdo: uma mutação que desliga o scope por padrão continua verde. | Crítico: mutação executada. |
| C6 | P3 | `systemd-run --scope` expande `$VAR` e `$$` no argv; falta `--expand-environment=no`. | Crítico: experimento executado. |
| C7 | P3 | Opt-out assimétrico: o Rust aceita `0\|false\|off`, o bash só `0`. | Conferido. |
| C8 | P3 | O autostart do hook passou a esperar o exec (até 2 s) com o user manager lento. | Crítico. |
| C9 | P3 | O daemon de gate do juiz (`loop_converged.py:371`) não usa o launcher e fica vivo depois do juiz. A doc "one launcher for every spawn site" é falsa. | Crítico: PID vivo observado. |
| C10 | P3 | O fd 9 do lock é herdado por netos do motor; um neto de vida longa segura o lock. | Crítico: experimento E3. |
| C11 | P3 | O lock padrão em /tmp com EACCES sai rc=1 sem JSON, o mesmo código de "reprovou". | Crítico: experimento E5. |
| C12 | P3 | `startup_identity` lê o spawner só depois do bind: com boot lento, atribui ao subreaper. | Crítico: não observado ao vivo. |

### Crítico B — wiring e integridade do índice

| ID | Sev | Achado | Verificação |
|---|---|---|---|
| B1 | P1 | = O1, confirmado de forma independente. 8 arquivos lidos entre 17:13 e 17:16 UTC ficaram com 0 `ast_inferred` e 790 `ast_resolved`. Nasceram as chaves fantasmas `src/protocol.rs` e `src/shared.rs` (fallback `crate::`→`src/…` do `populate_wiring_map`). Órfãos falsos confirmados: `MAX_INDEXABLE_FILE_BYTES`, `SpawnRoute`, `SAGA_FRAME_LEN`, `IPC_MAGIC_V1`. | Conferido (O1) e medido pelo crítico no DB read-only. |
| B2 | P2 | `file_changed` e `task_output` chamam `update_wiring_after_edit`, que apaga todos os consumidores, sem a reinferência de `record_direct_path_consumers`. É a assimetria C08 que a W4 disse ter fechado. 1.673 dos 1.691 arquivos com arestas inferidas estão expostos. | Conferido: `file_changed/mod.rs:71-76`, `task_output.rs:149-154`. |
| B3 | P2 | `post_read`, `file_changed` e `task_output` não passam pela política de admissão (`admission_refusal` só existe em `reindex.rs`). O gate 1-A depende da grafia `@companion/` e deixa passar o caminho absoluto: há 656 chaves absolutas em `file_knowledge`, uma delas `~/.claude/skills/.../loop_converged.py` gravada às 16:30. | Crítico: medição no DB. |
| B4 | P3 | Os DELETEs de wiring (`clear_wiring`, `purge_module_rows`, `clear_consumer_entries`) usam a chave crua; os INSERTs canonicalizam. Com um caminho absoluto, o produtor obsoleto sobrevive ao refresh. | Crítico. |
| B5 | P3 | `register_public_symbols` conta como registradas as linhas que o gate recusou, e a doc diz "rows registered". A asserção `wiring_entries == 1` do teste 1-A mede essa contagem inflada. | Conferido: `wiring.rs:454-473`. |
| B6 | P3 | O `index rebuild` disparado pelo WorktreeCreate pode resolver para a raiz do projeto pai, porque `has_project_marker` exige `.git` como diretório e num worktree ele é arquivo. Mitigado nos projetos atuais. | Conferido: `paths.rs:413-415`. |
| B7 | P3 | `--dir` apontando para um symlink valida o caminho canônico mas percorre com a grafia do link, gerando chaves duplicadas. | Crítico: análise estática. |
| B8 | P3 | `INSERT OR REPLACE` sobrescreve a proveniência sem comparar força: o inferido rebaixa o resolvido, e vice-versa. | Crítico. |
| B9 | P3 | `is_code_language` tem homônimo com semântica oposta (`wiring.rs:442` é deny-list, `detect_language.rs:39` é allow-list), além de uma cópia inline no `post_read`. | Crítico: `index find` achou 2 definições. |
| B10 | P3 | Testes fracos: o de refresh companion não semeia resíduo; o de worktree só confere o formato do Command; nenhum cobre a leitura preservando arestas inferidas; o teste de companion no storage é condicional à tabela existir. | Crítico. |
| B11 | P3 | A evicção feita no open não julga consumidores companion ou fora da política. `consolidation.rs:273` e `scip_ingest.rs:172` escrevem sem passar pelo gate. | Crítico: 0 linhas ao vivo hoje. |
| B12 | P3 | A inferência por nome liga tipos externos (`std::path::Path`, `criterion::Criterion`) a produtores homônimos do workspace. | Crítico: linhas vivas. |

Medições do crítico B no DB vivo (read-only):
- `wiring_map`: 86.093 linhas, sendo 12.393 produtores e 73.700 consumidores (70.402 `ast_inferred`).
- Linhas com chave companion: 0. Linhas com chave absoluta: 0.
- Produtores obsoletos em relação ao `symbols.db`: 0 de 12.393.
- Visibilidade dos produtores: 11.506 `public` e 887 `crate`.

Verificado OK pelo crítico B:
- produtores com fonte única;
- `post_edit`/`post_write` chamam o refresh;
- o rebuild com escopo não perde dados, porque a varredura só purga o que o walker recusa;
- `worktree_rebuild_command` não spawna para caminho inválido;
- `store_stats` mantém as mesmas consultas.

### Crítico D — parser C, touring-quality, hooks e drift de documentação

| ID | Sev | Achado | Verificação |
|---|---|---|---|
| D1 | P1 | `tree-sitter-bash 0.25.1` chama `isdigit(lexer->lookahead)` com codepoint (`scanner.c:1158` e `:1172`), e isso derruba o processo num build comum. O `.sh` é indexado pela gramática: um arquivo com `{` + codepoint dos planos altos derruba o daemon no rebuild. O registro "UB sem crash provado" está superado. | Reproduzido: harness C com runtime 0.26.9 + bash 0.25.1 sem modificação deu SIGSEGV em 3 de 3; controle negativo com as mesmas entradas em ASCII saiu com 0. O crítico mediu 20.480 codepoints que caem em páginas não mapeadas no layout do daemon vivo. |
| D2 | P1 | O F2.4 (segredos, P0 BLOCK) é diluído pela ponderação por LOC no escopo diretório: um segredo real vira Warn sem blocker. | Reproduzido: `fx2/b` (lib.rs limpo + keys.rs com chave AWS) deu 0,5 Warn com `blockers []`; o keys.rs sozinho deu 0,0 Fail `[F2_4]`. |
| D3 | P2 | `SKIP_DIRS` casa `third_party` em qualquer profundidade e também tira do corpus as dimensões P0 de segurança (F2.1, F2.4). Some código de primeira mão chamado `third_party` e o C vendorizado que é compilado no daemon. Introduzido na rodada 8. | Crítico: fixture `fx2/a` com segredo em `src/third_party/` pontuou 1,0 Pass. |
| D4 | P2 | A correção do vazamento cross-project no `pre_edit` é incompleta: `Some(root)` sem marcador normaliza para `$HOME`, e isso é o índice tantivy global. | Crítico: `tantivy_index.rs:1848`, `paths.rs:360-410`. |
| D5 | P2 | O cortex de produção chama `compose_edit_context(None)` com a raiz disponível e perde os sinais 14, 7a e 9 sem mostrar a ausência. | Crítico: `neural.rs:253`. |
| D6 | P3 | A guarda de ctype não cobre o bash, corta a linha no primeiro `//` mesmo dentro de literal, não vê macro nem `(isdigit)(c)`, e não confirma que o patch é o que compila. | Crítico. |
| D7 | P3 | O teste e2e de ctype passa com ou sem o fix por desenho; uma varredura de codepoints tornaria o teste determinístico. | Crítico. |
| D8 | P3 | `best_practices.rs:117` mantém uma lista própria de diretórios pulados. | Crítico. |
| D9 | P3 | Os testes de sinais tantivy abrem o índice global real sob `$HOME`. | Crítico. |

Drift de documentação (crítico D):
- `PATCHES.md` e `analysis.md` dizem que o bash não tem crash provado.
- "Num build normal a leitura cai em memória mapeada" não tem medição.
- O censo diz "29 outras gramáticas", mas são 28.
- O teste diz "o fix é o serialize limitado"; o fix efetivo é o cap de 254.
- O item 15 do `CLAUDE.md` anuncia "três contratos" e lista quatro.
- O item 16 diz "três decisões", mas o (d) é um follow-up.
- A exclusão de `third_party` é descrita só como correção do F1.3 e omite o efeito nas dimensões P0.
- O S11 diz "os sinais exigem raiz", o que só vale para None.
- O comentário do `touring.toml` não justifica `third_party`.
- O `CLAUDE.md` do touring-quality diz "P0 fail-closed".
- O `CLAUDE.md` do touring-cli não documenta a 3-A, cujo handler vive nele.

## Síntese

- **Total:** 8 P1, 16 P2 e 30 P3 distintos, contando a sobreposição O1=B1.
- **Introduzidos por esta sessão:**
  - A1 (fila sem limite);
  - A3 (piso = orçamento);
  - D3 (`third_party` nas dimensões P0);
  - C1, C2, C10 e C11 (herdados pelo wrapper que versionei sem revisar a lógica);
  - B5 (contagem inflada no meu `register_public_symbols`);
  - C5 e C7 (teste de conteúdo e opt-out assimétrico da 2-A);
  - A6, A7 e A8 (3-A);
  - drift de docs.
- **Anteriores e revelados agora:** O1/B1, B2, C3, O2, D1, D2, A2, A5, D4 e D5, entre outros. A REGRA #21 manda corrigir todos.
- **Verificado OK pelos críticos (resumo):**
  - sem transação aberta nos yields;
  - chave de cache do status idêntica entre os dois caminhos;
  - produtores com fonte única;
  - rebuild com escopo sem perda de dados;
  - patch do md correto em memória (cap em todos os pushes, deserialize consistente);
  - rota de scope funcionando ao vivo;
  - sem injeção no `--description`.

## Plano de correção proposto (fase 5, AGUARDANDO APROVAÇÃO)

**Lote 1 — P1 (integridade, segurança, estabilidade):**
- **F1 (O1+B1+B2).** Criar `wiring::refresh_file_wiring(db, key, lang, content)` como fonte única de produtores + consumidores declarados + inferidos. Usada por edição, leitura, `file_changed` e `task_output`. A leitura não toca o wiring quando o `content_hash` não mudou. Remover o bloco F9 e os consumidores do `populate_wiring_map`. Testes: uma leitura preserva as arestas inferidas e não cria chave `src/…`; conteúdo alterado dá o mesmo grafo que a edição.
- **F2 (D1).** Vendorizar `tree-sitter-bash` via `[patch.crates-io]` com dígito ASCII. Estender a guarda de ctype a todas as gramáticas vendorizadas, com um tokenizer que respeite comentários e macros. Teste filho com varredura de codepoints em bash e md. Atualizar `PATCHES.md`.
- **F3 (D2).** As dimensões P0 (F2.1, F2.4, F2.6) agregam por "qualquer arquivo em 0 é Block", nunca por média. Teste com 1 segredo entre N arquivos limpos.
- **F4 (D3).** Corpus de segurança separado do corpus de estilo: a segurança só pula `target/node_modules/.git`, e o skip de vendorizado passa a ser relativo à raiz. Teste com segredo em `src/third_party/`.
- **F5 (O2).** O F2.5 resolve o lock pela raiz do workspace (`[workspace]` mais externo que contém o alvo). Teste com fixture de membro com lock obsoleto. Remover `crates/touring-quality/Cargo.lock` (decisão H1).
- **F6 (A1).** Parar de drenar quando `deferred` atinge `PROJECT_CHANNEL_DEPTH`. Testes: limite, `MAX_SERVED_PER_YIELD` e ordem FIFO dos adiados.
- **F7 (C1+C2+C10+C11).** No wrapper:
  - o argv entra na chave;
  - `--fail-below`, `--output` e `--dims` nunca são servidos do cache;
  - a listagem inclui arquivos não rastreados;
  - o motor roda com `9>&-`;
  - o lock padrão vai para `$XDG_RUNTIME_DIR` e erro de abertura sai com 75 e mensagem.
  Testes para cada caso.
- **F8 (C3).** O SIGKILL do `restart` fica restrito aos pids do alvo ainda vivos. A seleção vira função pura, com teste.

**Lote 2 — P2:**
- A2: `WeakSender` e teste via `ProjectRuntime::new`.
- A3: piso do cliente = orçamento + margem, mensagem honesta, teste real no lugar do tautológico.
- A4: exit por `retryable` (decisão H2).
- A5: yields na varredura e no purge + doc verdadeira.
- B3: política de admissão nos três handlers.
- C4: fallback do `update-touring` por falha real do launcher + prontidão por connect.
- C5: teste comportamental do `start_daemon`.
- D4: recusar o fallback para `$HOME` em `tantivy_for`.
- D5: cortex passa a raiz e mostra os sinais indisponíveis.
- O3: tirar o `#[ignore]` usando tempdir.

**Lote 3 — P3:** todos os demais (A6-A11, B4-B12, C6-C9, C12, D6-D9, O4, O5) e o drift de docs.

**Decisões de Gabriel:**
- **H1:** remover o `Cargo.lock` morto do touring-quality (`git rm`).
- **H2:** contrato do exit code quando um heavy estoura o orçamento com `still_running`: manter 75 ou criar um código próprio.
- **H3:** instalar `shellcheck` (sudo).
- **H4:** escopo: os três lotes agora, ou só P1+P2.
- **H5:** release 30.4.47 depois das correções (reinicia os daemons de analise e konverter).

## Portão humano (registrado)

Em 14/09/2026, antes da fase 5, Gabriel aprovou:
- **Escopo:** lotes 1, 2 e 3 completos.
- **H1:** remover `crates/touring-quality/Cargo.lock`.
- **H2:** código de saída próprio para heavy com `still_running` (o 75 fica só para `retryable:true`).
- **H5:** release 30.4.47 com aviso às sessões.

A H3 (`shellcheck`, exige sudo) fica registrada como pendência do operador.

## Fase 5 — correções aplicadas

Cada correção tem um teste que falha sem ela. A prova é a mutação: o runner reverte a correção, roda o teste, exige falha e restaura o fonte (`target/audit-2026-09-14/mut_lote{1,2,3}.py`).

### Lote 1 (P1)

| Item | Correção | Prova |
|---|---|---|
| F1 (O1, B1, B2) | `wiring::refresh_file_wiring` é a fonte única de produtores, consumidores declarados e inferidos. Edição, leitura (só com `content_hash` novo), `file_changed` e `task_output` a usam. O passe F9 e o resolvedor próprio do `post_read` saíram. | 2 mutantes mortos |
| F2 (D1) | `third_party/tree-sitter-bash` via `[patch.crates-io]`, dígito ASCII no lugar de `isdigit`. Guarda de ctype sobre os três scanners, vendo através de comentário, literal, parênteses e `#define`; confirma que o patch é o que compila. Varredura de codepoints em processo filho para bash e md. | 2 mutantes mortos (bash, md) |
| F3 (D2) | `AggKind::FailClosedLoc` para F2.1 e F2.4: um arquivo reprovado reprova o escopo. | bateria de qualidade |
| F4 (D3) | Corpus de segurança separado do de estilo; vendorizado entra nas dimensões de segurança. | bateria de qualidade |
| F5 (O2) | F2.5 resolve o lock do workspace dono do alvo, respeitando `exclude`. Lock morto removido (H1). | 1 mutante morto |
| F6 (A1) | A fila `deferred` para na capacidade do canal; ordem FIFO preservada. | 1 mutante morto |
| F7 (C1, C2, C10, C11) | Wrapper de qualidade: argv na chave, `--output` sem cache, não rastreados na listagem, `9>&-`, lock em `$XDG_RUNTIME_DIR` com saída 75. | 5 mutantes mortos |
| F8 (C3) | O SIGKILL do `restart` fica restrito aos pids do alvo que ainda são `touring-daemon` (`force_kill_set`). | 1 mutante morto |

**Achado novo durante o F1: declaração lida como referência.** `extract_type_and_const_refs` capturava o nome no ponto de declaração (`pub struct TfIdfVectorizer;` devolvia `TfIdfVectorizer`), e todo tipo público consumia a si mesmo. No banco vivo:
- 6.375 autoarestas;
- 2.137 produtores públicos vivos só por autoaresta. Destes, 44 são só declaração (defeito do extrator, corrigido com `is_declared_type_name` e mutante morto) e 2.093 são usados apenas dentro do próprio arquivo.

**Decisão H6 (aberta, de Gabriel):** um `pub` usado só no próprio arquivo conta como consumido ou como órfão? Contar como órfão expõe 2.093 itens de visibilidade larga demais; contar como consumido mantém o número atual. Nada foi mudado nessa política.

### Lote 2 (P2) — 14 de 14 mutantes mortos

| Item | Correção |
|---|---|
| A2 | O runtime guarda `WeakSender` do próprio canal: derrubar o `ProjectRuntime` fecha o canal e o ator despejado termina. A cópia em `InferletService`, nunca setada, saiu. |
| A3 | `HEAVY_OP_CLIENT_FLOOR_SECS` = orçamento + 60 s. `is_heavy_hook` foi para o `touring-foundation`: servidor e cliente usam a mesma lista, e todo hook heavy espera, não só `rebuild`/`mutation-test`. Timeout de heavy manda consultar `index status`, nunca repetir. O teste tautológico virou uma desigualdade real contra as três esperas de 5 s. |
| A4 (H2) | `DaemonBusy.retryable`; `retryable:false` sai com **79** (`DAEMON_STILL_RUNNING_EXIT_CODE`), 75 só para `retryable:true`; ausência do campo lê como `true` (daemon antigo). |
| A5 | Yields antes de cada purga da varredura e do purge de módulos fantasmas; a doc do `actor_yield` diz que só o rebuild cede a vez. |
| B3 | Admissão do walker em `post_read` e em `refresh_file_wiring_from_disk` (`file_changed`, `task_output`). `task_output` separa artefatos citados (DAG, memória) de arquivos re-verificados. Um teste de lifecycle quebrado pelo F1 foi a pista. |
| C4 | Prontidão por `connect`, não por `-S`; fallback direto só quando o launch morreu antes de responder (rc 2); daemon lento nunca ganha um segundo launch. |
| C5 | Testes comportamentais que executam o `start_daemon` real contra stubs de daemon, `systemd-run` e user manager. |
| C6 | `--expand-environment=no` no bash e no Rust; expansão de `${USER}` provada ao vivo com e sem a flag. |
| C7 | Opt-out `0|false|off` no bash, igual ao Rust. |
| D4 | `scoped_root`: raiz nomeada que normaliza para `$HOME` sem ser o `$HOME` não recebe índice tantivy nem tool-outputs. |
| D5 | O cortex passa o runtime ao `compose_edit_context` e nomeia os sinais indisponíveis quando ele não abre. |
| O3 | Os três testes de lock usam tempdir; o de startup concorrente saiu do `#[ignore]`. |

### Lote 3 (P3)

| Item | Correção |
|---|---|
| A6, A7 | `building` nunca vai para o cache; geração ilegível devolve `None` (o ator responde); invalidação depois do selo. |
| A8 | A rota fora do ator conta em `record_hook_dispatch_named`. |
| A9 | O teste de panic exige a MESMA função de volta, sem reinstalar; os testes de yield do rebuild afirmam autocommit em todo yield; teste novo do teto `MAX_SERVED_PER_YIELD`. |
| A10 | `stage_symbol` + `commit_if_due`: o commit em lote cai entre arquivos, nunca no meio de um. |
| A11 | `cli-hook-memory-*` na allowlist de cessão. |
| B4 | Os cinco DELETEs de wiring canonicalizam a chave como os INSERTs. |
| B5 | `register_pub_symbol_counted`: `wiring_entries` conta só linhas escritas. |
| B6 | `.git` arquivo (worktree, submódulo) é marcador de projeto. |
| B7 | `--dir` symlink é percorrido no caminho canônico, sob a grafia da raiz. |
| B8 | Palpite mais fraco nunca rebaixa aresta mais forte. |
| B9 | `wiring::is_code_language` virou `declares_wireable_api`; a cópia inline do `post_read` usa o predicado. |
| B10 | Teste de refresh companion semeia resíduo; o de storage não é mais condicional; o de worktree prova a resolução da raiz. |
| B11 | A evicção no open remove consumidores companion (cobre a consolidação legada); o ingest SCIP passa pelo gate (`gated_edges_skipped`). |
| B12 | Tipos importados de outra crate (e caminhos inline da família std) ficam com o resolvedor de imports, não com a inferência por nome. |
| C8 | `spawn_daemon_detached_within`: o hook espera no máximo 250 ms pelo exec do scope. |
| C9 | O juiz para o daemon privado ao fim da cláusula cargo (verde, vermelha ou exceção). |
| C12 | O spawner é lido antes do lock e do bind. |
| D8 | `best_practices` usa `is_skipped_dir_name`. |
| D9 | Testes de sinais tantivy num projeto temporário com marcador; tautologias trocadas por asserções. |
| O4 | Supressões mortas removidas (`store.rs`, `pre_edit.rs`, cujo `ResultExt` nem era usado); `main.rs` com `cfg_attr`. De brinde: `touring-hook` não compilava com `--no-default-features` (`session-audit` sem o gate `utilities`). |
| O5 | `systemic_diag_v2.py --help` imprime o uso e não roda nada. |
| Docs | Itens 15, 16 e 17 do `CLAUDE.md`, comentário do `touring.toml`, `CLAUDE.md` do touring-quality, do touring-cli e do touring-server (exit 79), S11 e bash no `analysis.md`. |

Pendências registradas:
- H3 (`shellcheck`, sudo).
- H6 (política de autoconsumo).
- `judge_attest.py --attest`: o `loop_converged.py` mudou (C9 e alterações da sessão analise); a deriva é advisory e o `--attest` é ato humano.

### Achado novo depois do deploy: arestas fantasmas do scanner textual

Depois do `update-touring`, o `touring doctor` reportou `wiring_diagnostic: warning` com `kind_unknown=2`. Eram duas arestas de consumidor sem produtor, gravadas pelo caminho de edição quando meus edits reindexaram os arquivos:

| Aresta | Causa-raiz |
|---|---|
| `crates/touring-hook-runtime/src/wiring.rs` → `touring-hooks-shared/src/feature_flags.rs::f` | `extract_direct_path_expressions` lia um **comentário** (`crate::shared::feature_flags::f()`) como código. |
| `crates/touring-quality/src/verifications/f2_5_dep_cves.rs` → `touring-quality/src/lib.rs::real_engine` | `use super::super::real_engine` fica dentro de `mod tests { mod real_engine_tests {`. O scanner resolvia os `super` pela profundidade do arquivo, não pela dos módulos inline. |

As duas causas são anteriores a esta sessão. Exigir produtor existente no registro cortaria arestas legítimas quando o consumidor é indexado antes do produtor. Por isso a correção foi no scanner:
- um lexer mínimo que pula comentários (inclusive aninhados), strings, raw strings e char literals, sem confundir lifetimes;
- rastreio dos blocos `mod x { … }` inline, descontando os `super::` que eles consomem;
- quando todos os `super` ficam dentro do arquivo, o caminho nomeia um item do próprio arquivo e não gera aresta.

Testes: `extract_direct_path_ignores_comments_and_literals` e `extract_direct_path_discounts_supers_of_inline_modules`, com dois mutantes mortos (sem o skip de comentário; profundidade inline zerada).

### Gates do workspace (30.4.47)

- `cargo check --workspace`: rc=0.
- `cargo clippy --workspace --all-targets -- -D warnings`: rc=0.
- `cargo test --workspace --no-fail-fast`: 1 falha em 231 binários, `binary_e2e::version_matches_crate_version`. O binário release ainda era 30.4.46 (built 16:05) e o crate já dizia 30.4.47. Resolve com o build da release; foi rerodado depois do deploy.
- Daemon de gate do juiz de 13:48 ainda vivo, rodando um binário apagado: o sintoma exato do C9. Foi parado com `touring daemon-ctl stop --socket`.

### Release 30.4.47 propagada, e o que o deploy revelou no `update-touring`

`scripts/propagate-release.sh 30.4.47` terminou com rc=0: gate do espelho (345 arquivos), build de 8 min 22 s, toolchain congelada, canal default, analise e konverter em 30.4.47 (lock e `touring --version` batem) e prova comportamental 40/40.

No meio do caminho, o verify do `update-touring` imprimiu `ERROR: PID 3128873: running deleted binary — restart needed` e logo em seguida `done`, com exit 0. Havia dois defeitos, ambos anteriores a esta sessão:

| Defeito | Causa-raiz | Prova |
|---|---|---|
| O verify falhava aberto | Em `verify_or_restart`, `local rc=$?` vinha depois de `if verify_health; then return 0; fi`. Sem `else`, `$?` é o status do próprio `if`, sempre 0. O self-heal (`force_daemon_restart`) nunca rodou, e o deploy saiu 0 depois de reportar um daemon velho. | `bash -c 'f(){ if false; then return 0; fi; local rc=$?; echo rc=$rc; }; f'` imprime `rc=0`. |
| O resolvedor nomeava o antecessor | `global_daemon_pid` usava `lsof -t socket \| head -1`. Logo depois do `daemon-ctl restart`, o antecessor (3128873, subido por um hook durante o build) ainda fazia o flush de KPI com o listener aberto. Com o PID menor, ele foi escolhido, e o deploy novo (3134544) foi lido como binário apagado. | O `daemon.stderr.log` mostra o SIGTERM de 3128873 antes do spawn de 3134544; o verify veio 1 s depois. |

Correção:
- `verify_health || rc=$?` captura o status na própria chamada.
- O resolvedor lê primeiro a entrada do registro que o dono grava no bind (`/tmp/touring-daemons-<uid>/*.json`), a mesma fonte do `pid_for_socket` do `daemon-ctl`, confirmada por `/proc/<pid>/comm`.
- Sem entrada utilizável, fica o `touring-daemon` iniciado por último entre os que seguram o socket (campo 22 de `/proc/<pid>/stat`).

Testes novos em `scripts/test_update_touring.py` (23 verdes): três sobre o laço de verify (cura uma vez, falha persistente sai 4, daemon saudável nunca é reiniciado) e três sobre o resolvedor (registro vence o antecessor; sem registro vence o mais novo, ignorando processo que não é daemon; entrada de outro socket ou de outro processo é ignorada). Mutação: 6/6 mortos (`target/audit-2026-09-14/mut_update_touring.py`). Ao vivo, `update-touring --verify-only` resolve 3134544 com o binário novo.

Também nesta etapa: `docs/agentic-bench/run_bench.py` recebeu o pragma `touring-quality:allow-attack-fixture`. F2.1 foi de 0.0 (dois payloads SQLi intencionais) para 1.0, e os 24 testes do benchmark seguem verdes.

### Depois da propagação

- `cargo test -p touring-server --test binary_e2e version_matches_crate_version`: 1 passed, rc=0. A única falha do workspace test está resolvida.
- `touring index rebuild` (unit `touring-rebuild-47`): geração 22 `complete`, 5.314 arquivos, 160.937 símbolos, 0 erros, `phantom_modules_purged=2`, `wiring_tx_failures=0`, 519 s.
- `touring doctor -j`: **8/8 ok**. `wiring_diagnostic` passou de warning (`kind_unknown=2`) para `ok`, com `kind_unknown=0 non_wireable=0 unread=0 abs_paths=0`, 12.407 produtores e 71.277 consumidores.

## Rodada 2

A auditoria cruzada das correções desta rodada, feita por quatro críticos independentes e verificada pelo orquestrador, está em `docs/audits/cross-audit-2026-09-14-r2.md`: 5 achados P1, 5 P2 e 8 P3, mais 10 alegações dos críticos rejeitadas com evidência. O juiz das 17:35 saiu com rc=1, pelas causas registradas lá.
