# Mods do Claude Code — Pesquisa Consolidada

> **Data**: 05/10/2026 | **Claude Code**: v2.1.288 (release atual do repo oficial; build desta sessão idem)
> **Pedido**: Gabriel Gadea — consultar o NotebookLM (conteúdo de Claude Code + doc oficial), explorar os "mods", conferir no repo oficial e no Context7 e verificar boas práticas.
> **Fontes cruzadas**: doc oficial (code.claude.com) · repo `anthropics/claude-code` · Context7 (3 índices) · NotebookLM "Claude Code " · declarações `claude-code.d.ts` da build · relatório de subagente
> **Estado**: pesquisa concluída; **nenhum mod foi criado ou instalado** (ver §8).

**Marcadores de confiança** usados em todo o documento:

| Marcador | Significa |
|---|---|
| **[P]** | Fonte primária lida diretamente (doc oficial publicada, repo oficial, `.d.ts` da build) |
| **[C7]** | Confirmado por consulta ao Context7 (índice independente da doc oficial) |
| **[NB]** | Só no NotebookLM (transcrições de YouTube): relato de terceiros |
| **[S]** | Só no relatório do subagente `claude-code-guide`, não conferido por mim |
| **[I]** | Inferência minha; não está em nenhuma fonte |

---

## 1. Resumo executivo

1. **Mod** é um plugin cujo comportamento mora num *hooks module* (`register(on, options)`) que roda **dentro** do Claude Code e pode observar, reescrever ou responder eventos (tool calls, prompts, turnos, renderização) e desenhar UI própria. **[P]**
2. **Estão ligados por padrão desde a v2.1.287** (CHANGELOG: "Added Claude Mods: plugins may now modify deeper behavior"); a flag `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` do early access passou a ser ignorada. **[P]**
3. **A doc oficial existe e é completa**: `code.claude.com/docs/en/plugins/mods/*` (9 páginas). O **NotebookLM não contém doc oficial**: as 20 fontes do notebook "Claude Code " são transcrições de YouTube. Ele cobre mods por 3 vídeos e concorda com a doc oficial nos pontos conferidos (§6), mas admite que não traz a sintaxe da API. **[P]**
4. **O `README.md` de `mods/` no repo está defasado** (diz "early access, só onde function hooks estão habilitados"). A doc viva diz "on by default". **[P]**
5. **Context7**: a biblioteca `/llmstxt/code_claude_llms_txt` indexa as páginas de mods e confirmou as práticas quase literalmente; `/websites/code_claude` **não** devolveu as páginas de mods em 3 consultas; `/anthropics/claude-code` só traz o README defasado. **[C7]**
6. **Risco específico para o Touring [I]**: um mod que responde `tool.call` **sem chamar `next`** impede os hooks `PreToolUse` de settings (onde vivem `touring-hook`, `block_git.sh` e o guard de processo) de rodar (§7).

---

## 2. Fontes e método

| Fonte | Identificador | O que é | Cobertura de mods | Confiança |
|---|---|---|---|---|
| **Doc oficial** | `https://code.claude.com/docs/en/plugins/mods/{overview,reference,create,interface,events,api,test,troubleshoot,admin}.md` | Doc publicada. `WebFetch` em 03/10/2026 | `overview`, `reference`, `create`, `interface`, `events`, `api`, `test`, `troubleshoot` lidas **integralmente**; `admin` só por trechos (Context7 + subagente) | **[P]** |
| **Repo oficial** | `github.com/anthropics/claude-code` @ v2.1.288 (push de 03/10/2026 10:32Z) | `mods/` (README 6.470 B, `agents-md/`, `diff/`, `sec-default/`, `telemetry/`, `types/`, `tsconfig.json`), `CHANGELOG.md` | README + `diff/.claude-plugin/plugin.json`, `diff/hooks/hooks.json`, `diff/hooks/register.ts` (início) lidos via `gh api` | **[P]** |
| **Declarações da build** | `claude-code.d.ts`, 20.212 linhas, escrito pela skill `plugin-authoring` a cada sessão (caminho temporário em `/tmp/claude-1000/bundled-skills/2.1.288/…`) | Contrato exato da build: eventos, `$`, elementos | Âncoras confirmadas por script agregado (Apêndice B); `reference.md` da skill lido integralmente | **[P]** — vence qualquer página quando divergem |
| **Context7** | `/llmstxt/code_claude_llms_txt` (10.893 snippets) · `/websites/code_claude` (7.075) · `/anthropics/claude-code` (1.508) | Índices de doc | 8 consultas em 05/10/2026 (conector claude.ai autenticado via `/mcp`) | **[C7]** |
| **NotebookLM** | notebook "Claude Code " `65c3ecb1-a4ce-4901-9864-a2b28c04b469` (20 fontes, 100% YouTube) | Vídeos de terceiros | 3 vídeos sobre mods (abaixo) | **[NB]** |
| Subagente | `claude-code-guide` | Pesquisa nas páginas oficiais | Achou as 9 URLs; tabelas de eventos, elementos e limites **batem** com o que li, exceto `TurnDuration` e `InfoNotice`, que ele lista em terminal e Desktop e a doc diz só terminal; "agent fd" e o conteúdo de `admin` ele não sustentou | **[S]** onde não conferi |

