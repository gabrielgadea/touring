---
okf_version: "1.0"
type: Strategy
title: "Code Mode Best Practices — Claude Code + Touring"
description: "Estratégia consolidada a partir de 5 fontes: Cloudflare Code Mode (Sunil Pai), TanStack AI Code Mode (Jack Herrington ×2), deepseek-harness packages/code-runtime, tanstack/ai"
tags: [code-mode, codeact, programmatic-tool-calling, strategy, claude-code, touring]
timestamp: "2026-08-23T22:30:00-03:00"
status: "v1.1 — 5 fontes + fusão TOTAL com a investigação paralela da sessão analise (M1–M12, D0–D9, rodadas 2) + relatório final do agente tanstack; 25 práticas P1–P25, 15 oportunidades T1–T15 unificadas ao backlog decidido (task_1787534694944647530)"
plan: /index.md
plan_id: task_1787534575493195469
---

# Estratégia — Code Mode Best Practices (Claude Code + Touring)

## 1. Contexto e objetivo

Tool calling clássico quebra em escala: contexto inflado por catálogos de tools, N round-trips por consulta, composição via JSON frágil, e o LLM fazendo trabalho (math, filtro, agregação) que código faz melhor. As 5 fontes convergem na mesma resposta — **o modelo escreve código; o código fala com o sistema** — mas cada uma contribui um mecanismo distinto. Objetivo: destilar os mecanismos e mapeá-los em ações concretas para (a) como o Claude Code/TACO opera e (b) o produto Touring.

## 2. Síntese das fontes

### 2.1 Cloudflare — "Code Mode: Let the Code do the Talking" (Sunil Pai, AI Engineer 2026)

- **Escala do problema**: superfície da API Cloudflare = 2.600 endpoints ≈ 1,2–1,5M tokens se exposta como tools. Inviável como MCP.
- **Padrão `search` + `execute` (2 tools)**: ambas recebem **código como input**. `search` roda o código contra o OpenAPI spec inteiro (descoberta programática); `execute` expõe as funções encontradas. Resultado: 1,2M → ~1k tokens (**−99,9%**). Um incidente DDoS que custaria ~8 round-trips MCP vira 1 execução.
- **Sandbox nasce com ZERO capabilities**: nem fetch. Capacidades são concedidas explicitamente como APIs (capability-based security, estilo objeto-capability). Default recomendado: **nenhum fetch de saída, só as APIs injetadas**. Runtime: V8 isolates (startup rápido, ~10 anos de hardening); o runtime é intercambiável (WASM, interpretador JS custom) — o essencial são os atributos: *events, sandboxing, capability-security, embeddable/efêmero/rápido*.
- **"Inhabiting the state machine"** (Kenton Varda): dado acesso ao estado do sistema (array de strokes de um canvas), o modelo não gera um app de tic-tac-toe — ele **inspeciona o estado e joga**. Interação direta com o estado via código é um modo de operação emergente, não geração de programa.
- **Observabilidade absoluta**: é mandatório poder voltar ao código que executou uma ação ("por que na terça passada isso fez um trade de $2,3M?").
- **Agent DX**: "seus próximos bilhões de usuários são robôs que sonham em types e syntax errors" — docs markdown, **erros que dizem ao agente o próximo passo**, descoberta via search.

### 2.2–2.3 TanStack AI Code Mode (Jack Herrington — intro + Nx Conf)

- **Code mode é modelado como 1 tool** (`execute_typescript`). `createCodeMode({driver, tools})` devolve um tool TanStack padrão **+ um system prompt contendo os typings TypeScript das tools injetadas** e as instruções de uso. Drivers de isolate: QuickJS, Node, Cloudflare Worker.
- **Números medidos** (demo N+1 produtos): tools clássicos = 4 LLM calls, 9,8k de contexto, 27s, **resposta errada** ($139,25 — LLM fazendo média); code mode = 2 calls, 1,7k, 8s, **correta** ($137,75 — TS fez a média). Com skill reusada: 2 calls, 0,5k, 3s.
- **Code Mode Skills**: "o código gerado É um tool" — código que funcionou é **serializado como skill** {nome, input schema, output schema, código, descrição}, persistível em disco/DB, e **reusado** na próxima pergunta igual, pulando a geração. Extensão (Nx): um build script converte skills markdown (ex.: `nx show project | jq`) em code mode skills executáveis e **reescreve a skill original** para dizer "prefira o tool `list_projects` se disponível" — skills gerando skills.
- **Self-healing**: erro de execução no isolate → o modelo tenta outra abordagem no mesmo fluxo (visível na demo do "dependency neighborhood").
- **Observabilidade**: trace em tempo real código→tools (1 `execute_typescript` → 1 `mcp_workspace` call).
- **Modelo barato de propósito**: tudo roda em **Haiku** — "gera código extremamente rápido"; code mode é *eficiente, preciso E barato* → permite downgrade de modelo porque escrever código é tarefa mais natural para o LLM do que orquestrar tools.
- **UI dinâmica via código**: em vez de "JSON-specified UI", o código gera o relatório/painel ("why not just use code").
- **Convivência**: tools regulares + code mode no mesmo sistema — code mode não é tudo-ou-nada.

