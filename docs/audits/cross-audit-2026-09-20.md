# Cross-audit — o que o juiz não mede

> **Escopo**: commits `07cc5278` e `e7f47c1d` (21 arquivos `.rs`), mais o trabalho
> da sessão de 20-21/09/2026.
> **Método**: 7 fases da skill `TACO-cross-audit` + painel de 3 críticos cegos em
> sessão fresca, com lentes distintas (correção · reprodução · destrutividade).
> **Resultado**: **11 achados**, dos quais 9 corrigidos e provados por mutação.

## O fato central

O juiz (`loop_converged.py`) devolveu **exit 0, 8/8 cláusulas, Platinum 0,93845**
sobre este código. A auditoria encontrou onze defeitos nele.

Não há contradição: as oito cláusulas medem *compila*, *passa*, *não regride*,
*não cria órfão*. **Nenhum dos onze defeitos quebra qualquer uma delas.** Um
fechador que procura a chave errada não falha teste nenhum — ele silenciosamente
não faz o que promete. É a distinção que a skill nomeia entre *"does it crash?"*
e *"does it do what its purpose says?"*, e aqui ela separou um verde legítimo de
um sistema que não cumpria o próprio texto.

## O padrão que atravessa nove dos onze

**Em quase todos, o comentário correto estava ao lado do código que não o cumpria.**

| arquivo | o que o comentário dizia | o que o código fazia |
|---|---|---|
| `hook_registry.rs:873` | "the stage list is NOT spelled here — `close_scaffold_stages` walks the same `MIRROR_SCAFFOLD_STAGES`" | passava o id **cru**, duas linhas abaixo |
| `handlers/decompose.rs` | "this reconciles, it does not guess" | fechava trabalho `in_progress` e reivindicado |
| `task_lifecycle.rs:16` | "a fourth stage is added HERE and every consumer follows" | quatro sítios não seguiam |
| `doc_symbol_signal.rs` | "apagar a raiz de um processo vivo trocaria um vazamento por um flaky" | sem `/proc`, apagava as vivas |

Prosa correta ao lado de código incorreto passa em qualquer revisão que leia a
prosa — inclusive na do autor. Só um leitor adversarial com o código na mão pega.

## Os achados

### Corrigidos e provados

| # | defeito | evidência | correção |
|---|---|---|---|
| 1 | `close_scaffold_stages` contava a ausência de `"error"`; o handler sinaliza falta com `"subtask_updated": false` → retornava 3 tendo escrito 0 | teste novo: task sem scaffold ⇒ 0 | lê a afirmação positiva; resposta não parseável conta como não escrita |
| 2 | fechador usava id **cru** onde o scaffolder grava `cc_task_N` — a correção (c) de `07cc5278` nunca funcionou no caminho do hook | leitura direta; e2e reancorado no id cru | normaliza **dentro** da função: todo chamador correto por construção |
| 3 | a lista de estágios vivia em **4 sítios**, declarada "fonte única" com 2 corrigidos | `task_create`, `task_stop`, `hook_registry`×2 | os 4 derivam de `MIRROR_SCAFFOLD_STAGES`; cancelamento usa o mesmo fechador |
| 4 | `reconcile-stages` fecharia trabalho `in_progress` e sob lease ativa — a cadeia é linear, então "sem irmão aberto" é o **último da fila**, o que está executando | 5 `in_progress` vivos, 3 com claim; dos 5, **4 não são estágios de scaffold** | 4 guardas: só estágio do espelho · nunca `in_progress` · nunca sob claim ativa · exige irmão **terminal** |
| 5 | varredura do `/tmp` apagava por **nome**; `/proc` ausente ⇒ todas as raízes "órfãs", inclusive vivas | crítico provou: `touring_doc_symbol_export_2026` (sufixo é uma data) apagado | marcador `boot_id\|pid\|starttime`; `symlink_metadata`; `file_type` sem seguir link; sem `/proc`, não apaga; falhas reportadas |
| 6 | `a_failed_task_marks_every_stage_failed` percorria a **própria constante** do código — remover um estágio dela deixava o teste verde | mutação M6 do crítico | literais + guard que detecta a constante encolhendo |
| 7 | 4 sítios resolviam o banco pelo **cwd** sem walk-up; a escrita **criava** banco novo | `crates/.claude/touring/memory.db` com 5 entradas ao lado do real com 16.700 | `normalize_project_root` — o resolvedor **já existia**, ninguém perguntava a ele |
| 8 | timeout do provedor vazava thread e processo **dentro do daemon**: `drop(JoinHandle)` desanexa, `kill()` não alcança netos | daemon vive dias; um por timeout, sem teto nem log | `process_group(0)` + `killpg`: o neto morre, o pipe fecha, a thread termina |
| 9 | `index rebuild --wait` mentia: relógio só lido após o `sleep` (`--poll-secs 600 --wait-timeout-secs 60` esperava 600s), e payload sem `state` ⇒ **exit 0 sem ter esperado** | leitura direta | `EstadoGeracao` de 3 estados — "não sei" vira erro; sono respeita o orçamento restante |
| 10 | `similar_snippets` **circular**: corpo só persistido na 2ª execução, e o `INNER JOIN` só vê quem tem corpo — o mecanismo que existe para provocar a 2ª execução não enxergava quem rodou uma vez | **363 de 382 (95%)** sem corpo; vão cresceu 361→363 durante a auditoria | persistência na 1ª execução + teto no SQL; promoção na escada segue exigindo repetição |
| 11 | **`ladder_totals` sem teste nenhum**: o `- 1` é o único lugar onde "enrolar não é reusar" existe como código | mutação passava com **1911 testes verdes** e o script de auditoria dizendo `ok`; numerador iria de 10 → 390 e cruzaria o piso 0,20 sem um reuso a mais | teste ancorado no **banco**; mutação agora falha (`left: 4`) |

