<!-- OKF document -->
---
okf_version: "1.0"
type: Strategy
title: "Afordância de Code Mode — colapso do executor, SDK em contexto, fusão automática de rajadas"
description: "Estratégia para que code mode aconteça sem eu decidir que ele aconteça, derivada da leitura do harness do DeepSeek em disco e de 1.528 tool calls medidas nesta sessão."
plan_id: 2026-08-25-code-mode-afordancia
tags: [code-mode, afordancia, executor-collapse, deepseek-harness, gates, cli-suggester]
timestamp: 2026-08-25T11:40:00-03:00
authority: Gabriel Gadea
status: proposta — aguarda gate humano
---

# Afordância de Code Mode

## §0 — O que a crítica derrubou

A prova anterior (`run-1787655823337-1503057`, 10 sub-chamadas em 1 tool call) foi
escrita por mim, para provar o ponto, na hora de provar o ponto. Ela demonstra que
**sei** usar o SDK. Não demonstra nada sobre o que acontece quando não decido usá-lo.

O que acontece quando não decido está medido nesta mesma sessão:

| medida | valor |
|---|---|
| tool calls totais | 1.528 |
| chamadas Bash | 1.231 |
| adoção de code mode (só elegíveis) | **28%** de 647 |
| oportunidades perdidas | **465** |
| rajadas (≥3 da mesma classe) | 61 |
| `grep` como fração da inspeção atômica | 468 de 662 (**71%**) |

Uma afordância só está provada quando o code mode executa numa chamada que eu emiti
**como atômica**. Esse é o critério de aceitação de tudo abaixo.

## §1 — O que o harness do DeepSeek realmente faz

Fonte primária, clone em disco: `/home/gabrielgadea/references/code-mode-2026-08-23/deepseek-harness`.
Lidos integralmente: a nota-base do transporte (`feature/2026-06-15-code-mode.md`), o
postmortem do colapso (`bug-fix/2026-08-07-code-mode-executor-collapse.md`), a nota de
apresentação por agente (`feature/2026-08-05-per-agent-tool-presentation.md`) e o README
de `packages/core/agent-tool-presentation`.

Cinco partes sustentam o mecanismo — e **apenas uma é o prompt**:

1. **Colapso da apresentação.** `wireSchemas()` entrega ao modelo exatamente uma tool:
   `run_code`. As demais somem do wire; o SDK gerado (`.d.ts`) entra no system prompt.
2. **Colapso do executor.** `resolveExecution(name, scope, nested)`: uma chamada
   modelo-direta (`nested = false`) nomeando qualquer tool nativa resolve para `undefined`
   e vira `UNKNOWN_TOOL`. O postmortem existe porque a parte (1) sozinha **não impôs
   nada** — está escrito na fonte: *"schema omission is not enforcement when a direct
   caller can bypass it; denial must be tested through the executor"*. Provedores não
   interceptam nomes não anunciados; o modelo emitiu `write`, `read`, `bash` e **executou**.
3. **A negação carrega a rota.** Como as seções de guidance das tools nativas **permanecem**
   no prompt, uma negação nua fez o modelo concluir que o deployment estava inconsistente
   em vez de se corrigir. O `UNKNOWN_TOOL` passou a nomear o caminho de volta por `run_code`.
4. **Sub-chamadas são isentas por construção.** O discriminador é o token `parent`. Dentro
   do programa, a tabela inteira está disponível — e cada sub-chamada atravessa o pipeline
   completo (pre-execute, guards, post-execute, notificação), com identidade de execução
   própria. Segurança não é perdida por estar dentro do programa.
5. **Os contextos das sub-chamadas são diferidos.** `deferContext()` acumula os
   `additionalContexts` e o loop só os anexa **depois** do resultado externo — porque
   injetar dentro do `run_code` quebraria a adjacência chamada/resultado.

E duas decisões que eles **recusaram**, ambas relevantes para nós:

- **Recusaram o code mode obrigatório** ("always-exclusive, Cloudflare-faithful"): *"a
  coding agent's bread-and-butter single calls (bash, read, edit) are already ideal as
  native calls, and forcing every edit through a program taxes the common case."* Por isso
  existe `both`.
