---
okf_version: "1.0"
type: Plan
title: "Portfólio de fluxos modulares no ADW — 11 entregas em 3 ondas"
description: "Plano executável para dar ao runner ADW composição por fragmentos, persona por nó, fan-out com barreira, descoberta por propósito e planejamento sob névoa — mais três defeitos reproduzidos por execução."
plan_id: 2026-08-18-graph-engineering-flow-portfolio
dag: task_1787088570195129385
tags: ["#kind:plan", "#domain:adw", "#domain:loop-engineering", "#status:done", "#artifact:plan"]
timestamp: 2026-08-18T18:35:00-03:00
---

# Plano — Portfólio de fluxos modulares no ADW

> Análise, fontes e justificativa: [`cartografia-de-fluxos.html`](cartografia-de-fluxos.html) ·
> Estratégia: [`strategy-2026-08-18-graph-engineering-flow-portfolio.md`](strategy-2026-08-18-graph-engineering-flow-portfolio.md) ·
> DAG executável: `task_1787088570195129385` (11 subtarefas, 0 ciclos, validado)

## Objetivo

Transformar a `adw-library` — hoje 9 specs TOML monolíticos e indescobríveis — num **portfólio de
fluxos com peças reusáveis**, com uma forma guiada de **criar um fluxo novo para uma funcionalidade
específica** de qualquer projeto. No caminho, corrigir três defeitos reproduzidos por execução.

## Estado verificado (FACT — por leitura de código e execução)

| Achado | Evidência |
|---|---|
| Executor caminha **um nó por vez** — sem fan-out nem barreira | `adw.py::execute()` mantém `current: str`; `_next_edge() → str` |
| **Zero composição** entre specs | `load_spec` lê um TOML; sem `include`/`extends` |
| **Persona ausente** — só modelo e ferramentas | `_claude_cmd` nunca usa `--agent`/`--agents`; `tier` = 5 nomes → 3 modelos |
| Fluxos **invisíveis** no portfólio | `adw_description` (`miner.rs:237`) extrai só `[adw].description` (~10 palavras) vs `shell_header` (~50–300) |
| **DEFEITO 1** — lint deixa passar ciclo infinito | `_lint_cycles` tem `return` dentro do laço; spec de 2 ciclos passa com **exit 0** |
| **DEFEITO 2** — duas sessões recebem o mesmo subtask | `ready_subtasks` só lê; sem `claim`/`take_next`/`acquire` no crate |
| **DEFEITO 3** — retry re-executa o agente às cegas | 4 de 5 gates nunca referenciados em prompt; `resume_on_fail` não mitiga (o gate roda depois, em outro processo) |
| Fan-out **não** é ganho de performance | `audit`: 3 nós a 0,0–0,1 s; `strategy-loop`: `diagnose` 30,2 s domina `recall` 5,4 s → ~15% |

## A fronteira que ordena o plano

`adw.py` é **script**: muda e vale na hora, reverte apagando a linha.
Os crates são **Rust**: exigem `update-touring` + reinício do daemon + `touring update --project`.

Por isso as três entregas Rust viajam num **único deploy** — espalhá-las pagaria o pedágio 3×.

## As 11 entregas