### 2.4 deepseek-harness — code mode (código-fonte verificado)

Não há "um plugin": há **duas metades desacopladas** — o capability seam `ctx.codeRuntime` (`packages/code-runtime/` — *"runtimes know nothing about tools or sessions"*) e o consumer `run_code` no registry (`packages/core/tools/src/code-mode.ts`). Trocar worker-thread por container é troca de pacote, não redesign.

- **O modelo escreve o corpo de uma função async TS** (erasable-only), envelopado com wrap *position-preserving* — erros de runtime apontam para a linha que o modelo escreveu. Executa num `Worker` **novo por run, sem pool** (`env:{}`, `execArgv:[]`, heap 512MB); kernel REPL persistente foi **rejeitado de propósito**: estado entre runs é invisível ao session log e quebra replay.
- **Bindings**: 1 namespace `tools` + `errorClass ToolCallError` materializada de dados; null-prototype + `defineProperty` (tool chamada `__proto__` não quebra); identificadores validados contra conjunto de reservados **portável ES∪Python**.
- **SDK tipado no system prompt** (`renderToolsSdk`): `ToolArgsMap`/`ToolOutputMap`/`declare const tools`, ordem lexicográfica → texto byte-idêntico → **prefixo estável para KV cache**. Honestidade rara: *"trades end-tool schemas for generated SDK text… rather than promising a universal reduction"* — o ganho real é eliminar round-trips e manter intermediários fora do contexto, não o tamanho do prompt.
- **Pipeline duplo**: `run_code` atravessa o pipeline completo (hooks→permission→guards→execute→post-execute), e **cada sub-chamada atravessa o pipeline inteiro DE NOVO** com identidade determinística `<parent>:code:<n>` — sem atalho de permissão por "estar dentro do programa". Guards **monotônicos** (qualquer um nega, nenhum força allow); `ask` sem seam de approval = **deny** (fail-closed).
- **Colapso de modo com predicado único**: sob `mode:'code'`, chamada direta a outra tool é negada com mensagem que **ensina a rota** ("call `<name>` from inside a `run_code` program"), e a seção de prompt é gerada pelo **mesmo predicado** que o executor usa — o prompt não pode declarar regra que o registry não aplica.
- **Dois budgets independentes**: `computeMs` 60s de busy time **medido no worker** (`eventLoopUtilization`, ungameable) + `maxWallMs` 600s. Sub-dispatch com fila ordenada, classificação de concorrência **fail-closed** (só `true` exato paraleliza), cap `maxParallelSubCalls=10`.
- **Output**: `OutputLedger` 64MiB cobra a serialização exata, em duplicata (worker + host, peer hostil); overflow = falha explícita `output-limit`, nunca truncamento in-band. **Valores intermediários dos bindings NÃO têm cap** — a inversão que faz code mode valer: filtra 40MB de grep, retorna 200 bytes. **Spill** no resultado externo (preview head/tail + locator + `retrievalHint`) e separadamente na cópia durável de log.
- **Erro é campo, nunca rejection** — taxonomia ortogonal de 6 kinds (`exception|timeout|abort|worker-exit|invalid-output|output-limit`), cada `message` escrita para o modelo se auto-corrigir.
- **Decisão native/code é config opt-in** (`mode: native|code|both`), sem heurística de runtime. Rationale registrada: *"single calls (bash, read, edit) are already ideal as native calls; forcing every edit through a program taxes the common case"* — always-code (Cloudflare-faithful) foi rejeitado para coding agents.
- **Postura de confiança documentada**: *"containment, not a security boundary — model code has bash-equivalent trust"*; `isolation` é label diagnóstico, não claim de segurança. Peer hostil: toda mensagem do worker é revalidada e **reconstruída** campo a campo.

### 2.5 tanstack/ai — implementação (código-fonte verificado)

