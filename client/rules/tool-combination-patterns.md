# Tool Combination Patterns — Combinação de Ferramentas (constitutional, auto-load)

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-05-19
> **Authority**: Gabriel Gadea | **Origin**: Rn2 §11 — síntese para a constituição TACO
>   (`~/.claude/downloads/Agentes_de_Codigo_Ferramentas_Essenciais-Rn2.md`, v3.0)
> **Enforcement em runtime**: hook `cli-suggest` (`crates/touring-cli/src/cli_suggester.rs`)
>   + módulo `crates/touring-cli/src/workflow/` (`stage.rs` · `antipattern.rs` · `advise.rs`)
> **Complementa**: `touring-decision-matrix.md` (C01-C12) · `file-metadata-first.md` · `VP-Scout.md`

---

## Princípio mestre — Signal-to-Token Ratio (STR)

O **contexto é o orçamento**. De toda operação de ferramenta:

> **STR = (informação acionável obtida) / (tokens de saída injetados no contexto)**

Uma operação de **alto STR** devolve exatamente o que decide o próximo passo. Uma de
**baixo STR** devolve volume — e envenena o contexto.

**Regra que ordena tudo**: toda combinação de ferramentas DEVE aumentar o STR a cada
passo — cada operação estreita o escopo e descarta ruído, de modo que a próxima recebe
um alvo menor e mais limpo. **Ordene da operação de maior STR para a de menor**; mapas
e listas (alto STR) **antes** de conteúdo (baixo STR).

| Operação | STR | Por quê |
|---|---|---|
| `rg -l 'AuthValidator'` | **alto** | 3 nomes de arquivo — decide o próximo Read |
| `rg 'data'` (termo genérico) | **baixíssimo** | 4.000 hits — satura, não decide nada |
| `Read` de 5.000 linhas p/ achar 1 função | **baixíssimo** | 99% do conteúdo é descartado |
| `Read` com `offset/limit` no nó certo | **alto** | só a janela relevante |
| `touring ast meta` antes de editar | **alto** | 8 campos decidem se vale tocar o arquivo |

**Observável**: o counter `gate-metrics` `enrichment_signal_to_token_*` rastreia bytes
de contexto injetado por operação de hook (TR-2). `touring gate-metrics -j` o expõe.

---

## Os 5 níveis de profundidade (N0-N4)

Cada nível é o anterior **composto**. A maturidade de um agente é o nível mais alto
que opera de forma consistente.

| Nível | Nome | Unidade | Exemplo |
|---|---|---|---|
| **N0** | Comando atômico | 1 invocação | `Grep "foo"` |
| **N1** | Combo | 2-3 ferramentas encadeadas | `Glob → Grep → Read` |
| **N2** | Workflow de operação | sequência completa fim-a-fim | bugfix: localizar→entender→editar→validar |
| **N3** | Estratégia de sessão | orquestração de workflows | paralelismo, subagents, checkpoints, plano |
| **N4** | Meta-loop | o sistema melhora a si mesmo | RL reward, memory, gotcha, drift |

Um agente que só faz N0-N1 repete erros; um que opera N4 fica melhor a cada sessão.
O stack Touring/TACO **é a implementação de N4**.

---

## Matriz de combinação 6×6 (N1) — de → para

Dada a ferramenta da linha, qual a próxima coluna e o que a transição faz.
`—` = combinação trivial / não-aplicável.

| de ↓ \ para → | Glob | Grep | ast-grep | Read | Edit | Bash |
|---|---|---|---|---|---|---|
| **Glob** | refinar padrão | **buscar no universo estreitado** | escopo p/ scan estrutural | ler arquivo único conhecido | — | listar p/ batch |
| **Grep** | — | re-buscar refinando (`-l`→`-n`) | **escalar: lexical → estrutural** | **`file:line` → janela paginada** | editar direto se match único | contar/agir sobre lista |
| **ast-grep** | — | confirmar com texto | refinar regra | ler o nó casado | **rewrite estrutural em massa** | aplicar + validar |
| **Read** | — | buscar símbolo visto no trecho | verificar estrutura do nó | ler janela adjacente | **confirmar contexto → mutar** | — |
| **Edit** | — | — | — | re-confirmar (raro) | próxima edição | **validar: build/test** |
| **Bash** | — | parsear stderr p/ símbolo | — | ler log/arquivo gerado | corrigir a partir do erro | próximo comando |