| # | Entrega | Toca | Esforço | Depende | Gate (executável) |
|---|---|---|---|---|---|
| **A1** | Lint de ciclos | `_lint_cycles` · 19 L | XS | — | spec de 2 ciclos falha com exit 1 |
| **A2** | Feedback do gate no retry | 4 specs + lint | S | — | prompt da 2ª tentativa contém o motivo |
| **B3** | Persona por nó | `_claude_cmd` · 24 L | S | — | só a versão com persona reprova o artefato que narra sucesso |
| **B1** | Fragmentos e inlining | `load_spec` · 38 L | M | — | 9 specs recompostos geram planos idênticos |
| **B2** | `flow new` guiado | comando novo | M | B1 | fluxo criado passa `lint`+`test` sem edição manual |
| **B4** | Fan-out de leitura | `execute` 59 L · `_track_class_d` 17 L | L | — | `kill -9` em 3 ramos: resume não reexecuta os concluídos |
| **B5** | Painel de críticos | fragmento | S | B1·B3·B4 | quorum reprova; Class-D por ramo registrado |
| **B6** | Contrato de verificação | `_next_edge` · `run_loop` | M | A2 | gate inexecutável **escala** em vez de gastar retry |
| **C1** | Bloco `[purpose]` | `miner.rs` · 5 refs | S | B1 | intento do bugfix recupera `bugfix` no topo |
| **C2** | Reivindicação atômica | `decomposer.rs` · **66 refs** | L | — | duas sessões concorrentes recebem subtasks distintos |
| **C3** | Wayfinder no decompose | `decomposer.rs` + CLI | L | C2 | névoa alta gera decision tickets; cada decisão aponta à origem |

**Ondas** — A: correções (script) · B: capacidades (script) · C: Rust (um deploy).

## Caminho crítico e primeiro corte

**Caminho crítico do pedido**: `B1 → B2 → C1`. O fan-out (B4) **não está nele** — serve à topologia
em diamante, não à modularidade. Correção de uma inversão do plano anterior.

**Primeiro corte**: `A1 + A2 + B3`. Menos de cem linhas, nenhuma dependência, todas script, e cada
uma fecha um modo de falha silenciosa já reproduzido.

**Prontas sem dependência** (o que `decompose ready` devolve hoje): `A1 A2 B1 B3 B4 C2`.

## Detalhe operacional

### A1 — Lint de ciclos · XS
- **Onde**: `~/.claude/skills/Touring/scripts/adw.py::_lint_cycles` (L222, 19 linhas)
- **O quê**: `return` → `continue`; deduplicar ciclos já reportados (senão o mesmo ciclo é anunciado
  uma vez por membro).
- **Repro do defeito**: spec de 5 nós `start/a/b/x/y` com `start.on_pass=a`, `start.on_fail=x`,
  ciclo `a↔b` **com** saída e `x↔y` **sem**. Hoje: exit 0.
- **Gate**: `touring adw lint cyclebug2` → exit 1 citando `x → y`.

### A2 — Feedback do gate no retry · S
- **Onde**: `adw-library/{bugfix,chore,feature}.toml` + `_lint_node`
- **O quê**: o prompt do worker passa a referenciar `{{nodes.<gate>.summary}}`; o lint avisa quando um
  `on_fail` devolve controle a um agente sem referenciar o gate que reprovou.
- **Modelo**: `hotfix.toml` já faz certo — copiar o padrão.
- **Gate**: spec sintético cujo gate reprova com motivo específico; o prompt da 2ª tentativa o contém.

### B3 — Persona por nó · S
- **Onde**: `adw.py::_claude_cmd` (L401, 24 linhas) + parser de spec
- **O quê**: bloco `[node.X.persona]` compilado para `--agents '{"<role>": {...}}'` + `--agent <role>`.
  Definição **inline** — sem catálogo global, o fluxo permanece portável.
- **Campos**: `role` · `stance` · `refuses[]` · `lens` · `bar` · `emits` · `escalate_when` · `prompt`
- **As 10 técnicas**: postura padrão · negação de capacidade · escopo fechado · contrato de saída
  parseável · cegueira deliberada · lente atribuída · barra concreta · ônus invertido · escalada
  explícita · atalho proibido.
- **Lint**: nó de crítica sem `stance`, sem `emits` parseável ou com `resume_on_fail` não passa.
- **Gate**: mesmo fluxo, mesmo modelo, mesmas ferramentas — só a versão com persona reprova o
  artefato defeituoso que narra sucesso.