- **Code mode como tool**: `createCodeMode({driver, tools})` (`packages/ai-code-mode/src/create-code-mode.ts`) devolve um `ServerTool` padrão + system prompt gerado por `createCodeModeSystemPrompt`. `CodeModeTool = ServerTool<SchemaInput, SchemaInput, string, unknown>` — qualquer tool com `.execute()` entra.
- **Bindings**: `toolsToBindings(tools, 'external_')` (`create-code-mode-tool.ts:108`) converte cada tool em função global do sandbox; o driver Node injeta via `context.eval('async function external_<name>(input) {…}')` (`packages/ai-isolate-node/src/isolate-driver.ts:215`). `ToolBinding` = `{name, description, inputSchema, outputSchema, execute}`.
- **Typings gerados de JSON Schema**: `generateTypeStubs` → `jsonSchemaToTypeScript` (`packages/ai-code-mode/src/type-generator/json-schema-to-ts.ts`) — JSON-Schema-nativo (tipos básicos + enum + anyOf/oneOf; **sem** `$ref`/`$defs`). As assinaturas TS vão no system prompt.
- **Progressive disclosure dupla (lazy + code mode)**: tools com `lazy: true` ficam FORA do system prompt e são reveladas via tool sintético `discover_tools` (`create-system-prompt.ts:32-56` separa eager/lazy — stubs completos só para eager, catálogo "Discoverable APIs" para lazy). `createCodeMode({tools: await mcp.tools({lazy:true})})` = a maior compressão de contexto do framework (não documentada!).
- **MCP compõe estruturalmente**: `McpServerTool` é `ServerTool` com `metadata.mcp`; `inputSchema` MCP (JSON Schema cru) passa **por referência** até os stubs. Validação de input NÃO acontece host-side para schemas crus (só o servidor MCP valida).
- **Gate de secrets**: `warnIfBindingsExposeSecrets` (`validate-bindings.ts`) varre os `inputSchema` dos bindings procurando `apiKey`/`token`/`clientSecret`; handler `warn|throw|ignore|fn`.
- **4 perdas silenciosas na composição MCP→code mode** (não testadas no repo — anti-padrões a evitar em qualquer implementação): (a) `abortSignal` descartado no `toolContext` dos bindings → cancelamento morre dentro do sandbox; (b) `metadata` inteiro jogado fora → UI resources/annotations/roteamento somem; (c) `needsApproval` por-tool vira aprovação por-bloco-de-código (mudança de superfície de segurança); (d) nomes com `-`/`.`/`/` viram `SyntaxError` no `context.eval` (bindings não sanitizados).
- **Lifecycle**: fechar clients MCP via middleware terminal (`onFinish/onAbort/onError`), nunca `try/finally` em streaming (mata chamadas em voo).
- **Code Mode Snippets** (`packages/ai-code-mode-snippets` — o pacote das "skills" dos vídeos): snippets são funções TS que o LLM **cria, cataloga e invoca através de sessões** ("compounding capability over time"). Mecanismos verificados:
  - `codeModeWithSnippets({config, adapter, snippets:{storage, maxSnippetsInContext:5}, messages})` → `{toolsRegistry, systemPrompt, selectedSnippets}` — a **seleção de snippets relevantes usa um modelo barato dedicado** (`claude-haiku-4-5` no exemplo do README; o modelo principal fica separado), com cap de 5 no contexto (RAG sobre a biblioteca, não dump).
  - **Trust strategy com progressão por outcome medido** (`trust-strategies.ts`): `untrusted` (novo) → `provisional` (10+ execuções, ≥90% sucesso) → `trusted` (100+, ≥95%). `SnippetStats {executions, successRate}` — confiança é conquistada por estatística de execução, não declarada. **⚠ CORREÇÃO (rodada 2, sessão analise)**: o trust é **DECORATIVO** — `snippetToTool` nunca lê `trustLevel` (a doc admite: *"metadata only — does not currently gate execution"*); transições são monotônicas **sem rebaixamento** (um `trusted` que passa a falhar 100% fica ✓ para sempre); **sem invalidação/versionamento** (`dependsOn` é gravado e nunca lido; tool subjacente muda → snippet quebra só em runtime). Um porte deve fechar os dois buracos: trust GATEANDO execução + hash das assinaturas subjacentes (estilo `judge_attest`) + rebaixamento por janela de falhas.
  - `snippetsToTools`/`snippetToTool` (snippet promovido a tool direto — reuso sem `execute_typescript`), bindings `snippet_*` dentro do sandbox, `createSnippetManagementTools` (o LLM gerencia a própria biblioteca), storage plugável separando `code.ts` de `meta.json` (artefato legível, diffável, revisável por humano) + guarda path-traversal `SAFE_SNIPPET_NAME`. **⚠ CORREÇÃO (relatório final do agente)**: a composição snippet→snippet NÃO funciona — snippet executado recebe só `baseBindings` (`external_*`); chamar `snippet_b` de dentro de um snippet dá `ReferenceError`, e o prompt mostra exemplo de composição que só vale no nível de cima. Telemetria também difere: dentro de snippet-via-binding o emissor de eventos é no-op. Trust badge (✓/◐/○) é VISÍVEL ao modelo no prompt — o detalhe bom a copiar.
  - Lifecycle provado por teste determinístico: run 1 resolve via `execute_typescript` e registra o snippet; run 2 chama o snippet direto.