### Abertos

| # | achado | por que fica aberto |
|---|---|---|
| A | `audit-plan-completion.sh` é **prova vazia**: 29 `grep -q` + 2 `test -f`, nenhuma verificação executa código. Prova que o *texto* existe no disco | é a cláusula `cross_audit` do juiz — trocá-la por verificação executável é trabalho próprio, e o autor do script é o autor do código que ele verifica |
| B | `ProviderOutcome` não tem consumidor fora do arquivo: zero `tracing`, zero counter. ETXTBSY permanente, provedor quebrado e documento sem símbolos produzem saída **byte a byte idêntica** | o retry de hoje reduziu a frequência do vazio sem abrir o canal de diagnóstico que o próprio docstring diz ser o problema |
| C | `F1.2 Maintainability` **Fail pré-existente** em `handlers/decompose.rs` (2305→2417) e `kpi.rs` (2847→2931) | dívida que não criei mas agravei; divisão autorizada, ainda não executada |
| D | ciclo de dependência com **profundidade 1235** em `touring-resilience` | fora do escopo dos 21 arquivos; pré-existente |

## Testes que defendiam o defeito

Três, e cada um passava *porque* o defeito existia:

1. `an_unreadable_status_ends_the_wait_instead_of_looping_forever` — afirmava que
   payload ilegível "encerra a espera", que é precisamente o exit 0 sem espera.
2. O e2e do fechador passava `"cc_task_42"` — **corrigia na entrada do teste** o
   que o código deveria normalizar.
3. `enrolling_a_body_is_not_reusing_it` construía `LadderTotals` à mão: provava a
   aritmética do consumidor, nunca a do produtor.

## Sobre o método: cinco sondas minhas reportaram verde sem medir

Registrado porque é o achado mais transferível da sessão.

| sonda | o verde falso | causa |
|---|---|---|
| taxa do flaky | "0 falhas em 300" | glob estourou **E2BIG** sobre milhares de `.dwo`; `BIN=` vazio |
| o guard disso | abortou binário sadio | `tail -1` lia a linha **em branco** final do libtest |
| monitor de memória | 30 min, zero eventos | `pgrep -f` casava **o próprio shell** do monitor |
| prova por mutação | etapas "ok", nada rodou | `2>&1 \| grep "^test result:"` transformou erro de compilação em silêncio |
| prova por mutação (2ª) | script de **0 bytes** | `cat > A \|\| cat > B <<'EOF'` — o primeiro `cat` consumiu o heredoc |

Medi o sistema com rigor e aceitei os instrumentos no fiado. A regra que faltava
é simples: **toda sonda começa com prova de vida que casa conteúdo, não posição,
e aborta com exit ≠ 0 quando ela falha.** Silêncio nunca vira zero.

## Método que funcionou

**O painel cego pagou por si.** Dos 11 achados, 6 vieram dos críticos, e 3 deles
eu havia declarado resolvidos. A instrução decisiva foi dar-lhes **o código e os
commits, nunca minhas conclusões** — um crítico ancorado na hipótese do autor a
devolve confirmada.

O crítico de segurança encontrou **seis furos na minha própria correção** do
achado 5, incluindo um que eu *introduzi ao corrigir*: `is_file()` usa
`metadata()`, que segue symlink, reintroduzindo o vetor que a versão anterior,
medida, não tinha.

## Proveniência

- Suítes: `touring-hooks-shared` 471 passed · `touring-intelligence` 1534 passed ·
  `dag_lifecycle_closure_e2e` 11 passed · `touring-server` 1636 passed
- Mutações executadas: contrato do fechador · id cru · estágio removido da
  constante · `- 1` do agregado (todas vermelhas sob mutação, verdes restauradas)
- Gates P0 (F2.1/F2.4/F2.6/F4.3): **Pass/Diamond** nos 4 arquivos medidos;
  F2.5 e F4.5 `NotApplicable` — e N/A não é aprovação
- E2E do sistema: `overall_score` 0,859, status pass
- DAG real: `reconcile-stages` fechou 75, poupou os 2 com irmão aberto;
  `archive` 2/2; 112 terminais, 112 arquivadas, zero sem carimbo
