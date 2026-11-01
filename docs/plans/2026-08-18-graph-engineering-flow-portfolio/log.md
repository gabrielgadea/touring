---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-18-graph-engineering-flow-portfolio
tags: [loop, log]
timestamp: 2026-08-18T09:48:20.926308-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-18T18:35-03:00 — plano consolidado
Três rodadas de análise fechadas (transcrições → fontes primárias → Context7) mais a lacuna de
persona apontada pelo Gabriel. Escrito `plan.md` (OKF Plan) com 11 entregas em 3 ondas, separadas
pela fronteira script/Rust; DAG `task_1787088570195129385` criado e validado (0 ciclos).
Três defeitos reproduzidos por execução: `_lint_cycles` (ciclo infinito passa o lint),
`ready_subtasks` (sem claim atômico, 66 refs) e retry sem feedback do gate (4 de 5 specs).
Medição refutou o argumento de performance do fan-out: ganho real ~15% no `strategy-loop`,
irrelevante no `audit`. Caminho crítico do pedido corrigido para `B1 → B2 → C1`.

## 2026-08-18T19:51:26.282058-03:00 — P1 done

11 entregas do portfolio de fluxos ADW implementadas e verificadas: fragmentos componiveis, persona por no, fan-out read-only com barreira, contrato de verificacao PASS/REJECT/ESCALATE, criacao guiada, portfolio por proposito, claim atomico e Wayfinder. Mais 3 defeitos fora do plano, incluindo injecao de shell na biblioteca implantada.

## 2026-08-18T20:15-03:00 — implementação completa das 11 entregas

Executado o `plan.md` inteiro. Onda A e B em `adw.py` (1.092 → 1.560 linhas), onda C em Rust
(`miner.rs`, `decompose.rs`, `hook_registry.rs`, `cli/decompose.rs`) num único deploy via
`update-touring`.

**Gates** — `test_adw.py` 113 (baseline 41) · e2e Rust 127 · suítes completas dos 4 crates
tocados 1.658 · `clippy -D warnings` exit 0 · `touring adw lint` 0/0 em 8 specs da library e 6
instanciados · `touring e2e -j` 0,8699 pass · `doctor` 6/6 · `loop_converged.py` **exit 0**
(tier Platinum 0,9361) · `loop_doc_link_gate.py` CLEAN.

**C1 provado ao vivo**: `touring portfolio "corrigir um bug com memória institucional e gate de
verificação"` devolvia um script Python de outro projeto e nenhum ADW; agora devolve `bugfix`
em **1º lugar (29,58)**. Corpus 10.863 → 11.593.

**C2 provado ao vivo**: duas sessões pedindo trabalho ao mesmo DAG receberam `D-01` e `I-02` —
subtarefas distintas; a terceira recebeu `claimed:false` em vez de trabalho duplicado.

**Três correções fora do plano** (REGRA #21): injeção de shell na `adw-library` implantada (a
que `from-template` copia — o guard estrutural varria só os espelhos); `hello-factory` com
sentinela num scratchpad morto, incapaz de passar; e Class-D degradando sob painel de críticos.

Desvios declarados e evidência por entrega: `/plan.md#resultado--18082026`.

## 2026-08-18T22:15 — Auditoria cruzada das onze entregas

Reverificação por execução de toda afirmação do `/plan.md` e da
`/cartografia-de-fluxos.html`. A maioria dos gates se sustentou sob teste independente —
incluindo o mais forte: **6 sessões simultâneas** disputando 2 subtarefas produziram
exatamente 2 reivindicações e 4 recusas.

**Quatro afirmações não se sustentaram, e as quatro foram corrigidas:**

1. O gate do próprio B2 era inalcançável: `adw test` exige gravações que um fluxo
   recém-criado não tem, e um nó `human` pausava a caminhada. Agora `adw test` percorre
   o grafo com stubs **nomeados no relatório**, sem deixar a síntese vazar para execução real.
2. `when_not_to_use` — declarado obrigatório, e a mitigação contra "`[purpose]` vira
   propaganda" — existia em **zero** fluxos publicados. Os 8 agora o declaram.