### 2.6 Fusão com a investigação paralela (sessão `analise`, bundle `~/projects/analise/docs/plans/2026-08-23-code-mode/`) + rodada final do agente tanstack

Investigação independente sobre as MESMAS 5 fontes, com 2 rodadas CCE (ledger convergido 2×), 12 práticas-mestre M1–M12, matriz de lacunas contra baseline medido e diretivas D0–D9. Achados que esta estratégia não tinha:

- **🔴 D0 — BUG no `touring run` (medido por 3 sondas)**: stderr é ENGOLIDO em todas as linguagens (`stderr: ""` para traceback Python, `>&2` do bash, erro de sintaxe; timeout vira `exit -2` sem rótulo; `stderr_truncated: false` mente). Self-healing (P6/P15) é **impossível** sem isso; candidato a causa-raiz de `ceg_sandboxed_count=0`. Gotcha: `touring memory recall "gotcha:touring-run-stderr-engolido"`. **Pré-requisito de qualquer T7/T11.**
- **O prompt real do dsh é 94% SDK gerado**: 27 linhas de prosa / 416 de declarações tipadas (snapshot `system-prompt.expected.md`, 443L). Sob `mode:'code'` o SDK É a documentação (JSDoc dentro dos tipos). `SDK_INSTRUCTIONS` = 4 contratos em 4 bullets (chamar/falhar/concorrer/emitir) — *"every other intermediate result stays out of the conversation, so extract just what you need"*. `run_code` exige **2 args (`code` + `description`)** — modelo que emite só `{code}` perderia o programa em INVALID_ARGS, então ambas as descrições abrem nomeando os dois.
- **8 alternativas rejeitadas** (design note dsh 2026-06-15, com justificativas literais): `node:vm` não é isolamento (prototype-chain escapes) e não interrompe hot loop; **elisão/sumarização de resultados é COMPLEMENTAR ao code mode, não competidora**; sempre-exclusivo taxa o caso comum; aliases sanitizados desnecessários (*"models handle `tools[\"my-tool\"](…)` fine"* — chaves citadas resolvem o que o TanStack tenta sanitizar); tiers de visibilidade DEFERIDOS por falta de evidência; kernel REPL quebra *"every request is a pure function of the log"*.
- **Blueprint do SDK Python** (`py-types.ts:733-818`): TypedDict por tool + `class Tools(Protocol)` + docstring DENTRO do método + o parágrafo **STATIC STUB** obrigatório (*"exactly two names are bound: `tools` and `ToolCallError`… never `FooArgs(field=1)`, which raises NameError"* — TypedDict parece chamável). A mesma semântica exige prosa diferente por linguagem. Backend Python: subprocess CPython, JSON-lines no **fd 3** (stdout/stderr livres para o programa).
- **🎯 KV-cache hygiene (candidato D9)**: postmortem dsh — a sumarização usava `system` DIFERENTE e invalidava TODO o KV-cache do provedor; fix = parte variável no FIM como mensagem `user`. **Nossos hooks injetam timestamps/contadores/scores a cada prompt** — auditar quais injeções quebram o prefixo estável. Custo direto em toda sessão.
- **Contrafactual medido, não estimado**: a economia do TanStack é ESTIMADA (`efficiency.ts`, constantes fixas, `roundTripsActual=2`, disclaimer no código); **não há número medido code-mode-vs-tool-calling no repo** (models-eval compara modelos DENTRO do code mode; o A/B instrumentado existe mas sem resultado versionado). O argumento forte é estrutural: o dataset exige 22 tool calls no braço regular vs 1 `execute_typescript`. Mecanismo transferível: `MessageSizeOverlay` reconstrói o contrafactual a partir dos eventos `external_call/result` — mede economia real sem rodar o baseline. dsh Risks: **"no unconditional-savings claim"** — o custo do SDK pode rivalizar com os schemas nativos; medir sempre.
- **Modelo de REPLAY dos drivers remotos** (Cloudflare/Daytona): cada rodada re-executa o programa do zero com cache de resultados (`__ToolCallNeeded`, id sequencial `tc_N`); efeitos colaterais re-executam por rodada; `Promise.all` vira batching natural; teto `maxToolRounds=10`. Restrição de design para qualquer executor remoto: **o programa precisa ser determinístico até o próximo `external_*`**.
- **Guardas que faltam em ports** (rodada final do agente tanstack): fan-out `maxToolCalls=1000` só no driver Bun (*"deadline só limita wall-clock, não o burst"*); caps de log (10k entries/1M chars com marcador); probe de addon nativo em subprocesso (segfault não capturável em JS); assimetria de sanitização de nomes entre drivers (Cloudflare valida, Node/QuickJS não); QuickJS: asyncify corrompe refcounts → `newPromise`+`executePendingJobs`, e **vazar a VM em vez de abortar** quando o guest não settla; console NÃO streama (lote pós-execução), `external_*` streamam (dreno a 10ms); 1 isolate novo POR chamada de snippet.
- **Falta o gate de aprovação humana exatamente onde mais importa**: o core TanStack suporta `needsApproval` com edição de args pelo aprovador ("revise o TypeScript antes de rodar"), mas `createCodeModeTool` não o liga e a config não expõe knob — a tool que executa código arbitrário é a única sem gate.
- **UI-como-binding** (rodada 2): o relatório dinâmico são 25 bindings `external_report_*` DENTRO do sandbox; handlers persistidos como string de código executada em isolate próprio com bindings restritos + validador; grafo de reatividade declarado por binding; watchers em 2 fases com orçamentos próprios.
- **Capacidade se MEDE** (`ai-sandbox-*`): capabilities gated com `UnsupportedCapabilityError` (nunca no-op silencioso); duas flags "true por raciocínio" caíram quando medidas; suítes de conformidade com named skip.
- **Piso de capacidade**: 9 modelos locais excluídos do eval por não operarem code mode (TS inválido, alucinam resultados, não invocam execute) — **fallback para tool calling clássico é obrigatório**. Lazy: *"all sandbox bindings are always injected — lazy only defers documentation, not callability"*; *"partition by frequency, not capability"*.
- **Bindings como capability viva** (blog Cloudflare): *"already-authorized client interface… the AI cannot possibly write code that leaks any keys"* — a credencial nunca está no ambiente do sandbox.

