# Code Mode — manual do usuário (touring run / orchestrate / snippets)

> Wave 2026-08-23 "Code Mode Máximo" (bundle `docs/plans/2026-08-23-code-mode-best-practices/`).
> Princípio (5 fontes: Cloudflare, TanStack ×2, deepseek-harness, tanstack/ai): o modelo
> escreve código; o código fala com o sistema. N round-trips → 1 execução; intermediários
> morrem no sandbox; só o agregado volta ao contexto.

## Camadas de execução (E0–E5)

O Code Mode é estratificado em 6 camadas — da execução atômica à campanha e ao
factory — com a mesma lei em todas: **quem encerra o loop é código** (L2) e todo
sinal é lido **fail-closed**. Mapa completo, contratos (`NEW_FINDINGS=`,
`METRIC=`) e regra de escolha de camada: [code-mode-execution-layers.md](code-mode-execution-layers.md).
Destaque novo (2026-08-24): `touring adw campaign <flow> --until "<predicado>"`
— o loop de fluxo com curva registrada e feromônio por rodada.

## `touring run` — o canal de code mode sem MCP

```bash
touring run --lang python --code '<programa>'        # 12 linguagens (python, js/ts, bash, go, rust…)
touring run --lang python --file script.py --brief   # digest C5 < ~200 tokens
touring run --lang python --orchestrate --code '…'   # injeta o SDK touring.* (abaixo)
touring run --lang js --orchestrate --code '…'       # SDK-1: o MESMO SDK em JS (Promise; node/bun/deno)
touring run --sdk-stub                               # imprime o contrato tipado (.pyi, byte-estável)
touring run --lang bash --stream --code '…'          # OUT-1: saída espelhada no stderr CONFORME chega
touring run --lang python --input dados.json …       # QW-4: bytes do arquivo no stdin do programa
touring run --lang python --allow-net-port 443 …     # NET-1: TCP de saída só nessa porta (Landlock)
touring sandbox-runtimes status                      # RUN-1: preflight das 11 linguagens + venv
```

### Aliases tipados e fan-out no SDK (M0 paralelização-agentes, 29/08/2026)

- `query(hook, payload)` e `parallel([(hook, payload), …])` aceitam os **nomes
  tipados** como hook (`memory_recall`, `index_find`, `ast_blast`, …): uma tabela
  única (`SDK_HOOK_ALIASES`, `touring-foundation::orchestrate_allowlist`) alimenta
  os DOIS preludes (py/js) **e** o resolve no daemon — que só resolve para
  `origin` de sandbox (fora dele o alias não existe). Alias inválido erra
  **ensinando** a allowlist (A5); hook mutante nunca passa (allowlist read-only).
- Toda sub-chamada emitida por `touring.parallel` carimba `:par` no origin
  (`<run_id>:code:<n>:par` em `run_subcalls.jsonl`) — a adoção do fan-out é
  medida por `touring.code_mode.parallel_runs` (`touring kpi -j`): distinct runs
  com o carimbo, STUB até o primeiro sinal (zero nunca é inferido).

### O que volta (contrato do resultado)

| Campo | Semântica |
|---|---|
| `stdout` / `stderr` | os DOIS canais capturados (stderr real desde 2026-08-23 — antes era engolido) |
| `exit_code` | do programa; `-2` = sentinela de timeout |
| `failure` | `{kind, phase, message}` — taxonomia ortogonal: `exception \| timeout \| abort \| proc-exit \| invalid-output \| output-limit`; a `message` ensina a correção |
| `stored_path` + `retrieval_hint` | quando a visão inline foi cortada, o output COMPLETO está em disco: `Read <path> --offset/--limit` ou `grep <pattern> <path>` |
| `stdout_truncated`/`stderr_truncated` | honestos nas DUAS fronteiras (cap do sandbox E cap inline) |
| `harvest_hint` | quando o programa é generalizável (≥5 linhas, parametrizado, sem identificadores efêmeros): oferece re-rodá-lo com `--harvest <slug>`. Some quando o run já colheu |
| `snippet_trust` | badge + nível da escada (`○ untrusted` · `◐ provisional` · `✓ trusted`) quando o corpo executado é um snippet colhido — presente tanto na colheita quanto em todo reuso |
| `snippet_bindings` | sob `--orchestrate`: `available` (funções `snippet_*` que o programa tinha), `omitted_over_cap` (o que o teto cortou, nomeado) e `error` quando a biblioteca está inconsistente |

