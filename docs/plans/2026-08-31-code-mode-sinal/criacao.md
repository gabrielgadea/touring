---
type: Criacao
title: "code-mode-sinal-canal — canal de leitura read-only no sandbox expondo sinais dos hooks do touring"
plan_id: 2026-08-31-code-mode-sinal
tags: [briah, criacao, code-mode, hooks, sandbox, signal-injection, sdk]
timestamp: 2026-08-31T13:00:00-03:00
okf_version: "0.1"
---

# code-mode-sinal-canal — a concepção (Briah)

## A Emanação

Quero que venha a existir **um canal de leitura read-only** que exponha, dentro do sandbox do code mode, **os mesmos sinais que os hooks do touring injetam** (talvez até sinais aperfeiçoados e mais ricos) — meta-informação, predicate gates, sugestões, métricas, memory recall, gotcha match, pre-edit scores, file metadata, wiring impact, learning rewards. A criação nasce para que o programa escrito pelo modelo dentro do code mode tenha a mesma excelência que uma chamada CLI enriquecida pelos hooks do touring.

## O Telos

**Para que** o programa escrito pelo modelo tenha a mesma excelência que uma chamada CLI enriquecida pelos hooks do touring. O usuário imediato é o programador que escreve o programa Python dentro do sandbox do code mode — quando o programa roda, ele ganha toda a inteligência de análise que um CLI touring tem. **Para que** isso transforme a qualidade do código gerado em produção (e eleve o piso de robustez, segurança e precisão). A finalidade última: fechar o abismo entre "touring CLI enriquecido" e "touring code mode isolado", unificando a experiência do código assistido por IA.

## A Imagem

Fecho os olhos e a criação está pronta: o code mode operando com toda a inteligência de análise e previsibilidade de código do touring, gerando **resultados incríveis, extraordinários, ricos, precisos e surpreendentes**. O programador escreve 30 linhas em Python dentro do sandbox; o programa lê `touring.ast_meta(file)` e vê `blast_radius=4, quality=0.85`; lê `touring.gotcha_match(file)` que avisa "avoid shell=True (F2.1 Diamond)"; lê `touring.wiring_orphans()` que lista 3 candidatos a wiring; lê `touring.memory_recall("edit:authentication")` que devolve 2 lições canônicas. O programa fecha com um patch correto, validado pelos mesmos gates que o CLI faria — sem que o programador tivesse que conhecer cada gate. A cena: o programador vê uma tabela rica de sinais ao lado do código, vê o gate falar "EXITS_PASS, BEST_PRACTICES_OK, MEMORY_USED=2", e a criação cumpre o que prometeu.

## A Fronteira

Esta criação **NÃO** é, **NÃO** será, e **NÃO** fará:

1. **NÃO** é "transportar tudo do touring pro sandbox" — Landlock medido nega `~/.claude` (Permission denied medido 30/08/2026); há um limite físico do kernel que o canal tem que respeitar. Fora desse limite, a criação não existe.
2. **NÃO** é um sistema novo — é uma **extensão orgânica** do code mode existente, não uma substituição. O code mode continua sendo o mesmo; o canal é uma nova porta de entrada de sinais, não uma reescrita.
3. **NÃO** é um puxadinho — não é um script opcional que vive numa pasta `tools/` esperando alguém descobrir. **NÃO** é opt-in: o gate `BestPracticesGate` cobre aderência ao uso do canal, então o agente é forçado a invocá-lo quando escreve código que toca arquivos indexados.

## A Cadeia

Do estado atual à Imagem, **se** cada elo, **então** o seguinte:

1. **se** catalogarmos todos os sinais injetados pelos 24 hooks do touring → **então** sabemos o inventário exato a expor pelo canal (meta-informação, predicate gates, sugestões, métricas, memory recall, gotcha match, pre-edit scores, file metadata, wiring impact, learning rewards).
2. **se** classificarmos os sinais por tier (essencial / útil / opcional) → **então** a porta de entrada tem critério de prioridade mensurável e o canal entrega os essenciais com latência mínima.
3. **se** definirmos um canal de leitura read-only respeitando Landlock (SEG-2 já concedeu `~/.claude/{skills,rules,agents,commands}` read-only em 30.4.28) → **então** o sandbox ganha o sinal sem expor paths sensíveis (chaves, segredos, runtime state) — então a contenção do kernel é preservada por construção.
4. **se** o canal ler de um arquivo espelho populado por um hook PostToolUse que sincroniza os sinais mais recentes → **então** não viola a contenção do kernel e o sinal está sempre fresco (drift < 1 ciclo de hook).
5. **se** o code mode invocar o canal via SDK tipado no prompt (typed stub, `touring.ast_meta()`, `touring.gotcha_match()`, `touring.wiring_orphans()`, `touring.memory_recall()`, `touring.pre_edit()`) → **então** o modelo ganha todas as funções do CLI dentro do programa e o tipo serve de restrição de raciocínio (sem alucinação de parâmetro).
6. **se** o gate `BestPracticesGate` cobrir aderência ao uso do canal (invocação obrigatória de `touring.ast_meta()` antes de Write/Edit; invocação obrigatória de `touring.pre_edit` quando `blast_radius > 5`) → **então** o agente é forçado a usar o canal quando escreve código que toca arquivos indexados e o canal vira parte da esteira, não opcional.

## O Custo Invisível