**Vídeos de mods no notebook** (ids de fonte): `228752cc-a4aa-4063-b01a-71efcfa26096` ("Claude Code Mods Are Game Changers. Set Up These 5 NOW"), `b6d79d14-b775-484c-870b-6eb25f0ce1ef` ("Claude Mods - The Biggest Claude Code Upgrade!"), `6afb2ebe-0081-4a54-8458-1f6a28f9225b` ("Claude Mods Is The Biggest Claude Code Upgrade Since Skills").

**Outros notebooks verificados**: "Claude" (1 fonte, "Texto colado": guia sobre *evals*, sem Claude Code) e "Prompt Engineering for Claude" (65 fontes; inclui Claude Docs de prompting — Effort, Prompting best practices, XML tags, Reduce hallucinations, Computer use — **nenhum sobre Claude Code/mods**).

**Ordem de confiança adotada**: `.d.ts` da build > doc publicada > Context7 `llmstxt` > README do repo > NotebookLM. A própria doc manda confiar nos tipos da build quando houver divergência.

---

## 3. O que é um mod

### 3.1 Estrutura mínima **[P]**

```text
nome-do-mod/
├── .claude-plugin/plugin.json     # manifesto (nenhum campo novo obrigatório)
└── hooks/
    ├── hooks.json                 # { "modules": ["./register.js"] }  — um caminho, relativo
    └── register.js                # export function register(on, options) { ... }
```

- O módulo pode ser `.js .mjs .cjs .jsx .ts .mts .cts .tsx`; é sempre ES module; sem build step. Sem Node e sem DOM: tudo que sai do módulo passa por `$`.
- `types/index.d.ts` (e `"types"` no manifesto) é exigido quando o mod usa `$.state` ou adiciona um *namespace* à API.
- `hooks.json` pode conter também hooks de settings (`hooks`) ao lado de `modules`.
- Exemplo oficial completo (overview): conta tool calls e mostra o total ao lado do spinner.

```javascript
let calls = 0
export function register(on) {
  on('tool.call', async ($, e, next) => {
    calls += 1
    $.ui.invalidate('ui.render')
    return next(e)
  })
  on('ui.render', { component: 'Spinner' }, async ($, e, next) =>
    next({ ...e, props: { ...e.props, suffix: ' · tool calls: ' + calls + '…' } }))
}
```

### 3.2 Anatomia de um hook **[P]**

`on(evento, matcher?, hook)` → hook `($, e, next)`:

| Argumento | Papel |
|---|---|
| `$` | API de mods (`$.ui`, `$.fs`, `$.store`…). Cada chamada é escrita por extenso: namespace, depois método |
| `e` | Entrada do evento, **congelada** (para mudar, passe uma cópia a `next`) |
| `next(e)` | Próximo handler (outro mod, depois o comportamento do Claude Code). Resolve para o resultado do evento |
| `next.signal`, `next.budget`, `next.origin`, `next.to(e, tier)` | Aborto do evento, orçamento de tempo, origem `{plugin, tier}` e salto de tier (só `prependPlugins`/`appendPlugins`) |
| `.catch(handler)` | Handler de erro **da própria registração** (`next.error.kind` = `throw` ou `timeout`) |

