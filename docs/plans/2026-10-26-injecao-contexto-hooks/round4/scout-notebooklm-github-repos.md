---
plan_id: 2026-10-26-injecao-contexto-hooks
type: Research
title: "Scout NotebookLM 'GitHub Repos' — harnesses de agentes e injeção de contexto"
timestamp: 2026-10-26
role: scout
phase: 1
task: notebooklm-github-repos
---

# Scout — NotebookLM "GitHub Repos" (harnesses de agentes e injeção de contexto)

**Notebook:** `GitHub Repos` · id `0f7f9b3e-0376-4e96-9f76-502dfe20206c` · 107 fontes · atualizado 2026-10-26T16:42:41Z
**Consultas:** 6 (`nlm notebook query`, `--timeout 180`, uma conversa nova por consulta)

**Convenção de citação.** `[n]` = posição da fonte na saída de `nlm source list 0f7f9b3e-…` (1..107, ordem do servidor). Cada `[n]` vem acompanhado do repositório para não haver ambiguidade. O NotebookLM também numera citações por resposta (1..N dentro de cada resposta); essa numeração é **por consulta** e está registrada na tabela de rastreabilidade da seção 6.

---

## 1. Procedência do notebook

| Atributo | Valor | Verificação |
|---|---|---|
| Título exato | `GitHub Repos` | `nlm notebook list` → 171 notebooks; **apenas 1** com esse título (unicidade confirmada por varredura dos 171) |
| id | `0f7f9b3e-0376-4e96-9f76-502dfe20206c` | `nlm notebook get` |
| Nº de fontes | **107** | `nlm notebook list` e `nlm notebook get` concordam |
| Tipo das fontes | **107/107 `web_page`** | contagem por `type` no `source list` — nenhuma fonte é `file`, `text`, `drive` ou `youtube` |
| Status das fontes | 107/107 com `status: 2` (processada) | idem |
| Data declarada | `updated_at: 2026-10-26T16:42:41Z` | não há data declarada de criação nem de "última indexação" das fontes |

### Repositórios presentes (seleção relevante a esta tarefa)

`[48]` google-research/harness2 · `[13]` affaan-m/ECC · `[61]` mksglu/context-mode · `[81]` trailhq/Trail-Code · `[85]` xerj-org/xerj · `[86]` yvgude/lean-ctx · `[72]` ranxianglei/billion-context · `[101]` crates.io/reflex-search · `[106]` crates.io/tokio-prompt-orchestrator · `[15]` anomalyco/opencode · `[20]` anthropics/claude-code · `[49]` google-research/reasoning-bank · `[54]` langchain-ai/open-swe · `[4]` HKUDS/CLI-Anything · `[80]` tool-bunker/code-buster · `[67]` persiyanov/herdr-reviewr · `[12]` addyosmani/agent-skills · `[58]` mattpocock/skills · `[64]` multica-ai/andrej-karpathy-skills · `[27]` anthropics/knowledge-work-plugins · `[99]` crates.io/pond-db · `[96]` crates.io/memvid-core · `[89]` crates.io/claudectl · `[8]` NandhaKishorM/laya · `[23]` anthropics/claude-for-legal · `[53]` jackwener/OpenCLI · `[35]` cathrynlavery/diagram-design

O restante das 107 fontes é majoritariamente não relacionada (charting: `[30]` apexcharts, `[34]` c3, `[36]` chartist, `[37]` Chart.js, `[40]` d3, `[43]` frappe/charts; diagramação: `[35]`, `[42]` excalidraw, `[59]`/`[60]` mingrammer/diagrams, `[69]` drawnix, `[70]` plantuml, `[71]` react-diagrams; infra: `[73]` rust-lang/rust, `[82]` turborepo, `[1]` ComfyUI, `[3]` Graphite; etc.).

### Limitações de procedência (leitura obrigatória)

