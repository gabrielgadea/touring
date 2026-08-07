---
type: Strategy
title: Recuperação de contexto nos loops + avaliação da memória do Touring
description: O checkpointer era write-only e sua chave era irrecuperável por construção. Fixes R1-R3 e avaliação medida do uso da memória para registro e recuperação.
plan_id: 2026-08-02-loop-memory-recovery
tags: [loop, memory, recall, compaction, checkpointer]
timestamp: 2026-08-02T14:45:00-03:00
okf_version: "0.1"
---

# Recuperação de contexto nos loops + avaliação da memória

Parte do [bundle](/index.md). Antecedentes:
[modus operandi padrão](/../2026-08-02-loop-default-modus-operandi/strategy-2026-08-02-loop-default-modus-operandi.md) ·
[isolamento por sessão](/../2026-08-02-multi-session-loop-isolation/strategy-2026-08-02-multi-session-loop-isolation.md).

## Parte 1 — os três defeitos da recuperação (corrigidos)

### R1 — o checkpointer era write-only

`loop_snapshot` gravava no PreCompact e **nada jamais lia de volta**: grep por
leitores em `~/.claude/hooks/*.py|*.sh` e nos crates Rust retornou apenas o
leitor de KPI do `compliance.jsonl` e comentários. A retomada existia só como
prosa na SKILL.md — **persuasão**, exatamente o mecanismo que o diagnóstico de
23/07 provou não sobreviver a uma compactação (causa C4, *prior-zero
pós-compactação*). Resultado: o marker seguia **cobrando** um loop de que o
próximo contexto não tinha lembrança.

**Fix**: `loop_resume.py`, registrado em `SessionStart` + `PostCompact`.
Decisão de projeto — **evidência viva vence snapshot armazenado**: marker OUTER
é resolvido contra `loop_outer_gate` (artefatos em disco, recomputados agora) e
loop ativo contra `touring decompose get` (o DAG autoritativo); o snapshot em
memória entra como *enriquecimento*. Assim a injeção nunca pode afirmar um
"missing" obsoleto.

### R2 — a chave do snapshot era irrecuperável por construção

`loop_snapshot.py:57` usava `abs(hash(cwd)) % 10**8`. Python randomiza o hash de
`str` por processo (PYTHONHASHSEED) — medido, mesmo cwd:

```
86221029  →  375623  →  47015023
```

Cada compactação gravava sob chave nova e aleatória. Violação direta da
**REGRA #17** (id é derivação determinística, nunca emergente).

**Fix**: `loop_marker.state_key(marker)` — sha1 sobre o par canônico
(projeto, sessão), uma única definição usada pelo escritor e pelo leitor.
Verificado: três processos → `flow-state:strategy-outer:43224dc4d9af-242fb47a`
idêntico.

### R3 — a perna do `log.md` nunca engatava

`loop_snapshot` só faz append `if log.exists()` e **nada nunca criava o
arquivo** — nenhum bundle em disco tinha `log.md`. A perna humana/diffável da
"redundância tripla" era código morto.

**Fix**: `loop_diagnose.ensure_bundle_log` cria o log no nascimento do bundle
(nunca sobrescreve). Um único criador; o append passa a valer.

### Bug meu, pego pelo teste novo

`loop_resume` chamava o gate **sem** `--marker`, então o gate re-resolvia pelo
cwd do processo em vez do marker já resolvido — divergência sempre que o cwd do
payload difere do cwd do processo. O `marker_path` agora é passado adiante.

**Gates**: flow guard **45** (4 novos), e2e **18/18** (2 novos), ADW/explore/
scout/factory **76**.

## Parte 2 — avaliação medida da memória do Touring

Fonte: `.claude/touring/memory.db` (SQL direto) + `touring memory stats`.

### Registro (o lado que funciona)

| Métrica | Valor |
| --- | --- |
| entradas | **6.921** |
| tiers | reference 3.994 · semantic 2.410 · working 511 · ephemeral 5 · core 1 |
| entry_type | transcript_lesson 3.448 · insight 1.754 · lesson 1.147 · text 511 |

Namespaces por volume: `outcome:` **50,2 %**, `lesson:` 9,0 %, `vgp:` 8,0 %,
sem-namespace 5,8 %, `create:` 3,1 %. **58,4 % do acervo é gerado
automaticamente** (`outcome:`/`vgp:`/`create-script:`/`subtask:`); 41,6 % é
potencialmente curado.

### Recuperação (onde está o problema)

| Métrica | Valor |
| --- | --- |
| nunca recuperadas | 1.382 / 6.921 = **20,0 %** |
| p50 = p80 = p90 de acessos | **1** |
| 5 % mais acessadas | concentram **50,8 %** de todas as recuperações |
| média — entradas **curadas** | **0,88** |
| média — entradas **automáticas** | **2,11** (**2,4×** mais) |