Três ações possíveis sobre um evento: **observar** (`return next(e)`), **reescrever** (`next({ ...e, campo })`) e **responder** (retornar o resultado sem `next`, ex.: `{ deny: 'motivo' }` ou `{ result }`).

### 3.3 Ordem em que os mods rodam **[P]**

Os hooks de um evento formam **uma cadeia de middleware**; o primeiro é o mais externo e decide se os demais rodam. Ordem dos tiers:

1. guarda embarcado `sec-default@builtin` + mods da organização em `prependPlugins` (e demais mods "da organização");
2. mods que você instala (**user**);
3. mods da organização em `appendPlugins`;
4. outros mods embarcados.

Dentro de um tier, um mod roda antes dos mods que lista em `dependencies`; dentro de um módulo, na ordem das chamadas a `on`. `PreToolUse` de **managed settings** roda antes do 1º mod e o bloqueio é final; `PreToolUse` de **qualquer outro arquivo de settings e de `hooks/hooks.json` de plugins** roda **depois do último mod** que chamar `next`. `tool.check` dispara depois de tudo isso.

### 3.4 Onde mods rodam **[P]**

| Onde | Hooks rodam | O que o mod desenha |
|---|---|---|
| `claude` no terminal (inclui terminal integrado e plugin JetBrains) | Sim | Sim |
| Aba Code do app Desktop (exceto sessão WSL) | Sim | Sim (menos elementos só-terminal) |
| Sessão WSL no Desktop | Não (plugins indisponíveis) | Não |
| Extensão do VS Code (chat) | Sim | Não |
| `claude -p` e Agent SDK | Sim | Não |
| Remote Control | Sim, na sessão da máquina | No terminal da máquina |
| Sessão em nuvem | Sim, para plugin que chega à nuvem | Não |

### 3.5 Mods embarcados **[P]**

| Nome em `/plugin` | Função | Observação |
|---|---|---|
| `cc-plugin-diff` | `/diff` em pane ao lado do transcript | Fonte em `mods/diff` (muito modular: um arquivo por função) |
| `cc-plugin-agents-md` | Carrega `AGENTS.md` como instrução de projeto | Opção `userConfig`; fonte em `mods/agents-md` |
| `cc-plugin-sec-default` | Protege hooks/política/deny rules da organização contra mods instalados | Não desligável; modelo de mod de política; fonte em `mods/sec-default` |
| `cc-plugin-telemetry` | Registros de analytics de primeira parte | Adiciona `$.telemetry` via `engine.create`; fonte em `mods/telemetry` |
| `cc-plugin-plugin-authoring` | Skill `plugin-authoring` (escrever mods) | Só skill, sem código de mod |
| `cc-plugin-you-should-know` | Agente lateral que sinaliza o que você ou o Claude podem perder | **Desligado por padrão**: `/plugin enable cc-plugin-you-should-know@builtin` |

### 3.6 Mods de exemplo

- **Oficiais, no repo `anthropics/claude-code-playground`** (`claude-code/mods/`, sem suporte): `token-weather` (previsão do contexto acima do prompt), `blast-radius` (segura `rm -rf`/force push e mostra o impacto, com botões), `replay-theater` (`/replay` repassa as edições do último turno). **[P]** — só os nomes e descrições na doc; o repo do playground **não foi lido**.
- **Dos vídeos [NB]**: *Next Steps* (painel com 3 próximos passos clicáveis ao fim do turno), *CacheKeeper/Cache Clock* (contagem do cache e botões de compactar/handoff), *Collision Guard* (avisa se outra sessão editou o arquivo nos últimos 30 min), *Record Mode* (mascara PII na tela), *Goal UI*, *Lab gauge*, *Doom multiplayer*. Nenhum foi verificado em fonte primária.

### 3.7 Mod × hook de settings × skill × MCP **[P]**