**Decisão de Gabriel já registrada na outra sessão (23/08 ~22:23)**: D5+D8 (doutrina) só apresentar; D1–D4 + D6+D7 → **backlog formal** no DAG `task_1787534694944647530` (projeto `analise`; d0 = bug stderr, priority high, pré-req de d3); D9 = candidato pendente. As diretivas D1–D4 são deste lado (workspace touring) e equivalem a: D1≈T1+T4 · D2≈T3+T13 · D3≈T9+T10+T11 (dependente de D0) · D4≈T5 ampliado com contrafactual.

## 3. Best practices destiladas (P1–P10)

| # | Prática | Fonte | Essência |
|---|---|---|---|
| **P1** | **Código como interface, não JSON** | todas | O modelo escreve código tipado; loops/estado/paralelismo/matemática rodam no runtime, não no contexto. N round-trips → 1. |
| **P2** | **Superfície mínima: `search` + `execute`** | Cloudflare | Nunca expor o catálogo inteiro; 2 tools que aceitam código comprimem qualquer superfície (−99,9% tokens). Descoberta é programática, não enumerada. |
| **P3** | **Typings no prompt = progressive disclosure** | TanStack | O system prompt carrega as assinaturas tipadas das APIs injetadas — o modelo descobre a API por tipos, não por descrições verbosas. |
| **P4** | **Sandbox zero-capability + grants explícitos** | Cloudflare (+CEG) | Nasce sem nada (nem fetch); capacidades entram como APIs nomeadas. Deny-by-default é arquitetura, não configuração. |
| **P5** | **Código que funcionou vira tool (skills)** | TanStack | Serializar {código, schemas, descrição} de execuções bem-sucedidas e reoferecer — amortiza a geração e barateia queries repetidas 4–10×. |
| **P6** | **Self-healing por erro de execução** | TanStack | Syntax/runtime errors são feedback de alta qualidade; retry no mesmo fluxo com o erro como contexto. |
| **P7** | **Observabilidade absoluta do código** | Cloudflare+TanStack | Todo código executado é persistido e rastreável até as APIs que tocou; auditoria = reler o código. |
| **P8** | **Modelo barato para gerar código** | TanStack | Code mode roda bem em modelo pequeno (Haiku) — gerar código é mais fácil que orquestrar tools; rotear por tarefa. |
| **P9** | **Agent DX: erros que ensinam** | Cloudflare | Mensagens de erro dizem o próximo passo; docs markdown; descoberta via search. O agente é usuário de primeira classe. |
| **P10** | **Habitar o estado, não gerar apps** | Cloudflare | Expor o ESTADO do sistema como API permite ao modelo operá-lo diretamente — comportamento emergente sem código dedicado. |
| **P11** | **Lazy tools + `discover_tools` (disclosure dupla)** | tanstack/ai | Tools marcadas `lazy` ficam fora do system prompt; um tool sintético `discover_tools` as revela sob demanda. Compõe com code mode = compressão máxima. |
| **P12** | **Higiene de bindings: secrets scan + contexto completo + sanitização** | tanstack/ai (por contraste) | Varre schemas por parâmetros de credencial antes de injetar; propaga `abortSignal`/metadata/approval para dentro do sandbox; sanitiza nomes para identificadores válidos. As 4 perdas silenciosas do TanStack são o anti-exemplo. |
| **P13** | **Runtime separado do catálogo (seam)** | deepseek | O runtime recebe `(programa, funções nomeadas, signal)` → `{value, logs, error?}`; não sabe o que é tool/sessão/permissão. Trocar worker por container = trocar pacote. |
| **P14** | **Fresh-per-run; sem estado entre runs** | deepseek | Kernel REPL persistente rejeitado: estado entre runs é invisível ao log de sessão e quebra replay/auditoria. Para harness com replay é não-negociável — e barato. |
| **P15** | **Erro como campo, taxonomia ortogonal** | deepseek | 6 kinds independentes (`exception/timeout/abort/worker-exit/invalid-output/output-limit`), cada message escrita para o modelo escolher a correção certa. Colapsar em "erro" destrói o self-healing. |
| **P16** | **Sub-chamadas atravessam o pipeline de permissões inteiro** | deepseek | Cada chamada de dentro do programa passa por hooks/permission/guards de novo, com identidade determinística `<parent>:code:<n>`. Sem atalho por "estar dentro do código". Guards monotônicos + `ask` sem approval = deny. |
| **P17** | **Prompt e executor derivam do mesmo predicado** | deepseek | A regra declarada no prompt e a regra aplicada pelo registry vêm da MESMA função — o prompt não pode prometer o que o executor não aplica. Negação ensina a rota correta. |
| **P18** | **Dois budgets: busy-time medido no substrato + wall clock** | deepseek | CPU sozinho não vê await eterno; wall sozinho pune espera por tool lenta. Busy time medido DENTRO do sandbox é ungameable. |
| **P19** | **Intermediários sem cap; resultado externo capado com spill+locator** | deepseek | O programa filtra 40MB e retorna 200 bytes — capar o intermediário mata o caso de uso. Resultado grande → falha explícita `output-limit` ou spill com preview + locator + `retrievalHint` (nunca truncamento silencioso). |
| **P20** | **Code mode é opt-in; native para o caso comum** | deepseek | `mode native\|code\|both` por config/agente. Single calls (bash/read/edit) são ideais como tool call nativo; forçar tudo por programa taxa o caso comum. Code mode brilha em composição N≥3. |
| **P21** | **KV-cache hygiene: prefixo estável, variável no fim** | deepseek (postmortem) | Conteúdo variável (timestamps, contadores) em `system`/prefixo invalida TODO o cache do provedor. Parte variável vai ao FIM como mensagem `user`; declarações ordenadas deterministicamente. |
| **P22** | **Contrafactual medido; "no unconditional-savings claim"** | dsh Risks + TanStack | Nunca portar números estimados. Reconstruir o contrafactual dos eventos (`external_call/result` → tool-parts sintéticas) mede a economia real sem rodar o baseline. |
| **P23** | **Piso de capacidade + fallback declarado** | tanstack eval | Modelos pequenos não operam code mode (TS inválido, alucinam, não invocam execute). Todo deploy declara fallback para tool calling clássico. |
| **P24** | **Teto de fan-out e caps de log, além de timeout** | tanstack (Bun driver) | "O deadline só limita wall-clock, não o burst": `maxToolCalls` + caps de log com marcador de truncamento são budgets independentes do tempo. |
| **P25** | **Aprovação humana com edição do código** | tanstack (gap) | A tool que executa código arbitrário precisa do gate `needsApproval` com possibilidade de o aprovador EDITAR o programa antes de rodar — exatamente o knob que o TanStack esqueceu de ligar. |

