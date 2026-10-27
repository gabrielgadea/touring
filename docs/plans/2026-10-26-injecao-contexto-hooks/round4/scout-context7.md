---
plan_id: 2026-10-26-injecao-contexto-hooks
type: Research
title: "Context7 (docs oficiais Anthropic/Claude Code) — o que a Anthropic diz sobre injetar contexto em hooks"
timestamp: 2026-10-26
author: touring-scouter (round4)
context7_available: true
library_ids:
  - /websites/code_claude
  - /anthropics/claude-code
  - /llmstxt/code_claude_llms_txt
evidence_dir: round4/evidence
---

# Context7 — as melhores práticas oficiais da Anthropic para injeção de contexto

## 1. Procedência

### 1.1 O MCP do Context7 está autenticado? **Não — e isso é um resultado, não uma falha**

A tarefa pedia `mcp__context7__resolve-library-id` + `mcp__context7__query-docs`. O caminho MCP
**falha por autenticação**, verificado duas vezes (início e fim da rodada):

```
POST https://mcp.context7.com/mcp?client=claude-code-plugin
{"jsonrpc":"2.0","error":{"code":-32001,"message":"Authentication required. Please authenticate to use this MCP server."},"id":null}
```

O mesmo servidor responde **200** pela REST API oficial, que é o **mesmo backend e os mesmos
library ids** que o MCP serviria. O `claude mcp list` confirma o estado:

```
plugin:context7:context7: https://mcp.context7.com/mcp?client=claude-code-plugin (HTTP) - ! Needs authentication
```

**Decisão de método**: usei a REST API `/api/v1` do Context7 em vez do MCP. Isso é
*substituição de canal, não de fonte* — a evidência continua sendo o corpus indexado do
Context7, do qual extraí os trechos com o `Source:` original de cada página. `context7_available: true`
com a ressalva: **disponível via REST, indisponível via MCP autenticado**.

### 1.2 Library ids resolvidos

| library id | tokens | snippets | trust | bench | verified | lastUpdate |
|---|---:|---:|---:|---:|:-:|---|
| `/websites/code_claude` | 2.440.943 | 19.412 | 9,9 | 80,34 | sim | 2026-10-23 |
| `/anthropics/claude-code` | 143.318 | 1.573 | 9,5 | 58,89 | sim | 2026-10-26 |
| `/llmstxt/code_claude_llms_txt` | 2.019.993 | 10.893 | 9,9 | 80,27 | sim | 2026-10-03 |

Os três são **first-party Anthropic**: o site de documentação `code.claude.com`, o repositório
`github.com/anthropics/claude-code` (inclusive `plugins/plugin-dev/skills/hook-development/`, o
material oficial de como escrever hooks) e o `llms.txt` do site. `verified: true` nos três.

### 1.3 Queries e fetches efetivamente executados

**Buscas** (4 pedidas + 7 complementares): `"hook additionalContext systemMessage best practices context injection"`,
`"context engineering agents"`, `"anthropic context engineering blog"`, `"claude code best practices context"`,
`"claude code hook additionalContext"`, `"effective context engineering for AI agents"`,
`"building effective agents anthropic"`, `"claude code context window management auto compact"`,
`"writing effective tools for agents token efficiency"`, `"anthropic docs api reference"`.

**Fetches**: 45 topical (`?type=txt&topic=`) + 20 por tópico nomeado (`hook-development`,
`context-management`, `plugin-development`, `hooks-development`, `agents-md`, `subagents`,
`skills`, `prompt-engineering-guide`, `mods`, `mods-events`, `plugins-mods`, `hooks-security`,
`self-hosted-environments-configuration`, …) = **65 documentos baixados**, ~45 KB cada, salvos
íntegros em `round4/evidence/` (**71 `.txt` + 6 `.json`; 57 MB**).

**O que NÃO respondeu**: 9 library ids Anthropic-first-party candidatos
(`/websites/anthropic_com`, `/websites/anthropic_engineering`, `/websites/docs_anthropic_com`,
`/anthropics/engineering-blog`, …) → **HTTP 404 em todos**. O Context7 **não tem** o blog de
engineering da Anthropic (`anthropic.com/engineering`), nem o *Effective context engineering for
AI agents*. Isso é uma lacuna real e está registrada na §6.

### 1.4 Regra de citação usada aqui

- **CITADO** = trecho literal, com o `Source:` original da página (que o Context7 preserva).
- **PARÁFRASE** = minha leitura, marcada como tal.
- Nada de API, campo, URL ou nome de documento que eu não tenha lido.

