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
touring run --sdk-stub                               # imprime o contrato tipado (.pyi, byte-estável)
```

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

- **Wall clock**: `--timeout-ms` (default 30000, max 120000).
- **Busy time (CPU)**: `compute_ms` (default 60000) medido do `/proc/<pid>/stat` do próprio
  child — um hot loop expira o budget mesmo com wall folgado; espera por I/O não consome.
  A mensagem do timeout distingue as duas causas.

Caps de saída inline: `TOURING_RUN_MAX_STDOUT_BYTES` (8 KiB) / `TOURING_RUN_MAX_STDERR_BYTES`
(4 KiB), com truncamento head/tail e marcador explícito de elisão; o cap do sandbox (1 MiB) é
falha nomeada `output-limit`, nunca corte silencioso.

### CEG

Todo `touring run` atravessa o gateway X0..X7 antes de executar (perfil `sandboxed`; `trusted`
sob `--allow-forbidden`). `Deny` aborta com a razão e a rota correta; erro interno do gateway é
fail-open (o gate nunca brica a sessão).

## `--orchestrate` — o SDK `touring.*` dentro do sandbox

Um script Python consulta o daemon em UMA execução (code-mode sem MCP):

```python
hits = touring.tantivy_search("run_gateway")           # BM25
sym  = touring.index_find("SandboxResult")             # VGP
tree = touring.ast_blast("crates/touring-ceg/src/gateway/sandbox_executor.rs")
imp  = touring.wiring_impact("spawn_and_capture", 2)    # blast transitivo
mem  = touring.memory_recall("code mode #kind:lesson")  # memória facetada
```

9 métodos: `query · index_find · ast_blast · ast_overview · wiring_status · search ·
memory_recall · tantivy_search · wiring_impact`. Hooks fora da allowlist read-only recusam com
a lista do que existe. Contrato completo tipado: `touring run --sdk-stub` (byte-estável — cabe
em prompt cache). Postura: contenção, não fronteira de segurança (o proxy server-side é
follow-up registrado).

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
  `~/.claude/touring/run_subcalls.jsonl` (`{ts, origin, hook, payload_bytes,
  output_bytes}` — tamanhos, nunca conteúdo) e soma o custo contrafactual: cada
  sub-chamada é exatamente 1 tool call MCP que NÃO passou pelo contexto.
- Counters: `touring gate-metrics -j` → `code_mode_runs_count`,
  `code_mode_bytes_elided_total`, `code_mode_subcalls_count`,
  `code_mode_subcall_bytes_total` (economia MEDIDA de contexto — nunca estimada;
  os `subcall_*` agregam no daemon, que vive além do processo CLI).

## MCP code-first

`TOURING_MCP_CODE_MODE=1` reduz o handshake a 3 tools (`touring_search`,
`touring_ctx_execute`, `touring_memory_recall`) — o par search+execute da Cloudflare +
memória; as demais tools seguem invocáveis por nome. `TOURING_MCP_ALL_TOOLS` mantém
precedência como escape hatch.

## Higiene de KV-cache dos hooks

`python3 scripts/kv_cache_audit.py [--assert-stable]` roda cada hook registrado 2× com payload
congelado e acusa qualquer byte instável (medido 2026-08-23: 10/10 estáveis após a
bucketização do session_startup). Auditoria de erros agent-first:
`python3 scripts/error_message_audit.py` (baseline teach-ratio 0.339 — campanha W8b).