| | Mod | Hook de settings | Skill | Servidor MCP |
|---|---|---|---|---|
| O que é | Funções num plugin, no processo do Claude Code | Comando shell, HTTP ou prompt em evento | `SKILL.md` com instruções | Processo/serviço externo com tools |
| Muda | Tool calls, prompts, comandos, turnos, UI | Se tool call/prompt segue, args/resultado, contexto | O que o Claude sabe e faz | Que tools o Claude tem |
| Desenha na UI | **Sim** | Não | Não | Não |
| Estado entre eventos | Sim (módulo, `$.state`, `$.store`) | Não (um processo por evento) | — | Do servidor |
| Escolha quando | Quer pane/faixa/comando próprio ou reescrever evento | Quer bloquear/permitir/logar com script que já tem | Cola as mesmas instruções sempre | O Claude precisa de um sistema externo |

Diferença de semântica que importa: em hooks de settings, **todos os hooks que casam rodam até o fim** e o resultado mais restritivo vence (`deny > defer > ask > allow`); `deny` de um não impede os efeitos dos irmãos. **[C7]** Em mods a cadeia é sequencial: **quem responde sem `next` interrompe os seguintes**. **[P]**

---

## 4. Superfície da API (mapa)

Fonte: página `reference` + `.d.ts` da build. Detalhe de cada evento/método: os tipos da build (cada um com comentário e exemplo).

**Eventos** (cada um com seu resultado possível):

| Grupo | Eventos |
|---|---|
| Tools | `tool.call`, `tool.check`, `tool.describe` |
| Prompts | `prompt.submit`, `prompt.fill`, `prompt.suggest`, `prompt.edit`, `prompt.compose`, `prompt.section`, `prompt.context`, `prompt.attachment`, `skill.prompt`, `attribution.text` |
| Comandos/config | `command.run`, `command.describe`, `config.set`, `config.describe` |
| Turnos | `turn.start`, `turn.step` (async generator), `turn.complete` |
| Sessão | `session.start`, `session.end`, `session.compact`, `session.receive`, `session.send`, `session.append`, `session.attach`, `session.detach`, `session.measure` |
| Subagentes | `agent.offer`, `agent.spawn` |
| Interface | `ui.render`, `ui.resolve`, `ui.press`, `ui.input`, `ui.select`, `ui.focus`, `ui.scroll`, `ui.close`, `ui.message` |
| Outros mods | `plugin.register` (pode `{ refuse }`), `engine.create` |
| Telemetria | `telemetry.log`, `telemetry.mark` (exigem `{ to: 'collector' }`) |
| Hooks de settings | `classic.<Evento>` (`classic.Stop`, `classic.SessionStart`…) |
| Chamadas da API | **Toda** chamada de `$` é também um evento (`fs.read`, `model.complete`, `ui.open`…) que mods anteriores na cadeia interceptam |

**Namespaces de `$`**: `plugin`, `ui` (resolve, invalidate, open, close, panes, focus, scroll, toast, status, log, notice, ask, copy, blit), `command`, `tool`, `agent`, `model` (complete, fork, classify), `prompt`, `turn`, `session`, `config`, `settings`, `env`, `fs`, `store`, `state`, `clock`, `http`, `process`, `mcp`, `audio`, `telemetry`.

**Render sites**: `Pane`, `AbovePrompt`, `UserMessage`, `AssistantMessage`, `ToolUse`, `ToolResult`, `ToolGroup`, `CommandOutput`, `AskUserQuestion`, `ToolProgress`*, `Spinner`, `TurnDuration`*, `InfoNotice`*, `SessionMode`, `PromptHint` (*só terminal). O prompt de permissão **não** é render site.

**Elementos**: `Box`, `Text`, `Button`, `Link`, `Code`, `Markdown`, `Input`, `Select`, `Client` (ambos); `Svg` (só Desktop); `Raster`, `Image` (só terminal).

**Settings/variáveis**: `CLAUDE_CODE_PLUGIN_DIRS`, `CLAUDE_CODE_PLUGIN_DIR_WATCH`, `prependPlugins`, `appendPlugins`, `allowManagedModsOnly`, `allowModsToOverrideDenyRules`, `allowManagedHooksOnly`, `disableAllHooks`, `disableSideloadFlags`, `pluginConfigs`; `--safe-mode`/`--bare` desligam mods instalados (não os embarcados).

**Comandos**: `/plugin` (mostra "N mods active"), `/reload-plugins`, `claude plugin validate <dir> [--strict] [--json]`, `claude plugin test [dir]`, `claude --plugin-dir <dir>` (recarrega ao salvar). O README do repo cita `/plugin-types`; a página `create` descreve os tipos sendo escritos em `.claude-plugin/types/` a cada load. **[P]**