---

## 2. O que a Anthropic diz sobre contexto injetado

### 2.1 O canal `additionalContext` é o canal *de modelo*; ele vira um system reminder

**CITADO** (`code.claude.com/docs/en/hooks`, via `/websites/code_claude` topic `hooks`, linha 1056):

> The `additionalContext` field passes a string from your hook into Claude's context window.
> Claude Code wraps the string in a system reminder and inserts it into the conversation at
> the point where the hook fired. Claude reads the reminder on the next model request, but it
> doesn't appear as a chat message in the interface.

**PARÁFRASE**: o texto injetado não é "fala do usuário" nem "mensagem de chat" — é um
*system reminder* posicionado no ponto em que o hook disparou, lido no **próximo** request ao modelo.
Três consequências que o Touring já sente na prática:

1. **Não aparece na interface** — o operador não vê o que o modelo viu. Uma injeção inútil é
   invisível na tela: só a telemetria denuncia.
2. **Só é lida no request seguinte** — injetar num `PreToolUse` que devolve `deny` não gasta
   contexto do modelo que não vai acontecer.
3. **O ponto de inserção é o ponto do hook** — o mesmo texto lido em pontos diferentes é lido
   com pesos diferentes.

### 2.2 A proveniencia do reminder: "contexto da aplicacao, nao mensagem do usuario"

**CITADO** (`code.claude.com/docs/en/agent-sdk/modifying-system-prompts`, via `/websites/code_claude`
topic `context`, linha 1224) — a frase que a propria Anthropic manda colocar no system prompt
quando o embedding e customizado:

> The application adds system reminders to this conversation. Treat them as context from the
> application, not as messages from the user.

**PARÁFRASE (importante)**: a Anthropic diz ao modelo que reminder **nao e ordem do usuario**.
Isso muda a natureza do que esta injetado: um texto que *manda* ("MUST rodar `touring ast meta`")
chega ao modelo como **contexto de aplicacao** — informacao, nao instrucao do usuario. Um
"sermao" injetado assim e, na pratica, um texto informativo sobre regras que ninguem pediu para
obedecer. Isso **mede e explica** o resultado de 0,3% de adesao proximal do Touring.

### 2.3 Quando o reminder aparece (por evento)

**CITADO** (mesma fonte, secao "Where the Reminder Appears", linhas 338-343):

> - SessionStart and SubagentStart: at the start of the conversation, before the first prompt
> - UserPromptSubmit and UserPromptExpansion: alongside the submitted prompt
> - PreToolUse, PostToolUse, PostToolUseFailure, and PostToolBatch: next to the tool result
> - Stop and SubagentStop: at the end of the turn. The conversation continues so Claude can act on the feedback.
> - PostModelSwitch: with the next request after the switch.

**PARÁFRASE**: a doc **nao diz** "injete o maximo que puder". Ela **posiciona** cada canal. O
Touring tem 26 hooks; cada um decide *onde* o texto entra. Um `PostToolUse` que fala de blast
radius entra **junto do resultado da ferramenta** — o texto mais proximo da decisao do modelo.

### 2.4 Duas regras de silencio, ditas pela Anthropic (nao por mim)

**(a) O exemplo oficial de `prompt.submit` — "passes through unchanged"**

**CITADO** (`code.claude.com/docs/en/plugins/mods/events`, via `/websites/code_claude` topic
`context`, linha 115):

> Registers a prompt.submit hook that appends the current git branch name to Claude's context
> when the prompt mentions a pull request. **If the prompt doesn't mention a PR or git fails, the
> prompt passes through unchanged.** The user's transcript text is unchanged; Claude reads the
> added context line.

O codigo-fonte do exemplo (linhas 118-125) implementa isso como **guarda explicita**:

```javascript
on('prompt.submit', async ($, e, next) => {
  // Pass on a prompt that doesn't mention a pull request as it is
  if (!/\bPR\b|pull request/i.test(e.text)) return next(e)
  const git = await $.process.run(['git', 'branch', '--show-current'])
  // Outside a git repository the command fails, so there's no branch to add
  if (git.exitCode !== 0) return next(e)
  // Keep any context an earlier hook added, and add one more line for Claude
  return next({ ...e, context: [...(e.context ?? []), 'Current branch: ' + git.stdout.trim()] })
})
```

**PARÁFRASE**: tres licoes de projeto, todas literais no exemplo:
(i) **condicionalidade por conteudo do pedido** — nao injeta se o pedido nao e do tipo que o fato
serve; (ii) **falha do ferramental implica silencio** (fora de repo git, nao injeta); (iii)
**acumulacao sem sobrescrita** — `context: [...(e.context ?? []), ...]`, preservando o que outro
hook ja tinha acrescentado.

