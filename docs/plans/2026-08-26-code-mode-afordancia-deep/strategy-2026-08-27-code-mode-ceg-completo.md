<!-- OKF document -->
---
okf_version: "1.0"
type: Strategy
title: "Code mode + CEG sandbox — completos, deployados e provados na prática"
description: "Estratégia executada em 27/08/2026: DAG S1–S10 fechada (10/10), três correções P0 de segurança no CEG, e uma prova comportamental de 29 asserções armada como gate de release."
plan_id: 2026-08-26-code-mode-afordancia-deep
dag_id: task_1787779302447443970
tags: [code-mode, ceg, sandbox, seguranca, afordancia, prova-comportamental]
timestamp: 2026-08-27T13:00:00-03:00
authority: Gabriel Gadea
status: done
---

# Code mode + CEG — o que se fez, o que se mediu, o que fica

## 1. A tese que orientou tudo

**Afordância mora no executor, e o que o executor faz tem de ser medido, não lido.**

Nenhum dos quatro defeitos abaixo foi encontrado lendo código. Todos apareceram ao
comparar saídas de execuções reais. Onde a leitura bastava, ela já tinha passado por ali
antes — em alguns casos escrevendo o comentário que afirmava exatamente o oposto do que
o código fazia.

## 2. Os defeitos, e o sinal que os denunciou

| # | defeito | sinal que o revelou |
|---|---|---|
| **CEG-1** | `bash_capability_needs` emitia `Capability::Run` para toda palavra, builtins inclusive | `echo oi` e `curl http://evil.test` produziam o **mesmo** composite (0,675) |
| **CEG-2** | o waiver de shell era **cego**: rebaixava todo `Deny` a advisory | `curl https://example.com` devolveu **HTTP 200** no sandbox; o `socket` equivalente em Python era recusado |
| **CEG-3** | o waiver, corrigido pela classe, ainda dispensava o bloqueio destrutivo do X2 | `rm -rf /tmp/zz` **executou** sob um advisory que dizia "X2 STATIC blocked the code" |
| **S1** | a fachada MCP de 3 tools exigia env var em cada sessão | o escopo declarava `mode = "code"` e recebia as ~23 curadas mesmo assim |
| **S3** | o modo `code` negava a **1ª** inspeção de `grep`/`cat`/`find` | 115 transcripts: **22,5%** dessas chamadas são isoladas |
| **S4** | o SDK alcançava 8 de ~195 leituras; o stub era uma 2ª const à mão | contagem direta; o stub podia divergir da allowlist que impõe |
| **S10** | o T3-B mantinha estado por sessão e nunca decidia | 3 classes no mesmo turno: `t3_turn_first_passed = 3`, `t3_turn_fused = 0` |

O **CEG-2** é o mais grave e o mais instrutivo. O comentário no código prometia
*"real containment is the execution sandbox (rlimits + landlock + env-clear + …)"* —
mas **Landlock é filesystem-only**. O waiver cego nasceu para calar o ruído do CEG-1;
ao consertar a fricção, jogou fora a contenção junto.

## 3. A assimetria que passou a governar o CEG

Não é preferência de projeto. É o que a contenção do sandbox **sustenta**, medido:

| recurso | contido? | prova executada | política |
|---|---|---|---|
| filesystem | **sim** | `touch ~/.ssh/x` falha; `touch <ws>/x` funciona | `subprocess` é dispensável |
| rede | **não** | `curl` devolvia HTTP 200 sob o waiver cego | `network` nega duro |

Daí o waiver **seletivo**: só `subprocess` é dispensado, e nunca quando há bloqueio
destrutivo do X2 junto. `eval`/`exec`/`source` continuam exigindo grant — executam texto
como código, e isentá-los seria isentar o construto que um atacante procura.

Resultado medido: de 10 comandos benignos, **0** emitem advisory (antes: 100% do shell).

## 4. Duas decisões que contrariaram o plano

Um item de plano que a medição reprova deve ser **descartado**, não executado por
obediência.

- **S8 (reuso de socket) — descartado.** 100 queries em **6,1 ms** (0,06 ms/query) num
  run de 32 ms. Gerenciar lifecycle de socket por isso é complexidade sem retorno.
- **O *preview* do S5 — recusado.** Preview é conteúdo, e uma `memory_recall` pode
  carregar segredo — a própria regra do arquivo já dizia *"never the content"*. Entregue
  no lugar: latência e desfecho, que respondem o que o preview queria responder (qual
  hook é caro, qual falhou) sem transformar observabilidade em superfície de exfiltração.

## 5. O que ficou armado (e não só escrito)

| artefato | onde | como roda |
|---|---|---|
| Prova comportamental (29 asserções) | `scripts/prova_code_mode_ceg.py` | **gate 5.5** do `propagate-release.sh` — precisa do lado vivo |
| Guard D8 texto↔executor | `scripts/test_code_mode_sdk_section.py` | CI; mutação 4/4 pega |
| Instrumento de calibração | `scripts/s3_burst_distribution.py` | + `test_s3_burst_distribution.py` (11 testes) |
| Invariantes da allowlist | `run.rs` tests | 1 deles contra o **registry real** do daemon |

