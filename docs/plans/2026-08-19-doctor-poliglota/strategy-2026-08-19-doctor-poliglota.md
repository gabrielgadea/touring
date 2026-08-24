---
okf_version: "1.0"
type: Strategy
title: O que o doctor deve chamar de poluído
description: Trocar o predicado "é Rust?" por "é fonte que o grafo resolve?" — medido nos quatro projetos, o atual erra nas duas direções
plan_id: doctor-poliglota-2026-08-19
tags: [doctor, wiring, poliglota, diagnostico, touring]
timestamp: 2026-08-19T19:20:00-03:00
---

# O que o doctor deve chamar de poluído

## O predicado de hoje

```rust
let polluted = non_rust > 0 || kind_unknown > 0 || abs_paths > 0;
// non_rust = SUM(CASE WHEN module_file NOT LIKE '%.rs' THEN 1 ELSE 0 END)
```

Ele pergunta **"é Rust?"**. A pergunta que importa é **"é um arquivo-fonte que o
grafo de wiring consegue resolver?"**. Medido nos quatro projetos, a diferença
entre as duas perguntas faz o veredito errar nas **duas** direções.

## Falso positivo: punir poliglotismo

`analise` é um projeto Python. Tem 202.414 linhas não-`.rs` → warning permanente.

Mas com `TOURING_POLYGLOT_WIRING` desligado — o default — o caminho de **leitura**
filtra por `wireable_ext_sql(col, polyglot)`, que vira `LIKE '%.rs'`, em três
sítios: consultas de órfãos (`knowledge_wiring.rs:1044`), status de módulo
(`:1263`) e agregados (`:1341`). Essas linhas são **inertes**: nenhuma consulta
as lê, nenhuma resposta muda por causa delas. Avisar sobre elas é avisar sobre
algo que não pode afetar coisa alguma.

## Falso negativo: absolver o próprio workspace

`touring` tem **102 linhas de `benches/src/*.rs`**. Terminam em `.rs`, então o
predicado as declara limpas — e o `doctor` do projeto-carro-chefe diz `ok`.

Só que o filtro de leitura é **apenas por extensão**: `benches/` e `tests/` são
excluídos no gate de escrita (`is_indexable_module_file`), não no SQL de leitura.
Logo essas 102 linhas **entram** nas consultas de órfãos e inflam a contagem —
exatamente a não-confiabilidade que o warning existe para sinalizar, e que ele
não vê.

## A medição

Classificando pelo vocabulário do escritor (união máxima, `polyglot = true`):

| projeto | linhas | `.rs` lidas hoje | poliglota inerte | não-wireable | veredito hoje |
|---|---|---|---|---|---|
| touring | 128.007 | 127.905 | 0 | **102** | ok ← errado |
| analise | 216.306 | 13.877 | 9.432 | **192.997** | warning ← certo, motivo errado |
| konverter | 13.455 | 13.382 | 31 | **42** | warning |
| transferegov_pipeline | 514 | 514 | 0 | 0 | ok |

O lixo de `analise` não é "ser Python":

| linhas | motivo | exemplo |
|---|---|---|
| 127.528 | árvore vendorizada | `apps/api/.venv-test/lib/python3.12/site-packages/_pytest/…` |
| 29.341 | `tests/` ou `benches/` | `analise/tests/test_batch2md.py` |
| 25.093 | extensão fora do vocabulário | `.cipher/agent_outputs/agent_outputs.json` |
| 10.786 | `docs/` ou `scripts/` | `.claude/skills/antt-preliminary-analyzer/scripts/postprocess.py` |
| 249 | arquivo de teste | `mcp-servers/antt-code-executor/integration/test_cipher_client.py` |

Só **9.432** linhas de Python são fonte legítima — e são justamente as que não
são lidas.

## A semântica proposta

Três contadores, **um só julga**:

| contador | definição | efeito |
|---|---|---|
| `non_wireable` | não admissível sob o vocabulário **máximo** (`polyglot = true`): vendorizado, `docs/`, `scripts/`, `tests/`, `benches/`, arquivo de teste, extensão fora do vocabulário | **warning** |
| `unread` | fonte de linguagem suportada, não lida no modo atual | informativo — é a resposta para "por que meu projeto Python tem wiring magro" |
| censo | `rows/producers/consumers` sobre o conjunto que as respostas usam | descreve o que é lido |

`warning ⟺ non_wireable > 0 ‖ kind_unknown > 0 ‖ abs_paths > 0`

`non_wireable` é **independente de modo** de propósito: um `.json` ou um venv
não são wiring em leitura nenhuma. Já `unread` depende do modo, e é informação,
não defeito.

## Princípio

O mesmo de `root=` no 30.4.6: **o diagnóstico não pode discordar da coisa que
diagnostica**. O vocabulário já existe no escritor — `wireable_extensions`,
`is_indexable_module_file`, `is_non_rust_non_wireable`. O diagnóstico deve
consultá-lo, não reimplementar uma aproximação (`NOT LIKE '%.rs'`) que diverge
dele em ambas as pontas.