**(b) A implementacao de referencia de Stop-hook da Anthropic — "the hook stays silent when..."**

**CITADO** (`code.claude.com/docs/en/self-hosted-environments-configuration`, via
`/websites/code_claude` topic `hook-development`, linha 67):

> Nudges Claude once per turn if the project directory has uncommitted changes or unpushed
> commits, so work isn't lost when an idle session is released and so the "Create PR" button on
> claude.ai/code lights up. Drop this file at ~/.claude/hooks/ on the runner host and merge the
> accompanying Stop-hook settings block into ~/.claude/settings.json. **The hook stays silent when
> the directory isn't a git repository or has no remote.**

E o proprio codigo da referencia (linhas 84, 103, 106):

```sh
# stdout: {"decision":"block","reason":"..."} to nudge, or nothing to allow stop.
...
# Not a git repo -> nothing to nudge.
git -C "$d" rev-parse --git-dir >/dev/null 2>&1 || exit 0

# No remote -> "push to the remote" is unsatisfiable; bail.
[ -z "$(git -C "$d" remote 2>/dev/null)" ] && exit 0
```

**PARÁFRASE**: a **implementacao de referencia** da Anthropic — o exemplo que ela manda copiar
para self-hosted runners — e **estruturalmente silenciosa**: ela so fala quando a condicao
especifica e verdadeira, e tem **duas guardas de bail** (`exit 0` sem saida). Nao ha versao
"informe o usuario do estado do repositorio". A documentacao de hooks da Anthropic e, na pratica,
uma **colecao de hooks que ficam calados na maior parte do tempo**.

### 2.5 O que a doc diz sobre "ajuda vs. atrapela" — por canal

Nao ha uma frase "nao injete sermao" na doc. Mas ha **tres restricoes mecanicas** que produzem
exatamente esse efeito:

**Restricao 1 — o teto de 10.000 caracteres vira arquivo, e arquivo nao se le sozinho**

**CITADO** (linha 348, `/websites/code_claude` topic `context`; repetido em `hooks`, `context-management`,
`hook-development`, `hooks-development` — **5 copias independentes do mesmo trecho**):

> If a value exceeds 10,000 characters, Claude Code writes the text to a file in the session
> directory and passes Claude the file path with a preview of up to the first 2,000 characters
> instead. **Claude can read the file, but Claude Code doesn't ask it to.**

**PARÁFRASE**: acima de 10k caracteres, o valor injetado **degrada** para "caminho de arquivo +
previa de 2.000 caracteres". E a doc diz explicitamente que o harness **nao pede** a leitura. Um
sermao de 12k caracteres que o modelo "podia ler" vira, na pratica, uma instrucao de ler que
ninguem deu. **Este e o mecanismo exato pelo qual um sermao longo e estruturalmente inerte.**
O mesmo corte de 10k/2.000 reaparece em 5 documentos: e um numero de plataforma, nao de doc.

**Restricao 2 — o texto injetado nao e atributo do usuario, e e re-executado no resume**

**CITADO** (linha 349, mesma fonte):

> Claude Code saves the injected text in the session transcript. For mid-session events like
> `PostToolUse` or `UserPromptSubmit`, when you resume with `--continue` or `--resume`, Claude Code
> **replays the saved text rather than re-running the hook for past turns, so values like timestamps
> or commit SHAs become stale.** `SessionStart` hooks run again on resume with `source` set to
> `"resume"`, or `"fork"` if you added `--fork-session`, so they can refresh their context.

**PARÁFRASE**: (i) em `--continue`/`--resume`, o texto **antigo e replayado** — um hook que
injeta timestamp vira **mentira** no turno seguinte; (ii) `SessionStart` **roda de novo** no
resume, e e o canal que a doc indica para contexto que precisa ser **renovado** (o exemplo do
matcher `"compact"` re-injeta apos compactacao, linha 421-443).

**Restricao 3 — teto separado para o classificador**

**CITADO** (TypeScript SDK, `/llmstxt/code_claude_llms_txt` topic `hooks`, linhas 924-930):

```typescript
/**
 * Short note about this tool call's result for the auto mode
 * permission classifier. Capped at 2000 characters, shared across
 * all hooks that respond to the same call; honored on synchronous
 * hook responses only. Don't copy untrusted tool output into it.
 */
classifierContext?: string;
```