- **Adiaram os níveis por tool** ("this tool native, that tool code-only") porque *"its
  design depends on evidence about how models split usage under `both`"*.

E não reivindicam economia: *"the Agent Note makes no unconditional-savings claim.
Measured guidance is explicitly post-ship learning."*

## §2 — A assimetria: o que dá e o que não dá para copiar

O harness é da Anthropic. Não controlamos `wireSchemas`, nem a lista de tools, nem o
system prompt. A parte (1) é inalcançável.

**Mas a evidência deles diz que a parte (1) não é a que impõe.** A que impõe é a (2), o
executor — e o executor, para nós, é o hook `PreToolUse`, que roda in-daemon e decide
antes de a tool executar. As partes (2), (3), (4) e (5) são todas implementáveis.

| DeepSeek | Nosso equivalente | Estado |
|---|---|---|
| `mode: native\|code\|both` | `TOURING_CODE_MODE` + `.touring/touring.toml` | **a construir** (hoje só env global) |
| `presentAs(mode)` por escopo | config por projeto (+ sessão) | **a construir** |
| `wireSchemas()` → só `run_code` | — inalcançável | **N/A** (e não é a parte que impõe) |
| seção `tools:sdk` no prompt | `SessionStart` `additionalContext` com `--sdk-stub` | **a construir** |
| `resolveExecution` → `UNKNOWN_TOOL` | hook `PreToolUse` → `deny` com a rota | **existe** (G1/G6/`TOURING_CODE_ONLY`) |
| token `parent` isenta o aninhado | sub-chamadas ficam no sandbox, nunca chegam ao hook | **grátis por construção** |
| sub-chamada atravessa o pipeline | allowlist `READONLY_HOOKS` + `forbidden_calls` | **existe** |
| `deferContext()` | — hoje injetamos nudge **em cada** chamada | **a deletar** |
| `{code, description}` obrigatórios | `touring run` não tem `--description` | **lacuna** |
| worker novo, `env {}`, caps | sandbox CEG | **existe, e mais estrito** |
| `tool/code-dispatch` | `run_id` + journal | **existe** |

**A vantagem que temos e eles não tinham**: eles adiaram os níveis por tool por falta de
evidência sobre como o modelo divide o uso sob `both`. Nós rodamos em `both` há meses
**com instrumentação**. Os 465 casos, as 61 rajadas e a distribuição por classe são
exatamente a evidência que faltava a eles. O desenho que eles adiaram é o que devemos
construir — e por medição, não por analogia.

## §3 — Diagnóstico: por que eu não escolho o caminho preferido

Antes de qualquer coerção: o caminho preferido hoje é **pior** que o atômico. Três
defeitos, os três verificados por execução nesta sessão, não recordados.

**A1 — O sandbox é mais fraco que o Bash.** `touring run` nega a capability `subprocess`
sob o perfil `Sandboxed`; observei duas negações neste turno (`'for'` e `'subprocess'`).
Um programa que precise chamar `rg` não roda, enquanto a chamada Bash crua roda. O
DeepSeek justificou explicitamente a postura oposta — o worker deles é *bash-equivalent
by design, no unsafe-acknowledgement flags*, porque o harness já embarca `dsh-bash-local`
com **mais** autoridade ambiente. Nós invertemos isso e pagamos em adoção.

**A2 — `--brief` elide em silêncio e mente sobre isso.** Teste controlado: programa que
imprime 30 linhas; `--brief` devolveu 6 (head 3 + tail 3) com `"truncated": false`. Quem
usa `--brief` para caber no contexto perde 80% do resultado e é informado de que nada foi
perdido. É a família `sinais-de-progresso-que-mentem`, e é P0.

**A3 — O nudge dispara sobre o próprio code mode e sugere embrulhá-lo.** Cinco vezes
neste turno, o `code-mode-loop` recebeu um comando que **já era** `touring run` e emitiu
como MUST `touring run --lang bash --code 'touring run --lang python …'`. A camada de
persuasão não é apenas cara — está errada, e ensina um antipadrão.