## 4. Mapeamento → Claude Code (como o TACO opera)

| Prática | Estado atual | Ação |
|---|---|---|
| P1 | Reflexos #8/#9 (compute-in-code, sandbox-first) já mandam; adoção irregular (lição cont.¹⁰: disponibilidade ≠ adoção) | **Endurecer o reflexo**: toda operação ≥3 itens vira 1 script (`touring run`/Bash único), nunca N tool calls; `--brief` sempre que o valor final basta |
| P2 | ToolSearch deferred tools JÁ é search+execute para MCP (~140 tools fora do handshake) | Tratar ToolSearch como padrão-Cloudflare: buscar em lote, nunca 1-a-1 |
| P3 | Skills/rules carregam comandos; sem typings executáveis | Ao delegar a subagents, incluir assinaturas concretas (não descrições) — já exigido pela injection-density invariant |
| P5 | `#kind:snippet` + portfolio + TACO-skilling existem | **Fechar o loop**: código que resolveu → `memory store #kind:snippet` com purpose/domain no phase-close (hoje é manual e esquecido) |
| P6 | systematic-debugging + gotcha | Erro de script = feedback imediato para retry no MESMO turno, não abandono do code mode |
| P8 | Agent tool aceita `model`/`effort` | Delegar geração de código mecânico/varreduras a subagents `haiku`/effort low |