---

## 5. Boas práticas

Coluna **C7**: ✔ = também confirmado no Context7; — = confirmado só na doc lida direto.

### 5.1 Segurança e confiança

| # | Prática | Fonte | C7 |
|---|---|---|---|
| S1 | Mod **não é sandboxed**: roda com suas permissões, lê segredos (env, settings), vê todo prompt e tool call, pode aprovar tool calls e gastar seu plano | overview | ✔ |
| S2 | **`claude plugin validate ./mod` antes de instalar**: as linhas `hooks:` e `calls:` listam eventos e chamadas (`fs`, `http`, `process`…) sem executar | overview, create | ✔ |
| S3 | Processo que um mod inicia roda **fora** do sandbox do Claude Code | overview, security | ✔ |
| S4 | **Auto-update** do marketplace pode mudar arquivos que você já revisou | plugins/security | ✔ (novo) |
| S5 | O `bin/` de um plugin entra no `PATH` do Bash | plugins/security | ✔ (novo) |
| S6 | Mod pode reestilizar a UI, **menos o prompt de permissão** | overview | — |
| S7 | Mod que aprova (`tool.check`) pode passar por cima de `ask` e de `PreToolUse` não-managed; contra `deny` só com `allowModsToOverrideDenyRules` — senão a chamada continua negada ("tried to lift a deny rule") | overview, troubleshoot | — |

### 5.2 Guards, falha e política

| # | Prática | Fonte | C7 |
|---|---|---|---|
| G1 | Hook que falha **antes** de `next` é pulado e o comando **roda** → guard usa **`.catch` fail-closed** devolvendo `{ deny }` | events | ✔ |
| G2 | Texto do `deny` é lido pelo Claude como resultado da tool: escreva como **instrução acionável** | events | — |
| G3 | Confirmar com `$.ui.ask`: comece de `answer = 'Refuse'`, use `try/catch` (rejeita se dispensado e em `claude -p`); a espera **dentro de chamada da API** não conta nos 10 s do hook, `await` de promise própria conta | events | — |
| G4 | Regex sobre o comando é **lembrete, não barreira** (erra `git push -f`); comando fixo → permission rule; decisão que depende de estado → `tool.check`; proteção real → no host (branch protection) | events | — |
| G5 | Responder sem `next` **curto-circuita** `PreToolUse` de settings não-managed; managed roda antes e é final | events | — |
| G6 | Política de organização: `plugin.register` + `.catch` → `{ refuse }` (sem o `.catch`, o check falha aberto); posicione por `prependPlugins`; confira em `claude --debug` (`hooks module X loaded … tier prepend`; `tier user` ou "skipped" = não pegou) | admin | ✔ |
| G7 | Em *auto mode*, hook que **altera o input** depois da revisão do classificador faz a chamada ser negada ("a hook changed this call's input after the model wrote it") | troubleshoot | — |

### 5.3 Estado e reload

| # | Prática | Fonte | C7 |
|---|---|---|---|
| E1 | Escolha pelo tempo de vida: **variável de módulo** (some a cada reload) · **`$.state`** (sessão; sobrevive a reload; redesenha sozinho) · **`$.store`** (entre sessões; JSON em `~/.claude/plugins/store/`, 4 MiB) | interface | ✔ |
| E2 | `/clear`, `/resume`, `/branch` **zeram `$.state`** e `session.start` não dispara de novo → recarregue do `$.store` em `classic.SessionStart` com `{ source: ['clear','resume','fork'] }` | interface | ✔ |
| E3 | `$.store` é **compartilhado entre sessões e não atômico**: uma chave por item e **releia antes de gravar** | interface | ✔ |
| E4 | Hook de render **só lê**; escreva de callback (`onPress`) ou de outro evento; `atom/read/update` de `claude-code` fazem leitura-aplicação-escrita com `ifVersion` | interface, `.d.ts` | — |
| E5 | `$.state` exige `types/index.d.ts` com `interface PluginState`, `"types"` no manifesto e `plugin`/`key` como **literais** | interface | — |

### 5.4 Interface