**PARÁFRASE**: a Anthropic separa o **canal do classificador de permissao** (`classifierContext`,
2.000 chars, **compartilhado entre todos os hooks da mesma chamada**) do canal do modelo
(`additionalContext`). E da uma **regra de qualidade de fonte**: *"Don't copy untrusted tool output
into it."* — e a unica regra explicita de qualidade de conteudo injetado que encontrei na doc
inteira, e ela e sobre **procedencia**, nao sobre estilo.

---

## 3. O que a Anthropic diz sobre hooks

### 3.1 A anatomia do output

**CITADO** (`code.claude.com/docs/en/hooks` — "Hook Callback Output", via `/websites/code_claude`
topic `hooks`, linhas 672-691):

> Your callback returns an object with two categories of fields: top-level fields accepted on every
> event, and `hookSpecificOutput` which controls the current operation. **Return `{}` to allow the
> operation without changes.**
>
> ### Top-level Fields
> - **systemMessage** - Shows a message to the user. Some events discard it or deliver it elsewhere.
> - **continue** (`continue_` in Python) - Determines whether the agent keeps running after this
>   hook. Some events discard it or deliver it elsewhere.
>
> ### hookSpecificOutput
> Fields depend on the hook event type:
>
> #### PreToolUse
> - **permissionDecision** - One of `"allow"`, `"deny"`, `"ask"`, or `"defer"`. If `"defer"`, the
>   turn ends with a result message whose `stop_reason` is `"tool_deferred"`.
> - **permissionDecisionReason** - Reason for the permission decision.
> - **updatedInput** - Updated tool input.
>
> #### PostToolUse
> - **additionalContext** - Appends information to the tool result.

### 3.2 `additionalContext` vs `systemMessage` vs `permissionDecisionReason` — uma tabela

| campo | para quem | o que faz | o que **nao** faz |
|---|---|---|---|
| `hookSpecificOutput.additionalContext` | **o modelo** | entra no context window como system reminder, no ponto do hook | nao aparece como mensagem de chat na UI; acima de 10k chars degrada para caminho de arquivo |
| `systemMessage` (top-level) | **o usuario** (por esta doc) | "Shows a message to the user" | e **descartado** em notificacao: *"Notification hooks can't block or modify notifications; their `systemMessage` and `continue` fields are discarded"* (linha 618) |
| `hookSpecificOutput.permissionDecisionReason` | o modelo + o usuario | razao da decisao de permissao; `deny` > `defer` > `ask` > `allow` quando multiplos hooks discordam | nao injeta contexto; nao substitui output |
| `hookSpecificOutput.reason` (PostToolUse `decision:"block"`) | o modelo | *"adds the `reason` next to the tool result. Claude still sees the original output"* | nao substitui o output — para isso existe `updatedToolOutput` |

**CITADO** — prioridade entre hooks (linha 694):

> When multiple `PreToolUse` hooks return different decisions, `deny` takes priority over `defer`,
> which takes priority over `ask`, which takes priority over `allow`.

### 3.3 Uma **contradicao real** na doc da Anthropic sobre `systemMessage`

Isto precisa ficar registrado, porque o Touring injeta em `systemMessage`:

- **Fonte A** — `code.claude.com/docs/en/hooks` (API reference, linha 676):
  > **systemMessage** - Shows a message to the user. Some events discard it or deliver it elsewhere.
- **Fonte B** — `github.com/anthropics/claude-code/plugins/plugin-dev/skills/hook-development/SKILL.md`
  (o material oficial de como escrever hooks), em **6 lugares**:
  > `{ "continue": true, "suppressOutput": false, "systemMessage": "Message for Claude" }`
  > `{ "hookSpecificOutput": {...}, "systemMessage": "Explanation for Claude" }`

**PARÁFRASE**: a API reference diz que `systemMessage` e **para o usuario**; o skill oficial de
desenvolvimento de hooks escreve, em 6 exemplos, que e **"Message for Claude"** / "Explanation for
Claude". Uma das duas esta desatualizada — e a doc nao reconcilia. **Para o Touring isso e
materialmente relevante**: se `systemMessage` for mesmo "para o usuario", entao usar `systemMessage`
para conveyar instrucoes ao modelo e **usar o campo errado** — e o "sermao" do Touring, que vai
por `systemMessage`, pode nem estar chegando ao modelo pelo caminho que se supunha.

> **Nao resolvo a contradicao.** As duas fontes sao first-party Anthropic e ambas estao presentes
> no corpus do Context7 com o mesmo `Source:`. O que posso afirmar e que **o campo
> `additionalContext` e o campo documentado como "para o modelo"**, e que ele e o que a doc
> descreve como "passes a string into Claude's context window". Qualquer payload que o Touring
> queira que o **modelo** leia deve ir por `additionalContext`, nao por `systemMessage`.