## 5. Mapeamento → Touring (gaps e oportunidades)

Baseline medida (2026-08-23): `touring run` 12 langs deny-by-default ✅P4; CEG X0..X9 ✅P4; mas `ceg_sandboxed_count=0` (tudo fast-path), `sandbox_tee_persisted_count=0` (P7 inativo), `pillar_induction 0/0` (nudges desarmados). Masters ✅P2 parcial.

| # | Oportunidade | Prática | Esboço |
|---|---|---|---|
| **T1** | **Bindings Touring DENTRO do sandbox** | P1+P3+P4 | Hoje o código em `touring run` não enxerga as APIs do Touring; o modelo mistura sandbox + shell externo. Injetar uma stdlib `touring.*` (py/js) com bindings tipados para index/ast/wiring/memory/tantivy — capability-gated por perfil CEG. 1 execução faz descoberta+análise+síntese. |
| **T2** | **`search`+`execute` como fachada MCP** | P2 | Reduzir o handshake MCP (23 tools) a 2-3 tools código-first (`touring_search`, `touring_execute`); os ~119 tools por nome viram funções descobríveis programaticamente. |
| **T3** | **Skill harvest automático do sandbox** | P5 | Execução bem-sucedida em `touring run` com padrão reutilizável → auto-oferta de `memory store #kind:snippet` com input/output schema inferidos (análogo direto de Code Mode Skills; o substrato — hashtag library + portfolio — já existe). |
| **T4** | **Typings no nudge/prompt** | P3 | `cli-suggest`/pillar induction injetarem a ASSINATURA da API sandbox (não só o comando), cumprindo a injection-density invariant. |
| **T5** | **Tee de observabilidade ligado** | P7 | `sandbox_tee_persisted_count=0` — persistir todo código executado + APIs tocadas (journal por execução, consultável: "que código fez X?"). |
| **T6** | **Erros agent-first no CLI** | P9 | Toda mensagem de erro do `touring` CLI diz o próximo comando (muitas já dizem; auditar as que não). |
| **T7** | **Self-healing no runner** | P6 | `touring run` com falha → retornar erro estruturado {linha, sugestão, snippet corrigível} em vez de stderr cru. |
| **T8** | **Roteamento de modelo no ADW** | P8 | Nós `agent` de ADWs que só geram código → modelo barato por default (factory/RL decide). |
| **T9** | **Spill com locator + retrievalHint no `touring run`** | P19 | Output grande do sandbox → preview head/tail + caminho em disco + hint de leitura (`read --offset/--limit`, `grep <path>`), nunca truncamento silencioso. O `--brief` atual comprime; falta o locator recuperável. |
| **T10** | **Dois budgets no sandbox** | P18 | Hoje `touring run` só tem wall-clock (30s/120s). Adicionar busy-time medido no processo filho (ungameable) — espera por I/O não consome, loop quente expira. |
| **T11** | **Erro estruturado com taxonomia no runner** | P15+P6 | `touring run` retornar `{kind: exception\|timeout\|abort\|proc-exit\|invalid-output\|output-limit, message}` com message escrita para o modelo se corrigir — alimenta o self-healing e o action-outcome learning. |
| **T12** | **Sub-chamadas gated no futuro bindings** | P16 | Quando T1 existir: cada chamada `touring.*` de dentro do sandbox atravessa o CEG (X0..X9) de novo com identidade `<run_id>:code:<n>` — sem atalho de permissão por estar dentro do código. |
| **T13** | **Trust progression nos snippets da memória** | P5 + trust-by-stats | Snippets `#kind:snippet` ganham `{executions, success_rate}` alimentados por `learning reward`, com níveis `untrusted→provisional→trusted` (thresholds TanStack: 10+/90%, 100+/95%). Portfolio/recall priorizam trusted; `cli-suggest` só sugere snippet ≥ provisional. **Fechar os buracos do TanStack**: trust GATEIA execução; hash das assinaturas subjacentes invalida (estilo judge_attest); rebaixamento por janela de falhas; badge visível ao modelo. |
| **T14** | **🔴 D0 — corrigir stderr engolido no `touring run`** | pré-req de P6/P15 | BUG medido (3 sondas): `stderr:""` em todas as linguagens, timeout `exit -2` sem rótulo, `stderr_truncated:false` mente. Sem isso, self-healing e taxonomia de erro (T7/T11) são impossíveis. **Priority high; já registrado como subtask `d0` no DAG `task_1787534694944647530` (projeto analise).** |
| **T15** | **Auditoria KV-cache das injeções de hooks (D9)** | P21 | Hooks (Touring cli-suggest + loop-engineering + prompt-enhancer) injetam timestamps/contadores/scores a cada prompt — auditar o que quebra o prefixo estável do provedor; mover o variável para o fim / estabilizar o texto. Custo direto em TODA sessão. Decisão pendente de Gabriel (candidato D9). |