Na armação da própria prova escrevi `$ROOT` onde a variável do script é `$WORKSPACE`.
O gate teria falhado em silêncio — apareceu porque **executei** o bloco isolado.

## 6. Gates de saída

**15.838 testes / 272 suites / 0 falhas** · clippy `-D warnings` limpo · doctor **7/7** ·
`touring explore` convergiu (exit 0, 2 rodadas secas) · DAG **10/10** · 0 órfãos novos ·
deployado via `update-touring` · prova **29/29**.

Três falhas **pré-existentes** corrigidas no caminho (REGRA #21). Nunca tinham rodado:
os gates anteriores eram `--lib` e elas viviam em testes de integração. Duas codificavam
o contrato de antes do G9; uma, o waiver cego.

## 7. Fechamento dos pontos em aberto (v30.4.16, 27/08 tarde)

Todos os quatro foram fechados, e três deles revelaram algo que a lista não previa.

| # | ponto | desfecho |
|---|---|---|
| 1 | propagar a `analise`/`konverter` | **v30.4.16 propagada**, lock verificado, prova 35/35 em cada. **Correção de fato**: `analise` declara `mode = "code"` — eu havia afirmado que ambos declaravam `both`; só `konverter` não declara. |
| 2 | `dd of=` escapava da detecção | `writes_via_argument` (`of=`, `--output=`, `-o` após verbo que grava), com o negativo guardado: `ls -o` não é escrita. |
| 3 | proxy server-side do SDK | **o furo era real e foi provado.** Ver abaixo. |
| 4 | KPI precisa de tempo | a **leitura** ficou pronta (`inspect_burst_share`); o veredito segue sendo uso, não mais uma medição hoje. |

### 3 — a allowlist era uma sugestão, e deu para provar

`touring.READONLY_HOOKS` é um atributo Python mutável. Um programa no sandbox fez:

```python
touring.READONLY_HOOKS = touring.READONLY_HOOKS + ("cli-memory-store",)
touring.query("cli-memory-store", {...})   # gravou na memória do daemon
```

Uma tupla num objeto de cliente não impõe nada. A fronteira agora é do lado do socket
onde o chamador não escreve o código que a verifica: `touring_foundation::orchestrate_allowlist`
é a fonte única, o daemon recusa hook fora da lista quando o request carrega `origin` de
sandbox, e o SDK propaga a **razão** do daemon — antes o programa via só `success=false`,
e um deny que não ensina é obstáculo.

### O que EU quebrei no caminho, e como apareceu

Duas coisas, ambas pegas por gate e não por leitura:

- **Regressão de classificação.** O waiver seletivo transformou dois defeitos *latentes* em
  deny duro: `2>&1` era partido pelo separador `&` e o `1` residual virava nome de programa;
  `2>/dev/null` contava como escrita de arquivo. Juntos, um comando comum somava
  `subprocess` + `fs-write` e negava. O waiver cego anterior escondia os dois. 4 testes.
- **Flaky que eu introduzi.** Meus testes do S3 disparam denies → `record_route_offer` →
  `bump_arm`, contaminando o contador **global** de arm. Pior: inventei um grupo serial
  `arm_counters` **paralelo** ao canônico `gate_metrics` que o arquivo já usava — por isso
  as duas primeiras tentativas pioraram em vez de corrigir. Realinhar ao grupo existente:
  8/8 estável.

E o gate 5.5 pegou um falso negativo dele mesmo: rodava em cima do restart dos daemons do
passo 5. Corrigido com espera ativa por `doctor` saudável — um gate que reprova por
transiente é um gate que se aprende a ignorar.

## 8. O que segue em aberto

1. **O veredito do limiar do S3** — `touring kpi inspect_burst_share` já lê a razão; o
   número precisa de dias de uso para dizer se 300s/2ª acertou. Muito abaixo de ~0,775 é
   gate apertado demais; muito acima é fricção voltando.
2. **Flaky do `wasm::typed` sob `--workspace`** — `WasmRunner::new()` falha ao criar a
   engine sob carga pesada (5/5 e serial passam isolados). Mecanismo NÃO reproduzido;
   não inventei correção. O que mudou: os 10 `.expect("engine")` agora carregam o erro
   real, para que a próxima ocorrência seja diagnosticável.
3. **`konverter` não declara `[code_mode] mode`** — roda em `both`. Ligar `code` lá é
   decisão de Gabriel, não consequência técnica desta propagação.

## 9. Referências

- Medição da rajada: `scripts/s3_burst_distribution.py --since 2026-08-01`
- Manual atualizado: `docs/code-mode.md` (§CEG, §orchestrate, §observabilidade, §MCP, §divergências do dsh)
- Memórias: `ceg-downgrade-cego-furo-de-rede:2026-08-27` · `s1-transport-mcp-facade-por-escopo:2026-08-27` · `s3-recalibrate-predicado-de-rajada:2026-08-27` · `s4-surface-allowlist-gerada:2026-08-27` · `s5-s10-ceg-fechamento-do-bloco:2026-08-27` · `s8-conn-reuse-medido-e-descartado:2026-08-27` · `prova-comportamental-code-mode-ceg:2026-08-27`