**A4 — A afordância que já funcionava entregava um programa quebrado.** Descoberto ao
executar o P0, e é o mais grave dos quatro porque atinge exatamente a emenda do Gabriel.
O G1 fez o que devia: eu emiti 4 greps atômicos e ele me devolveu um `touring run` com os
comandos reais fundidos. Só que `cli_suggester.rs:2887` truncava cada comando a 240 chars
(`cmd.chars().take(240)`), então o corpo saiu cortado no meio de um caminho — `crates/tou'` —
com as aspas equilibradas e o comando pela metade. Um programa que **parece** completo e
não roda. O mesmo corte estava na rota do `TOURING_CODE_ONLY` (`:3248`).

O princípio que substitui a truncagem: **comando inteiro ou nenhum**, com a omissão
declarada. Um comando ausente aparece na contagem que o remédio informa; um truncado se
disfarça de programa. É a mesma família de A2 — sinal que afirma completude que não tem.

Enquanto A1-A4 existirem, coagir para o code mode é empurrar para um caminho degradado.
**T0 vem antes de tudo.**

## §4 — A estratégia

Quatro camadas. T0 é reparo; T1 é substituição; T2 e T3 são a afordância propriamente
dita. T3 é a única que responde à crítica de origem.

### T0 — Reparar o caminho preferido (pré-requisito, sem o qual nada mais é honesto)

- **T0.1** Elevar a postura do sandbox para *bash-equivalent* no perfil usado por
  `touring run` quando o comando é inspeção pura: liberar `subprocess` para um allowlist
  de binários de leitura (`rg`, `find`, `git log/diff/show`, `cargo metadata`), mantendo
  negação para mutação. Justificativa textual da própria fonte do DeepSeek (§Trust
  posture): contenção, não fronteira de segurança, porque o Bash ao lado já tem mais.
- **T0.2** Corrigir `--brief`: `truncated` deve ser `true` sempre que `head_tail` não
  cobrir o stdout inteiro, e o envelope deve carregar `stored_path` para o corpo elidido
  (o mecanismo de spill já existe). Teste de mutação obrigatório.
- **T0.3** `loop_rewrite_candidate` e os nudges de code mode passam a retornar `None`
  quando o comando já contém `touring run`/`touring exec` fora de aspas — hoje o guard
  existe em `loop_rewrite_candidate` mas o **nudge** do suggester não o aplica.
- **T0.4** Adicionar `--description` a `touring run` (obrigatório sob `--orchestrate`),
  espelhando o contrato de dois argumentos do `run_code`. É o rótulo que o journal e o
  card de resultado precisam para que uma execução de programa seja auditável como uma
  tool call é.

### T1 — SDK uma vez, e o nudge CONVERTIDO (emenda do Gabriel, 25/08)

> **A emenda muda esta camada.** A proposta original era deletar as injeções por
> chamada. Gabriel corrigiu: *"o nudge de persuasão deve injetar contexto com snippet
> que substitua as n+ tool calls"*. Deletar seria jogar fora o único canal que já
> alcança o modelo no instante da decisão. O que a medição condena não é a injeção —
> é a injeção que **exorta** em vez de **entregar**. Um nudge que devolve o programa
> pronto derrubou o custo de adotá-lo a zero, e afordância é exatamente isso: mudar
> `U(a) = P·V − C(tokens)` pelo lado do `C`, não pelo lado da retórica.
>
> Implementado: o caminho de laço parou de traduzir para python com
> `# then your per-file op over files` e passou a levar o laço **verbatim** ao
> sandbox (`bash_code_mode_command`, que também perdeu a truncagem de 200 chars).
> `loop_code_mode_command` e `loop_glob` foram **removidas**, não silenciadas.

Hoje: **1.196 injeções de nudge**, 1,12 MB de contexto nesta sessão, pagas por todas as
ferramentas (Bash 66% das chamadas, Edit 170%, e **o próprio code mode em 94%**).

Proposta: uma seção `SessionStart` com a saída de `touring run --lang python --sdk-stub`
(1,3 KB, byte-estável, ordenada lexicograficamente) + a regra do modo vigente. Prefixo
estável, portanto cacheável — a mesma propriedade que o README deles registra em
*"KV Cache effect: the presentation is fixed when the agent is composed, so its request
prefix is stable for the session's life."*