### Budgets (dois, independentes)

- **Wall clock**: `--timeout-ms` (default 30000, max 600000).
- **Busy time (CPU)**: `--compute-ms` (default 60000) medido do `/proc/<pid>/stat` do próprio
  child — um hot loop expira o budget mesmo com wall folgado; espera por I/O não consome.
  A mensagem do timeout distingue as duas causas.
- **Salvage (OUT-1, 28/08)**: o sentinela `-2` carrega a saída PARCIAL — o que o programa
  imprimiu antes do kill viaja em `stdout` (hash+spill inclusos) e o stderr do filho lidera
  a mensagem-ensino. Antes o timeout chegava de mãos vazias e o retry aprendia nada.

Caps de saída inline: `TOURING_RUN_MAX_STDOUT_BYTES` (8 KiB) / `TOURING_RUN_MAX_STDERR_BYTES`
(4 KiB), com truncamento head/tail e marcador explícito de elisão; o cap do sandbox (1 MiB) é
falha nomeada `output-limit`, nunca corte silencioso.

### CEG

Todo `touring run` atravessa o gateway X0..X7 antes de executar (perfil `sandboxed`; `trusted`
sob `--allow-forbidden`). `Deny` aborta com a razão e a rota correta; erro interno do gateway é
fail-open (o gate nunca brica a sessão).

**O waiver do shell é SELETIVO (27/08/2026).** Um `Deny` de shell cuja ÚNICA classe negada é
`subprocess` vira advisory e o comando roda; qualquer outra classe — rede acima de tudo — e
qualquer bloqueio destrutivo do X2 negam de verdade, exatamente como nas linguagens de código.

A assimetria não é preferência, é o que a contenção do sandbox sustenta, medido:

| recurso | contido pelo sandbox? | prova | política |
|---|---|---|---|
| filesystem | **sim** | `touch ~/.ssh/x` falha; `touch <workspace>/x` funciona | `subprocess` é waivable |
| rede | **não** (Landlock é FS-only) | `curl https://example.com` devolvia HTTP 200 sob o waiver cego | `network` nega duro |

Antes disso o waiver era cego: `curl` era X6-negado como `network`, rebaixado, e executava —
enquanto o `socket` equivalente em Python era corretamente recusado. Mesma capability, mesmo
veredito, destinos opostos decididos pela linguagem.

Os builtins do shell (`echo`, `cd`, `for`, `test`, …) deixaram de exigir grant de subprocess:
antes toda palavra virava `Capability::Run`, então `echo oi` negava com o MESMO composite
(0.675) que `curl http://evil.test`. Um gate que dispara em 100% dos runs legítimos não carrega
informação — e era esse ruído que justificava o waiver cego. `eval`/`exec`/`source` continuam
exigindo grant: executam texto como código.

Hoje, de 10 comandos benignos medidos, **0** emitem advisory.

## `--orchestrate` — o SDK `touring.*` dentro do sandbox

Um script Python consulta o daemon em UMA execução (code-mode sem MCP):

```python
hits = touring.tantivy_search("run_gateway")           # BM25
sym  = touring.index_find("SandboxResult")             # VGP
tree = touring.ast_blast("crates/touring-ceg/src/gateway/sandbox_executor.rs")
imp  = touring.wiring_impact("spawn_and_capture", 2)    # blast transitivo
mem  = touring.memory_recall("code mode #kind:lesson")  # memória facetada
```

**71 hooks de leitura alcançáveis** (S4, 27/08/2026), com 16 atalhos tipados para os mais
usados: `query · parallel · ast_blast · ast_meta · ast_overview · ast_tdg · doctor ·
find_references · gotcha_match · index_find · memory_recall · search · tantivy_search ·
wiring_impact · wiring_orphans · wiring_status`. Tudo o mais chega por
`touring.query(hook, payload)`.

**`parallel` (SDK-1, 28/08)** — fan-out de leituras independentes com pool limitado a 10
(dsh `maxParallelSubCalls`): `touring.parallel([(hook, payload), …])` devolve os resultados
NA ORDEM; um slot que falha vira `{'parallel_error': …}` sem anular os N−1 restantes.
Medido ao vivo: 6 `index_find` em 54 ms sequencial → 15 ms paralelo (3,76×). O contador
`origin` é mintado sob lock — `parallel` chama `query` de worker threads e uma sequência
duplicada subcontaria o d4.