As **8 entradas mais recuperadas do acervo inteiro são todas ruído automático**,
lideradas por `outcome:edit:transcript-e57e3c84:failure` com **1.158**
recuperações. Ou seja: o canal de recall está dominado por transcrições de
falha auto-geradas, que **espremem para fora** as lições curadas — o feromônio
ACO está diluído 2,4:1 a favor do ruído.

> Ressalva honesta: `access_count` provavelmente incrementa quando a entrada é
> *devolvida num result set*, não quando é *usada*. Isso mede **participação na
> recuperação** — que é exatamente o que importa para poluição de contexto —,
> não utilidade comprovada.

### Causa estrutural: 21 % do acervo está fora do corpus ANN

`memory_entries` = 6.921 · `embeddings` = 5.520. Join por chave:

| | entradas | nunca lidas |
| --- | --- | --- |
| no corpus ANN | 5.470 | 643 (**11,8 %**) |
| **fora** do corpus ANN | 1.451 | 739 (**50,9 %**) |

Estar fora do corpus **quadruplica** a chance de a entrada nunca ser recuperada
(confiança 0,9 — correlação forte com mecanismo plausível e direto).

### Três comandos quebrados na própria superfície de memória

| Comando | Sintoma | Gravidade |
| --- | --- | --- |
| `touring memory list` | devolve `{"count":0,"entries":[]}` enquanto `stats` reporta 6.921 | inspeção manual impossível |
| `touring memory reindex` | `Daemon returned success=false` — **é o remédio** para o gap do ANN acima | o conserto do gap está inacessível |
| `touring memory stats` | `gotcha_stats: total 13, unresolved 383.107, resolved 0` | aritmeticamente impossível |

Agravante: `cli_memory_reindex` produz mensagens específicas (ex.: *"ANN recall
not initialised — daemon startup did not call init_ann_memory"*), mas o wrapper
CLI as **descarta** e imprime só `success=false`. Viola o princípio "falhe
loud". Um recall observado também reporta `M-510 TF-IDF activated: 20 candidates
from corpus of 0` — corpus TF-IDF vazio, a investigar.

E `touring kpi -j` **não expõe nenhum KPI de memória**: eficácia de recall é
inteiramente não medida — o mesmo padrão que a aderência a protocolo tinha antes
do `compliance.jsonl`.

### Incidente ao vivo: `reindex` derrubou o subsistema de memória

Rodei `touring memory reindex` para tentar fechar o gap do ANN. Ele falhou —
e **minutos depois todas as operações de memória passaram a falhar**, inclusive
`recall`, que funcionava: cada chamada gastava ~15 s e retornava
`success=false`. `touring doctor -j` seguia 5/6 ok e o daemon estava vivo, ou
seja, a degradação era **do subsistema, não do processo**. Restaurado com
`touring daemon-ctl restart` (REGRA #19 — jamais `pkill`).

**Quarto defeito, descoberto pelo incidente**: `memory_entry_count` foi de 6.921
para **6.923** durante a janela de falha. Os dois `store` que reportaram
`success=false` **gravaram de fato** (verificado por SQL: 3.483 e 11 chars, tier
semantic). Ou seja, `memory store` é **não-atômico e reporta errado** — a linha
commita e a RPC diz falha, provavelmente porque a indexação ANN posterior
fracassa e derruba a resposta inteira. Consequências: um chamador com retry
duplica a entrada; um chamador que confia no erro acredita ter perdido a lição.

Fica em memória a entrada de sondagem `probe-short-2026-08-02` (11 chars) — não
há comando de remoção na superfície `touring memory`.

## Veredito

**Registro: saudável.** Volume alto, tiers coerentes, gravação confiável, e os
reflexos #6/#7 (store + reward) acontecem de fato.

**Recuperação: comprometida por três causas independentes** — (a) 21 % fora do
corpus e o comando de conserto quebrado; (b) ruído automático dominando o
ranking 2,4:1; (c) zero telemetria, logo nenhum ciclo de melhoria.

Pós-R1/R2/R3 a recuperação **do estado do loop** deixou de depender da memória:
`loop_resume` prefere gate + DAG (evidência viva), usando a memória só como
enriquecimento. Isso é deliberado — dado o estado medido do canal de recall,
apoiar a retomada nele seria construir sobre base instável.

## Próximo passo proposto (decisão do Gabriel)

1. Corrigir os três comandos (`list`, `reindex`, `gotcha_stats`) e parar de
   engolir o erro do daemon — são defeitos Rust, exigem `update-touring`.
2. Separar o ruído: `outcome:*` em tier próprio (ou fora do corpus de recall),
   preservando lições curadas no topo do ranking.
3. KPI `touring.memory.*` (recalls, hit-rate, share curado vs automático) para
   fechar o loop de medição.