| # | Prática | Fonte | C7 |
|---|---|---|---|
| U1 | Árvore a partir de `$.ui.resolve(e)`; dimensione por `e.props.bodyColumns`; grade de células = **um `Raster`**, nunca um `Box` por célula; sem `Raster` no Desktop → faça fallback em texto; **teste nas duas superfícies** | interface, reference, test | — |
| U2 | Pane aberto **sem o usuário pedir** só aparece com ≥144 colunas (110 após aberto uma vez); aberto por comando/botão aparece em qualquer largura; confira `isPlaced`; para avisar use `$.ui.toast` | interface | ✔ |
| U3 | `focus`, `closeOnEscape`, `holdToasts`, `autoFocus` aceitam **só `true`** (omita em vez de `false`) | interface | — |
| U4 | **`key` em todo controle** (testes endereçam por `key`); árvore inválida faz o engine desenhar o seu e registra `ui.render (X) refused: motivo` | interface, troubleshoot | ✔ (key) |
| U5 | Redesenho por `$.ui.invalidate('ui.render')`: 10/s (30/s no terminal para o pane visível); chamadas mais rápidas são fundidas | interface, reference | — |

### 5.5 Trabalho em segundo plano e modelo

| # | Prática | Fonte | C7 |
|---|---|---|---|
| B1 | Trabalho que vive além de um evento: inicie em `session.start` com `$.clock.every/after`; timers param no reload; passe **`next.signal`** a trabalho longo | api | — |
| B2 | **Registre comandos por último** no `session.start`: nome tomado lança erro e o hook é pulado | api | — |
| B3 | `$.model.complete`: defina `maxTokens`/`timeoutMs`, cheque `isAnswered`; `$.model.fork` reaproveita o cache de prompt; ambos gastam o plano/API key do usuário | api | — |
| B4 | Texto de `prompt.section`/`prompt.context` que muda entre requisições **invalida o prompt cache** | events | — |

### 5.6 Qualidade, validação e ciclo de desenvolvimento