> **Unificação de backlog**: as oportunidades T equivalentes às diretivas D já decididas por Gabriel vivem no DAG `task_1787534694944647530` (projeto `analise`): d0 (=T14) · d1 (≈T1+T4) · d2 (≈T3+T13) · d3 (≈T9+T10+T11) · d4 (≈T5+contrafactual) · d6/d7 (pipeline analise). Extras exclusivos desta estratégia, ainda sem subtask: T2 (fachada MCP search+execute), T6 (erros CLI agent-first), T8 (roteamento de modelo em ADW), T12 (sub-chamadas gated CEG — desenho para quando T1 existir), T15/D9 (pendente decisão).

## 6. Próximos passos (HUMAN GATE — atualizado pós-fusão)

**Já decidido por Gabriel (23/08 ~22:23, sessão analise)**: D1–D4 + D6+D7 em backlog formal (`task_1787534694944647530`, projeto analise, d0 priority high); D5+D8 apenas doutrina registrada; nada implementado.

**O que resta decidir**:

1. **Ordem de ataque do backlog Touring** — recomendação revisada pós-fusão: **d0 (T14, o bug de stderr) PRIMEIRO** — é pré-requisito de d3 e candidato a causa-raiz de `ceg_sandboxed_count=0`; depois d3 (taxonomia+spill), d1 (SDK stub), d2 (snippet→binding com trust GATEANDO).
2. **D9/T15 (KV-cache hygiene dos hooks)** — candidato novo, pendente de aprovação; custo direto em toda sessão.
3. **Extras exclusivos desta estratégia** (T2/T6/T8/T12) — promover a subtasks do mesmo DAG ou manter em observação.

Ao aprovar: executar via INNER loop com gates (as fases d* já têm dono e evidência nos dois bundles).

## 7. Proveniência

- **Cópia durável** (clones + transcripts, ex-scratchpads efêmeros): `~/references/code-mode-2026-08-23/{deepseek-harness, tanstack-ai, subs}` (tanstack @HEAD d3aa104).
- Transcrições: `subs/*.txt` (yt-dlp, legendas oficiais/auto, dedup).
- **Bundle da investigação paralela** (sessão analise, 2 rodadas CCE): `~/projects/analise/docs/plans/2026-08-23-code-mode/` — `strategy` (M1–M12, D0–D9, matriz de lacunas), `fontes-videos` (27 achados V*), `fonte-tanstack-ai`+`rodada2-tanstack` (TS-1..17 + prompts literais), `fonte-deepseek-harness`+`rodada2-dsh` (DSH-1..16 + 8 alternativas + SDK Python). Memórias: `code-mode:estrategia-5-fontes-2026-08-23`, `code-mode:rodada2-2026-08-23` (memory.db do projeto analise).
- Diagnóstico: [/diagnostics/](/diagnostics/) · Ledger CCE: `~/.touring-explore/code-mode-best-practices-para-claude-code-e-tour.ledger.json` (converged, lente external=visited).
- Backlog decidido: DAG `task_1787534694944647530` (projeto analise) — d0/d1/d2/d3/d4/d6/d7.