3. Mesmo escrito, o campo era **truncado para fora do corpus**: o documento indexável
   começava pelo cabeçalho boilerplate e o corte caía em 606 caracteres. O bloco curado
   passou a vir primeiro.
4. Névoa não medida era reportada como `clear` no `frontier` — falha-aberta no único eixo
   que o Wayfinder existe para exibir. Agora é `unknown`; o teste que afirmava o contrário
   codificava o defeito.

Gates: 122 testes `test_adw.py` · 128 e2e Rust · 1.492 + 338 lib · `clippy -D warnings`
exit 0 · `touring e2e -j` 0,8699 pass · doctor 6/6 · espelho `client/` limpo.
Detalhe: `/plan.md#auditoria-cruzada--18082026`.

## 2026-08-19T01:40 — Segunda auditoria cruzada: os desvios viraram implementação

O pedido mudou de "auditar" para "garantir que tudo foi implementado". Verificação
enumerável dos dois documentos: **10/10** técnicas de postura, **8/8** práticas do
Context7, sectioning e voting presentes. Três lacunas reais, e as três eram o mesmo tema.

**Fan-out dinâmico não existia** — `branches = "{{inputs.lenses}}"` com `template`
clonado por valor, o `Send` do LangGraph e a forma real do gauntlet. Escrevê-lo
despedaçava a string em um ramo por caractere. Sem ele o 6º padrão canônico
(*orchestrator-workers*) seguia descoberto e `max_branches` guardava um número que o
lint já contava. Implementado com teto verificado **antes** de rodar qualquer ramo.

**`fanout-lenses` entregue** — a peça do kit que exercita sectioning, ausente até aqui.
**`quorum:N` e `best_effort` implementados**; `policy` recusado pelo lint, porque o
exemplo da cartografia o contradiz com `on_branch_fail` na mesma linha.

Dois defeitos apareceram ao usar o que foi construído: o **inliner** tinha a mesma
confusão string-vs-lista, e o **espelho `client/` nunca adotou arquivo novo** —
`is_noise` recebia caminho absoluto, que contém `~/.claude`, e `.claude` está na lista
de ignorados, então todo arquivo live era ruído. O `--check` dizia CLEAN com arquivo
faltando; o teste passava porque sua raiz falsa não tinha `.claude` no caminho.

Gates: 136 testes `test_adw.py` · 21 guards do repo · espelho 272 arquivos limpo ·
lint 0 erros. Detalhe: `/plan.md#o-que-a-segunda-auditoria-encontrou-1908`.

## 2026-08-19T02:20 — Documentação profissional do que foi construído

Documentação escrita **dentro da arquitetura que o repo já tem** — Diátaxis
(`tutorial/how-to/reference/explanation`), ADRs em MADR e CHANGELOG em
Keep a Changelog — em vez de impor uma estrutura nova.

- `docs/explanation/adw-flow-portfolio.md` — conceito + referência completa do
  portfólio: schema do spec, `[purpose]`, os 11 fragmentos, fan-out estático e
  dinâmico, persona, contrato de verificação, erros de lint e o que fazer.
- `docs/how-to/author-an-adw-flow.md` — no estilo curto da casa (Goal/Steps/
  Verify/Pitfalls), incluindo as três armadilhas que já custaram caro.
- **ADRs 0002-0005** — composição por inlining (e por que não subgrafo em
  runtime); fan-out read-only com políticas declaradas (e por que `policy` é
  recusado); contrato de três vereditos (e por que estagnação é opt-in);
  reivindicação atômica (e por que `fog` não faz default para `clear`).
- `CHANGELOG.md` — entradas 30.4.0 e 30.4.1; o arquivo saltava de 30.3.0.
- `README.md` + `docs/adr/README.md` — índices atualizados.

**Dois defeitos que a própria documentação revelou**: `gen_reference --validate`
estava vermelho — os 4 hooks do `decompose` (C2/C3) tinham deixado
`docs/reference/{hooks,modules}.md` obsoletos; regenerado, 234 hooks, gate verde.
E `CONTRIBUTING.md` prometia "os gates" sem listar três que o CI já exige —
um contribuidor seguindo o doc levaria build vermelho.

Links verificados por execução (0 quebrados); todo comando citado conferido por
`--help` antes de ser documentado.