Deletados: `code-mode-loop`, `master-cli`, `learning-memory` como injeções **por chamada**.
O que sobrevive por chamada é só o que uma negação precisa carregar (a rota, §1.3) —
que é a forma diferida deles, não a intercalada nossa.

Isso é uma **deleção medida**: a persuasão custou 1,12 MB e a adoção que induziu foi 28%.

### T2 — Colapso do executor, por escopo e por classe medida

`TOURING_CODE_MODE = native | code | both`, resolvido na ordem **sessão (env) → projeto
(`.touring/touring.toml`) → default (`both`, o comportamento de hoje)**. É o `presentAs`
deles: um projeto em `code` convive com outro em `native` na mesma máquina.

Sob `code`, uma chamada Bash modelo-direta cuja classe está no conjunto de fan-out é
**negada** — não sugerida — com a rota derivada do comando verbatim. Sub-chamadas dentro
do sandbox nunca chegam ao `PreToolUse`, então a isenção do `parent` token é nossa de graça.

O conjunto de classes vem da medição, não da intuição. Da distribuição desta sessão:

| classe | ocorrências | entra em `code`? | por quê |
|---|---|---|---|
| `grep`/`rg` | 468 | **sim** | 71% da inspeção atômica; 61 rajadas |
| `cat`/`head`/`tail` | 165 | **sim** | leitura de N arquivos é o caso canônico |
| `find` | 18 | **sim** | varredura por definição |
| `ls` | 11 | não | chamada única domina; taxa o caso comum (a recusa deles) |
| mutação / build / git | — | **nunca** | fora de `scan_class_of` por construção |

`TOURING_CODE_ONLY=1` já implementa ~90% disto (`cli_suggester.rs:3237`); falta o
**escopo** (hoje é env global do daemon) e a **calibração por classe** (hoje nega toda
`scan_class_of`, incluindo `ls`).

### T3 — Fusão automática da rajada do turno (a resposta à crítica)

T2 nega uma chamada por vez e me manda escrever o programa. Isso ainda é uma decisão
minha. **T3 é onde a decisão sai de mim.**

Quando K ≥ 2 chamadas Bash de classe fan-out chegam **no mesmo turno** (mesma sessão,
dentro da janela, sem `PostToolUse` intercalado — que é a assinatura exata do batch
paralelo do Claude Code), o daemon:

1. deixa a **primeira** executar intacta (o resultado dela é real, nada se perde);
2. **nega as K−1 restantes** com uma única rota derivada, que embute os K−1 comandos
   **verbatim** fundidos em um programa.

Custo: um round-trip. Ganho: N→1 permanente, e a derivação do programa foi do harness.
No turno seguinte eu emito **um** `touring run` que eu não projetei — que é precisamente
o que a prova anterior não era.

Duas variantes foram consideradas e a ordem importa:

- **Variante A (hold-and-fuse)**: segurar a resposta da primeira por ~25 ms, coletar os
  irmãos, reescrever a primeira via `updatedInput` com o programa fundido e negar as
  demais. Zero round-trips extras. Mais rápida e mais arriscada: se o programa fundido
  falhar, perdem-se as K chamadas de uma vez.
- **Variante B (first-wins, fold-the-rest)**: a descrita acima. Sem espera, sem risco
  para a primeira, perda máxima de um round-trip.

**Recomendo B primeiro.** A já está desenhada e só deve ser ligada depois que a telemetria
de B provar a frequência e o acerto da detecção de rajada. O `updatedInput` já se provou
vivo no G2 e no G8, então A não é especulativa — é uma otimização com risco assimétrico,
e a ordem certa é medir antes.

## §5 — O que é deletado