### 3.4 `additionalContext` tem que estar aninhado — fora, e **ignorado em silencio**

**CITADO** (`code.claude.com/docs/en/hooks-guide`, via `/websites/code_claude` topic `hooks`,
linha 1181):

> Inject text into Claude's context on every prompt. **additionalContext must be nested inside
> hookSpecificOutput; placing it at the top level of the JSON is silently ignored.**

**PARÁFRASE**: erro de forma = erro invisivel. Nenhum log, nenhum aviso. Um hook que manda
`{"additionalContext": "..."}` no topo **nao injeta nada e nao reclama**. Isso e uma classe
inteira de falha silenciosa que a doc da Anthropic nomeia explicitamente.

### 3.5 stdout, stderr e exit code

**CITADO** (skill oficial hook-development, via `/anthropics/claude-code` topic `hooks`, linha 962):

> Exit code 0 prints stdout to the transcript, while **exit code 2 returns stderr to Claude**.

**CITADO** (`code.claude.com/docs/en/hooks`, linha 50, via topic `context`):

> A hook that only adds context can instead print plain text to stdout, which Claude Code adds to
> Claude's context.

**CITADO** (`code.claude.com/docs/en/hooks`, UserPromptSubmit Decision Control, linha 283):

> There are two ways to add context to the conversation on exit code 0: plain text stdout (added to
> Claude's context) or JSON with `additionalContext` (added as context). **Neither channel produces
> a visible transcript entry; both are injected as a system reminder that starts with the hook's
> name.**

**PARÁFRASE**: tres canais distintos de entrega, com audiencias diferentes:
`stdout` (exit 0) → contexto do modelo **e** transcript; `additionalContext` → contexto do modelo,
**sem** entrada de transcript; `stderr` + exit 2 → **bloqueio**, entregue ao modelo. E o
system reminder **comeca com o nome do hook** — a procedencia do texto injetado e nomeada
visivelmente para o modelo.

### 3.6 O "sermao" tem um teto duro de **8 bloqueios** do Stop hook

**CITADO** (`code.claude.com/docs/en/hooks-guide`, linha 985):

> Claude Code overrides a Stop hook after it blocks eight consecutive times with no tool call in
> between. Parse the stop_hook_active field from the JSON input and exit early when it is true to
> allow Claude to stop.

**PARÁFRASE**: um hook que insiste e **desligado pela plataforma** apos 8 tentativas. Nenhum
aviso — so o hook deixa de funcionar. A doc de referencia ainda da o `exit 0` para sair cedo.

---

## 4. Criterios de qualidade declarados — o que a doc diz que torna um bloco BOM ou RUIM

### 4.1 O veredito honesto

**A doc oficial da Anthropic NAO publica um criterio de qualidade para o conteudo de
`additionalContext`.** Nao ha "so what?", nao ha "acionavel", nao ha "nao-derivavel", nao ha
"especifico". Varri os 65 documentos por
`noise|redundan|concise|minimi|only include|too much|verbose|bloat|overload|waste|actionable|so what|just-in-time|silent|omit|token budget`
— os unicos acertos foram `verbose` (opcoes de CLI), `concise` (estilos de saida) e `budget`
(knobs de contexto). **Nao inventei o criterio: ele nao esta la.**

### 4.2 O que a doc **de fato** declara como criterios (tudo verificavel)

| # | Criterio declarado | Onde | Tipo |
|---|---|---|---|
| C1 | **Procedencia**: *"Don't copy untrusted tool output into it."* | JSDoc do `classifierContext` (TS SDK) | regra de conteudo |
| C2 | **Teto de tamanho**: >10.000 chars → vira arquivo + previa de 2.000; *"Claude Code doesn't ask it to"* | `hooks`, `context`, `context-management`, `hook-development`, `hooks-development` (5 docs) | limite mecanico |
| C3 | **Teto do classificador**: 2.000 chars, compartilhado entre todos os hooks da mesma chamada | JSDoc `classifierContext` | limite mecanico |
| C4 | **Forma obrigatoria**: `additionalContext` aninhado em `hookSpecificOutput`, senao *silently ignored* | `hooks-guide` | regra de forma |
| C5 | **Condicionalidade**: *"If the prompt doesn't mention a PR or git fails, the prompt passes through unchanged"* | exemplo oficial `prompt.submit` | regra de acionamento |
| C6 | **Silencio por ausencia de precondicao**: *"The hook stays silent when the directory isn't a git repository or has no remote"* | implementacao de referencia de Stop-hook | regra de acionamento |
| C7 | **Falha do ferramental implica silencio**: `git.exitCode !== 0` → `return next(e)` | exemplo oficial `prompt.submit` | regra de acionamento |
| C8 | **Acumulacao sem sobrescrita**: `context: [...(e.context ?? []), ...]` | exemplo oficial `prompt.submit` | regra de composicao |
| C9 | **Nao duplicar**: SubagentStart *"is injected only when the subagent's context doesn't already hold a copy from an earlier run"* | `hooks`, `context`, `subagents` | regra de deduplicacao |
| C10 | **Proveniencia declarada ao modelo**: *"Treat them as context from the application, not as messages from the user"* | `agent-sdk/modifying-system-prompts` | regra de enquadramento |
| C11 | **Contexto renovavel**: `SessionStart` roda de novo no resume; matcher `"compact"` re-injeta | `hooks-guide` | regra de ciclo de vida |
| C12 | **Reentrancia**: `stop_hook_active` → `exit 0` (evita o corte de 8) | `hooks-guide` + implementacao de referencia | regra de robustez |

### 4.3 A leitura que eu faco (PARÁFRASE, minha) — e o que ela implica

Os criterios C5, C6, C7 sao, juntos, **uma formulacao diferente do "So what?"** do Touring:

> **Injete apenas quando a precondicao especifica do fato for verdadeira E voce puder dizer
> em uma linha o que o modelo nao derivaria sozinho. Falhou a derivacao do fato? Silencio.**

O "So what?" do Touring (acionavel? nao-derivavel? especifico?) e o C5-C7 da Anthropic
(condicional? precondicao verdadeira? falha → calar?) sao **a mesma ideia com nomes diferentes** —
e a versao da Anthropic e **operacional** (vem com codigo de referencia e guardas de `exit 0`),
enquanto a do Touring e **declarativa** (esta num docstring e num teste).

A diferenca que a doc da Anthropic acrescenta e o Touring nao tem: **C2 (10k/2k)** e **C3 (2k)**.
A versao da Anthropic diz o que acontece quando voce **ignora** o tamanho — o texto deixa de ser
injetado. A do Touring diz o que fazer quando o texto e pequeno — que e metade do problema.

---

## 5. Implicacoes para o Touring

### 5.1 O veredito "fato funciona, sermao nao funciona" — **a doc corrobora o mecanismo, nao so o dado**

O Touring mediu: FATO 35,5% do volume ≈52% de conformidade; SERMAO 18,7% ≈0,3% de adesao.

A doc da Anthropic da **tres mecanismos** que explicam essa separacao:

1. **C10 — enquadramento**: o reminder e "contexto da aplicacao, nao mensagem do usuario".
   Um "MUST touring ast meta …" chega como *informacao sobre uma regra*, nao como pedido do
   usuario. **Sermao injetado por hook e estruturalmente um conselho, nao uma ordem.** O
   resultado de 0,3% nao e "o modelo e burro" — e o formato.
2. **C2 — degradacao por tamanho**: acima de 10k chars, o valor deixa de ser lido e vira um
   caminho de arquivo que o harness nao manda abrir. **Sermao longo e inerte por mecanismo.**
3. **C3 — contencao**: o `classifierContext` e compartilhado entre todos os hooks da mesma
   chamada, com teto de 2.000 chars. Varios hooks falando ao mesmo tempo **se disputam**.

### 5.2 O contrato "So what?" — **ganha 4 clauses novas, todas com citacao**

O contrato atual do Touring (`crates/touring-hook-handlers/src/hooks/pre_read.rs:1-7`, verificado
por leitura: *"SILENCE IS THE DEFAULT. Only inject context when it passes the 'So what?' test:
Is it actionable? Is it non-derivable? Is it specific to THIS file?"* — com o teste correspondente
em `pre_read_tests.rs:1220-1241`, `audit_purpose_silence_is_default_invariant`, verificado) **esta
alinhado com C5-C7**. As clauses novas:

- **C2-clause (teto)**: *o valor injetado tem que caber inteiro em 10.000 caracteres.* Acima
  disso, o harness nao injeta o texto — joga um caminho. Se o fato nao cabe, **e outro fato**
  (mais curto, ou um caminho para o modelo ler por decisao propria).
- **C3-clause (contencao)**: *o total injetado por chamada precisa caber no orcamento do canal.*
  2.000 chars para o classificador, 10.000 para o modelo.
- **C9-clause (dedup)**: *nao reinjetar o que o contexto ja tem.* A plataforma ja faz isso no
  SubagentStart; o Touring pode (e deve) fazer nos seus.
- **C1-clause (procedencia)**: *nao copiar output de ferramenta nao confiavel para o canal
  injetado.* A regra da Anthropic e literal: *"Don't copy untrusted tool output into it."*

### 5.3 O canal — **`additionalContext`, nao `systemMessage`**

A doc define `additionalContext` como *"passes a string into Claude's context window"*, e e o
unico campo documentado como aimed-at-model. `systemMessage` e documentado como *"Shows a
message to the user"* na API reference (com a contradicao da §3.3).

**Implicacao verificavel para o Touring**: qualquer payload que o Touring quer que o **modelo**
leia deve ir por `hookSpecificOutput.additionalContext`. Se o payload do "sermao" esta indo por
`systemMessage`, **ele pode nem estar chegando ao modelo** — o que seria uma explicacao
*adicional* para o 0,3%, e uma verificacao barata (contar payloads por canal no corpus de
`evidence/injectors-corpus.txt`).

### 5.4 Tres bugs/riscos **concretos e verificaveis** que a doc expoe no Touring

| # | Achado | Evidencia | Verificacao no Touring |
|---|---|---|---|
| R1 | `additionalContext` no topo do JSON e **silenciosamente ignorado** | `hooks-guide` linha 1181 | grep no payload builder do Touring: `additionalContext` tem que estar dentro de `hookSpecificOutput` |
| R2 | **Contexto replayado no resume** fica stale (timestamp, commit SHA) | `hooks` linha 349 | hooks do Touring que injetam timestamp: o valor envelhece no `--continue` |
| R3 | **Stop hook e desligado apos 8 bloqueios** consecutivos sem tool call | `hooks-guide` linha 985 | hook de Stop do Touring: tem guarda de `stop_hook_active`? |

### 5.5 O padrao de referencia que a Anthropic publica — **o modelo mais proximo do "hook de alto sinal"**

A implementacao de referencia de Stop-hook (C5/C6/C7/C12) e, estruturalmente, **exatamente o
que o Touring chama de hook de alto sinal**:

```
guarda 1: nao e repo git            → exit 0 (silencio)
guarda 2: nao tem remote             → exit 0 (silencio)
guarda 3: stop_hook_active           → exit 0 (silencio)
guarda 4: sem mudancas nao commitadas → exit 0 (silencio)
acao:     commit/push pendente       → 1 linha de "decision":"block" com "reason"
```

**Quatro guardas de bail, uma mensagem, zero dashboard.** O "sermao" do Touring inverte
exatamente isso: **guarda zero, mensagem longa, dashboard**.

---

## 6. O que NAO foi encontrado (lacunas honestas)

1. **Nao existe, no Context7, o material da Anthropic sobre "context engineering".** Testei 9
   library ids candidatos de first-party (`/websites/anthropic_com`, `/websites/anthropic_engineering`,
   `/websites/docs_anthropic_com`, `/websites/anthropic_com_engineering`, `/websites/engineering_anthropic_com`,
   `/anthropics/engineering-blog`, `/websites/anthropic_research`, `/websites/anthropic_news`,
   `/websites/anthropic`) — **todos HTTP 404**. O search mais direto
   (`"effective context engineering for AI agents"`) devolveu **zero** material Anthropic: so
   material de terceiros (`jasontang-ai`, `coleam00`, `neolabhq`, `muratcankoylan` — este ultimo
   `verified: true`, trust 9,8, mas **nao e Anthropic**). **Logo: a orientacao oficial da Anthropic
   sobre engenharia de contexto, se existe, NAO foi lida.** Nao a substituo por blog de terceiro.
2. **Nao encontrei criterio de qualidade de conteudo injetado na doc.** Nem "so what?", nem
   "acionavel", nem "nao-derivavel". Ver §4.1 — a varredura foi feita e o resultado e negativo.
   O que existe sao 12 criterios **mecanicos/operacionais** (§4.2), nao um criterio editorial.
3. **A doc nao diz "nao injete sermao".** Minha inferencia de que C1-C12 sustentam o veredito
   "fato funciona, sermao nao" e **PARÁFRASE MINHA**, marcada como tal em §5.1. A doc sustenta
   *condicoes de contorno mecanicas*; a explicacao do 0,3% e leitura minha, coerente com C10/C2/C3.
4. **A contradicao sobre `systemMessage` (§3.3) nao foi resolvida** — as duas fontes sao
   first-party e ambas estao no corpus. Registrada como contradicao, nao como fato.
5. **Nao ha, no corpus, medicao de adesao.** A Anthropic publica o *formato* e os *limites*, nao
   o efeito. Os numeros de adesao (52% vs 0,3%) sao **medidos pelo Touring**, nao pela Anthropic —
   a doc da Anthropic e **compativel** com eles, nao a origem deles.
6. **Faltam as paginas `code.claude.com/docs/en/agent-sdk/*` completas** (so o que o Context7
   indexou por topico). Cobertura por topico, nao integral da doc.
7. **O MCP autenticado do Context7 continua indisponivel.** O `resolve-library-id` /
   `query-docs` oficiais nao foram executados; o conteudo veio da REST API do mesmo servico.

---

## 7. Indice de evidencias

**Manifesto e logs**: `round4/evidence/c7-fetch-manifest.json` (65 fetches, URL + bytes + ok),
`round4/evidence/c7-search-log.json` (11 buscas com id/trust/bench/verified),
`round4/evidence/c7-key-passages.txt` (39 KB, passagens com numero de linha).

**Documentos baixados** (71 `.txt`, ~45 KB cada): prefixes `doc-` (45 topical),
`doc2-` (20 por topico nomeado), `doc3-` (7 complementares). Cada arquivo comeca com
`# SOURCE_URL:` e `# LIBRARY:` + `| TOPIC:`, e cada trecho traz a linha `Source: https://…`
do documento original de onde o Context7 extraiu.

**Trechos-chave e onde estao** (arquivo:linha):

| Assunto | Evidencia |
|---|---|
| `additionalContext` → system reminder | `doc-websites_code_claude-hooks.txt:1056`, `-context.txt:320` |
| teto de 10.000 chars → arquivo | `doc-websites_code_claude-context.txt:348`, `-hooks.txt:1084`, `doc2-websites_code_claude-context-management.txt:140`, `doc2-websites_code_claude-hook-development.txt:905` |
| replay no resume → stale | `doc-websites_code_claude-context.txt:349` (+ 4 copias) |
| "contexto da aplicacao, nao do usuario" | `doc-websites_code_claude-context.txt:1224` |
| "passes through unchanged" | `doc-websites_code_claude-context.txt:115` |
| "the hook stays silent when…" | `doc2-websites_code_claude-hook-development.txt:67` |
| "Don't copy untrusted tool output" | `doc-llmstxt_code_claude_llms_txt-hooks.txt:928` |
| dedup no SubagentStart | `doc-websites_code_claude-context.txt:8` |
| aninhamento obrigatorio | `doc-websites_code_claude-hooks.txt:1181` |
| `systemMessage` = "to the user" | `doc-websites_code_claude-hooks.txt:676` |
| `systemMessage` = "Message for Claude" | `doc-anthropics_claude-code-hooks.txt:805`, `doc2-anthropics_claude-code-hook-development.txt:1482` (+4) |
| `systemMessage` descartado em Notification | `doc-websites_code_claude-hooks.txt:618` |
| prioridade deny>defer>ask>allow | `doc-websites_code_claude-hooks.txt:694` |
| exit 2 → stderr ao Claude | `doc-anthropics_claude-code-hooks.txt:962` |
| corte de 8 bloqueios do Stop | `doc-websites_code_claude-hooks.txt:985` |
| `classifierContext` 2.000 chars | `doc-llmstxt_code_claude_llms_txt-hooks.txt:924-930` |
| matcher `compact` re-injeta | `doc-websites_code_claude-context.txt:421` |
| `autoCompactWindow` / `skillListingBudgetFraction` | `doc-websites_code_claude-context.txt:354`, `doc-llmstxt_code_claude_llms_txt-best-practices.txt:460` |
| MCP sem auth | `curl` direto, `{"code":-32001,"message":"Authentication required…"}` (2x) |

**Verificacao do lado Touring** (o que este relatorio afirma sobre o codigo do Touring, e onde):
`crates/touring-hook-handlers/src/hooks/pre_read.rs:1-7` (docstring "SILENCE IS THE DEFAULT",
lido) · `crates/touring-hook-handlers/src/hooks/pre_read_tests.rs:1220-1241`
(`audit_purpose_silence_is_default_invariant`, lido) · simbolos `pre_read`,
`compose_high_signal_context`, `cila_budget` confirmados por `touring index find`
(definicoes em `crates/touring-hook-handlers/src/hooks/pre_read.rs:1069`, `:54`).