1. **As fontes são landing pages do GitHub, não os repositórios.** Cada `web_page` é o HTML da página do repositório — o README renderizado com a navegação do site ao redor. Verificado em `[48]`: o conteúdo bruto tem 21.227 caracteres (21.248 bytes), dos quais **11.423 (53,8%) são chrome de navegação do GitHub** ("Sign in", "Marketplace", menus de "View all features") antes do README começar na **linha 805**. **Nenhum arquivo do repositório foi ingerido como fonte.**
2. **Consequência para harness2: `BENCHMARKS.md` e `EXPERIMENTS.md` NÃO estão no notebook.** O README os *linka* (URLs `github.com/google-research/harness2/blob/main/docs/BENCHMARKS.md` e `…/docs/EXPERIMENTS.md`), mas o conteúdo desses dois documentos **não foi ingerido**. O que o notebook sabe de harness2 é o README, e nada mais. O paper `arXiv:2610.14224` também não está.
3. **A resposta do NotebookLM é geração de LLM sobre as fontes, não recuperação.** Onde ela expande além do texto-fonte, está inventando (verificação no §4.9). Todo número citado neste documento foi reconferido no `nlm source content` da fonte correspondente.
4. **Datas declaradas dos repositórios não estão nas fontes.** Nenhuma fonte carrega data de release, tag ou commit; `updated_at` do notebook é data de uso, não de conteúdo.
5. **Cobertura enviesada.** 107 fontes num notebook chamado "GitHub Repos" não formam um corpus representativo de harnesses — é uma curadoria do início de outubro, não uma varredura sistemática.

---

## 2. harness2 especificamente

**Presença: CONFIRMADA.** `google-research/harness2` está no notebook como fonte `[48]`, tipo `web_page`, URL `https://github.com/google-research/harness2`, status processada. **Não está ausente.**

### O que a fonte de fato contém (texto do README, verificado)

- **Framework**: Harness² — "Recursive Agent Harnessing for an Open World", Google Research, Apache-2.0, arXiv `2610.14224` (2026). Autores listados no README: Xu, Chen, CuiZhu, Dalvi Mishra, Ming, Yu, Han, Lee, Pfister. Disclaimer explícito: "This is not an officially supported Google product."
- **Definição**: "A **harness** is the system around a model that enables it to act as an agent: prompts, tools, memory, skills, orchestration, and more. **Harnessing** is the process of optimizing that system." Pesos do modelo permanecem fixos — o que se otimiza é o entorno.
- **Oito componentes editáveis** `[48]`: *system prompt, guardrails, memory, tool guidance, subagents, skills, scripts, plugins*. "They control context construction, the tool interface, delegation, and behavior inside the tool loop."
- **Dois níveis de recursão** `[48]`:
  - *Task-level* — refina o harness da tarefa corrente por um passo contrastivo **Propose–Probe–Compose**: propõe edições, mede o efeito delas na mesma tarefa, compõe um harness refinado a partir dos contrastes comportamentais entre execuções, **sem ground-truth**.
  - *Harnessing-level* — mantém um *improvement harness* persistente com os harnesses adotados e lições sustentadas por evidência, para guiar refinações futuras.
- **Executores**: OpenCode (padrão) e Codex (`--substrate cx`). **Benchmarks**: JobBench (trabalho profissional), LAB (trabalho jurídico), WorkBuddy-Bench (escritório, web e código).
- **Recursão**: `--k 1` = um passo Propose–Probe–Compose; `--k K --mode parallel` = sondar K candidatos gerais + K específicos de domínio e compor K composições ("explores wider"); `--k K --mode sequential` = refinar ao longo de K passos sucessivos ("refines deeper").
- **Protocolo de avaliação**: "Judging and reporting must use the same benchmark, executor, subset, `--k`, `--mode`, and seed as the refinement run."
- **Riscos declarados pelo próprio repo**: "Executor network access is unrestricted, and Codex runs with its sandbox disabled. Public benchmark rubrics may be accessible online" — o repo manda configurar restrições de rede antes da avaliação para o agente não alcançar gabaritos.
- **Resultados (única frase de número do README)** `[48]`: "Across three professional benchmarks and four model–harness configurations, a single Harness² step improves GLM-5 and Gemini 3.5 Flash over their base harnesses, with gains of up to **11.5 points on WorkBuddy-Bench**, **4.5 points on JobBench**, and **11.8 percentage points on LAB's partial pass rate**. Increasing the task-level recursion budget brings further gains."

### O que a fonte NÃO contém (lacuna dura)

- **Conteúdo de `BENCHMARKS.md`** — ausente. Só existe como URL linkada. Portanto: **não há, no notebook, o detalhe de como JobBench/LAB/WorkBuddy-Bench são construídos, nem a tabela por benchmark, nem os números por modelo.**
- **Conteúdo de `EXPERIMENTS.md`** — ausente. Só a URL. Portanto: **não há no notebook o protocolo de seleção de tarefas, concorrência, reprodutibilidade, comparações de baseline, nem a seção `#web-access` nem `#results-and-resume` — apenas os títulos das seções e as frases do README que as resumem.**
- **Paper arXiv 2610.14224** — ausente do notebook.