Pré-mortem: daqui a 6 meses, a manchete é **"Canal de leitura do code mode é descontinuado por drift"** — os sinais copiados à mão ficam stale em 30 dias e ninguém detecta (Landlock mudou em kernel novo e o canal parou de ler metade dos hooks); o gate `BestPracticesGate` foi marcado como opt-in e ignorado; a última release removeu 3 dos 5 hooks expostos sem atualizar a porta; o canal vira ruído.

**As alternativas descartadas** (com o porquê):

- (a) **Forkear o code mode com cópia total do touring** — descartada: custo 6× maior de manutenção e duas bases divergentes em 6 meses. Trade-off inaceitável.
- (b) **Hook chain duplicado entre touring e code mode** — descartada: drift dobrado, dois sistemas para manter sincronizados, regressões em dobro.
- (c) **Pós-processamento dos sinais antes de injetar** — descartada: vira latência inaceitável no turno do code mode (cada programa leva ~5-30s a mais); degrada a UX sem entregar benefício proporcional.
- (d) **Confiar só no `memory recall` e ignorar os outros sinais** — descartada: perde os sinais estruturados (pre-edit scores, file metadata, wiring) que são o maior valor do touring.

O que mata a criação, no fim, é **deixar o canal ser opt-in** — o gate tem que ser fail-closed, não advisory.

## O Pronto

**Pronto** é uma conjunção (AND) de 4 critérios medidos — nenhum sozinho basta:

1. **Comando verde** (verificável por exit code): `python3 scripts/test_code_mode_signal_injection.py --verbose` retorna **exit 0** com **N ≥ 8 testes verdes** (cada teste exercita uma função do SDK tipado: `ast_meta`, `gotcha_match`, `wiring_orphans`, `memory_recall`, `pre_edit`, `tantivy_search`, `parallel`, `query`).
2. **Número mensurável** (verificável por KPI composto): `touring kpi -j` retorna `code_mode_signal_use.composite ≥ 0.80` em produção **≥ 7 dias** consecutivos (cobre flutuação de turno e cold-start).
3. **Gate** (verificável por script de elite): `python3 docs/elite_aggregate.py --check` retorna **≥ Gold (0.80)** no escopo `code-mode-sinal` — todas as 13 gates (architecture, security_advisories, performance, testing, documentation, ci_cd_devops, modularization, scalability, extensibility, craftsmanship, dependencies, ux, product_docs) acima do piso.
4. **Evento** (verificável por release + ADW): a próxima release propagada (**v30.5.x**) inclui o canal + o gate, e **1 ADW** da library roda end-to-end produzindo código que usa o canal pelo menos 1 vez — prova comportamental ao vivo, não por versão.

Pronto é quando **todos os 4** passam — não sensação, não porcentagem, não "está bonito". **Exit codes + KPI composto + gate elite + release propagada com prova viva.**

## O Prompt Perfeito

> Construa um **canal de leitura read-only** que exponha, dentro do sandbox do code mode, **exatamente os mesmos sinais** que os 24 hooks do touring injetam no contexto do LLM (meta-informação, predicate gates, sugestões, métricas, memory recall, gotcha match, pre-edit scores, file metadata, wiring impact, learning rewards, parallel fan-out, query escape hatch).
>
> **API**: SDK tipada no prompt do code mode — `touring.ast_meta(file)`, `touring.gotcha_match(file)`, `touring.wiring_orphans()`, `touring.memory_recall(topic)`, `touring.pre_edit(file)`, `touring.tantivy_search(query)`, `touring.parallel(calls)`, `touring.query(hook, payload)`. Cada função retorna JSON canônico tipado, nunca prosa. Erros estruturados (taxonomia ortogonal: exception/timeout/abort/proc-exit/invalid-output/output-limit) com mensagem que ensina a correção.
>
> **Contenção**: o canal DEVE respeitar Landlock (read-only no `~/.claude/{skills,rules,agents,commands}` via SEG-2 já concedido em 30.4.28; nada fora desse allowlist). O canal lê de um arquivo espelho populado por um hook PostToolUse que sincroniza os sinais mais recentes — assim a contenção do kernel é preservada por construção.
>
> **Gate fail-closed**: `BestPracticesGate` deve cobrir aderência ao uso do canal — invocação obrigatória de `touring.ast_meta()` antes de Write/Edit, invocação obrigatória de `touring.pre_edit` quando `blast_radius > 5`. O gate é fail-closed (bloqueia o gate se aderência < 1.0), não advisory.
>
> **Pronto** (AND dos 4 critérios): (a) `python3 scripts/test_code_mode_signal_injection.py --verbose` exit 0 com N≥8 testes verdes; (b) `touring kpi -j` retorna `code_mode_signal_use.composite ≥ 0.80` em produção ≥ 7 dias; (c) `python3 docs/elite_aggregate.py --check` ≥ Gold (0.80) no escopo `code-mode-sinal`; (d) a próxima release propagada (v30.5.x) inclui canal+gate, e 1 ADW roda end-to-end usando o canal pelo menos 1 vez.
>
> **Anti-goals firmes**: NÃO é forkear o code mode com cópia total (custo 6×); NÃO é hook chain duplicado (drift dobrado); NÃO é transporte de todos os arquivos do touring (Landlock medido nega `~/.claude`); NÃO é sistema novo (é extensão orgânica); NÃO é puxadinho opt-in (é fail-closed via gate); NÃO é pós-processamento dos sinais (vira latência); NÃO é só memory recall (perde os sinais estruturados).