### B1 — Fragmentos e inlining · M
- **Onde**: `adw.py::load_spec` (L137, 38 linhas) + resolvedor novo
- **O quê**: `[[use]] module/as/with` resolvido por **inlining com prefixo de namespace**
  (`critic.judge`), com `[fragment] inputs/entry/exit`. Depois do inlining o spec é plano — motor,
  journal, resume e lint intocados.
- **Acompanha**: `touring flow explain <nome>` imprime o spec plano resolvido (a composição nunca é a
  única representação).
- **Kit inicial**: `recall-pack` · `diagnose-pack` · `fanout-lenses` · `critic-panel` · `gate-rust` ·
  `gate-quality50` · `conflict-guard` · `human-approve` · `phase-close` · `converge`.
- **Gate**: um fluxo composto e o monolito que ele substitui resolvem para o **mesmo grafo plano**
  e a **mesma sequência de execução**. (O gate original dizia "os 9 specs da library reescritos";
  nenhum foi migrado — ver Desvios.)

### B2 — `flow new` guiado · M (dep B1)
- **O quê**: prior-art obrigatório com veredito (`reuse|extend|supersede|create_new`) → destino em uma
  frase → jobs → arestas de dependência real → portão humano → composição a partir de fragmentos →
  `lint` + `test`.
- **Gate**: o fluxo criado passa `touring adw lint` e `touring adw test` sem edição manual.

### B4 — Fan-out de leitura · L
- **Onde**: `execute` (L524, 59 L) · `_run_single_node` (34 L) · `_track_class_d` (17 L) · `Journal` · lint
- **Restrito a ramos read-only** — todo fan-out que as fontes pedem é de leitura; escrita paralela
  fica fora (o `race` já resolve por cópia, e o `conflict-check` por write-set).
- **Três requisitos não-opcionais** (cada um é falha silenciosa):
  1. `merge` declarado (`collect|tally|concat`) — sem ele, 3 ramos gravam em `results[node.name]` e
     2 se perdem: o *last-write-wins* que o LangGraph documenta como default.
  2. `on_branch_fail` declarado — default `all` (falha-fechado). As docs do LangGraph **não**
     especificam esse caso; aqui a Lei L2 obriga.
  3. `_track_class_d` guarda **um slot** de `last_agent` — vira mapa por ramo, senão a Lei L3
     degrada justamente sob fan-out.
- **Mais**: `max_branches` obrigatório no fan-out dinâmico (o lint de orçamento multiplica o custo do
  template pelo teto).
- **Gate**: `kill -9` no meio de um fan-out de 3 ramos; o resume retoma sem reexecutar os concluídos.
  E um spec com 2 ramos gravando a mesma chave sem `merge` **falha o lint**.

### B5 — Painel de críticos · S (dep B1·B3·B4)
- **O quê**: fragmento `critic-panel` — N críticos com **lentes distintas**, `session = "fresh"`,
  `bar` com referente concreto, quorum contado por código.
- **Gate**: artefato defeituoso que narra sucesso → o painel reprova e o Class-D é registrado.

### B6 — Contrato de verificação · M (dep A2)
- **O quê**: três vereditos (`PASS|REJECT|ESCALATE`), postura padrão REJECT, detecção de estagnação
  no ledger antes do retry, parada por custo verificada em runtime, kill switch por run.
- **Por quê `ESCALATE`**: hoje um teste que **não pôde rodar** (ambiente quebrado) é indistinguível de
  um teste que rodou e reprovou — e gasta tentativas de agente contra o que nenhum agente resolve.
- **Gate**: gate cujo comando não pôde executar escala, sem consumir o orçamento de retry.

### C1 — Bloco `[purpose]` · S (dep B1) — **Rust**
- **Onde**: `crates/touring-server/src/portfolio/miner.rs::adw_description` (5 refs)
- **O quê**: `intent` · `when_to_use[]` · **`when_not_to_use[]` (obrigatório)** · `inputs[]` ·
  `produces[]` · `tags[]`; o minerador compõe o documento indexável com esse bloco + resumo dos nós.