**SDK JS (SDK-1, 28/08)** — `--lang js|node|bun|ts --orchestrate` injeta o MESMO SDK como
cliente Promise-based (`node:net` via dynamic import — funciona em node -e, bun -e e deno
eval). Gerado das MESMAS tabelas `SDK_METHODS`/`READONLY_HOOKS` que o Python e o stub (D8:
uma fonte, três renderizações — o guard `js_sdk_mirrors_the_same_method_table` prova).
Todo método devolve Promise: `await touring.index_find("X")` dentro de
`(async () => { … })()`.

Eram 8 hooks escritos à mão dentro da string Python, de ~195 que o daemon registra: um programa
no sandbox alcançava 8 leituras e voltava ao modelo para todo o resto — o que anula o ganho do
code mode. Pior, o stub que ANUNCIA a superfície era uma segunda const escrita à mão ao lado,
livre para divergir da allowlist que IMPÕE.

Hoje `READONLY_HOOKS` + `SDK_METHODS` são a fonte única, e o SDK e o stub são GERADOS dela —
inclusive a docstring de `query()`. Quatro guards fecham o contrato: todo hook existe no
registry REAL do daemon (`all_daemon_hook_names()`, não texto), todo atalho aponta para hook
allowlistado, stub e SDK declaram os mesmos métodos com as mesmas docstrings, e nenhum verbo
mutante entra na lista.

A curadoria é por PROPÓSITO, nunca por padrão de nome: um filtro sobre verbos mutantes deixava
passar `cli-gotcha-add`, `cli-jobs-spawn` e `cli-saga-begin` — ausência de palavra perigosa não
é prova de segurança. Fora deliberadamente: `cli-ast-grep` (tem modo `--rewrite`) e
`cli-wiring-suggest`.

Contrato completo tipado: `touring run --lang python --sdk-stub` (byte-estável — cabe em prompt
cache). Postura: contenção, não fronteira de segurança (o proxy server-side é follow-up
registrado).

## Snippets com confiança MEDIDA

Código que funcionou vira memória reutilizável — e a confiança é conquistada por outcome,
nunca declarada (fecha os buracos do TanStack: lá o trust é decorativo, sem rebaixamento e
sem invalidação):

- **Registrar** — `touring run … --harvest <slug>`: o **executor** persiste o corpo como
  memória `#kind:snippet` (tags `#lang:<lang>` e `#process:code-mode` derivadas dele) sob a
  chave canônica `snippet:<slug>` e o matricula na escada, em um comando só.
- **Medir** — a partir daí é automático: **todo run do mesmo corpo** é reidentificado pelo
  digest (`code_sig`) e registra seu próprio outcome; o resultado devolve `snippet_trust`
  (badge + nível) ao modelo. A escada é `untrusted ○ → provisional ◐ (10+ exec, ≥90%) →
  trusted ✓ (100+, ≥95%)` com **rebaixamento** (≥5 falhas nas últimas 10) e **invalidação**
  quando o corpo muda. `touring learning reward "snippet:<key>" <valor>` segue valendo como
  a via manual. Implementação: `touring-intelligence/src/rl/memory/snippet_stats.rs`.
- **Consultar**: `touring memory query "#kind:snippet #lang:python"`.
- **Compor** — sob `--orchestrate`, todo snippet Python **≥ provisional** vira uma função
  `snippet_<slug>(*args)` do programa, que devolve o stdout do snippet (os `*args` chegam
  como `sys.argv`). O payload lista o que ficou disponível em `snippet_bindings.available`.
  Um snippet pode chamar outro — a composição funciona.

### Bindings `snippet_*` — as quatro recusas (W3b)

Antes de injetar qualquer coisa, o conjunto é validado. Uma biblioteca inconsistente
**não derruba o run** (o programa pode nem usar snippets): os bindings simplesmente não
entram, e `snippet_bindings.error` diz o que consertar.

| Recusa | Quando | Por quê |
|---|---|---|
| `Cycle` | `a` chama `b` que chama `a` | em runtime isso recorre até estourar a pilha; o erro traz o **caminho** do ciclo |
| `NameCollision` | `foo-bar` e `foo.bar` | sanitizam para o mesmo identificador; deixar o último vencer é o bug silencioso |
| `EmbeddedSecret` | `client_secret = "<literal>"` no corpo | um binding viaja em **toda** execução orquestrada; ler do ambiente nunca é acusado |
| `MultilineLiteral` | literal de string cruzando linhas | virar binding é ser indentado numa função, e indentar mudaria o texto do literal |

