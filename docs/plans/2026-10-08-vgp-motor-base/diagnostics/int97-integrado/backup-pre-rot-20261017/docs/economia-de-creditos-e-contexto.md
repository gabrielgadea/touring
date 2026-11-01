---
okf_version: "1.0"
type: doc
title: "Economia de créditos e contexto — divisão operacional (Anthropic × Kimi × MiniMax)"
description: "Corpo canônico da regra que os CLAUDE.md de ~/projects/touring (regra 33) e ~/projects/analise (seção 📌) apontam: os quatro orçamentos, a matriz de roteamento por classe de subtarefa, os fatos datados de cada modelo com fonte oficial, o protocolo de delegação às sessões irmãs e o protocolo de atualização quando surge modelo novo."
plan_id: docs
tags: [rules, custo, contexto, modelos, delegacao, kimi, minimax]
timestamp: 2026-10-08T12:10:00-03:00
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

O orçamento 4 é o que não se vê: 1M de janela não é licença. Leitura que entra no contexto do
orquestrador é paga de novo em todo turno seguinte (a preço de cache read, e cheia depois de
1 h ociosa, quando o cache expira na assinatura).

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
| Buscar, extrair, classificar, rotear, recuperar documentação (Context7), varredura mecânica | **subagente `model: haiku`** (Haiku 5.5) | *"classification, routing, extraction, and subagent tasks"* [P/haiku-5-5] · *"For simple subagent tasks, specify `model: haiku`"* [C/costs] |
| Corpus longo, lote mecânico extenso, segundo leitor independente, trabalho que pode esperar | **sessão irmã** Kimi K3 1M ou MiniMax M3.1-Flash-Preview 1M | cota fora da Anthropic; contexto de 1M; herdam o harness (§3.1) |

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
- `fork`, que roda no modelo da sessão por definição;
- `model:` declarado no frontmatter da definição, **inclusive `inherit` escrito** (o autor decidiu
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
`client/hooks/tests/test_agent_model_gate.py` (43; o guarda D8 exige que a lista acima espelhe
`EXEMPTIONS` um a um, na ordem — confere a palavra-chave, não o sentido: mudou a semântica de uma
isenção, reescreva o item à mão). Controle positivo ao vivo em 08/10: a chamada sem `model` voltou
recusada com a matriz; a com `model: sonnet` passou. Uma revisão independente (subagente
`sonnet`) achou 2 vereditos errados na 1ª versão — plugin fora do cache e agente de projeto em
diretório-pai — e 3 perdas de veredito por encoding; todos viraram teste antes da correção.

**Kimi ou MiniMax?** O Kimi K3 serve a engenharia longa e autônoma, com raciocínio por corpus
grande (*"sustain long engineering sessions"* [K/blog]; thinking em `max` por default). O
MiniMax M3.1-Flash-Preview serve lotes rápidos e numerosos, entrada multimodal (texto, imagem,
vídeo) e leitura em volume (*"approximately 150 tokens per second"*; *"frequent coding,
financial research, multimodal understanding and frontend work"* [M/agent]). É preview, sem
SLA e com rate limit dinâmico no pico, então não confie a ele o caminho crítico com prazo.

## 2. Modelos Anthropic (verificados em 08/10/2026)

Os quatro têm contexto de 1M (sem variante `[1m]`, em todos os planos), saída de 128K,
thinking adaptativo que **não desliga** e corte de conhecimento em jun/2026 [P/models; C/model-config].

| Modelo · id | Posicionamento oficial | US$/MTok in · out (cache read) | Effort default no CC | Latência | Lançamento |
|---|---|---|---|---|---|
| Fable 5.1 · `claude-fable-5-1` | *"demanding reasoning and long-horizon agentic work"* | 10 · 50 (0,25) | high | Slower | 01/09/2026 |
| Opus 5.5 · `claude-opus-5-5` | *"long-running agentic coding and knowledge work"* | 4 · 20 (0,20) | medium | Moderate | 22/09/2026 |
| Sonnet 5.5 · `claude-sonnet-5-5` | *"The best combination of speed and intelligence"* | 2 · 10 (0,10) | medium | Fast | 28/09/2026 |
| Haiku 5.5 · `claude-haiku-5-5` | *"high-volume, latency-sensitive tasks such as classification, extraction, and routing"* | 0,10 · 0,50 até 100K de prompt; 0,50 · 2,50 acima | medium | Fastest | 07/10/2026 |

**Mecânica no Claude Code** [C/model-config; C/sub-agents]:
- Aliases: `default` · `best` (o Fable onde houver, senão o Opus) · `fable` · `opus` · `sonnet` · `haiku` · `opusplan` (opus no plan mode, sonnet na execução).
- Modelo do subagente, em ordem de precedência: o parâmetro `model` da invocação, depois o `model:` do frontmatter (`sonnet|opus|haiku|fable|<id>|inherit`), depois `CLAUDE_CODE_SUBAGENT_MODEL`, e por fim o modelo da sessão. `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` força um único modelo.
- Effort: `low` · `medium` · `high` · `xhigh` · `max`.
  - `medium` serve para *"day-to-day engineering work with a clear scope"*.
  - `high` serve para *"fixing a bug in an existing codebase"*.
  - `max` *"is prone to overthinking"*.
  - O frontmatter de subagente aceita `effort:`.
- **Cada modelo tem o próprio cache**: trocar de modelo com `/model` no meio da sessão relê tudo sem cache. Quando precisar de outro modelo, prefira um subagente a trocar o modelo da sessão principal.
- Ressalvas do Fable: retenção de 30 dias (fora de zero data retention), *"Parallel tool calling is more variable"*, e `tool_choice` `any`/`tool` devolve 400 [P/whats-new-fable-5-1].
- Custo de orquestração (todas as frases de [C/costs], salvo indicação):
  - Agent teams: *"approximately 7x more tokens than standard sessions when teammates run in plan mode"*.
  - Workflows: o aviso `Large workflow` aparece acima de 25 agentes ou 1,5M tokens [C/workflows].
  - CLAUDE.md: *"Aim to keep CLAUDE.md under 200 lines"*.

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
- **`/clear` entre tarefas não relacionadas.** Custa zero; rode `/rename` antes. Use `/compact <foco>` só quando a continuidade importa, porque ele é uma requisição grande.
- **Effort antes de modelo.** E nunca `/model` no meio da sessão (§2, cache por modelo).
- **Custos invisíveis.** `/loop`, tarefas agendadas, goal check-ins e mensagens entre sessões reenviam o contexto inteiro a cada disparo [C/costs, "Why usage climbs"].
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
| Boas práticas do Claude Code | Context7 `/websites/code_claude`. O MCP do plugin exige OAuth; sem ele, use a REST `https://context7.com/api/v2/context?libraryId=/websites/code_claude&query=<tema>` |

**Procedimento.** É a própria regra aplicada:
1. A pesquisa vai para subagentes. Use `sonnet` para docs que pedem julgamento e `haiku` para recuperação mecânica. Eles devolvem trecho literal mais URL e marcam UNVERIFIED o que não acharem.
2. O orquestrador confere **na fonte** as afirmações que sustentam regra.
3. Atualize §2 e §3 com a data.
4. Ajuste a matriz §1 se o posicionamento mudou.
5. Grave a lição no grafo de memória.

A pesquisa de 08/10 custou 4 subagentes (3 sonnet e 1 haiku) e 3 conferências diretas.

## 7. Lacunas medidas (UNVERIFIED em 08/10/2026) e potencializações propostas

- **O modelo agora enxerga a própria franquia (implementado em 08/10, ordem do Gabriel).** O payload da status line traz `rate_limits.five_hour` e `rate_limits.seven_day`, cada um com `used_percentage` (0–100) e `resets_at` (epoch), só para Pro/Max e só depois da 1ª resposta da sessão; cada janela pode faltar sozinha [C/statusline]. O `status_monitor.sh` (versionado em `client/hooks/` desde então; `~/.claude/hooks/` tem o symlink) passa o payload, em background, a `client/hooks/rate_limits_snapshot.py`, que grava `~/.claude/touring/rate_limits.json` com **uma leitura por provedor** (`backend_provider.provider_of`, a mesma fonte do gate: uma irmã nunca sobrescreve a cota da Anthropic). Ausência nunca vira zero: payload sem leitura não cria nem apaga nada. Consultar antes de um fanout: `python3 ~/.claude/hooks/rate_limits_snapshot.py --show` (exit 3 sem leitura; `--json` para código). O `agent_model_gate` cita a mesma linha na recusa. 1ª leitura real: *"5 h 5% (zera 16:30) · 7 d 96% (zera 09/10 03:00)"*. Testes: `client/hooks/tests/test_rate_limits_snapshot.py` (18).
- **Kimi.** Não se sabe a unidade da cota (requisição ou token). Os domínios divergem: `api.kimi.com/coding` (o local, que funciona) contra `api.kimi.ai/coding` (página de terceiros-tools). A data exata de lançamento do K3 também falta.
- **MiniMax.** Não se sabe o máximo de saída, o preço nem os benchmarks do M3.1-Flash-Preview. A doc oficial recomenda `CLAUDE_CODE_AUTO_COMPACT_WINDOW=524288`; a função local usa `1000000` (decisão do Gabriel).
- **Anthropic.** A janela exata das mensagens "Opus limit" não foi encontrada. O effort default do Sonnet 5.5 diverge: `high` na API, `medium` no Claude Code.
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