| item | razão |
|---|---|
| `loop_code_mode_command` + `loop_glob` | traduziam o laço para python com o corpo como placeholder; `bash_code_mode_command` leva o laço verbatim e roda. **Removidas**, não silenciadas (REGRA #0) |
| a truncagem de 240 chars no G1 e de 200 em `bash_code_mode_command` | entregavam programa cortado com aspas equilibradas — pior que nenhum |
| ~~os nudges por chamada~~ | **revogado pela emenda do Gabriel**: eles não são deletados, são convertidos em entrega de programa. O que sai é a exortação, não o canal |
| a reivindicação de 84,8% de redução de contexto | o benchmark debitou as injeções só do braço atômico; **a memória `prova:code-mode-eficiencia-medida:2026-08-25` deve ser substituída, não corrigida na margem** |

## §6 — Medição (nenhuma reivindicação antes do número)

A/B por escopo de projeto, três braços — `native`, `both` (baseline de hoje), `code` — sobre
o mesmo conjunto de tarefas, reportando:

- `adoption_ratio` (contador vivo, `touring status -j | jq .code_mode`)
- bytes de contexto por turno, separando **payload**, **comando** e **nudge** (as três
  grandezas que o benchmark anterior misturou)
- tempo de parede
- **corretude** — o braço que responde errado mais barato não venceu nada

Adotamos a honestidade da fonte: nenhuma reivindicação incondicional de economia. A
orientação de quando preferir cada modo é aprendizado pós-embarque.

## §7 — Riscos e o que recusamos

- **Recusamos o code mode obrigatório.** Pela mesma razão que eles: `Edit` de um arquivo,
  um `cargo check`, um `git log` são ideais como chamadas nativas. Forçar tudo por
  programa taxa o caso comum. `both` continua o default.
- **Risco de negar o que não deveria.** Mitigação: a rota carrega o comando verbatim, e
  `TOURING_GATE_OK=1` por-comando é o escape consciente e **contado**.
- **Risco de atenção humana.** O colapso deles termina *antes* do pipeline de política,
  para que nenhum humano seja consultado sobre uma negação determinística. Nossa negação
  deve fazer o mesmo: nunca virar prompt de aprovação.
- **Kill switch humano**, sempre: `TOURING_CODE_GATES_DISABLED=1` no env do daemon.
  A LLM não liga nem desliga modo — isso é decisão do Gabriel, como manda a REGRA #19
  para a família de env do daemon.

## §8 — Fases propostas

| fase | conteúdo | gate de saída |
|---|---|---|
| **P0** | T0.1-T0.4 — reparar sandbox, `--brief`, nudge recursivo, `--description` | teste de mutação em cada um; `cargo test` verde |
| **P1** | T1 — SDK em `SessionStart`, deleção dos 3 nudges por chamada | bytes de nudge por turno → ~0; SDK presente 1× |
| **P2** | T2 — `TOURING_CODE_MODE` por escopo + classes calibradas | `code` num projeto não afeta outro; `ls` passa, `grep` nega |
| **P3** | T3-B — fusão da rajada do turno (first-wins, fold-the-rest) | **prova de aceitação**: emitir K greps atômicos e receber 1 programa que não escrevi |
| **P4** | Medição A/B + substituição da memória inflada | relatório com as 3 grandezas separadas |
| **P5** | T3-A (hold-and-fuse) — só se a telemetria de P3 justificar | zero round-trip extra, sem perda de lote |

**O gate de P3 é o critério que a crítica do Gabriel estabeleceu**: só está provado quando
o programa que roda é um que eu não decidi escrever.

---

## Referências

| Fonte | Local |
|---|---|
| Nota-base do transporte `run_code` | `references/code-mode-2026-08-23/deepseek-harness/.agents/notes/implemented/feature/2026-06-15-code-mode.md` |
| Postmortem do colapso do executor | `.../bug-fix/2026-08-07-code-mode-executor-collapse.md` |
| Apresentação por agente (`presentAs`) | `.../feature/2026-08-05-per-agent-tool-presentation.md` |
| README do row de apresentação | `.../packages/core/agent-tool-presentation/README.md` |
| Gates vivos (G1/G2/G6/G8/CODE_ONLY) | `crates/touring-cli/src/cli_suggester.rs` |
| Transporte e SDK nossos | `crates/touring-server/src/cli/run.rs` (`--orchestrate`, `--sdk-stub`) |
| Diagnóstico OKF desta rodada | `/diagnostics/touring-20260825T081347.md` |
| Ledger CCE (convergido, 4 rodadas) | `.touring-explore/afordância-de-code-mode-no-harness--colapso-do-e.ledger.json` |
| Plano do bundle | `/plan.md` |