Teto de `MAX_BINDINGS = 20`, ordenado por confiança e uso; o que sobra é **nomeado** em
`snippet_bindings.omitted_over_cap`, nunca cortado em silêncio.

> **Sem primitiva de avaliação dinâmica**: cada snippet é emitido como função Python real.
> O scanner de forbidden calls do sandbox lê o preâmbulo como leria código do usuário — um
> `exec` ali dispararia `[CEG WARNING] … code injection` em **todo** run orquestrado (e sob
> `TOURING_CEG_FORBIDDEN_ENFORCE=1` bloquearia o run). Um aviso que aparece sempre é um
> aviso que ninguém lê. Como bônus, um corpo real enxerga os globais do módulo — que é
> justamente o que faz a composição funcionar.

> **Por que o executor e não o hint (W3b, 24/08/2026)**: até aqui o `harvest_hint`
> *pedia* ao modelo que rodasse um `memory store` com um slug livre — e o predicado que
> alimenta a escada só reconhecia chaves `snippet:*`. Resultado medido: a tabela
> `snippet_stats` **nunca havia sido criada**, com zero execuções em produção. Duas
> rupturas, uma só causa: o extrator e o verificador não vinham da mesma fonte. Hoje
> `harvest_key`/`is_snippet_key` são esse par único, e quem popula é o executor — a
> afordância, não o pedido (`rules/touring-4-pillars.md`, D8).

## Observabilidade

- Journal por execução: `~/.claude/touring/run_journal.jsonl`
  (`{ts, run_id, language, exit_code, duration_ms, failure_kind, bytes_elided}`).
- **Identidade e contrafactual das sub-chamadas (C2-W0, 2026-08-24)**: todo run
  ganha `run_id` (`run-<epoch_ms>-<pid>`, também no campo `runId` do resultado);
  o child o recebe como `TOURING_RUN_ID` e o SDK do `--orchestrate` carimba cada
  sub-chamada `origin: <run_id>:code:<n>`. O daemon registra cada uma em
  `~/.claude/touring/run_subcalls.jsonl` e soma o custo contrafactual: cada
  sub-chamada é exatamente 1 tool call MCP que NÃO passou pelo contexto.
- **Par start/settle (S5, 27/08/2026)**: cada sub-chamada grava DOIS registros —
  `{phase:"start", ts, origin, hook, payload_bytes}` antes do despacho e
  `{phase:"settle", …, output_bytes, duration_ms, ok}` depois. Casá-los pelo
  `origin` deixa órfãos que são exatamente as sub-chamadas que NÃO voltaram
  (hook travado, daemon morto no meio). Sem o `start`, "não voltou" e "nunca
  aconteceu" produzem o mesmo journal — a ambiguidade que já fez este workspace
  ler "parou de escrever" como "terminou".
- **Sem conteúdo, por decisão**: tamanhos, latência e desfecho; nunca o dado. O
  item de plano do S5 pedia também um *preview* do output — recusado: preview é
  conteúdo, e uma `memory_recall` ou um corpo de símbolo pode carregar segredo.
  Latência e desfecho respondem o que o preview queria responder (qual hook é
  caro, qual falhou) sem transformar um arquivo de observabilidade numa
  superfície de exfiltração. Guardado por teste estrutural.
- Counters: `touring gate-metrics -j` → `code_mode_runs_count`,
  `code_mode_bytes_elided_total`, `code_mode_subcalls_count`,
  `code_mode_subcall_bytes_total` (economia MEDIDA de contexto — nunca estimada;
  os `subcall_*` agregam no daemon, que vive além do processo CLI).

## MCP code-first

Um escopo que declara `[code_mode] mode = "code"` no seu `.touring/touring.toml` reduz o
handshake a 3 tools (`touring_search`, `touring_ctx_execute`, `touring_memory_recall`) — o par
search+execute da Cloudflare + memória. As demais seguem invocáveis por nome via `tools/call`:
só o ANÚNCIO estreita.

Precedência: `TOURING_MCP_ALL_TOOLS` (lista tudo) > `TOURING_MCP_CODE_MODE=1` força / `=0`
desliga (kill switch por sessão) > a declaração do escopo > a superfície curada (~23).