## Os três sítios (C08)

Um conceito, três implementações — a lição do dia diz para achar todas antes de
editar uma:

1. `touring-server/src/cli/doctor.rs::check_wiring_diagnostic` — o que se roda.
2. `touring-cli/src/cli/health.rs::cli_doctor` — despachado como `cli-doctor`,
   alcançado só por testes de IPC.
3. `touring-storage/src/knowledge_wiring.rs::wiring_db_diagnostic` — o SQL que
   produz `WiringDbDiagnostic.non_rust_rows`, consumido por (2).

## Fases

| fase | conteúdo | de quem é |
|---|---|---|
| **P1** | vocabulário do escritor exposto como fonte única + teste de propriedade (o que a escrita recusa é o que o diagnóstico conta) | meu |
| **P2** | semântica nova nos três sítios | meu |
| **P3** | prova por execução nos quatro projetos (102 / 192.997 / 42 / 0) | meu |
| **P4** | expurgo das linhas não-wireable (como o expurgo de estrangeiras de hoje) | **Gabriel** — apaga dado |
| **P5** | ligar `TOURING_POLYGLOT_WIRING` em `analise` | **Gabriel** — muda o que o projeto responde |

P1-P3 são trabalho comum e sigo sozinho. P4 e P5 mudam dados e comportamento de
outros projetos: são decisão dele.

## Adendo — o que a execução mudou no plano (2026-08-19, pós-P5)

Gabriel reordenou: **P5 primeiro**. *"Não faz sentido o touring ter toda uma
infraestrutura poliglota e deixar subutilizada."* Ligar o opt-in antes de mexer
no diagnóstico inverteu a ordem de descoberta — e foi mais barato, porque a
infraestrutura exercida denuncia sozinha quem nunca aprendeu.

### O que a inversão revelou

| # | achado | como apareceu |
|---|---|---|
| 1 | o **leitor discorda do escritor**: filtro de leitura por extensão, gate de escrita por caminho | órfãos de `analise` 5.975 → 142.689 ao ligar o opt-in; 72,4% dos produtores legíveis eram virtualenv |
| 2 | o expurgo (30.4.8) ficou **mais severo que o gate** — julgava o consumidor por um predicado que a escrita nunca aplica | 13.621 arestas de teste legítimas apagadas no `touring`; órfãos 4.232 → 2.498 entregaram |
| 3 | resolvedor **Python e Java fabricava arquivos** — sem probe de disco, ao contrário de Rust e TS/JS | 88 `module_file` fantasmas em `analise`, 76 com `symbol_kind='unknown'` |
| 4 | `&s[..s.len().min(N)]` **mata o project-actor** em texto acentuado | 5 panics do daemon do `analise`: `byte index 60 … inside 'ê'` — 242 sítios na árvore |

Nenhum dos quatro estava no plano. Todos vieram de exercitar a infraestrutura,
não de planejar sobre ela.

### A medição de P3 envelheceu — e para melhor

A tabela acima ("A medição") foi tirada **antes** do expurgo estrutural. Depois
dele, `analise` caiu de 216.306 para **24.026** linhas: as 192.997 não-wireable
que P4 previa expurgar manualmente já não existem — `migrate_evict_ungated_rows`
as remove em toda abertura de banco. O expurgo deixou de ser uma operação e
virou um invariante, que é a forma mais forte da mesma coisa.

O falso NEGATIVO, porém, sobreviveu intacto até P5: as 102 linhas de
`benches/src/*.rs` do próprio `touring` continuavam absolvidas por terminarem
em `.rs`. É o teste `a_rust_file_the_gate_rejects_is_still_pollution` que agora
o prende.

### Fases, como realmente correram

| fase | conteúdo | estado |
|---|---|---|
| **P1** | `polyglot_wiring` como config por projeto (env > projeto > usuário > false) | feito (30.4.7) |
| **P2** | flag do processo → propriedade do banco; 5 leitores migrados | feito (30.4.7) |
| **P3** | `polyglot=on\|off` ao lado de `root=` no doctor | feito (30.4.7) |
| **P4** | opt-in ligado em `analise`/`konverter`/`transferegov`; alinhamento leitor↔escritor | feito (30.4.8, corrigido em 30.4.9) |
| **P5** | resolvedor poliglota com prova de disco · semântica de três contadores nos 3 sítios · guard UTF-8 | feito (30.4.10) |

`touring` fica com `polyglot = off` por decisão medida, não por omissão: o ganho
seriam 45 arquivos: de 1.128 arquivos JS, 1.087 eram rustdoc gerado.

## Referências

- Plano: [/plan.md](/plan.md) · Log: [/log.md](/log.md)
- Diagnóstico OUTER: [/diagnostics/touring-20260819T191958.md](/diagnostics/touring-20260819T191958.md)
- Precedente do dia (raiz do banco, expurgo de estrangeiras): `docs/plans/2026-08-19-topologia-honesta/`