### Sobre a pergunta "harness2 mede o efeito da injeção?" (ablação)

**Resposta honesta: o notebook não contém a ablação.** O único evidência de efeito é a frase-resumo do README (11,5 / 4,5 / 11,8 pontos, gains crescentes com o budget `--k`). A resposta do NotebookLM à consulta 2 tentou produzir uma tabela "ablação" (o que muda entre harness base e refinado, papel de cada um dos 8 componentes) — **isso é expansão inventada**: o README nomeia os 8 componentes e diz que eles controlam context construction / tool interface / delegation / behavior; ele **não** descreve o papel de cada um. A frase "accumulating lessons via harnessing-level recursion consistently drives further performance gains" também é inferência: o README só diz que aumentar o budget de recursão traz ganhos adicionais.

**O que é legítimo dizer**: harness2 é o caso mais próximo no corpus de *medir o efeito de mudar o harness* — mas a medição está no paper e nos docs do repo, não no que o NotebookLM absorveu. **A fonte primária virou o GitHub/arXiv, não o notebook.**

---

## 3. Estratégias de montagem de contexto por harness

Legenda: `[n]` = fonte do notebook (repo). "Verificado" = a frase existe no `source content` da fonte.

| Harness / sistema | O que injeta | Por quê | Fonte |
|---|---|---|---|
| **Harness²** `[48]` | 8 componentes editáveis: system prompt, guardrails, memory, tool guidance, subagents, skills, scripts, plugins | Controlar *context construction*, interface de ferramentas, delegação e comportamento no tool loop | `[48]` (README) |
| **context-mode** `[61]` | Hooks de lifecycle (`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `PreCompact`, `SessionStart`) interceptam saída de ferramenta **antes** dela entrar no prompt; nudge de roteamento para `ctx_execute`; snapshot `PreCompact` ≤2 KB; "Session Guide" re-injetado no resume; busca FTS5/BM25 sob demanda | 98% de redução de contexto via sandbox; o snapshot priorizado preserva estado crítico (arquivos em edição, tarefas, decisões) que a truncagem destruiria | `[61]` |
| **context-mode — cadência** `[61]` | Re-disparo do nudge a cada **10ª chamada correspondente** (faixa 1..100, default 10; `1` = "toda chamada", "~250 tokens/call") | "The original implementation fired only once per session, which got lost after context compaction in MCP-heavy sessions" | `[61]` |
| **ECC** `[13]` | Regras de projeto por `CLAUDE.md`/`AGENTS.md`/`GEMINI.md`; skills por `SKILL.md` com *frontmatter* (só nome+descrição no startup); comando `/context-budget`; MCPs limitados | "Each MCP tool description consumes tokens from your 200k window, potentially reducing it to ~70k. Keep under 10 MCPs enabled per project. Keep under 80 tools active." | `[13]` |
| **Trail-Code / Graft** `[81]` | "Graft map": ponteiros para símbolos + linhas "crux" (a guarda/estado central) + arestas tipadas (`depends_on`, `uses`) + **blast radius** após editar um arquivo | Substituir leitura de arquivo de 5.000 linhas por passagem AST; o mapa é determinístico, não estatístico | `[81]` |
| **XERJ** `[85]` | Passagem AST exata (arquivo:linha) via índice, em vez de grep/file dump | 2,7× menos tokens de saída que grep e 26× menos que memória isolada, **mesmo solve rate** | `[85]` |
| **LeanCTX** `[86]` | Modos de leitura especializados (`map`, `signatures`, `diff`, `lines:N-M`, `density:0.4`) + compressão de saída de shell hook sobre **85+ CLIs** (`git`, `docker`, `npm`, `cargo`) | Filtrar stream ruidoso de terminal antes de chegar ao modelo | `[86]` |
| **OpenCode** `[15]` | Agentes embutidos `build` (dev), `plan` (read-only, nega edição de arquivo por padrão), `@general` (subagente de busca); plugins em `experimental.chat.system.transform` injetam blocos de roteamento de system prompt e resumo da sessão anterior | Surrogate de `SessionStart` | `[15]` |
| **Claude Code** `[20]` | Hooks nativos `SessionStart`/`PreToolUse`/`PostToolUse`/`Stop`; regras de projeto; statusline context; skills; prompts de delegação a subagentes | o hook é o ponto de interceptação programática | `[20]`, `[61]`, `[13]` |
| **CLI-Anything** `[4]` | `SKILL.md` + `HARNESS.md` gerados (estrutura de subcomandos, hints de argumento, passos multi-fase); flags `--help` auto-descritivas; saída JSON canônica (`{"ok":true,"data":…}`) em vez de texto livre; REPL skin | Descoberta autônoma de superfície de comando sem conhecimento pré-treinado; eliminar ambiguidade de parsing | `[4]` |
| **Code-Buster** `[80]` | Relatório de análise estática em JSON/SARIF/Markdown — ciclos de dependência, código morto, duplicação, complexidade, segurança — com **rule ID estável, severidade, score de confiança, file:line, rationale e plano de remediação** | Grounding determinístico e offline: "does not upload source code, call an AI provider, consume model tokens" | `[80]` |
| **herdr-reviewr** `[67]` | Comentários inline do revisor humano, com **file path, número de linha e contexto exato do código**, prependados por um cabeçalho `review_prefix` configurável (ex.: "Fix each comment, then run the tests.") | Human-in-the-loop: feedback vira payload de prompt em vez de copy-paste manual | `[67]` |
| **billion-context** `[72]` | Ferramentas `compress`/`decompress`/`search_context` injetadas no stream da API; o modelo dobra mensagens antigas em resumos hierárquicos em passos de ~50k tokens | Compressão incremental em faixas limitadas preserva o prefix cache (95–97% de hit) | `[72]` |
| **reflex-search** `[101]` | Busca symbol-aware via MCP que devolve passagens por símbolo | Schema MCP deferido custa round-trip extra, mas a passagem é mais completa | `[101]` |
| **tokio-prompt-orchestrator** `[106]` | Deduplicação de contexto repetido entre agentes paralelos | "A plain request queue sends every duplicated context to the API" | `[106]` |
| **pond-db** `[99]` / **memvid-core** `[96]` | Memória fora do prompt: ingestão de sessões em datasets columnares (SQL/FTS/vector) e "Smart Frames" imutáveis com timestamp | Consultar decisões passadas sem encher a janela ativa | `[99]`, `[96]` |

### Ausências confirmadas (resultado válido e importante)

Verificado por varredura das 107 URLs: **Aider, OpenHands, SWE-agent, Codex CLI, Continue, Cline, Roo e Goose NÃO são fontes do notebook.** O próprio NotebookLM reconheceu a lacuna na consulta 4 ("detailed source-code specifications for the exact prompt assembly pipelines of SWE-agent, Aider, and OpenHands are not explicitly provided in the notebook materials") — isso é honesto e confirmado.

Observações de borda:
- **Codex** aparece no corpus apenas como (i) executor dentro do harness2 `[48]`, (ii) plataforma-alvo do context-mode via `AGENTS.md` `[61]`, (iii) um dos agentes citados no material do ECC `[13]`. O repositório `openai/codex` não é fonte.
- **SWE-bench** aparece como *benchmark* (não como harness) em `[81]`.
- **OpenHands/SWE-agent/Aider** são lacuna dura — a comparação de estratégias de montagem de contexto entre esses três **não existe** neste notebook.

---

## 4. Métricas e experimentos — números concretos

Todos os números abaixo foram **verificados no texto bruto da fonte** (`nlm source content <source_id>`), não apenas na resposta do NotebookLM. A coluna "conf." traz o veredito da reconferência.

### 4.1 A/B do Trail-Code / Graft — injeção *push* de mapa de código `[81]`

Varredão de 162 runs, 2 repositórios (o próprio repo e um serviço Node/Express real), 3 trials por braço. **Única diferença entre os braços: o graft map está ligado ou não.** Juiz: Opus 4.8 com piso de keyword obrigatório ("a fast but wrong answer couldn't win by being fast"). Custo cache-aware (reads ≈0,1×, writes 1,25×).

| Métrica (média/tarefa) | Agente frio | Com graft map (push) | Melhoria |
|---|---|---|---|
| Custo ($) | 0,0429 | 0,0292 | −32% |
| Uncached input tokens | 8,070 | 4,650 | −42% |
| Tool calls | 4,2 | 2,3 | −46% |
| Latência (s) | 39,8 | 15,8 | −60% |
| Correctness | 93% | 93% | igual |

**push vs pull** (fonte primária, texto literal): "Push, the column above, gave up 3 correctness points on one corpus for its savings. The pull variant never answered worse than cold on any corpus, and gave up most of that speed for something bigger: correctness jumped to 98%, 5 points over cold, the strongest single result in the sweep. **Push when speed is what you need; pull when being right matters more.**" → *conf.: **verificado** no texto-fonte.*

**SWE-bench Verified** (50 instâncias reais, mesmo modelo nos dois braços — Claude Sonnet 5 — mesmas imagens Docker, mesmos limites de turno; "No judge model, no similarity score": o patch é aplicado e os testes do mantenedor rodam):

| | Claude Code frio | Com graft map | Ganho |
|---|---|---|---|
| Correção | 27/50 (54%) | 33/50 (66%) | +12 pts |
| Tokens | 142,0M | 109,4M | −23% |
| Custo | $52,34 | $42,43 | −19% |
| Tool calls | 1.370 | 1.031 | −25% |
| Requisições de API | 2.455 | 1.875 | −24% |
| Wall-clock | 13.094s | 8.922s | −32% |

→ *conf.: **verificado** no texto-fonte (a grafia é `27 / 50`, com espaços — a busca por "27/50" falha e produz falso negativo).*

**PocketBase** (Go, ~350 arquivos, 15 tarefas, Claude Opus, mesmas ferramentas de arquivo; única diferença = graft map ligado):

| | Claude Code padrão | Com graft map | Ganho |
|---|---|---|---|
| Custo | $13,91 | $11,02 | −21% |
| Wall-clock | 2.044s | 1.762s | −14% |
| PRs reproduzidos | 5/5 | 5/5 (mesmos arquivos do mantenedor) | igual |

Pergunta mais difícil (OAuth2 entre provedores): $2,19 → $0,84; "2,6× cheaper on the hardest cross-file question". → *conf.: **verificado**.*

### 4.2 A/B do XERJ — memória vs grep vs passagem indexada `[85]`

8 tarefas, 4 linguagens, 16 runs por braço, contagem real de tokens de `claude -p`:

| Braço | Output tokens | Custo | Resolvido |
|---|---|---|---|
| Só memória | 260.916 | $11,18 | 11/16 |
| Agente grep-driven | 26.477 | $3,27 | 16/16 |
| XERJ (passagem indexada) | 9.982 | $1,58 | 16/16 |

→ "2.7x fewer output tokens than grep and 26x fewer than memory alone at the same solve rate, and up to **278x fewer on a single Java task**". → *conf.: **verificado**.*

### 4.3 Hooks vs arquivos de instrução — o número mais diretamente aplicável ao Touring `[61]`

O context-mode publica uma tabela por plataforma com duas colunas, **"With Hooks"** e **"Without Hooks"**, cujo valor é **"% saved"** (contexto economizado):

| Plataforma | Hooks | Arquivo de instrução | With Hooks | Without Hooks |
|---|---|---|---|---|
| Claude Code | sim (auto) | `CLAUDE.md` | ~98% saved | ~60% saved |
| Gemini CLI | sim | `GEMINI.md` | ~98% | ~60% |
| VS Code Copilot | sim | `copilot-instructions.md` | ~98% | ~60% |
| Cursor | sim | `context-mode.mdc` | ~98% | ~60% |
| OpenCode | plugin | `AGENTS.md` | ~98% | ~60% |
| Codex CLI | sim | `AGENTS.md` | ~98% | ~60% |
| Pi | sim | `AGENTS.md` | ~98% | ~60% |
| Antigravity / Zed | **não** | `GEMINI.md`/`AGENTS.md` | — | **~60%** |

Texto-fonte: "The GEMINI.md file is the only enforcement method (~60% compliance). There is no programmatic interception." E: "Antigravity and Zed do not support hooks. They rely solely on manually-copied routing instruction files (AGENTS.md / GEMINI.md) for enforcement (~60% compliance)."

**Correção importante**: a resposta do NotebookLM rotulou esse par como "98% de redução de contexto via hooks vs 60% de redução via prompt instructions" — a métrica real é **"contexto economizado"**, e o eixo real é **hook programático vs arquivo de instrução**. A substância ("hook vence instrução") está correta; o nome da métrica foi diluído. → *conf.: **verificado**.*

### 4.4 Sobrecarga de schema MCP `[13]`

"Each MCP tool description consumes tokens from your 200k window, potentially reducing it to ~70k. Keep under 10 MCPs enabled per project. Keep under 80 tools active." → *conf.: **verificado**.*

### 4.5 Vetores de desperdício — custos unitários `[61]`

- Snapshot de Playwright: **56 KB**
- 20 issues do GitHub: **59 KB**
- Uma fatia de access log: **45 KB**
- "After 30 minutes, 40% of your context is gone."
- Sandbox `ctx_execute`: **até 98% de redução** (315 KB de saída bruta → 5,4 KB)
- Snapshot `PreCompact`: **≤2 KB**, com descarte por prioridade (conta de chamada MCP e classificação de intenção caem primeiro; tarefas/decisões/arquivos modificados são preservados)
- Re-disparo de nudge a cada 10ª chamada: **~250 tokens/call** no modo mais agressivo

→ *conf.: **verificado**.*

### 4.6 Busca symbol-aware (reflex-search) `[101]`

Ratios são Reflex ÷ built-in (>1 = Reflex custa mais). **Tokens (95% CI)**:

| Tarefa | Modelo | Ratio de tokens | Custo | Turnos (mediana) | Acurácia |
|---|---|---|---|---|---|
| 9 find-all-usages | Opus 5.5 | 1,66 [1,05; 1,69] | 1,27× | 2 → 3 | igual |
| 9 find-all-usages | Sonnet 5 | 1,65 [1,06; 2,25] | 2,07× | 2 → 4 | **Reflex mais completo em 6 de 9 tarefas** |
| 13 comprehension / cross-module | Opus 5.5 | 1,26 [1,07; 1,30] | 1,10× | 6 → 6 | igual |

Causa do custo: "It is round-trips, not payload. Claude Code defers MCP tool schemas, so the first Reflex call costs an extra ToolSearch turn." → *conf.: **verificado**. Nota: o NotebookLM chamou os ratios de "cost ratio"; no texto-fonte 1,26–1,66 são **tokens**, e o custo é 1,10×–2,07×.*

### 4.7 Deduplicação de contexto em swarm `[106]`

"24 Claude Code agents ran on one RTX 4070, and deduplication cut API use to **26% of expected**." → *conf.: **verificado**.*

### 4.8 Harness² `[48]` — único número de efeito de harness no corpus

+11,5 pts (WorkBuddy-Bench) · +4,5 pts (JobBench) · +11,8 pp (LAB partial pass rate), um passo, GLM-5 e Gemini 3.5 Flash, 3 benchmarks e 4 configurações modelo-harness; ganhos crescem com o budget `--k`. → *conf.: **verificado** (frase do README).*

### 4.9 Deduplicação: **o que a resposta do NotebookLM inventou**

Registro explícito, porque o número_false-positive é o que o judge precisa ver:

| Alegação da resposta do NotebookLM | Veredito |
|---|---|
| "raw GitHub issues (59 KB), API access logs (45 KB), and **large data CSVs** … (e.g. **85 KB CSVs**)" | ❌ **FABRICADO.** "85 KB" não existe em `[61]` (verificado: `85 KB` → 0 ocorrências; "CSV" → 1 ocorrência, sem número). Os números reais são 56/59/45 KB. |
| Descrição do papel de cada um dos 8 componentes do harness2 ("Guardrails: safety checks and operational constraints", "Memory: context retention mechanisms…") | ❌ **FABRICADO.** O README só *nomeia* os 8 e diz que eles controlam context construction / tool interface / delegation / behavior. |
| "accumulating lessons via harnessing-level recursion **consistently drives further performance gains**" | ❌ **INFERÊNCIA.** O README diz apenas "Increasing the task-level recursion budget brings further gains". |
| "Reflex cost ratio 1,26× to 1,66×" | ⚠️ **DESLOCADO.** Esses são ratios de *tokens*; o custo é 1,10×–2,07×. |
| "162 runs … on Claude Sonnet 5" | ⚠️ **DESLOCADO.** Claude Sonnet 5 é explicitado para o braço SWE-bench; o sweep de 162 runs tem juiz Opus 4.8 e não explicita o modelo do executor no trecho. |
| "96% / 60% context reduction" | ⚠️ **NOME ERRADO, substância certa.** É "% saved", com hook vs arquivo de instrução. |

---

## 5. Implicações para o Touring

Contexto do mandante: o Touring injeta **27,8% da janela do modelo** por hooks; FATO 35,5% do volume com ≈52% de conformidade; SERMÃO 18,7% com ≈0,3% de adesão; ALERTA sem alvo 20,1% com 0%; RUÍDO 5,3%.

1. **O número externo que mais replica o problema do Touring é o ~60% de adesão por arquivo de instrução vs ~98% por hook** `[61]`. O SERMÃO do Touring (18,7% do volume, 0,3% de adesão) é a **mesma classe de falha** que o context-mode documenta: texto de instrução que o modelo ignora. A leitura de 0,3% é ordens de grandeza pior que os 40% de não-adesão do 60% — o que sugere que o problema do Touring não é "instrução em vez de hook", é **instrução que não é observável nem re-emitível**. O corpus não traz nenhum número para "nudge re-emitido a cada N chamadas" além do **~250 tokens/call** de `[61]` — que é exatamente o custo de se re-emitir um nudge a cada chamada. Isso dá um teto aritmético para calibrar o nudge do Touring.

2. **"Push" e "pull" são trade-offs explícitos, não hierarquia** `[81]`. Push (injetar mapa): −32% custo, −60% latência, **93% de correção igual ao frio**. Pull (recuperar sob demanda): **98% de correção, 5 pts acima do frio**, ao custo da velocidade. O Touring hoje injeta 27,8% da janela — o braço "push". O dado que falta ao Touring é a curva de correção do braço "pull"; sem ela, qualquer corte de injeção é uma escolha cega entre as duas colunas. **Medir push-vs-pull no Touring é o experimento faltante**, e ele é barato: a mesma tarefa, mapa pré-injetado vs mapa recuperado sob demanda.

3. **A taxa de acerto do Touring pode ser maior sem injetar mais** `[85]`. Mesma taxa de resolução (16/16) com 2,7× menos tokens que grep e 26× menos que memória isolada. O corpus sugere que o alavancador do Touring não é *volume de injeção* e sim **densidade do payload injetado** — o que ecoa o contrato `pre_read.rs:1-7` ("SILENCE IS THE DEFAULT / So what?").

4. **O padrão de carga útil vencedora é "ponteiro + trecho + coordenadas", não "arquivo"** `[81]`, `[85]`, `[80]`, `[67]`. Toda fonte vencedora devolve `file:line` + o trecho mínimo + uma estrutura estável (rule ID + severidade + confidence + rationale em Code-Buster; símbolo + linhas "crux" + arestas tipadas no graft map; path + linha + contexto no herdr-reviewr). O payload de hooks do Touring, para ter adesão, provavelmente precisa dessa mesma forma — o que é testável.

5. **A cadência de re-emissão é um parâmetro de projeto, não um detalhe** `[61]`. "Re-fires every 10th matching call… Set to 1 for 'every call' (most aggressive — adds ~250 tokens/call)". O Touring pode tratar a cadência de cada família de nudge como knob explícito, com o custo por chamada medido.

6. **"O que não deve ser injetado" tem uma lista concreta e verificável** `[61]`, `[4]`, `[80]`: saída bruta de ferramenta (56/59/45 KB), dumps de arquivo, secrets não redigidos, contexto duplicado entre agentes paralelos (dedup → 26% da API). Para o Touring, o item 4 (credenciais redigidas antes da persistência/re-injeção) e o item 5 (dedup em fan-out) são checagens diretas do que o corpo de hook já faz.

7. **O harness2 oferece o vocabulário de 8 componentes** `[48]` — system prompt, guardrails, memory, tool guidance, subagents, skills, scripts, plugins — e a distinção entre *task-level* (ajuste local, Propose–Probe–Compose, sem ground-truth) e *harnessing-level* (lição persistente sustentada por evidência). O Touring já tem a segunda forma (memory store + learning reward); a **primeira é o mecanismo que falta**: um *Propose–Probe–Compose* local à tarefa, em que uma edição de hook é proposta, medida **contra a mesma tarefa**, e composta a partir do contraste comportamental entre execuções — **sem ground truth**. É literalmente um A/B do payload de injeção, dentro da tarefa.

8. **A disciplina de comparabilidade do harness2 é a que falta ao Touring** `[48]`: "Judging and reporting must use the same benchmark, executor, subset, `--k`, `--mode`, and seed as the refinement run." Para medir o efeito de uma mudança de injeção no Touring, o executor, o subset, o k e a seed precisam ser fixos entre o braço de controle e o braço tratado — e o julgamento feito com o **mesmo** benchmark dos dois braços.

---

## 6. Lacunas honestas

1. **`BENCHMARKS.md` e `EXPERIMENTS.md` do harness2 não estão no notebook.** Só as URLs. Toda a tabela de benchmarks, o protocolo de tarefas, a concorrência, a reprodutibilidade e as comparações de baseline estão fora. → **A fonte primária do harness2 passa a ser o GitHub/arXiv, não o NotebookLM.**
2. **O paper `arXiv:2610.14224` não está no notebook.**
3. **Aider, OpenHands, SWE-agent, Codex CLI, Continue, Cline, Roo, Goose não são fontes.** A comparação entre esses harnesses é impossível a partir deste notebook.
4. **Nenhuma fonte é o repositório**: são landing pages. Para qualquer número que importe (ex.: uma tabela completa de benchmarks do harness2, ou os fixtures do Trail-Code), é preciso ir ao GitHub.
5. **"Context rot" não é uma métrica medida em lugar nenhum do corpus.** Aparece como conceito em `[61]`/`[13]`, e o ECC inclusive **proíbe** compactar no meio da implementação ("you'll lose variable names, file paths, partial state"), mas nenhum experimento no notebook quantifica degradação por saturação de janela. O claim do mandante sobre "context rot" **não tem contraparte experimental aqui**.
6. **Não há nenhum número de adesão no corpus comparável ao do Touring.** O corpus mede *tokens/economia/correção*; a adesão a nudge (o problema do Touring: 0,3% no SERMÃO) **não é medida por nenhum dos repositórios**. Os 98%/60% de `[61]` são de *contexto economizado*, não de *adesão a instrução*. Usar esse par como se fosse adesão seria ler errado a fonte.
7. **Nenhuma fonte mede o custo de injeção em tokens por turno do lado do hook.** O único valor unitário é os ~250 tokens/call do re-disparo mais agressivo `[61]`.
8. **O notebook é um repositório curado, não um corpus.** 107 fontes de um recorte temporal (início de outubro), majoritariamente não relacionadas a agentes. Ausência de um harness aqui **não** é evidência de que ele não exista.
9. **O NotebookLM é um sintetizador, não um recuperador.** Todas as 6 respostas trazem tabelas e inferências que não estão no texto-fonte (5 casos catalogados no §4.9). Qualquer número daqui que não tenha passado pelo `nlm source content` **não deve subir para o documento consolidado**.

---

## 7. Rastreabilidade das consultas

| # | Pergunta (resumo) | `conversation_id` | Fontes usadas (→ índice) | Resposta |
|---|---|---|---|---|
| 1 | harness2: componentes, benchmarks, resultados | `8f709cde-7f4f-4286-a124-2558925b7931` | `[48]` | 6 citações, todas → `[48]` |
| 2 | harness2: o que muda base→refinado, ablation | (nova) | `[48]` | 5 citações → `[48]`; **expansão inventada** (§4.9) |
| 3 | context engineering: token budget, wasted, context rot | (nova) | 14 fontes: `[48] [75] [61] [13] [81] [89] [12] [35] [58] [85] [86] [72] [99] [96]` | 49 citações |
| 4 | estratégia de montagem por harness | (nova) | 16 fontes: `[13] [61] [85] [12] [8] [49] [54] [72] [86] [64] [35] [4] [81] [101] [27] [15]` | 44 citações; **auto-declara a lacuna** SWE-agent/Aider/OpenHands |
| 5 | medir efeito da injeção: A/B, ablation, o que (não) injetar | (nova) | 7 fontes: `[61] [81] [85] [101] [48] [64] [106]` | 31 citações; **1 número fabricado** (§4.9) |
| 6 | hooks que injetam antes da chamada de ferramenta | (nova) | 7 fontes: `[61] [13] [4] [23] [53] [80] [67]` | 34 citações |

**Método de verificação.** Para cada número citado neste documento, o texto bruto da fonte foi obtido com `nlm source content <source_id>` e o token numérico foi procurado literalmente; os casos `27 / 50` (com espaços) e `85 KB` (inexistente) foram tratados explicitamente. Extração e conferência feitas em `touring run --lang python` (code mode), com contagem literal de ocorrências e extração de janela de contexto em torno do match.

---

