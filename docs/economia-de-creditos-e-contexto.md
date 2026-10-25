---
okf_version: "1.0"
type: doc
title: "Economia de créditos e contexto — divisão operacional (Anthropic × Kimi × MiniMax)"
description: "Corpo canônico da regra que os CLAUDE.md de ~/projects/touring (regra 33) e ~/projects/analise (seção 📌) apontam: os quatro orçamentos, a matriz de roteamento por classe de subtarefa, os fatos datados de cada modelo com fonte oficial, o protocolo de delegação às sessões irmãs e o protocolo de atualização quando surge modelo novo."
plan_id: docs
tags: [rules, custo, contexto, modelos, delegacao, kimi, minimax]
timestamp: 2026-10-17T15:00:00-03:00
---

# Economia de créditos e contexto — divisão operacional

> Diretriz de Gabriel, 08/10/2026. Os dois `CLAUDE.md` carregam só a regra operacional; os
> **fatos voláteis** (preço, janela, limite, posicionamento) moram aqui, cada um com data e
> URL oficial, porque envelhecem — e um fato velho dentro de uma constituição carregada em
> toda sessão é pior que nenhum. Ao achar divergência entre este arquivo e a fonte oficial,
> a fonte vence e este arquivo é corrigido (§6).

## 0. Os quatro orçamentos

| # | Orçamento | Como se gasta | Fato verificado (08/10/2026) |
|---|---|---|---|
| 1 | **Franquia Anthropic** (assinatura) | toda requisição da sessão principal **e** de subagentes, workflows, teammates, `/loop`, goal check-ins e mensagens entre sessões | janela móvel de 5 h + janela semanal, *"shared across all models, so the developer can't restore access by switching models with `/model`"*; só as mensagens "Opus limit"/"Sonnet limit" são por família [C/costs] · *"A single burst of heavy activity, such as a large workflow fanout, can exhaust the weekly allowance"* [C/errors] |
| 2 | **Fable 5.1** | requisições com alias `fable`/`best` | Max e assentos Premium: *"up to 50% of your weekly usage limits on Fable models at no extra cost"*, e *"use them faster than other Claude models"*; Pro e assentos Standard: só créditos pagos [S/15424964] · em `-p` e no Agent SDK sem prompt: *"Claude Code bills it without asking"* [C/model-config] |
| 3 | **Cotas das sessões irmãs** | sessões Claude Code com backend Kimi ou MiniMax | cotas próprias, **fora** da franquia Anthropic (§3) |
| 4 | **Janela de contexto do orquestrador** | cada requisição reenvia a conversa inteira | *"a one-line question in a session that has been open all day still draws usage for the whole conversation"* · `/compact` *"is itself a large request"* · `/clear` *"costs nothing"* [C/costs] |