**Três transições de maior valor** (alto STR — sempre preferir):
- **Grep → Read**: a coordenada `file:line` vira `offset/limit` exatos — nunca `Read` antes de `Grep`.
- **Read → Edit**: o Read confirma o `old_string` exato → o Edit não falha por contexto divergente.
- **Edit → Bash**: toda mutação imediatamente validada (compilar/testar) — fecha o loop (P9).

---

## Os 10 padrões de combinação (P1-P10)

Vocabulário nomeado, à maneira de *design patterns*. Os padrões se aninham.

| # | Padrão | Definição | Sequência canônica | Quando |
|---|---|---|---|---|
| **P1** | **Funnel** | Da ferramenta barata à cara; **para assim que resolver** | `rg` → busca semântica → `ast-grep` → LSP | sempre que localizar |
| **P2** | **Pipeline** | Saída de A estreita o escopo de B | `Glob` → `Grep` → `Read` → `Edit` | toda operação multi-etapa |
| **P3** | **Fan-out** | Operações independentes num só bloco de mensagem | 3 `Grep` + 2 `Read` simultâneos | alvos sem dependência mútua |
| **P4** | **Map-First** | Estrutura/metadata antes de conteúdo | `touring ast meta` → `ast blast` → `Read` | antes de ler/editar arquivo grande |
| **P5** | **Probe-Confirm** | Sonda larga barata → confirmação estreita | `rg -l` → `rg -n -C2` no arquivo certo | localização ambígua |
| **P6** | **Mirror** | 2+ callsites similares comparados lado a lado | `ast find A` ‖ `ast find B` → diff | espelhar padrão / detectar assimetria-bug |
| **P7** | **Speculate** | Validar a mutação **antes** de aplicá-la | `rg -r` preview / `touring pre-edit` / `plan-speculate` | antes de Edit/Write |
| **P8** | **Subagent-Isolation** | Workflow que polui contexto → delegado; volta só a conclusão | `Task(tools=Read,Grep,Glob)` | exploração ampla, varredura |
| **P9** | **Verify-After** | Toda mutação seguida de validação | `Edit` → `cargo check` / `test` | sempre, pós-Edit |
| **P10** | **Checkpoint** | Persistir estado/lição antes de op longa/arriscada | `touring memory store` | refactor L3+, antes de risco |

Um refactor cross-file típico = P4 (mapear) + P6 (espelhar callsites) + P7 (especular)
+ edição + P9 (validar) + P10 (checkpoint). Os 10 padrões estão codificados como
`enum WorkflowPattern` em `crates/touring-cli/src/workflow/advise.rs`.

---

## Os 10 antipadrões de combinação (A1-A10)

| # | Antipadrão | Por quê é ruim | Correto |
|---|---|---|---|
| **A1** | `Read` antes de `Grep` | lê volume p/ achar o que `Grep` daria em `file:line` | P5: localizar, depois ler |
| **A2** | `grep`/`rg`/`cat`/`find`/`sed` no Bash p/ inspeção | perde paginação, filtros, parsing | tools `Grep`/`Read`/`Glob`/`Edit` |
| **A3** | `ast-grep`/LSP para o que regex resolveria | custo de parsing/daemon sem ganho | P1: subir de camada só se a anterior falhar |
| **A4** | Editar callsites um a um | perde um; assimetria → bug | P6 mapeia todos antes de mutar |
| **A5** | Mutar sem validar | regressão silenciosa | P9: `Edit` → build/test sempre |
| **A6** | Tratar `rg` exit 1 como erro | "nada encontrado" é resultado válido | exit 1 ≠ falha (`0`=match, `1`=sem match, `2`=erro) |
| **A7** | Subagente para tarefa trivial | overhead de spawn > trabalho | delegar só se (a) lê muitos arquivos + (b) independente + (c) conclusão curta |
| **A8** | `Read` em série do que poderia ser fan-out | round-trips desperdiçados | P3: bloco paralelo |
| **A9** | `-P`/`--pcre2` por padrão | reintroduz backtracking (ReDoS) | motor linear default; PCRE2 só sob necessidade real |
| **A10** | Pular o mapa, ler conteúdo direto | baixo STR, satura contexto | P4: Map-First sempre |