- **Gate** (o teste que falha hoje): `touring portfolio "corrigir um bug com memória institucional e
  gate de verificação"` recupera `bugfix` no topo.

### C2 — Reivindicação atômica · L — **Rust · risco alto**
- **Onde**: `crates/touring-server-reasoning/src/reasoning/decomposer.rs` — `ready_subtasks` tem
  **66 referências**
- **Mitigação do risco**: entra como operação **nova** (ler+reivindicar num passo, com lease
  expirável), sem alterar a assinatura existente. Os 66 call-sites seguem intactos e migram por
  escolha.
- **Desenho**: frontier = aberto ∩ desbloqueado ∩ **não-reivindicado** (do `wayfinder/SKILL.md`).
- **Gate**: duas sessões concorrentes pedindo trabalho recebem subtasks distintos.

### C3 — Wayfinder no decompose · L (dep C2) — **Rust**
- **O quê**: tipo de ticket (`decision` vs `implementation`; subtipos research/prototype/grilling/task)
  + eixo HITL/AFK; névoa e frontier; **mapa como índice** (a decisão vive no ticket, o mapa guarda um
  ponteiro).
- **Teste de névoa** (literal da fonte): vira ticket se a pergunta pode ser **enunciada com nitidez
  agora** — não se pode ser respondida agora.
- **Gate**: objetivo com névoa alta produz decision tickets antes do plano; cada decisão do mapa
  aponta para seu ticket de origem.

## Definição de pronto — igual para as onze

- [ ] O gate da entrega executa e passa; comando registrado no bundle.
- [ ] Os **41 testes** de `test_adw.py` continuam verdes (baseline de regressão existente).
- [ ] `touring adw lint` verde nos **8 fluxos** da library e nos **6** instanciados.
      (A library tem 9 `.toml`; `tiers.toml` é o mapa tier→modelo, não um fluxo. O plano
      dizia "9 specs" aqui e "8" no resultado — a contagem certa é 8.)
- [ ] Entregas Rust, ainda: `cargo check` + `clippy -D warnings` + testes do crate + `touring e2e -j`
      sem regressão.
- [ ] Decisão persistida como memória com faceta (`#kind:lesson #domain:adw`).

## Riscos

| Risco | Onde | Prob. | Impacto | Mitigação |
|---|---|---|---|---|
| Mexer em `ready_subtasks` quebra consumidor (66 refs) | C2 | alta | alto | operação **nova**, sem alterar a assinatura; call-sites migram por escolha |
| Fan-out corrompe resultados por colisão de chave | B4 | média | alto | `merge` exigido pelo lint na colisão; nunca default silencioso |
| Fan-out degrada a Lei L3 | B4 | média | alto | `last_agent` vira mapa por ramo + teste sintético sob fan-out |
| Rebuild Rust quebra projetos pinados | C1·C2·C3 | média | alto | um único `update-touring`; toolchains imutáveis até `--force`; rollback por `touring update --rollback` |
| Fan-out dinâmico explode custo | B4 | média | médio | `max_branches` obrigatório; lint de orçamento multiplica pelo teto |
| Persona vira prosa decorativa | B3 | média | médio | campos lintáveis: sem `stance`/`emits` ou com `resume_on_fail`, não passa |
| Fragmentos viram indireção ilegível | B1 | média | baixo | `flow explain` imprime o spec plano |
| Mais agentes viram mais ruído | B5 | média | médio | lentes distintas obrigatórias; N críticos idênticos rejeitado |
| `[purpose]` vira propaganda | C1 | média | baixo | `when_not_to_use` obrigatório; lacuna exibida |
| Wayfinder vira waterfall | C3 | baixa | médio | tickets `prototype` — o antídoto que a fonte declara |
| **Superconstruir** | todas | alta | médio | primeiro corte de 3 entregas < 100 linhas; B4+ só depois de `B1→B2→C1` em uso real |