**Tetos determinísticos de fanout (verificado em 24/10/2026).** Subagentes, workflows e teammates puxam do orçamento 1, e três controles os limitam por código, não por prosa: `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` (padrão 20; no teto o `Agent` falha com `Concurrent subagent limit reached` e o erro manda o Claude não repetir; v2.1.217 ou posterior; sessões com ultracode ativo ficam isentas), `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH` (padrão: até 3 camadas abaixo da conversa principal; `1` desliga o aninhamento) (https://code.claude.com/docs/en/sub-agents) e, no Agent SDK, `max_budget_usd` (`maxBudgetUsd` em TypeScript). O guia do Opus 5 diz por que olhar para eles: *"Claude Opus 5 delegates to subagents more readily than prior models. Delegation is worthwhile for genuinely independent, sizeable tracks of work, but it multiplies cost and time when applied to small tasks"*, e nomeia os três como os tetos determinísticos, ressalvando que versões antigas do Claude Code não os têm (https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5). Estado local: nenhuma das duas variáveis aparece no `~/.claude/settings.json` global lido em 24/10/2026 (o `settings.local.json` e o ambiente do shell não foram lidos).

O orçamento 4 é o que não se vê: 1M de janela não é licença. Leitura que entra no contexto do
orquestrador é paga de novo em todo turno seguinte (a preço de cache read, e cheia depois de
1 h ociosa na conversa principal da assinatura, e depois de 5 min nos demais pedidos: ver o TTL abaixo).

**TTL do cache por balde de pedido (verificado em 24/10/2026).** A **conversa principal** recebe 1 h na assinatura, dentro do uso incluído; com chave de API, créditos ou nuvem recebe 5 min, e cai para 5 min também quando a assinatura estoura a cota e a sessão passa a créditos. **Todo o resto** (subagentes, workflows, teammates, forks e compactação) recebe 5 min, mesmo na assinatura, exceto os pedidos auxiliares que a Anthropic controla no servidor. Efeito em fanout: quem retoma um subagente depois de mais de 5 min paga a escrita de novo, e a escrita de 1 h custa 2× a entrada, contra 1,25× a de 5 min. Controles (v2.1.242 ou posterior): `promptCacheTtl` / `CLAUDE_CODE_PROMPT_CACHE_TTL` para a principal e `subagentPromptCacheTtl` / `CLAUDE_CODE_SUBAGENT_PROMPT_CACHE_TTL` para o resto; `FORCE_PROMPT_CACHING_5M=1` vence todos (https://code.claude.com/docs/en/prompt-caching e https://platform.claude.com/docs/en/about-claude/pricing).

**S8 (08/10/2026) — o conselho dos hooks deixou de ser reenviado inteiro a cada chamada.**
Medido em 12 transcripts deste projeto: o conselho injetado custava ~900 B por chamada de
ferramenta, e como quase nunca é byte-idêntico (carrega o arquivo/número da vez), a dedupe por
bloco com TTL de 300 s o deixava passar — repetindo-se a cada 5 min embora a 1ª cópia já
estivesse na janela. O `touring-hook` agora deduplica por SEÇÃO na saída central do daemon
(`run_hook_command`): bloco repetido (exato ou por molde — diretivas exatas, telemetria com
números/paths normalizados) é omitido até a próxima compactação; linha de prosa repetida some;
diretiva `MUST/SHOULD/MAY` com path novo é diretiva nova e sempre emite. O teto por turno
(`TOURING_TURN_BUDGET`) baixou de 3000 para 2000 pela medição (p50 do turno = 16 072 B; com a
dedupe o gasto legítimo são as seções novas, ~2 blocos × ~900 B). A/B sobre corpus fixo de
79 253 payloads reais (12 maiores transcripts encerrados): **−42,2 %** de bytes injetados no
modo deploy-completo, com controle positivo (nenhum MUST perdido na 1ª emissão). Detalhe:
`docs/plans/2026-10-08-compactacao-65/diagnostics/a2_resultado.json`.

## 1. Matriz de divisão operacional

**Regra de bolso**: antes da 1ª ação de uma tarefa não trivial, divida-a; cada parte desce ao
executor **mais barato que ainda passa no juiz**. O que é determinístico vira código
(`touring run`, script) antes de virar modelo — custo de modelo zero.

| Classe da subtarefa | Executor | Por quê (fonte) |
|---|---|---|
| Orquestrar, decidir, sintetizar, julgar a entrega de um delegado, arquitetura | **sessão principal — Opus 5.5** (default do Claude Code em Pro/Max/Team/Enterprise/API) | *"For long-running agentic coding and knowledge work"* [P/models] · *"Tuning effort is often a better lever than switching models"* [P/choosing] |
| Problema ambíguo e longo em que o Opus 5.5 em effort alto já falhou: causa-raiz de horas, decisão arquitetural L4+, pesquisa profunda multietapa | **Fable 5.1** (`fable`) — interativo, com consentimento; em headless só com o sim do Gabriel | *"For demanding reasoning and long-horizon agentic work"* [P/models] · *"Hand it ambiguous problems: root-cause investigations, outage debugging, and architecture decisions"* [C/model-config] · custa 2,5× o Opus 5.5 por token |
| Implementar com spec clara, testes, refactor delimitado, revisão de código, pesquisa web com julgamento, teammates de agent team | **subagente `model: sonnet`** (Sonnet 5.5) | *"Sonnet handles most coding tasks well and costs less than Opus"* · *"Use Sonnet for teammates"* [C/costs] |
| Buscar, extrair, classificar, rotear, recuperar documentação (Context7), varredura mecânica | **subagente `model: haiku`** (Haiku 5.5), com effort `medium` na varredura de vários passos | *"classification, routing, extraction, and subagent tasks"* [P/haiku-5-5] · *"For simple subagent tasks, specify `model: haiku`"* [C/costs] · **limites (24/10/2026)**: em `low`, nos prompts longos de agente, o Haiku *"is more likely to skip a search, stop early, or skip a check"* (https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-haiku-5-5); Terminal-Bench 4.0 de 39,2% (esforço não informado) contra 70,6% do Sonnet 5.5, e Chartography de 46,4% contra 61,6% (https://www.anthropic.com/claude-haiku-5-5 e https://www.anthropic.com/claude-sonnet-5-5) — **nunca implementação agêntica de vários passos nem leitura de gráfico denso** |
| Corpus longo, lote mecânico extenso, segundo leitor independente, trabalho que pode esperar | **sessão irmã** Kimi K3 1M ou MiniMax M3.1-Flash-Preview 1M | cota fora da Anthropic; contexto de 1M; herdam o harness (§3.1) |

**Quando NÃO delegar (fato oficial, 24/10/2026).** A matriz diz para onde uma subtarefa desce *quando* se delega; não diz *se* se delega. A Anthropic mediu, com workers Sonnet 5 (uma geração anterior), que *"Delegation saved money in only two situations"*: trabalho rotineiro com cauda longa de custo e trabalho maior que uma janela de contexto. Fora deles, *"When the work is one dependent chain, or fits in a single context without a long cost tail, the orchestrator's model alone at lower effort came out ahead in every case measured"*. A mesma página manda ajustar um modelo antes de somar um segundo (*"Tune one model before you add a second: sweep effort on your current model first"*) e, sobre o Haiku 5.5 como worker, medir no próprio trabalho (*"before you use it as a worker, measure it on your own workload"*); a medição publicada não cobre o Haiku 5.5 como worker (https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence). No Claude Code, a doc manda ficar na conversa principal quando a tarefa pede vaivém frequente (*"the task needs frequent back-and-forth"*) ou quando a latência importa (*"A subagent that isn't a fork starts fresh and may need time to gather context"*) (https://code.claude.com/docs/en/sub-agents). A ordem das alavancas fica: código determinístico, effort do orquestrador, delegar só o que paga, modelo menor. A regra de bolso acima vale **depois** de decidir delegar.

**A matriz vale só para sessões Anthropic** (decisão de Gabriel, 08/10/2026). Dentro de uma
sessão irmã (backend Kimi ou MiniMax), ela e seus subagentes ficam no **próprio modelo**: não
roteiam para opus/sonnet/haiku/fable nem delegam à franquia Anthropic. É também o que as
funções do `~/.bashrc` impõem, porque apontam `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL` e
`CLAUDE_CODE_SUBAGENT_MODEL` para o modelo da sessão; um `model: haiku` ali resolve para o
próprio Kimi/MiniMax. Não "corrija" isso para o mapeamento sugerido pela doc da Moonshot (haiku
→ `kimi-k2.7-code`). A sessão sabe que é irmã pelo `ANTHROPIC_BASE_URL` definido ou pela
marca `(esta sessão)` no `sessoes_irmas.py`.

**Aplicada por código: o gate `agent_model_gate` (08/10/2026).** Prosa não aplicava a matriz
(§7, aderência). O hook global `client/hooks/agent_model_gate.py` (PreToolUse, matcher `Agent`,
symlink em `~/.claude/hooks/`) **recusa a chamada `Agent` sem `model` numa sessão Anthropic** e
devolve a matriz acima com os valores reais da chamada (subagent_type, description, modelo da
sessão lido do transcript); reemiti-la com `model` custa uma linha, e `model: "opus"` é herdar
de propósito. Passa sem `model` quando a decisão já foi tomada em outro lugar, na ordem em que o
predicado testa:
- `AGENT_MODEL_GATE_DISABLED=1` no env da sessão — kill switch, **só humano**;
- sessão irmã Kimi/MiniMax, pelo mesmo `provider_of` do `sessoes_irmas.py` sobre o
  `ANTHROPIC_BASE_URL` que o hook herda (um teste exige que os dois predicados casem); só os
  provedores nomeados isentam — um gateway desconhecido pode estar na frente do Opus e é julgado;
- `fork`, que roda no modelo da sessão por definição (nenhuma regra de ajuste o alcança);
- `model:` declarado no frontmatter da definição (julgado antes pelo ajuste abaixo, nos tipos de busca e de implementação), **inclusive `inherit` escrito** (o autor decidiu
  herdar: 5 agentes do plugin `claude-security` fazem isso); `model` ausente não é decisão. A
  definição é procurada como o Claude Code a resolve [C/sub-agents]: projeto, subindo do cwd até a
  raiz do repositório (a mais próxima vence) > `~/.claude/agents` > plugin, pelo `installPath` do
  `installed_plugins.json` e por `plugins/synced` (nunca o cache por data, que guarda versões velhas);
- built-in com default próprio: `claude-code-guide` (haiku) e `statusline-setup` (sonnet).
  `general-purpose`, `Explore` e `Plan` herdam o modelo da sessão [C/sub-agents] e são julgados;
- `CLAUDE_CODE_SUBAGENT_MODEL` definido (as funções das irmãs o exportam).

A recusa também cita a cota gravada pela status line (§7), porque o momento de escolher o modelo é
o momento em que a cota importa; sem leitura gravada, ela não afirma nada sobre a cota.
Erro interno do gate libera a chamada (fail-open), mas cada chamada `Agent` vira uma linha em
`~/.claude/touring/agent_model_gate.jsonl` com o motivo (`why`), sem prompt nem description — a
série que mede a aderência daqui em diante e o insumo de rótulos da Laya (§7). Registro:
`client/claude/hooks.settings.json` → `scripts/install_claude_hooks.py --apply`. Testes:
`client/hooks/tests/test_agent_model_gate.py` (190; o guarda D8 exige que a lista acima espelhe
`EXEMPTIONS` um a um, na ordem — confere a palavra-chave, não o sentido: mudou a semântica de uma
isenção, reescreva o item à mão). Controle positivo ao vivo em 08/10: a chamada sem `model` voltou
recusada com a matriz; a com `model: sonnet` passou. Uma revisão independente (subagente
`sonnet`) achou 2 vereditos errados na 1ª versão — plugin fora do cache e agente de projeto em
diretório-pai — e 3 perdas de veredito por encoding; todos viraram teste antes da correção.

**Ajuste modelo × tarefa e consentimento do Fable (17/10/2026).** Presença não é adequação: um
`model: "opus"` explícito num `Explore` passava como "decisão tomada". Depois das isenções de sessão
(kill switch, irmã) e do `fork`, o gate julga o `model` que a **chamada nomeia** — o parâmetro vence a
definição e o env na precedência do Claude Code [C/sub-agents], então nenhum dos dois o protege. Omitir
`model` também não escapa: o modelo que a **definição** fixa no frontmatter é o que de fato rodaria, e é
julgado pela mesma regra (a razão ensina a passar `model: "haiku"`, que vence a definição); `inherit` e
definição sem `model` não têm regra de ajuste, o default de built-in (`statusline-setup` = sonnet) não é
frontmatter e segue passando, e o env `CLAUDE_CODE_SUBAGENT_MODEL` é decisão tomada em outro lugar e não
é julgado. Determinístico pelo `subagent_type`, com o `model` normalizado em minúsculas, nesta ordem:
- busca (`Explore`, `touring-scouter`, `claude-code-guide`, `statusline-setup`) com sonnet, opus ou
  fable → recusa `fit:busca-exige-haiku`;
- fable em qualquer tipo → recusa `fable:sem-consentimento`, salvo consentimento vigente
  (`fable:consentido`);
- implementação (`touring-engineer`, `general-purpose`, `touring-architect`, `touring-auditor`,
  `touring-scriber`, `Plan`) com opus → recusa `fit:opus-fica-na-sessao-principal`;
- qualquer outro tipo: só a regra de presença e a do Fable.

A razão da recusa ENSINA o caminho de volta (gate mudo degrada o retry): busca → `model: "haiku"`, ou
a síntese na sessão principal, ou `touring-engineer`/`general-purpose` com `sonnet`; opus → fica na sessão
principal, implementação com spec vai a `sonnet`, busca a `haiku`; fable → pedir ao Gabriel.

O consentimento do Fable é decisão humana com prazo: `~/.claude/touring/fable_consent.json` =
`{"valido_ate": "<ISO-8601 com fuso>", "criado_em", "motivo"}`. Quem liga é o Gabriel, no prompt:
`! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h [--motivo "..."]` (revoga com
`--revogar-fable`). O teto de 24 h é do **leitor**, não só do escritor: arquivo ausente, expirado,
ilegível, sem fuso ou a mais de 24 h de agora = sem consentimento (o `consent` fica no log, ao lado do
`why`). Um consentimento vigente não faz do fable um bom `Explore` (a regra de busca vem primeiro) nem
libera opus. O Bash do próprio agente também poderia rodar o comando de consentimento (o `!` do usuário
não passa por hook PreToolUse, mas o Bash do modelo passa), então `client/hooks/fable_consent_guard.sh`
(PreToolUse em `Bash` e em `Write|Edit|MultiEdit|NotebookEdit`, shell puro, ~3 ms) **nega ao modelo**:
em `Bash`, comando com `--consentir-fable` ou `fable_consent.json`; nas ferramentas de arquivo, caminho com
`fable_consent.json`. Revogar (`--revogar-fable`) é seguro e passa; o **conteúdo** de um Write/Edit não é
julgado, e casa-se o nome exato `fable_consent.json`, não o radical `fable_consent` (que também está no nome
do próprio guard, do teste e do hook: um guard que tranca a própria manutenção acaba desligado).
**Limitação conhecida (residual):** o guard é textual — um comando que monta o caminho ou a flag em tempo de
execução (`"fable_""consent.json"`, `$(printf ...)`) ou chega a um shell por uma ferramenta MCP de execução
não é visto. O fechamento de substrato segue sendo o `sandbox.filesystem.denyWrite` do Claude Code nesse
caminho, não implementado; e que o `!` do TUI tenha `/dev/tty` (outra via de endurecimento) segue por verificar.

**Indução de delegação (17/10/2026).** O gate fiscaliza a escolha quando se delega; nada induzia
delegar (haiku = 0,2% dos tokens Anthropic desde 08/10 pela régua oficial, deduplicada por `message.id`,
`scripts/model_routing_adherence.py`; o Opus principal lia e buscava sozinho). `client/hooks/delegation_nudge.py` só **aconselha** — `additionalContext`, nunca
`permissionDecision`. Em `PreToolUse` de `Read|Grep|Glob`, janela deslizante de 10 min por sessão
(Read = tamanho do arquivo, limitado por `limit` ou pelo teto padrão de 2000 linhas, a 80 B/linha;
Grep/Glob = 1 chamada): com ≥ 6 chamadas ou 150 KB na janela devolve 1 aviso (no máximo 1 a cada 10 min
por sessão) com os 3 alvos mais recentes e o `Agent(subagent_type="Explore", model="haiku", ...)`
derivado deles. Em `UserPromptSubmit`, lê só a cauda (~200 linhas) do transcript: mais de uma família
Anthropic nos turnos da sessão principal avisa que a troca de modelo invalida o cache (§2) e que o
effort sobe antes (regra 17) — 1 aviso por troca, e troca com mais de 30 min não é notícia. Só a sessão
principal de backend Anthropic: pulam o kill switch `DELEGATION_NUDGE_DISABLED=1`, a sessão irmã (o
mesmo `provider_of`) e o subagente (`agent_id` do JSON de entrada [C/hooks]; de reserva,
`/subagents/` no `transcript_path`). Estado em `~/.claude/touring/delegation_nudge/<sessão>.json`, um
log por aviso em `delegation_nudge.jsonl`. Registro pendente de deploy (matchers abaixo em §7).
Testes: `client/hooks/tests/test_delegation_nudge.py` (66; 27 mutantes mortos). O guard do consentimento tem `client/hooks/tests/test_fable_consent_guard.py` (61; 17 mutantes do script e 5 do registro no template, todos mortos); o ajuste por definição, 8 mutantes mortos.

**Kimi ou MiniMax?** O Kimi K3 serve a engenharia longa e autônoma, com raciocínio por corpus
grande (*"sustain long engineering sessions"* [K/blog]; thinking em `max` por default). O
MiniMax M3.1-Flash-Preview serve lotes rápidos e numerosos, entrada multimodal (texto, imagem,
vídeo) e leitura em volume (*"approximately 150 tokens per second"*; *"frequent coding,
financial research, multimodal understanding and frontend work"* [M/agent]). É preview, sem
SLA e com rate limit dinâmico no pico, então não confie a ele o caminho crítico com prazo.

## 2. Modelos Anthropic (verificados em 08/10/2026)

Os quatro têm contexto de 1M (sem variante `[1m]`, em todos os planos), saída de 128K,
thinking adaptativo e corte de conhecimento em jun/2026 [P/models; C/model-config].

**Desligar o thinking depende da superfície (verificado em 24/10/2026).** No Claude Code não desliga em nenhum dos quatro: *"You can't turn thinking off on Opus 5.5, Sonnet 5.5, Haiku 5.5, or the Fable models"*, e `alwaysThinkingEnabled: false` ou `MAX_THINKING_TOKENS=0` salvos não têm efeito ali (https://code.claude.com/docs/en/model-config). Na API a resposta é por modelo: o Opus 5.5 mantém o thinking sempre ligado; o Haiku 5.5 aceita `thinking: {"type":"disabled"}` em `low` a `high` (em `xhigh` e `max` dá 400); o Sonnet 5.5 recusa `disabled` (400) e o modo mais baixo é `between_tools` (https://platform.claude.com/docs/en/build-with-claude/effort e https://platform.claude.com/docs/en/models/sonnet-5-5/whats-new-sonnet-5-5). Para o Fable 5.1 na API a verificação não cobriu.

| Modelo · id | Posicionamento oficial | US$/MTok in · out (cache read) | Effort default no CC | Latência | Lançamento |
|---|---|---|---|---|---|
| Fable 5.1 · `claude-fable-5-1` | *"demanding reasoning and long-horizon agentic work"* | 10 · 50 (0,25) | high | Slower | 01/09/2026 |
| Opus 5.5 · `claude-opus-5-5` | *"long-running agentic coding and knowledge work"* | 4 · 20 (0,20) | medium | Moderate | 22/09/2026 |
| Sonnet 5.5 · `claude-sonnet-5-5` | *"The best combination of speed and intelligence"* | 2 · 10 (0,10) | medium | Fast | 28/09/2026 |
| Haiku 5.5 · `claude-haiku-5-5` | *"high-volume, latency-sensitive tasks such as classification, extraction, and routing"* | 0,10 · 0,50 até 100K de prompt; 0,50 · 2,50 acima | medium | Fastest | 07/10/2026 |

**Fatos datados dos modelos 5.5 — verificados em 23/10/2026.** Leitura direta das páginas oficiais da Anthropic (platform.claude.com e anthropic.com). Registro da verificação: `/home/gabrielgadea/projects/analise/docs/plans/2026-10-23-vitrine-elite/diagnostics/modelos_5_5_fonte_oficial.md`. Onde a fonte não traz o número, a linha diz "não consta na fonte verificada". Preços em US$ por MTok.

- **Haiku 5.5** · `claude-haiku-5-5` · lançado em 07/10/2026
  - Janela de contexto: 1M tokens. Saída máxima: 128K tokens (300K no Message Batches API, com o beta `output-300k-2026-03-24`). Fonte: https://platform.claude.com/docs/en/models/haiku-5-5/overview
  - Preço de entrada: US$ 0,10 para prompts até 100k tokens; US$ 0,50 acima. Preço de saída: US$ 0,50 até 100k tokens; US$ 2,50 acima. Fonte: https://platform.claude.com/docs/en/about-claude/pricing
  - Cache leitura: US$ 0,01 até 100k tokens (US$ 0,05 acima). Cache escrita de 5 min: US$ 0,125 até 100k (US$ 0,625 acima). Cache escrita de 1 h: US$ 0,20 até 100k (US$ 1 acima). Fontes: https://platform.claude.com/docs/en/about-claude/pricing e https://platform.claude.com/docs/en/models/haiku-5-5/overview
  - Thinking: adaptive. Níveis de esforço: `low`, `medium`, `high`, `xhigh`, `max`; default `medium`. Fonte: https://platform.claude.com/docs/en/build-with-claude/effort
  - Batch: desconto de 50%. Entrada US$ 0,05 e saída US$ 0,25 até 100k tokens de prompt; entrada US$ 0,25 e saída US$ 1,25 acima. Fonte: https://platform.claude.com/docs/en/about-claude/pricing (verificado em 24/10/2026).
  - Notas da mesma página: tokenizador novo, em que o mesmo texto conta cerca de 30% mais tokens que no Haiku 4.5; cache mínimo de 512 tokens; retirada não antes de 07/10/2027.
- **Sonnet 5.5** · `claude-sonnet-5-5` · lançado em 28/09/2026
  - Janela de contexto: 1M tokens. Saída máxima: 128K tokens (300K no Batch API, beta). Fonte: https://platform.claude.com/docs/en/models/sonnet-5-5/overview
  - Preço: entrada US$ 2 / MTok; saída US$ 10 / MTok. Fontes: https://platform.claude.com/docs/en/models/sonnet-5-5/overview e https://platform.claude.com/docs/en/about-claude/pricing
  - Cache leitura: US$ 0,10 / MTok. Cache escrita de 5 min: US$ 2,50 / MTok. Cache escrita de 1 h: US$ 4 / MTok. Fontes: https://platform.claude.com/docs/en/models/sonnet-5-5/overview e https://platform.claude.com/docs/en/about-claude/pricing
  - Thinking: adaptive (o modo mais baixo é `between_tools`, que desliga o pensamento antecipado). Níveis de esforço: `low`, `medium`, `high`, `xhigh`, `max`; default `high` na API (o Claude Code usa `medium`, ver §7). Fonte: https://platform.claude.com/docs/en/build-with-claude/effort
  - Batch: US$ 1 de entrada / US$ 5 de saída. Fonte: https://platform.claude.com/docs/en/about-claude/pricing
- **Opus 5.5** · `claude-opus-5-5` · lançado em 22/09/2026
  - Janela de contexto: 1M tokens. Saída máxima: 128K tokens (300K no Batch API, beta). Fonte: https://platform.claude.com/docs/en/models/opus-5-5/overview
  - Preço: entrada US$ 4 / MTok; saída US$ 20 / MTok. Fontes: https://platform.claude.com/docs/en/models/opus-5-5/overview e https://platform.claude.com/docs/en/about-claude/pricing
  - Cache leitura: US$ 0,20 / MTok. Cache escrita de 5 min: US$ 5 / MTok. Cache escrita de 1 h: US$ 8 / MTok. Fontes: https://platform.claude.com/docs/en/models/opus-5-5/overview e https://platform.claude.com/docs/en/about-claude/pricing
  - Thinking: adaptive, sempre ligado (não pode ser desligado). Níveis de esforço: `low`, `medium`, `high`, `xhigh`, `max`; default `medium`. Fonte: https://platform.claude.com/docs/en/build-with-claude/effort
  - Batch: US$ 2 de entrada / US$ 10 de saída. Fonte: https://platform.claude.com/docs/en/about-claude/pricing

**Divergências do NotebookLM corrigidas pela fonte oficial (23/10/2026).** O caderno do NotebookLM é síntese, não fonte primária; quando diverge, a fonte oficial vence (§6).

1. **Cache de escrita do Haiku 5.5:** o caderno disse US$ 0,12. O oficial é US$ 0,125 (escrita de 5 min) e US$ 0,20 (escrita de 1 h). Fonte: https://platform.claude.com/docs/en/about-claude/pricing
2. **Terminologia de thinking:** o caderno disse "extended thinking". O Haiku 5.5 usa adaptive thinking, ligado por padrão e controlado por `effort`. O modo manual com `budget_tokens` (o "extended") é de modelos anteriores. Fonte: https://platform.claude.com/docs/en/models/haiku-5-5/overview
3. **TerminalBench:** o caderno disse 39-40%. O oficial é 39,2% (Terminal-Bench 4.0, pass@1; o esforço usado não consta). Fonte: https://www.anthropic.com/claude-haiku-5-5
4. **"72,4% em Computer Use no esforço Max":** o 72,4% aparece na tabela do anúncio como OSWorld 2.1 (subconjunto "Offline subset"), e a página não o vincula ao esforço Max. Essa afirmação não deve sair como está. Fonte: https://www.anthropic.com/claude-haiku-5-5

**Nuance do "75% mais barato":** é verdade como custo médio de execução por tarefa, segundo o anúncio ("On average, it now costs around 75% less to run"). Não é desconto de tabela: pela nota de rodapé do anúncio, o preço cai 90% para requisições até 100k tokens e 50% acima disso, e o cálculo inclui o tokenizador novo. Não escrever "75% de desconto no preço". Fonte: https://www.anthropic.com/claude-haiku-5-5

**Mecânica no Claude Code** [C/model-config; C/sub-agents]:
- Aliases: `default` · `best` (o Fable onde houver, senão o Opus) · `fable` · `opus` · `sonnet` · `haiku` · `opusplan` (opus no plan mode, sonnet na execução).
- **Versões mínimas do Claude Code (verificado em 24/10/2026).** Sonnet 5.5 exige a v2.1.284 ou posterior (*"Sonnet 5.5 requires Claude Code v2.1.284 or later"*) e o Haiku 5.5, a v2.1.293 ou posterior (https://code.claude.com/docs/en/model-config). O modo fork (subagente que herda a conversa e compartilha o cache do pai) vem ligado por padrão a partir da v2.1.302, e num terminal interativo desde a v2.1.232 (https://code.claude.com/docs/en/sub-agents). Máquina atual: `claude --version` = 2.1.310 em 24/10/2026, acima de todos esses mínimos.
- **Os aliases resolvem para versões antigas fora da API da Anthropic (verificado em 24/10/2026).** `sonnet` e `haiku` valem Sonnet 5.5 e Haiku 5.5 na API da Anthropic; no Claude Platform on AWS, Sonnet 4.6 e Haiku 4.5; no Amazon Bedrock e no Google Cloud Agent Platform, Sonnet 4.5 e Haiku 4.5; no Microsoft Foundry, Sonnet 4.5 e Haiku 4.5. A página diz que o alias resolve *"to the newest version on the Anthropic API and to an earlier version on some other providers"* (https://code.claude.com/docs/en/model-config). Só importa se o projeto sair da API da Anthropic.
- Modelo do subagente, em ordem de precedência: o parâmetro `model` da invocação, depois o `model:` do frontmatter (`sonnet|opus|haiku|fable|<id>|inherit`), depois `CLAUDE_CODE_SUBAGENT_MODEL`, e por fim o modelo da sessão. `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` (v2.1.257 ou posterior) força um único modelo em todos os subagentes, teammates e agentes de workflow; enquanto ligado, ignora o `model` das definições e o Claude não consegue passar `model` ao iniciar um subagente. **Sem `_FORCE`, a variável sozinha não altera o `Explore` nem o `Plan`**, e o `general-purpose` só a usa quando nada mais atribui modelo (https://code.claude.com/docs/en/sub-agents, verificado em 24/10/2026). Tabela do modelo efetivo logo após esta lista.
- Effort: `low` · `medium` · `high` · `xhigh` · `max`.
  - `medium` serve para *"day-to-day engineering work with a clear scope"*.
  - `high` serve para *"fixing a bug in an existing codebase"*.
  - `max` *"is prone to overthinking"*.
  - No Sonnet 5.5, `max` também regride na medição do anúncio: FrontierCode 1.1 Main de 46,2% em Max contra 52,1% em Xhigh. A nota 2 atribui a queda a rodadas de code-review que dividem a revisão entre muitos subagentes, com timeout e edição fora de escopo em dois casos (https://www.anthropic.com/claude-sonnet-5-5; paráfrase, o trecho exato não foi conferido no HTML). É argumento contra fixar `effort: max` em workers Sonnet. O guia do Sonnet 5.5 mediu que um prompt de amortecimento, em `max`, cortou cerca de um terço do custo da sessão (https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-sonnet-5-5).
  - O frontmatter de subagente aceita `effort:`.
- **Cada modelo tem o próprio cache**: trocar de modelo com `/model` no meio da sessão relê tudo sem cache. Quando precisar de outro modelo, prefira um subagente a trocar o modelo da sessão principal.
  - O par (verificado em 24/10/2026): trocar o **effort** no meio da sessão **mantém** o cache no Opus 5.5, no Sonnet 5.5, no Haiku 5.5 e no Fable 5.1, com chave de API ou assinatura. Não vale no Amazon Bedrock, no Google Cloud Agent Platform, no gateway de apps Claude, com `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS` nem com configuração HIPAA (https://code.claude.com/docs/en/prompt-caching, seção "Changing effort level"). Por isso effort é a alavanca barata e `/model` não.
- Ressalvas do Fable: retenção de 30 dias (fora de zero data retention) e *"Parallel tool calling is more variable"* [P/whats-new-fable-5-1].
- **Chamadores de API (Agent SDK, scripts): `tool_choice` forçado (verificado em 24/10/2026).** `tool_choice` `any`/`tool` devolve 400 no Fable 5.1 [P/whats-new-fable-5-1], no Sonnet 5.5 (https://platform.claude.com/docs/en/models/sonnet-5-5/whats-new-sonnet-5-5) e no Opus 5.5 (https://platform.claude.com/docs/en/about-claude/models/choosing-a-model). O Haiku 5.5 aceita, com 406 tokens de overhead de prompt contra 286 em `auto`/`none` (https://platform.claude.com/docs/en/about-claude/pricing). Entre os quatro, só o Haiku 5.5 aceita.
- Custo de orquestração (todas as frases de [C/costs], salvo indicação):
  - Agent teams: *"approximately 7x more tokens than standard sessions when teammates run in plan mode"*.
  - Workflows: o aviso `Large workflow` aparece acima de 25 agentes ou 1,5M tokens [C/workflows].
  - CLAUDE.md: *"Aim to keep CLAUDE.md under 200 lines"*.

**Modelo efetivo do subagente quando a chamada omite `model` (verificado em 24/10/2026; https://code.claude.com/docs/en/sub-agents):**

| Tipo | Modelo efetivo |
|---|---|
| `Explore` | o da sessão; numa sessão Fable, com assinatura, conta Console ou gateway LLM por `ANTHROPIC_BASE_URL`, roda no Opus que o alias `opus` resolve (em Bedrock, Agent Platform, Foundry, Claude Platform on AWS e gateway de apps Claude, fica no da sessão). `CLAUDE_CODE_SUBAGENT_MODEL` sozinho **não** o altera; só o par com `_FORCE=1` |
| `Plan` | herda a sessão, salvo `CLAUDE_CODE_SUBAGENT_MODEL` com `_FORCE=1` |
| `general-purpose` | `CLAUDE_CODE_SUBAGENT_MODEL`, se definido e nada mais atribuir modelo; senão o da sessão |
| fork | sempre o da sessão (não aceita modelo mais barato); compartilha o cache do pai |
| `statusline-setup` · `claude-code-guide` | Sonnet · Haiku (internos) |

## 3. Sessões irmãs

### 3.1 O que são e como nascem

São **sessões Claude Code** com o backend trocado pelas funções do `~/.bashrc`:

| Função | `ANTHROPIC_BASE_URL` | Modelo | Janela |
|---|---|---|---|
| `kimi-cc` | `https://api.kimi.com/coding/` | `k3` | 262k |
| `kimi1m` | `https://api.kimi.com/coding/` | `k3[1m]` | 1M |
| `minimax` | `https://api.minimax.io/anthropic` | `MiniMax-M3.1-Flash-Preview` | 1M |

`claude_reset` restaura a Anthropic.

Por serem Claude Code, **herdam deste diretório** o CLAUDE.md, os hooks (`block_git`, o guard
de processos touring e os gates de qualidade), as skills e as permissões, e aparecem no
`ListAgents`. Isso **não** vale para o binário `kimi-code` (`~/.kimi-code/bin/kimi` 0.42.0):
ele é outro harness e não herda nada (memória `reference-delegar-ao-kimi-cli`).

**Quem é quem se decide por código, nunca pelo nome.** O `ListAgents` mostra o nome da sessão,
nunca o backend:

```bash
python3 ~/.claude/tools/sessoes_irmas.py --text     # nome → provedor → modelo → status (--all: todos os diretórios)
```

O helper cruza `~/.claude/sessions/<pid>.json` com `/proc/<pid>/environ` e lê só chaves
whitelistadas, então segredo não sai. Testes: `~/.claude/tools/test_sessoes_irmas.py` (4/4). O
controle positivo ao vivo de 08/10 bateu com a verdade medida: `analise-a0` é o kimi
`k3[1m]`, `analise-a9` é o minimax e `analise-6b` é o anthropic.

### 3.2 Kimi K3 (fonte: Moonshot, lida em 08/10/2026)

- **Modelo.** Id de API `kimi-k3`, com contexto de 1.048.576 tokens. A saída tem default de 131.072 [K/quickstart].
- **Thinking e sampling.** O thinking fica sempre ligado; `reasoning_effort` aceita `low`, `high` ou `max`, com default `max`. A temperatura é fixa em 1.0 [K/quickstart]. As funções locais desligam o thinking adaptativo porque o endpoint o rejeita.
- **Preço da API.** US$ 3 de entrada, 15 de saída e 0,30 de cache por MTok [K/pricing]. Aqui o uso é pela **assinatura** (endpoint `coding/`), cobrado por cota, não por token.
- **Cota.** A janela é de 5 h. Os planos legados (Andante, Moderato, **Allegretto**, Allegro) somam **um limite de 7 dias**; o `kimi1m` local se anuncia "Allegretto+". O help center estima *"about 300–1,200 requests every 5 hours"* [K/membership]. Já medimos um 403 semanal no meio de um trabalho (19/09/2026).
- **Saldo.** Pelo `/usage` dentro da sessão Kimi, ou em `kimi.com/code/console`.
- **Pontos fortes e limites declarados.** Engenharia longa (*"strong long-horizon coding performance"*). O próprio anúncio diz que ele *"trails"* o Claude Fable 5 [K/blog].

### 3.3 MiniMax M3.1-Flash-Preview (fonte: MiniMax, lida em 08/10/2026)

- **Modelo.** Id `MiniMax-M3.1-Flash-Preview`, com contexto de 1M. O máximo de saída não é publicado [M/anthropic-api].
- **Disponibilidade.** Só pelo M Plan ou MiniMax Code. Não há preço público por token.
- **Desempenho e arquitetura.** Cerca de 150 tokens/s. É um MoE de 428B parâmetros com 23B ativos. A entrada aceita texto, imagem e vídeo [M/agent].
- **Thinking.** O thinking **não desliga**, e o effort tem default `max` [M/text-generation]. Para lotes simples, lance com `CLAUDE_CODE_EFFORT_LEVEL=medium minimax`: o default `max` gasta a cota mais rápido.
- **Cota do M Plan.** Janelas de 5 h e semanal, independentes, que começam no 1º uso. *"Usage from all tools counts toward the same M Plan usage limits"* [M/usage-rules]. O saldo fica em `platform.minimax.io/console/usage`.
- **Ressalvas de Preview.** Sem promessa de uso gratuito, preço não fixado [M/agent], e *"may implement dynamic rate limiting"* no pico [M/faq].

## 4. Protocolo de delegação a uma sessão irmã

1. **Medir.** Rode `sessoes_irmas.py --text`. A irmã está `idle`? É do provedor certo para a classe (§1)?
2. **Brief autocontido**, num único `SendMessage`. Sempre em texto: um `@arquivo` não anexa nada do outro lado. O brief traz:
   - a 1ª linha como assunto completo;
   - o objetivo e os caminhos de entrada (a irmã lê do disco, não cole o conteúdo);
   - os **arquivos de escrita exclusivos**, nomeados;
   - as proibições (sem commit/push/stash, nada fora da lista);
   - o **critério de aceite verificável**, com comando e exit code esperado, escrito **antes** de ver a entrega;
   - o formato do relatório final, curto e único.
3. **Não conversar.** Use `notify_when_idle: true` em vez de perguntar "terminou?". Cada mensagem entregue a uma sessão ociosa vira um turno que reenvia o contexto inteiro *dela*, e a resposta reenvia o *seu* [C/costs, "Cross-session messages"]. O ideal é um brief e um relatório.
4. **Aceite por código.** A entrega é hipótese até o **seu** juiz aprová-la (teste, gate, exit code). A narrativa do delegado não é evidência: no T007 o MiniMax se autocorrigiu em três pontos, e uma 1ª leitura teria aceitado o errado.
5. **Sem permission laundering.** Nunca peça a uma irmã o que foi negado ou bloqueado nesta sessão.
6. **Plano B de cota.** Se a irmã bater 403 ou o limite de 5 h no meio, termine você mesmo a partir dos artefatos que ela deixou em disco.

## 5. Higiene de contexto do orquestrador

- **Leitura pesada fica fora deste contexto.** Mande para um subagente (*"the verbose output stays in the subagent's context"* [C/costs]) ou para o `touring run` (agregado-não-dump, D5 da `touring-4-pillars`). Aqui entra só o agregado.
- **Delegar só o que paga.** Nas medições da Anthropic, delegar só economizou em dois casos (trabalho rotineiro com cauda longa de custo e trabalho maior que uma janela); em cadeia dependente, ou tarefa que cabe numa janela sem cauda longa, o orquestrador sozinho em effort menor venceu em todos os casos medidos (§1, "Quando NÃO delegar"; https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence, 24/10/2026). Inferência: manter a saída verbosa fora do contexto (bullet acima) é um critério de contexto, distinto do custo medido ali; pese os dois.
- **`/clear` entre tarefas não relacionadas.** Custa zero; rode `/rename` antes. Use `/compact <foco>` só quando a continuidade importa, porque ele é uma requisição grande.
- **Effort antes de modelo.** E nunca `/model` no meio da sessão (§2, cache por modelo).
- **Custos invisíveis.** `/loop`, tarefas agendadas, goal check-ins e mensagens entre sessões reenviam o contexto inteiro a cada disparo [C/costs, "Why usage climbs"].
- **Início frio do subagente (verificado em 24/10/2026).** Todo subagente que não é `Explore` nem `Plan` começa com contexto novo e recarrega toda a hierarquia de CLAUDE.md da sessão (`~/.claude/CLAUDE.md`, regras do projeto, `CLAUDE.local.md`, arquivos de política gerenciada, `AGENTS.md`), mais o snapshot de git status; não vê o histórico, as skills já invocadas nem os arquivos já lidos, e as skills só entram pelo campo `skills`. `Explore` e `Plan` pulam o CLAUDE.md e o git status, mas o CLAUDE.md de subdiretório e as regras por caminho ainda carregam quando eles trabalham com arquivos de lá. Num subagente customizado, `omitClaudeMd: true` (v2.1.271 ou posterior) deixa só a política gerenciada, indicado para quem tira tudo do prompt de delegação; a regra que precise chegar ao subagente deve ser repetida no prompt (https://code.claude.com/docs/en/sub-agents, "What loads at startup"). Subagentes compactam sozinhos quando a própria janela enche: `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` vale para eles, e o frontmatter aceita `autoCompactWindow` (inteiro de 100000 a 1000000; `200k` é lido como 200; v2.1.296 ou posterior; sem efeito se `CLAUDE_CODE_AUTO_COMPACT_WINDOW` estiver definida). **Medição local (projeto `analise`, 24/10/2026)**: a hierarquia soma ≈ 115 KB de arquivos (≈ 29 a 33 mil tokens, estimativa a 3,5–4 bytes por token, sem o tokenizador); é teto, porque não se verificou quais das 17 regras globais um subagente carrega de fato. O início frio de 30 mil tokens custa, a preço de escrita de cache de 5 min, ≈ US$ 0,004 no Haiku, US$ 0,075 no Sonnet e US$ 0,15 no Opus (cálculo sobre a tabela de preços). Detalhe: `/home/gabrielgadea/projects/analise/docs/plans/2026-10-23-vitrine-elite/diagnostics/modelos_haiku_sonnet_5_5.md` §5.2.
- **Medição** (o Gabriel vê; o modelo não): `/usage` mostra as barras do plano, a atribuição por subagente, skill e MCP, e as flags de long context e cache miss; `/context` mostra o que ocupa a janela.

## 6. Protocolo de atualização de modelos

**Gatilhos.** Qualquer um destes dispara a atualização:
- um id de modelo que não está em §2 ou §3, visto no env, no schema da ferramenta Agent, no `/model` ou num anúncio;
- uma mudança comunicada de versão, preço ou limite;
- um erro de modelo desconhecido;
- uma divergência entre este arquivo e a fonte;
- um id de modelo **fixo** no frontmatter de agente (`model: claude-…`), em `~/.claude/agents/*.md` e em `.claude/agents/*.md` dos projetos. Ele envelhece calado. Em 08/10 os cinco `touring-*` ainda fixavam `claude-sonnet-4-6`, uma geração atrás. Prefira o alias (`sonnet`, `haiku`, `opus`), que acompanha a versão. Trocados por `sonnet` no mesmo dia: os 5 `touring-*` (vivo + espelho `client/agents` + `MANIFEST.sha256`) e os 3 `aco-*` do analise.

**Fontes oficiais.** Use só estas:

| Provedor | Fontes |
|---|---|
| Anthropic | `platform.claude.com/docs/en/about-claude/models/overview` · `…/models/choosing-a-model` · `…/about-claude/pricing` · `code.claude.com/docs/en/{model-config,sub-agents,costs}` · `support.claude.com` (planos, Fable) · `anthropic.com/news` |
| Moonshot / Kimi | `platform.kimi.ai/docs` (`pricing/chat`, `guide/kimi-k3-quickstart`, `guide/claude-code-kimi`) · `kimi.com/code/docs` · `moonshotai.github.io/kimi-code` · `huggingface.co/moonshotai` |
| MiniMax | `platform.minimax.io/docs` (`guides/text-generation`, `api-reference/text-anthropic-api`, `m-plan/*`) · `minimax.io/blog` · `agent.minimax.io/tools` |
| Boas práticas do Claude Code | Context7 `/websites/code_claude`. O MCP do plugin exige OAuth; sem ele, use a REST `https://context7.com/api/v2/context?libraryId=/websites/code_claude&query=<tema>`. O servidor `context7` que não é do plugin (`mcp__context7__query-docs`) respondeu sem OAuth em 24/10/2026 e devolveu texto literal que corroborou a resolução do modelo do subagente, o par `CLAUDE_CODE_SUBAGENT_MODEL`/`_FORCE` e a seção de custo |

**Procedimento.** É a própria regra aplicada:
1. A pesquisa vai para subagentes. Use `sonnet` para docs que pedem julgamento e `haiku` para recuperação mecânica. Eles devolvem trecho literal mais URL e marcam UNVERIFIED o que não acharem.
2. O orquestrador confere **na fonte** as afirmações que sustentam regra.
3. Atualize §2 e §3 com a data.
4. Ajuste a matriz §1 se o posicionamento mudou.
5. Grave a lição no grafo de memória.

A pesquisa de 08/10 custou 4 subagentes (3 sonnet e 1 haiku) e 3 conferências diretas.

## 7. Lacunas medidas (UNVERIFIED em 08/10/2026) e potencializações propostas

- **Roteamento de modelos — fechado × aberto (17/10/2026).**
  - Fechado, em código e com teste: omissão de `model` (08/10); adequação modelo × tarefa — busca não vai a sonnet/opus/fable, implementação não vai a opus (§1); Fable só com consentimento humano de ≤ 24 h, com o teto no leitor (§1); indução de delegação por rajada de leitura e aviso de troca de modelo (§1, `delegation_nudge`).
  - FECHADO (a), no caminho direto: o agente não concede o próprio consentimento do Fable — `fable_consent_guard.sh` nega ao modelo o `--consentir-fable` e a escrita de `fable_consent.json` (§1). Resíduo declarado: o guard é textual (indireção em tempo de execução e ferramenta MCP de execução não são vistas); o `denyWrite` do sandbox seguiria sendo o fechamento de substrato.
  - FECHADO (b): omitir `model` num tipo de busca ou implementação cuja definição fixe um modelo inadequado (sonnet/opus/fable em busca, opus em implementação) agora é recusado — o modelo do frontmatter é julgado como o da chamada (§1). Hoje o `touring-scouter` vivo é `haiku` e nenhuma definição real dispara a regra (varredura de 192 arquivos de agentes em 17/10). O espelho `client/agents/touring-scouter.md` ainda diz `sonnet`: **a sincronizar no deploy**.
  - Limites declarados, não fechados: (c) o gate vê o **tipo**, não a tarefa: busca disfarçada de `general-purpose` com sonnet passa — só classificador por prompt (probabilístico, candidato à Laya) alcança; (d) o aviso de delegação mede rajada de leitura na sessão principal, não a qualidade do que foi delegado — a eficácia se mede cruzando `delegation_nudge.jsonl` com os transcripts (linha de base: haiku = 0,2% dos tokens Anthropic pela régua deduplicada). O aviso também **não distingue os casos em que delegar custa mais**: a Anthropic mediu que delegar só economizou em dois casos (trabalho rotineiro com cauda longa de custo e trabalho maior que uma janela) e que, em cadeia dependente ou tarefa que cabe numa janela, o orquestrador sozinho em effort menor venceu em todos os casos medidos (§1, "Quando NÃO delegar"; https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence, 24/10/2026). **A conferir, ainda não feito**: o efeito líquido em custo do aviso, cruzando `delegation_nudge.jsonl` com a medição de aderência (`scripts/model_routing_adherence.py`); (e) o aviso de troca de modelo vê a troca **depois** do 1º turno com o modelo novo, nunca antes.
  - Registro (fonte `client/claude/hooks.settings.json`, pendente de `install_claude_hooks.py --apply` e dos symlinks em `~/.claude/hooks/`): `delegation_nudge.py` em `PreToolUse` `Read|Grep|Glob` e em `UserPromptSubmit` (`timeout: 5`; medido em 17/10: p50 24 ms, p95 32 ms em 200 chamadas sintéticas); `fable_consent_guard.sh` em `PreToolUse` `Bash` e `Write|Edit|MultiEdit|NotebookEdit` (`timeout: 2`; medido: p50 3,2 ms, p95 3,6 ms, contra 42,5 ms do `block_git.sh`, que usa `jq`).

- **O modelo agora enxerga a própria franquia (implementado em 08/10, ordem do Gabriel).** O payload da status line traz `rate_limits.five_hour` e `rate_limits.seven_day`, cada um com `used_percentage` (0–100) e `resets_at` (epoch), só para Pro/Max e só depois da 1ª resposta da sessão; cada janela pode faltar sozinha [C/statusline]. O `status_monitor.sh` (versionado em `client/hooks/` desde então; `~/.claude/hooks/` tem o symlink) passa o payload, em background, a `client/hooks/rate_limits_snapshot.py`, que grava `~/.claude/touring/rate_limits.json` com **uma leitura por provedor** (`backend_provider.provider_of`, a mesma fonte do gate: uma irmã nunca sobrescreve a cota da Anthropic). Ausência nunca vira zero: payload sem leitura não cria nem apaga nada. Consultar antes de um fanout: `python3 ~/.claude/hooks/rate_limits_snapshot.py --show` (exit 3 sem leitura; `--json` para código). O `agent_model_gate` cita a mesma linha na recusa. 1ª leitura real: *"5 h 5% (zera 16:30) · 7 d 96% (zera 09/10 03:00)"*. Testes: `client/hooks/tests/test_rate_limits_snapshot.py` (18).
- **A cota agora também está na barra de status (18/10/2026).** O `status_monitor.sh` mostra `5h 60% ↻1h04 · 7d 50%` ao fim da linha, cada janela colorida pela própria faixa (verde < 70, amarelo < 85, vermelho ≥ 85; `QUOTA_WARN_PCT`/`QUOTA_ALERT_PCT` ajustam), o `↻` só na janela de 5 h e só enquanto o reset está no futuro. **Ausência nunca vira zero**: payload sem `rate_limits` (outro backend, versão antiga), janela sem número ou com lixo não desenha nada daquela janela. Aos 85% da janela de 5 h o monitor arma `~/.claude/touring/context_alert/<sessão>.quota` (porcentagem + `resets=<epoch>`, a identidade da janela) e o `check_context.sh` (PostToolUse, o MESMO canal do aviso de contexto) o entrega **uma vez por janela** — junto do aviso de contexto, quando os dois vencem na mesma chamada, em uma só mensagem. Sai quando a leitura cai abaixo do limiar; render sem leitura não desarma. Testes: `client/hooks/tests/test_status_monitor_quota.py`.
- **Kimi.** Não se sabe a unidade da cota (requisição ou token). Os domínios divergem: `api.kimi.com/coding` (o local, que funciona) contra `api.kimi.ai/coding` (página de terceiros-tools). A data exata de lançamento do K3 também falta.
- **MiniMax.** Não se sabe o máximo de saída, o preço nem os benchmarks do M3.1-Flash-Preview. A doc oficial recomenda `CLAUDE_CODE_AUTO_COMPACT_WINDOW=524288`; a função local usa `1000000` (decisão do Gabriel).
- **Anthropic.** A janela exata das mensagens "Opus limit" não foi encontrada. **Fechado em 24/10/2026**: o effort default do Sonnet 5.5 é `high` na API (https://platform.claude.com/docs/en/models/overview) e `medium` no Claude Code, que também usa `medium` para o Opus 5.5 e o Haiku 5.5 (*"Opus 5.5, Sonnet 5.5, and Haiku 5.5 default to `medium`"*; https://code.claude.com/docs/en/model-config). Exceção do Claude Code: um nível padrão definido pela organização vence o do modelo.
- **Aderência medida (08/10/2026, transcripts de 14 dias de analise e touring).** O método foi varrer as chamadas `Agent/Task` e cruzar com o modelo de cada sessão.
  - Das 171 chamadas de subagente feitas em sessões Opus 5.5, **92 (54%) eram `general-purpose` sem `model`** e herdaram o Opus por omissão.
  - Outras 16 eram forks, que não se roteiam. As 10 do Explore **também herdaram o Opus**: as docs oficiais (lidas em 08/10) dizem que o Explore usa o modelo da conversa principal. Somando, **102 de 171 (60%)** herdaram por omissão.
  - Só 43 de 233 chamadas, no total, escolheram `sonnet`/`haiku` explicitamente.
  - O gargalo é a decisão **não tomada**, não o custo de decidir. Um roteador Jev/Laya não resolve isso sozinho; resolve um gate no executor (hook `PreToolUse` em `Agent`) — **instalado em 08/10** (§1, `agent_model_gate`).
  - Script: `aderencia_modelo_subagente.py` no scratchpad da sessão `10fb71c6`. Vale promover a KPI.
- **Jev/Laya como roteador (avaliado em 08/10/2026).**
  - No harness, o Jev zero-shot empatou com a palavra-chave no roteamento por intenção (0,533 × 0,536). A Laya com fine-tuning em 560 exemplos fez 0,608, e chega a acc 0,926 só com conf ≥ 0,6, num suporte de 22,5%. Fonte: `docs/plans/2026-09-30-jev-harness-potencializacao/experiments/{g2,w42-student}`.
  - No analise, o Jev nunca vê dado ANTT, então só a Laya local serve ali.
  - Caminho: gate determinístico agora. Laya treinada com as decisões explícitas + desfecho, quando houver cerca de 1.000 rótulos.
- **Registro de hooks com drift (achado e corrigido em 08/10).** O `~/.claude/settings.json` vivo tinha 3 hooks fora da fonte versionada: `evidence_guard`, `prose_integrity_gate` e `skill_picker_hook` (este sem `timeout`, logo com o default de 600 s). Um `install_claude_hooks.py --apply` ingênuo os teria **apagado**. Reconciliado antes de registrar o gate: fonte regenerada do vivo pelo `dump()` do instalador (+30 −0), `timeout: 5` declarado (medido: 48 ms, o juiz roda em background), `--check` rc 0.
- **Matcher morto (achado em 08/10, aguarda decisão do Gabriel).** A fonte registra `touring-hook cli-suggest` em `matcher: "Task"`, e `crates/touring-cli/src/cli_suggester.rs` só classifica `"Task"` (`classify_task`). A ferramenta de subagente chama-se `Agent` [C/hooks], então essa sugestão não dispara para subagentes: nos transcripts de 21 dias (analise + touring), 277 chamadas vieram como `Agent` e 0 como `Task`. Corrigir é trocar o matcher e o braço do `match` juntos; muda o contexto injetado em toda chamada `Agent`, por isso não foi feito sem ordem.
- **Higiene.** O `~/projects/touring/CLAUDE.md` tem mais de 500 linhas, contra a meta oficial de < 200 e o teto duro de 400 da REGRA #16. É dívida registrada.

## Fontes (siglas)

- **Anthropic.**
  - **P** = `https://platform.claude.com/docs/en` (`/about-claude/models/overview`, `/about-claude/models/choosing-a-model`, `/about-claude/pricing`, `/models/{fable-5-1,opus-5-5,sonnet-5-5,haiku-5-5}/…`).
  - **C** = `https://code.claude.com/docs/en` (`/model-config`, `/sub-agents`, `/costs`, `/errors`, `/statusline`, `/workflows`).
  - **S** = `https://support.claude.com/en/articles/15424964-claude-fable-models-on-your-plan`.
- **Kimi.**
  - **K/quickstart** = `https://platform.kimi.ai/docs/guide/kimi-k3-quickstart`.
  - **K/pricing** = `https://platform.kimi.ai/docs/pricing/chat`.
  - **K/blog** = `https://www.kimi.ai/blog/kimi-k3`.
  - **K/membership** = `https://www.kimi.com/en/help/kimi-code/membership-guide`.
- **MiniMax.**
  - **M/anthropic-api** = `https://platform.minimax.io/docs/api-reference/text-anthropic-api`.
  - **M/text-generation** = `https://platform.minimax.io/docs/guides/text-generation`.
  - **M/agent** = `https://agent.minimax.io/tools/m3-1-flash-preview`.
  - **M/usage-rules** = `https://platform.minimax.io/docs/m-plan/usage-rules.md`.
  - **M/faq** = `https://platform.minimax.io/docs/m-plan/faq.md`.