Até 27/08/2026 a fachada existia mas exigia a env var em CADA sessão, então um projeto que já
declarava `code` seguia recebendo as ~23 curadas — afordância declarada e desligada, quebrando
em silêncio. O tipo e o parser da declaração são ÚNICOS
(`touring_foundation::code_mode`), consumidos pelos DOIS executores que a impõem: o hook
`PreToolUse` e o handshake MCP. Duplicá-los era o caminho para hook e handshake divergirem.

## Diretrizes de elaboração de código (E/A/M — 28/08/2026)

Como ESCOLHER a rota, ACERTAR o programa e MEDIR a aderência. Corpo completo, fontes e
racional: `docs/plans/2026-08-27-code-mode-aderencia-sandbox/strategy-2026-08-27-code-mode-aderencia-sandbox.md`.

| Grupo | Diretriz (condensada) |
|---|---|
| **E — Escolher** | E1 uma ferramenta de execução + API tipada no prompt (nunca o catálogo). E2 regra de custo declarada: loop/condicional/agregação → programa; op simples → tool direta. E3 o colapso mora no EXECUTOR e o deny nomeia a rota (D8). E4 transport nomeia TODOS os args obrigatórios. E5 progressive disclosure para catálogo grande. E6 code é objetivamente mais barato (150k→2k tokens; loops 11-15×). |
| **A — Acertar** | A1 SDK plana (nunca fluent/OO). A2 um objeto de config nomeado. A3 stub tipado com doc densa. A4 retornos JSON canônicos tipados. A5 erros estruturados, taxonomia ortogonal. A6 output-limit explícito, nunca corte mudo. A7 concorrência declarada (read-only sobrepõem — `touring.parallel`, pool 10; mutantes correm sós). A8 exemplo canônico completo no prompt. A9 prompt byte-estável (KV-cache). A10/A14 retry com autocorreção, MÁX 3 tentativas — na 4ª muda de estratégia, nunca repete o mesmo corpo. A11 trust paritário ao bash; segredos NUNCA no sandbox. A12 fail-LOUD em linguagem desconhecida. A13 scripts bons viram ativos (escada ≥10@90% → ≥100@95%). |
| **M — Medir** | M1 régua dedicada: `touring kpi -j` → `code_mode_adherence` (success_rate, wasted_attempts_retry_pairs, by_failure_kind/language, do run_journal) **+ `touring.code_mode.inspect_burst_share`** (28/08 — fração da inspeção atômica que vira rajada negada pelo S3; advisory lte 0.75; era braço `derived:` órfão sem commitment desde 27/08). M2 aderência é modelo × apresentação — abaixo do piso de capacidade nenhum prompt salva. M3 contenção determinística no substrato (não evitável por código malicioso). |

## Divergências conscientes do harness DeepSeek (dsh)

O dsh é a referência mais próxima do que fazemos, e o postmortem dele de 07/08/2026 é a fonte
da tese central: *"schema omission is not enforcement when a direct caller can bypass it;
denial must be tested through the executor"*. Onde divergimos, é por medição — e vale registrar
por quê, para que a próxima leitura do dsh não seja tomada como correção pendente.

| tema | dsh | touring | por quê |
|---|---|---|---|
| onde o colapso mora | apresentação no registry (`UNKNOWN_TOOL` estrutural, pré-pipeline) | no executor (`PreToolUse`) **e** no registry (handshake MCP) | não mandamos no wire da Anthropic; o executor é a metade que impõe, e o registry é a que ensina |
| granularidade do colapso | por classe de ferramenta | por RAJADA, não por classe | medido em 115 transcripts: 22,5% da inspeção é isolada, e cobrar dela é taxar o caso comum — a recusa que o próprio dsh documenta |
| níveis por classe | ADIADO (*"depends on evidence about how models split usage under `both`"*) | implementado | a evidência que lhes faltava nós temos: rodamos em `both` instrumentado e medimos |
| trust do canal code | bash-equivalent por design | idem, com uma exceção medida | a rota code nunca é mais assustadora que a bash, EXCETO onde a contenção não existe (rede) |
| rewrite do comando | — | mantido (`updatedInput` no transcript) | o G8 reescreve o laço em vez de negá-lo; um remédio que executa é seguido, um que exorta não é |

## Higiene de KV-cache dos hooks

`python3 scripts/kv_cache_audit.py [--assert-stable]` roda cada hook registrado 2× com payload
congelado e acusa qualquer byte instável (medido 2026-08-23: 10/10 estáveis após a
bucketização do session_startup). Auditoria de erros agent-first:
`python3 scripts/error_message_audit.py` (baseline teach-ratio 0.339 — campanha W8b).