> Duas fontes independentes (Isenberg e Anthropic) alertam para o mesmo: adicionar complexidade
> **apenas** quando ela demonstravelmente melhora o resultado. O risco real deste trabalho não é
> subconstruir.

## Operar o DAG

```bash
TID=task_1787088570195129385

touring decompose ready    $TID          # o que pode começar agora
touring decompose get      $TID          # estado completo
touring decompose validate $TID          # 11 subtarefas, 0 ciclos

touring decompose update   $TID <ID> --status in_progress
touring decompose update   $TID <ID> --status completed

# baseline de regressão, antes e depois de cada entrega
python3 ~/.claude/skills/Touring/scripts/test_adw.py      # 41 testes
for s in ~/.claude/skills/Touring/adw-library/*.toml; do
  touring adw lint "$(basename "$s" .toml)"
done
```

## Resultado — 18/08/2026

**As onze entregas estão implementadas e cada gate foi executado.** DAG
`task_1787088570195129385`: `decompose ready` vazio.

| Onda | Entregas | Onde | Gate executado |
|---|---|---|---|
| A | A1 · A2 | `adw.py` + 4 specs | spec de 2 ciclos falha com exit 1 citando `x → y`; nenhuma aresta gate→agente da library retoma cega |
| B | B1 · B2 · B3 · B4 · B5 · B6 | `adw.py` 1.092 → 2.1k L | 113 testes (era 41); composição executa idêntica ao monolito; `kill -9` no meio do fan-out não reexecuta ramo concluído |
| C | C1 · C2 · C3 | `miner.rs` · `decompose.rs` · registry · CLI | 128 testes e2e Rust; `clippy -D warnings` limpo; `bugfix` sobe de ausente para **rank 1** (29,58 na entrega; 24,06 após a auditoria reordenar o documento, contra 13,98 do segundo colocado) |

### O que os gates mediram

- **122** testes em `test_adw.py` (baseline 41; 113 na entrega, +9 na auditoria) · **4.341** testes Rust nos 4 crates tocados (84 suítes),
  0 falhas · `clippy -D warnings` **exit 0**
- `touring adw lint` **0 erros** nos 8 fluxos da library e nos 6 instanciados; **0 avisos** exceto
  `hello-factory` (`agent \`build\`: no allowed_tools`), que é anterior a este trabalho.
  A auditoria de 18/08 mediu isso: a afirmação original de "0 avisos" não se sustentava
- `touring e2e -j` **0,8699 pass**, 0 fases falhas · `touring doctor` **6/6**
- REGRA #0: os 4 símbolos `pub` novos têm 17 · 4 · 10 · 7 consumidores — zero órfãos

### Correções fora do plano, dentro do escopo (REGRA #21)

1. **Injeção de shell na biblioteca implantada.** A `adw-library` vivia em três cópias e o
   guard estrutural varria só duas — não a que `library_dir()` lê e de onde `from-template`
   copia. A correção de posicionais chegara ao espelho e às instanciações e **nunca** à cópia
   implantada, que por dez dias entregou a todo projeto novo `touring memory recall
   '{{vars.symptom}}'` interpolado dentro de `bash -c`. Reproduzido, corrigido, e o guard
   agora cobre `adw.library_dir()`; `test_library_and_repo_mirror_agree` falha se os lados
   divergirem. **Um guard que não cobre o artefato em uso não é um guard.**
2. **`hello-factory` não podia passar.** O sentinela do gate apontava para o scratchpad de
   uma sessão morta; o diretório não existe, o `touch` falha e o gate reprova para sempre.
3. **Class-D degradava sob painel.** Um bloco `parallel` assumia a vaga de `last_agent` mesmo
   sem afirmar nada, apagando o "all done ✅" do trabalhador — e a reprovação seguinte deixava
   de contar como divergência. Agora um bloco só substitui a alegação se **fez** uma.