Os antipadrões de maior frequência empírica (forense de 575.821 tool-calls — Rn3 §1)
são detectados em runtime pelo `enum AntipatternKind` (`workflow/antipattern.rs`):
`BashGrepRaw` (35.975×), `BashCatHeadTail` (44.487×), `BashFind` (3.494×),
`EditWithoutRead` (2.273×), `ReadWithoutLocate` (46.307×), `BashPcre2Default` (A9).
O hook `cli-suggest` injeta a conversão canônica no `additionalContext`.

---

## Modelo de maturidade (M0-M4)

| Nível | Característica | Sintoma observável |
|---|---|---|
| **M0 — Ingênuo** | `grep` cru, `cat` arquivo inteiro, `sed -i` | "alucina"; contexto saturado de `node_modules` |
| **M1 — Higiênico** | ripgrep, `offset/limit`, exclusões | contexto limpo; ainda trata ferramentas em isolamento |
| **M2 — Combinatório** | aplica P1-P10; pipelines de alto STR | estreita escopo a cada passo; valida pós-mutação |
| **M3 — Estratégico** | paralelismo, subagents, planejamento, checkpoints | janela de contexto sob controle em sessões longas |
| **M4 — Auto-evolutivo** | meta-loop: reward, memory, gotcha, drift | melhora mensurável entre sessões |

**Alvo TACO**: operar consistentemente em **M4**.

---

## Acoplamento com a Decision Matrix C01-C12

Esta rule é o **vocabulário de combinação**; a `touring-decision-matrix.md` é o
**mapa tarefa → comandos**. Aplicam-se juntas:

| Categoria (C01-C12) | Padrões de combinação dominantes |
|---|---|
| C01 READ-SCAN / C02 READ-COMPREHEND | P4 Map-First, P8 Subagent-Isolation |
| C03 SYMBOL-LOOKUP | P1 Funnel, P5 Probe-Confirm |
| C04 PRE-EDIT-TRIAGE | P4 Map-First, P7 Speculate |
| C05/C06 EDIT-MINOR/MAJOR | P2 Pipeline, P6 Mirror, P9 Verify-After, P10 Checkpoint |
| C07 NEW-SYMBOL | P7 Speculate, P9 Verify-After |
| C08 CROSS-CALLER-COMPARE | **P6 Mirror** (obrigatório) |
| C09 DEBUG-ROOT-CAUSE | P5 Probe-Confirm, P1 Funnel, P10 Checkpoint |
| C10 ARCHITECTURAL / C11 DEPENDENCY-FLOW | P4 Map-First, P8 Subagent-Isolation |
| C12 SYSTEM-HEALTH | P3 Fan-out (`doctor`+`status`+`e2e` paralelos) |

**Enforcement em runtime**: o hook `cli-suggest` (`PreToolUse` para Bash/Read/Edit/
Write/Grep/Glob/Task/WebFetch/WebSearch) consome o módulo `workflow/`, infere o
`WorkflowStage` atual e injeta no `additionalContext` (a) o estágio, (b) o próximo
passo de elite (`advise_next_step`), e (c) qualquer `AntipatternKind` detectado com
sua contagem histórica e conversão canônica. Skip → operação opera em M1, não M4.

---

## Referências cruzadas

| Tópico | Local |
|---|---|
| Decision Matrix tarefa → comandos | `~/.claude/rules/touring-decision-matrix.md` |
| File metadata first (P4) | `~/.claude/rules/file-metadata-first.md` |
| VP-Scout cadeias de verificação | `~/.claude/skills/Touring/references/VP-Scout-rule.md` |
| Code Execution Gateway (sandbox de execução) | `~/.claude/skills/Touring/references/code-execution-gateway.md` |
| Documento-fonte Rn2 (combinações²) | `~/.claude/downloads/Agentes_de_Codigo_Ferramentas_Essenciais-Rn2.md` |
| Documento-fonte Rn3 (validação forense) | `~/.claude/downloads/Agentes_de_Codigo_Ferramentas_Essenciais-Rn3.md` |
| Módulo runtime (`WorkflowStage`/`WorkflowPattern`/`AntipatternKind`) | `crates/touring-cli/src/workflow/` |
| Hook de enforcement | `crates/touring-cli/src/cli_suggester.rs` |

---

_v1.0 — 2026-05-19 | Materializa Rn2 §11: STR, níveis N0-N4, matriz 6×6, padrões_
_P1-P10, antipadrões A1-A10, maturidade M0-M4 como rule constitucional auto-load._