| # | Prática | Fonte | C7 |
|---|---|---|---|
| Q1 | Validação estática: `$.ns.metodo` por extenso (nada de `const ui = $.ui`, destructuring ou índice computado); nome de evento **literal**; sem `import()` dinâmico; só `import` relativo + `claude-code`; sem redeclarar `on` | create | — |
| Q2 | **Um `on` por matcher**: `session.start` duas vezes sem matcher falha o load | events | — |
| Q3 | Testes com `claude plugin test` (`claude-code/testing`): stubs **antes** da 1ª chamada em `$`; `session.start` não roda sozinho; `turn.step` é generator; `mock.clock/store/env`; `tier(...)`; laço sobre `['terminal','desktop']`; 5 s por teste | test | ✔ |
| Q4 | Desenvolva com **`--plugin-dir`** (recarrega ao salvar); plugin instalado roda cópia em cache por versão | create, troubleshoot | — |
| Q5 | CI: `claude plugin validate --strict --json`; depuração: `/reload-plugins`, aba **Errors** do `/plugin`, `claude plugin list`, `claude --debug[-file]` | plugins/*, troubleshoot | ✔ (websites) |
| Q6 | Nome do plugin não pode parecer da Anthropic (ex.: `claude-…`) | create | — |
| Q7 | Em divergência, **os tipos da build vencem** qualquer página; a API muda entre releases — registre no README a versão testada | create, reference | — |
| Q8 | Uma falha de worker descarrega o mod ("it crashed the hooks worker"); 3 quedas sem causa rastreável desligam **todos** os mods não embarcados até `/reload-plugins`: nunca bloqueie a thread (laço sem `await`) | troubleshoot | — |

### 5.7 Limites **[P]** **[C7]**

| Limite | Valor |
|---|---|
| Tempo do hook por evento (sem contar `next` e chamadas da API, salvo `$.clock.sleep`) | 10 s |
| `.catch` | 1 s |
| Todos os `session.end` somados | 1,5 s |
| `$.process.run` | 30 s padrão, máx. 10 min |
| `$.model.complete` `maxTokens` | 1024 padrão, até 64.000 |
| `$.fs.read/write` | 4 MiB por arquivo |
| `$.store` | 4 MiB de JSON no total |
| `$.session.messages()` | 4.096 entradas mais novas |
| `$.ui.invalidate('ui.render')` | 10/s (30/s terminal) |
| `$.ui.toast` | 4 s (ou `timeoutMs`) |
| Pane sem pedido | 144 colunas (110 após abrir uma vez) |
| Nome de comando/tool/subagente/pane | `[A-Za-z0-9_-]`, até 64 |
| Texto num `Text` / `Code` / `Markdown` | 10.000 chars |
| `Raster` | até 512×256; `Image` até 2 MiB; `Svg` até 131.072 chars |
| Um teste de `claude plugin test` | 5 s (ajustável) |

---

## 6. Reconciliação entre fontes

| Afirmação | NotebookLM | Oficial | Context7 | Veredito |
|---|---|---|---|---|
| Mods rodam **dentro** do harness; skills/hooks/MCP, fora | ✔ | ✔ | ✔ | Confirmado |
| Instalação/gestão por `/plugin`; `--plugin-dir` com hot reload | ✔ | ✔ | ✔ | Confirmado |
| `claude plugin validate` lista eventos e chamadas | ✔ | ✔ | ✔ | Confirmado |
| Reload zera variáveis do módulo; guardar em estado do Claude Code | ✔ | ✔ (`$.state`/`$.store`) | ✔ | Confirmado |
| Exemplos token weather / blast radius / replay theater | ✔ | ✔ (playground) | — | Confirmado |
| Mod de privacidade mascara a UI mas o modelo vê o dado real | ✔ | ✔ no `reference.md` da skill (reescrever o `text` de um `UserMessage` muda só a linha, nunca o que o modelo leu); a doc pública só diz que o mod vê todo prompt | — | Confirmado, com a fonte primária sendo a skill |
| "agent fd" é reimplementado como mod | ✔ (vídeo) | A doc lista `cc-plugin-agents-md` | — | **Erro de transcrição**: o vídeo quis dizer `AGENTS.md` **[I]** confiança 0,9 |
| Anthropic planeja migrar mais features do core para mods | ✔ (vídeo) | Não consta | — | **Não confirmado** |
| "Early access; só onde function hooks estão habilitados" | — | **Contradiz**: "on by default" desde v2.1.287; flag antiga ignorada | `/anthropics/claude-code` devolve o texto defasado | **README do repo defasado**; vale a doc viva |
| Mods = versão final dos *function hooks* (Boris Cherny, set/2026) | ✔ (vídeo) | A nota sobre `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS` "durante o early access" corrobora o nome antigo | — | Nome confirmado; **autoria e data do tweet não verificadas** |
| Marketplace = pasta ou repo GitHub com um JSON; `/plugin install nome@marketplace` | ✔ | ✔ | — | Confirmado |
| Cache de 60 min (CacheKeeper) | ✔ (vídeo) | Não consta | — | **Não verificado** |

---

## 7. Aplicação ao Touring **[I]**

Esta seção é inferência minha sobre o ambiente do Gabriel; os fatos de base vêm das seções anteriores.

### 7.1 Fatos relevantes (com fonte)

- Hooks `PreToolUse` configurados em `~/.claude/settings.json` (shim `touring-hook` / `cli-suggest`, `block_git.sh`, `touring-process-guard.sh`) são "hooks de qualquer outro arquivo de settings": rodam **depois do último mod**, e **só se esse mod chamar `next`**. **[P]** (§3.3, G5)
- Um mod que responde `{ result }` a `tool.call` pula a permissão e o tool; com `{ deny }` bloqueia. **[P]**
- Mods rodam no processo do Claude Code e **sem sandbox**; o CEG do Touring cobre as ações de Bash, não o código do mod. **[P]**
- Um hook que trava a thread derruba o worker e pode descarregar todos os mods não embarcados. **[P]**

### 7.2 Candidatos a piloto (não testados)

| Ideia | Mecanismo | Ganho | Risco |
|---|---|---|---|
| Painel de saúde Touring (daemon, doctor, hw-slot, KPI de code mode) acima do prompt | `ui.render` `AbovePrompt` + `$.clock.every` + `$.process.run(['touring','status','-j'])` | Visibilidade contínua sem custo de contexto | Baixo; só leitura. Nunca `pkill` (REGRA #19); `$.process.run` não usa shell |
| Guard fail-closed da classe destrutiva (espelho de `block_git.sh`) com confirmação interativa | `tool.call` + `.catch` → `{deny}` + `$.ui.ask` | Estado e UI na confirmação | **Duplica** o guard existente; cuidado com G5/G7 e com a ordem dos tiers |
| Telemetria de adoção (`session.measure`, `turn.complete`) | Hooks de observação (`return next(e)`) | Alimentar `touring kpi -j` | Custo de manutenção; API pode mudar entre releases |

### 7.3 Guard rails se houver piloto

- Nunca responder `tool.call` sem `next` exceto para `deny` deliberado.
- Escrever em `~/.claude/dev-mods/<sessão>/` dispara o prompt "Enable hot reloading for this session?" para o Gabriel; sem a resposta dele, nada carrega.
- `claude plugin validate` + `claude plugin test` como gate; registrar a versão testada (hoje v2.1.288).
- Decisão de criar qualquer mod: **do Gabriel** (a pesquisa não a pressupõe).

---

## 8. Lacunas e limites desta pesquisa

- A página `admin.md` foi lida **só por trechos** (Context7 e relatório do subagente); `gallery.md` e o repo `claude-code-playground` **não** foram lidos.
- O `mods/types/claude-code.d.ts` publicado no repo não foi lido; usei o da build da sessão.
- **Nenhum mod foi criado, instalado ou testado**; todas as práticas vêm de documentação, não de execução.
- No Context7, estas práticas foram conferidas **só na doc lida direto**: lista completa dos tiers, regras de validação estática (Q1), props `true`-only (U3), um `on` por matcher (Q2), G2–G5, G7, B1–B4, Q4, Q6–Q8.
- O MCP do NotebookLM está na 0.9.14 (há 0.15.1; `uv tool upgrade notebooklm-mcp-cli`).

---

## Apêndice A — Linha do tempo (CHANGELOG do repo) **[P]**

| Versão | Mudanças ligadas a mods |
|---|---|
| **2.1.287** | `Added Claude Mods: plugins may now modify deeper behavior`; mod embarcado `You should know` (`/plugin enable cc-plugin-you-should-know@builtin`) |
| **2.1.288** | `$.ui.selection()` para mods; correções: botão de mod executando ação de outro botão após reinício; erro de interface em fullscreen com mod mostrando linhas acima do prompt; `--plugin-dir` sem "Configure options" em `/plugin`; `claude plugin test` reportando mods como desligados remotamente |

## Apêndice B — Âncoras do `claude-code.d.ts` da build (v2.1.288)

20.212 linhas. Para localizar: `export type Register` L8673 · `Pane: {` L9632 · `export type ToolCallResult` L11997 · `interface PluginState` L7323/L7325/L11142/L11313 · `'tool.call'` L3741/4235/6242/14119 · `'ui.render'` L3765/4243/9048/14119 · `'session.start'` L4063/4355 · `'turn.step'` L1038/4181/4399/11448/11477. O arquivo é **regenerado por sessão**: as linhas mudam entre builds.

## Apêndice C — Como reproduzir

```bash
# Repo oficial: mods, changelog
gh api repos/anthropics/claude-code/contents/mods --jq '.[].name'
gh api -H 'Accept: application/vnd.github.raw' repos/anthropics/claude-code/contents/mods/README.md
gh api -H 'Accept: application/vnd.github.raw' repos/anthropics/claude-code/contents/CHANGELOG.md | grep -n -i -E '\bmods?\b'

# Doc oficial (WebFetch): https://code.claude.com/docs/en/plugins/mods/<página>.md
#   páginas: overview reference create interface events api test troubleshoot admin

# Context7 (conector claude.ai, autenticar com /mcp): usar /llmstxt/code_claude_llms_txt para mods

# NotebookLM: notebook 65c3ecb1-a4ce-4901-9864-a2b28c04b469 (mcp__notebooklm-mcp__notebook_query)
```

Memória associada: `mods-claude-code-fontes-oficiais` (auto-memory do projeto) e `ref:claude-code-mods-fontes-oficiais:2026-10-03` (`touring memory recall`).