4. **Quatro tripwires de contagem de hooks, corrigidos um a um.** Os 4 verbos novos do
   `decompose` quebraram quatro literais duplicados em quatro arquivos — descobertos em três
   execuções sucessivas, exatamente o modo de falha que o comentário de um deles já registrava
   ("atualizar um subconjunto foi como `cli-memory-credit` chegou ao CI com três testes
   vermelhos"). Além de corrigir os quatro, `every_hook_count_tripwire_agrees_with_the_others`
   passou a comparar **todos os sítios de uma vez** — provado por execução: com um sítio
   divergente o teste falha nomeando os dois valores; restaurado, fica verde.

### Um falso positivo que quase virou relatório

`viz`, `wiring orphans` e `status` passaram a expirar em 15 s e o teste `test_graph_svg_output`
reprovou. Não era regressão: o **load average estava em 51,6** por causa das minhas próprias
suítes `cargo` concorrentes, e o daemon respondia fora da janela do cliente. Com a carga
assentada, `viz orphans` responde em **11 ms** e a suíte passa 15/15. Registrado porque o
sintoma — "todo o surface de viz quebrado" — é convincente e teria custado uma investigação
inteira a quem lesse só o vermelho.

### Desvios declarados

- **`flow new`/`flow explain` viraram `adw new`/`adw explain`.** `touring adw` encaminha
  argumentos ao script, então nasceram sem custo de deploy Rust; um comando `flow` exigiria
  um ciclo de build para a mesma capacidade.
- **Detecção de estagnação é opt-in** (`stagnation_rounds = N`, default 0). Gate que reprova
  em silêncio (`exit 1`, sem saída) é trivialmente "estagnado": ligada por padrão, cortaria em
  2 tentativas um spec cujo autor escreveu `max_retries = 5` — um default silencioso passando
  por cima de uma declaração explícita, exatamente o que este plano existe para eliminar.
- **Equivalência de composição provada por execução, não por diff vazio.** Namespacing muda os
  nomes (`recall.memory`), então specs compostos não são byte-idênticos aos monolíticos; o teste
  prova o que importa — mesmo grafo resolvido e **mesma sequência de execução**.
- **O espelho `client/` foi sincronizado e automatizado** (18/08, a pedido do Gabriel).
  O relato inicial de "26 arquivos" era uma medição parcial — só `client/skills`; o número real
  era 27 divergentes mais 5 ausentes. Junto vieram 134 artefatos gerados (6,5 MB, incluindo
  bancos de runtime) que estavam num repositório público porque o `.gitignore` ancorava
  `.claude/` só na raiz. Hoje: `scripts/sync-client-skills.py`, gate no `propagate-release.sh`
  e verificação de manifesto no CI.

## Auditoria cruzada — 18/08/2026

Toda afirmação deste plano e da [cartografia](cartografia-de-fluxos.html) foi reverificada por
execução, não por releitura. As onze entregas existem e a maioria dos gates se sustentou sob
verificação independente. Quatro afirmações não se sustentaram, e as quatro foram corrigidas.

### O que se sustentou

| Verificação | Evidência desta auditoria |
|---|---|
| A1 — ciclo sem saída atrás de um legítimo | spec sintético de 5 nós: `exit 1`, `cycle without exit: x → y` |
| B2 — criação compondo fragmentos | 3 fragmentos + 1 job próprio, 8 nós namespaced, prior-art com lacunas, `lint` 0/0, zero edição manual |
| B4 — fan-out com barreira | `explain` mostra o diamante composto; 17 testes cobrem merge, resume por ramo, recusa de ramo com ferramenta de escrita |
| C1 — descoberta por intento | `bugfix` em **rank 1** para o intento que antes devolvia um script de outro projeto (29,58 antes da auditoria; 24,06 depois de reordenar o documento, contra 13,98 do segundo) |
| C2 — reivindicação atômica | **6 sessões simultâneas** sobre 2 subtarefas: exatamente 2 reivindicações, 4 recusas, zero duplo-atribuição |
| C3 — decisão gateia implementação | `gated_by_open_decisions: true`, `untraceable_implementation` vazio quando a implementação aponta sua origem |
| REGRA #0 | os 4 símbolos `pub` novos: 17 · 4 · 10 · 7 consumidores |
| Saúde | `touring e2e -j` **0,8699 pass** · `clippy -D warnings` exit 0 · espelho `client/` limpo |

### O que a auditoria refutou

**1 — `adw test` não conseguia validar um fluxo recém-criado.** Este é o próprio gate B2:
"o fluxo criado passa `lint` **e** `test` sem edição manual". O lint passava; o test não.
`_agent_mock` exige uma gravação, um fluxo novo não tem nenhuma, então **todo** nó `agent`
falhava com `no recording` — e um nó `human` pausava a caminhada por cima disso. O gate era
inalcançável por construção justamente para os fluxos que o comando existe para produzir.
Corrigido: sob `adw test` — e só ali — um agente sem gravação é percorrido por um stub, e o
portão humano é auto-aprovado. Três guardas mantêm a correção honesta: o stub é **nomeado** no
relatório (`synthesized`), porque uma caminhada sintetizada nunca pode se ler como uma
regravada; um spec que **declara** `driver = "mock"` continua exigindo sua gravação, para que a
síntese não vaze para execução real; e o texto do stub evita toda palavra de sucesso, senão ele
forjaria a divergência Class-D que a Lei L3 existe para detectar.

**2 — `when_not_to_use` existia em zero fluxos.** O minerador Rust lê `[purpose]` e o `adw new`
exige o campo, mas os 8 fluxos publicados nunca receberam o bloco: consumidor sem produtor. A
mitigação declarada na tabela de riscos contra "`[purpose]` vira propaganda" não estava em vigor
em lugar nenhum. O ganho de rank do gate C1 veio do cabeçalho e dos passos, não do bloco.
Corrigido: os 8 fluxos declaram `[purpose]`, e cada `when_not_to_use` nomeia o fluxo que é o
certo do outro lado da fronteira — `hotfix` manda usar `bugfix` depois, `chore` manda usar
`feature` quando há decisão de API. Guardado por teste sobre a library inteira.

**3 — o campo obrigatório era truncado para fora do corpus.** Mesmo escrito, `when_not_to_use`
não chegava ao índice. O documento indexável é limitado a 600 caracteres e começava pelo
cabeçalho boilerplate — `Instantiate:`, `Run: --var …` — que consumia o orçamento. Medido: corte
em **606 caracteres**, no meio de `use explore-plan instead`. Corrigido invertendo a ordem: o
bloco curado primeiro, o cabeçalho com o que sobrar. O teste que já cobria o campo não pegou
isso porque seu fixture tinha cabeçalho de uma linha — a condição real nunca era exercida.

**4 — névoa não medida era reportada como `clear`.** O `frontier` respondia "nada incerto aqui"
sobre trabalho que ninguém avaliou: falha-aberta no único eixo que o Wayfinder existe para
exibir, e o enum já tinha `unknown`. Corrigido. Um teste existente afirmava `fog.clear == 2` e
portanto **codificava o defeito**; suas outras asserções — o default de `kind`, que tem
justificativa histórica documentada — foram mantidas.

### Desvios que faltavam declarar — e o que foi feito com eles (19/08)

Dois dos três deixaram de ser desvios. O terceiro permanece, com a razão explícita.

- **`fanout-lenses` — ENTREGUE.** O kit tem 11 peças: as 10 nomeadas nos dois documentos mais
  `prior-art`. Faltava justamente a que exercita *sectioning*, e sem ela `parallel` — a entrega
  mais cara — tinha um único consumidor no kit.
- **`quorum:N` e `best_effort` — IMPLEMENTADOS.** `on_branch_fail` aceita
  `all|any|ignore|best_effort|quorum:N`, e o lint recusa um quórum maior que o número de ramos
  (portão que só saberia falhar). `quorum:N` não era açúcar: é a condição que `all`/`any` não
  expressam — "ramos suficientes rodaram limpos" — e é pergunta diferente da que o gate de
  veredito responde ("o que os críticos concluíram"). `best_effort` é o nome das fontes para
  `ignore`; os dois valem.
- **`policy` — RECUSADO, com a razão dita.** A cartografia traz `policy` **e** `on_branch_fail`,
  e seu próprio exemplo combina `policy = "quorum:2"` com `on_branch_fail = "all"`: duas fontes
  de verdade para uma decisão, já se contradizendo na página. Aceitar as duas embarcaria essa
  contradição, então o lint recusa `policy` apontando o campo que detém a semântica.
- **Nenhum spec da library é composição** — desvio mantido. Os fragmentos chegam à produção por
  `adw new`, que emite `[[use]]`. Migrar os 8 renomearia seus nós (`recall.memory`), mudando
  prompts e nomes de journal de fluxos em uso: custo real, ganho secundário. Registrado como
  possível, não como dívida.

### O que a segunda auditoria encontrou (19/08)

**1 — Fan-out dinâmico não existia.** Os dois documentos especificam
`branches = "{{inputs.lenses}}"` com um `template` clonado por valor — o `Send` do LangGraph, a
forma que o gauntlet realmente tem, e a única expressão do padrão canônico
**orchestrator-workers** (subtarefas decididas em runtime). Só a forma estática existia, e
escrever a dinâmica despedaçava a string em **um ramo por caractere**. Sem ela, `max_branches`
guardava um número que o lint já sabia contar, e o 6º padrão canônico seguia descoberto.
Implementado: resolução em runtime, clone por valor com `{{branch.value}}`/`{{branch.index}}`,
teto verificado **antes** de rodar qualquer ramo (excedê-lo recusa o bloco em vez de truncar), e
a regra de lentes distintas na sua forma dinâmica — um template de lente fixa é recusado, porque
N clones idênticos compram uma opinião N vezes.

**2 — O inliner tinha o mesmo defeito.** Compor `fanout-lenses` num fluxo novo falhava: o
inlining iterava `branches` como lista e namespaçava caractere a caractere. `template` nomeia um
nó e precisava ser qualificado; a string precisava ficar intacta. Encontrado pela própria peça
nova, ao usá-la.

**3 — O espelho `client/` nunca adotou arquivo novo.** `is_noise` recebia o caminho **absoluto**,
que contém `~/.claude` — e `.claude` está na lista de ignorados. Logo **todo** arquivo do lado
live era classificado como ruído e a varredura contribuía nada: o espelho só sincronizava
caminhos que já conhecia. Passava despercebido porque *atualizações* funcionavam, então o
`--check` dizia CLEAN com um arquivo faltando. O teste dedicado passava porque sua raiz falsa
era um `tmp_path` sem `.claude` no caminho — a condição real nunca era exercida. `is_noise`
agora **recusa** caminho absoluto, o que transformou o defeito em erro alto e revelou um quarto
sítio de chamada que eu não havia encontrado.

## Referências

| Tópico | Local |
|---|---|
| Análise completa e fontes | [`cartografia-de-fluxos.html`](cartografia-de-fluxos.html) |
| Estratégia (6 movimentos originais) | [`strategy-2026-08-18-graph-engineering-flow-portfolio.md`](strategy-2026-08-18-graph-engineering-flow-portfolio.md) |
| Transcrições dos vídeos | [`sources/`](sources/) |
| Diagnósticos do OUTER | [`diagnostics/`](diagnostics/) |
| Histórico do bundle | [`log.md`](log.md) |
| Runner ADW | `~/.claude/skills/Touring/scripts/adw.py` (1.092 L) |
| Specs da library | `~/.claude/skills/Touring/adw-library/*.toml` (9) |

---

_OKF Plan · bundle `2026-08-18-graph-engineering-flow-portfolio` · DAG `task_1787088570195129385`_
