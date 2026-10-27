# Diagnóstico — Portfólio Tipado de Scripts Reutilizáveis

> **Data**: 2026-10-27 | **Autor**: TACO (orquestrador) | **Pedido**: Gabriel — "evitar criar e recriar scripts semelhantes; portfólio tipado consultado antes de criar; diretório que persista entre sessões"
> **Método**: mineração determinística de 2.420 transcripts do Claude Code (6,4 GB) + NotebookLM ("Claude Code" 25/09/2026; "GitHub Repos") + leitura direta de harness2/rtk/prime-agent
> **Scripts desta análise**: `scripts/transcript_script_miner.py` (minerador), `scripts/transcript_miner_detail.json` (detalhe) — já criados em diretório persistente, como manda a tese.

---

## 1. Sumário executivo

A criação de scripts é intensa e saudável (code mode funciona: `touring run --lang` aparece **21.152×** como comando real em 809 transcripts), mas a dinâmica tem três falhas estruturais medidas:

1. **81,2% dos scripts nascem efêmeros ou inline** (24.146 `python3 - <<EOF` sem arquivo + 9.875 writes em `/tmp`/cache/scratchpad) — morrem com a sessão.
2. **A escada de persistência existe e nunca é usada**: `--harvest` (que transforma um `touring run` em snippet reutilizável) aparece **0×** como comando real em todo o corpus; a memory Touring tem apenas **10 snippets**.
3. **A recriação é semântica, não textual**: duplicata exata é rara (1,0%), mas basenames genéricos de mesma finalidade são reescritos dezenas de vezes entre sessões — `probe.py` em 14 sessões, `mutantes.py` em 18, `dump.py` em 12, `fetch.py`/`build.py`/`show.py`/`final.py` em 10 cada.

O ecossistema (NotebookLM + repos) converge na mesma arquitetura que falta aqui: **explore once → generate script → execute deterministically**, com scripts promovidos a CLIs tipadas (`--json`/`--schema`/`--help-agent`/`--dry-run`) num diretório persistente e consultável, e carregamento just-in-time. O Touring já tem 90% da infraestrutura (portfolio, snippets com facetas, harvest, spill); **o que falta é o passo obrigatório de consulta-antes-de-criar e o destino persistente default**.

---

## 2. Evidência interna — o que as sessões realmente fazem

Fonte: `scripts/transcript_script_miner.py` sobre `~/.claude/projects/**/*.jsonl` (2.420 transcripts, 0 erros de parse). Evento = Write de arquivo de script, heredoc `cat > arq <<`, ou `python3 - <<`/`-c` inline >400 chars.

### 2.1 Volumes

| Métrica | Valor |
|---|---|
| Eventos de criação de script | **41.923** |
| — inline Python (sem arquivo) | 24.146 (57,6%) |
| — Write de arquivo (.py/.sh/.js/…) | 12.111 |
| — heredoc `cat > arq` | 5.666 |
| Comandos reais `touring run --lang` | 21.152 (em 809 transcripts ≈ 33%) |
| Comandos reais com `--harvest` | **0** |
| Snippets na memory Touring | **10** |

### 2.2 Destino (o problema que o Gabriel apontou)

| Destino | Eventos | % |
|---|---|---|
| inline (nunca vira arquivo) | 24.146 | 57,6% |
| efêmero (`/tmp`, `/var/tmp`, `.cache`, scratchpad, tool-results) | 9.875 | 23,6% |
| persistente `docs/plans/` | 1.328 | 3,2% |
| persistente `scripts/` | 1.255 | 3,0% |
| persistente hooks / outros | 5.319 | 12,7% |
| **Total efêmero ou sem arquivo** | **34.021** | **81,2%** |

### 2.3 Finalidade (por que scripts são criados)

| Finalidade | Eventos |
|---|---|
| teste/verificação | 12.704 |
| análise/relatório | 4.919 |
| auditoria/cross-audit | 1.415 |
| corpus/dataset | 1.110 |
| juiz/julgamento | 729 |
| benchmark/medição | 664 |
| sync/propagação | 315 |
| validação de plano | 296 |
| scrape/fetch | 275 |
| migração/rewrite | 137 |
| (não classificado) | 19.359 |

### 2.4 Duplicação

- **Exata** (hash normalizado idêntico, paths/sessões distintos): 193 grupos, 414 eventos = **1,0%**. Quando a duplicata exata ocorre, é retry na mesma tarefa (ex.: `q6_alcance.py` reescrito 24× no mesmo scratchpad).
- **Semântica** (mesmo basename genérico em ≥3 sessões distintas — proxy de "reinventado do zero"): `mutantes.py` 18 sessões · `probe2.py` 17 · `probe.py` 14 · `dump.py` 12 · `fetch.py`/`build.py`/`show.py`/`final.py`/`conftest.py` 10 · `verifica.py`/`mutar.py`/`mutacao.py`/`rodar.sh`/`ver.py` 9. É aqui que mora o desperdício que o Gabriel observou: não se copia o texto, **reinventa-se o propósito**.
- **Tendência**: 29.033 dos 41.923 eventos (69%) são de outubro/2026 — o volume cresce e a ausência de portfólio custa mais a cada mês.

---

## 3. O que o ecossistema recomenda (fontes externas)

### 3.1 NotebookLM "Claude Code" (25/09/2026, 94 fontes)

- **Padrão canônico**: *Explore Once → Generate Script → Execute Deterministically*. Inspeção única por subagente descartável; síntese de script **fortemente tipado reutilizável** armazenado em `bin/` ou `.agents/scripts/`; depois o agente o invoca como primitiva atômica "processando milhares de registros sem consumir um único token adicional de LLM".
- **CLI-First for Agents**: todo script persistente deve ter `--json`, `--schema`, `--help-agent`, `--dry-run`. CLIs instaladas têm **custo zero de contexto em repouso**; a transição MCP-estático → CLI+Skills mediu **−85% de tokens em repouso** e acurácia de seleção de ferramenta 49% → 88,1% (Opus 4.5).
- **Skills vault just-in-time**: biblioteca em diretório separado (`.agents/skills-vault/`); a skill entra no contexto só no turno necessário.
- **Diretórios**: nunca poluir a raiz; `/active/tmp/` ou `.scratch/` para uso único (com varredura periódica de limpeza); persistentes em `bin/`, `.claude/skills/`, `.agents/scripts/`.

### 3.2 NotebookLM "GitHub Repos" (107 fontes)

- **context-mode**: sandbox devolve só stdout — sessão típica de 315 KB de dados brutos cai para 5,4 KB (**−98%**); "the LLM should program the analysis, not compute it".
- **harness2 (google-research)**: o harness expõe 8 componentes editáveis — *system prompt, guardrails, memory, tool guidance, subagents, skills, **scripts**, plugins* — e opera recursão em 2 níveis: task-level (Propose–Probe–Compose, refina o harness da tarefa sem ground-truth) e harnessing-level (**acumula ferramentas e lições validadas por evidência para guiar tarefas futuras** — é exatamente o portfólio que falta). Um único passo de refinamento mediu +11,5 pts WorkBuddy-Bench, +4,5 JobBench, +11,8 p.p. LAB partial-pass.
- **Trail-Code**: learnings como arquivos (Decisions/Preferences/Skills/Guardrails) em `.trail/` do repo e `~/.trail/` pessoal; o agente salva e consulta sozinho; mapa de código determinístico: **−46% tool calls, −42% tokens, −60% tempo, SWE-bench 54%→66%**.
- **XERJ (reference coding)**: consultar código que já resolveu o problema antes de escrever — **2,7× menos tokens de saída que grep, 26× menos que memória pura**, mesmo solve rate (16/16), custo $11,18→$1,58.
- **ECC**: <10 MCPs, <80 ferramentas ativas; Memory Vault `ecc.memory.v1` em Markdown portável entre harnesses; "instincts" com score de confiança que evoluem para skills via `/evolve`.
- **pond-db / memvid / ruflo**: histórico de sessões como arquivo consultável (SQL/vetorial) — "como resolvemos isso antes?" vira query, não arqueologia.

### 3.3 Leitura direta dos repos (agente de pesquisa, 27/10/2026)

**harness2 (google-research)** — raiz + `docs/BENCHMARKS.md` + `docs/EXPERIMENTS.md`:
- *Scripts* são um dos 8 componentes editáveis do harness ("tool guidance and scripts define the tool interface"); a reflexão armazena "adopted harnesses and evidence-supported lessons" para tarefas futuras.
- **EXPERIMENTS.md tem o mecanismo mais próximo do portfólio pedido**: a reflexão grava o harness adotado e as lições numa ***per-domain library***; "tasks within a domain run in order and **read only earlier entries**" (leitura prefixada — biblioteca curada por domínio); libraries separadas por benchmark/executor/mode/budget/seed; resultados persistem em `runs/` e o mesmo comando faz resume; "failed evaluations remain unscored rather than being counted as zero" (sinal ausente ≠ zero — mesma lei L2 do loop-engineering).
- BENCHMARKS.md é guia operacional (como rodar LAB/JobBench/WorkBuddy), **sem** números de desempenho e sem evidência isolada de reúso de scripts (ablação não reportada — os +11,5/+4,5/+11,8 são do harnessing recursivo como um todo).

**rtk-ai/rtk** — proxy que comprime output de comandos shell antes do LLM (hook PreToolUse, não chamada programática):
- **Não cobre** reúso de scripts nem persistência de artefatos; tangencial: config em `~/.config/rtk/pipeline.toml`, `rtk gain --history` com métricas.
- Medido: **60–90% menos tokens** no output de comandos dev comuns, overhead <10ms; ressalva honesta: tokens estimados como bytes/4 ("percentages are reliable but the absolute token numbers are approximate"). Relevância para nós: é a camada de **compressão de saída**, ortogonal ao portfólio — complementa, não substitui.

**PrimeIntellect-ai/prime-agent** — agente com REPL Python persistente (RLM, daemon-backed):
- **O mais explícito dos 5 em code mode**: ferramentas e subagentes são *function calls* dentro de um REPL persistente; `rlm.spawn(...)` retorna resultados programaticamente; "file operations, shell commands, tool use, subagents happen through code".
- **Reúso tipado**: "skills são **pacotes Python importáveis**, e o skill creator embutido transforma workflows recorrentes em skills de projeto ou pessoais"; o Continual Harness guarda "supplemental prompts, memories, skill descriptions e **reusable subagent specifications** como estado durável"; `/refine` persiste lições revisáveis como prompts/memórias/skills/subagent specs.
- **Persistência além da janela**: agentes daemon-backed continuam quando o terminal desconecta e podem ser reanexados; "active sessions, Python REPL state, schedules, and subagents keep running"; snapshots com rollback; "durable harness state can outlive a single chat window".
- Sem benchmark quantitativo nos temas (cita preprint arXiv 2608.23552 sem números na página; stars/commits não são resultado).

**Síntese da leitura direta**: code mode explícito só no prime-agent (REPL persistente, tools como function calls — análogo ao nosso `touring run`); reúso/biblioteca em harness2 (libraries per-domain, leitura prefixada) e prime-agent (skills como pacotes importáveis + specs de subagente reutilizáveis como estado durável); números medidos relevantes: harness2 (+11,5/+4,5/+11,8 — sem ablação do componente scripts) e rtk (60–90% de compressão de tokens, adjacente). **Nenhum dos 5 tem um "portfólio tipado de scripts consultado antes de criar" como componente nomeado — o mais próximo é a per-domain library do harness2 e as skills-pacote do prime-agent; a proposta da seção 5 é, portanto, um passo além do estado da arte consultado, não uma cópia.**

---

## 4. Diagnóstico — os 5 gaps

| # | Gap | Evidência | Custo |
|---|---|---|---|
| G1 | **Criação sem consulta** — nenhum passo "procurar no portfólio antes de escrever" existe no fluxo | `--harvest` 0× em 2.420 transcripts; 10 snippets na memory; `touring portfolio` existe (TIER 3) e não é gate | Recriação semântica (§2.4) |
| G2 | **Destino efêmero é o default** — inline/scratchpad/tmp em vez de diretório persistente | 81,2% dos 41.923 eventos | Script morre com a sessão; próxima sessão recomeça do zero |
| G3 | **Recriação semântica invisível** — duplicata exata é 1%, então nenhum alarme textual dispara | `probe.py`/`mutantes.py`/`dump.py` reescritos 12–18× | Dezenas de reescritas do mesmo propósito por mês |
| G4 | **Scripts sem contrato** — nascem como one-off, sem `--json`/`--schema`/docstring de propósito, então não são reutilizáveis mesmo quando persistem | top basenames são genéricos (`final.py`, `b1.py`, `s1.py`) | Mesmo o que persiste em `docs/plans/` não é descobrível por finalidade |
| G5 | **A infraestrutura Touring está a um passo de fechar isso** e não fecha: snippets facetados (`#kind:snippet`), `touring portfolio "<intento>"`, `--harvest <slug>`, spill com `retrieval_hint` — tudo existe, nada é obrigatório | Reflexo #8 fala em executar, não em consultar/colher | A adoção não emerge da disponibilidade (lição já registrada: "adoption must be actively induced") |

---

## 5. Recomendação — arquitetura do portfólio

### 5.1 Destinos (hierarquia de persistência)

| Camada | Diretório | O que entra | Critério de promoção |
|---|---|---|---|
| L0 descartável | `/tmp/touring-run-*` (sandbox, já existe) | experimento de 1 uso | — |
| L1 do plano | `docs/plans/<plano>/scripts/` | scripts da tarefa em curso (este diagnóstico já nasceu assim) | default de todo Write de script |
| L2 do projeto | `<projeto>/scripts/` (já usado: 1.255 eventos) | utilitários do repo | usado ≥2× no plano |
| L3 global | `~/.claude/scripts-portfolio/` (novo) + registro como snippet facetado | script tipado com contrato CLI | usado em ≥2 projetos/sessões |

### 5.2 Contrato do script de portfólio (tipagem)

Todo script L2+ carrega: codetag `#tags: kind:script purpose:<p> domain:<d> lang:<l>` (já indexado pelo post_write), docstring de propósito, `--json`/`--dry-run` quando aplicável, e entrada na memory como `#kind:snippet` (é o que o `memory sync-tags` já faz de graça quando o codetag existe).

### 5.3 Consulta-antes-de-criar (o passo que falta)

1. **Reflexo novo** (CLAUDE.md/skills): antes de Write de script, `touring portfolio "<intento> #kind:script"` + `touring memory query "#kind:snippet #purpose:<p>"` — 2 comandos, <50ms.
2. **Enforcement por afordância** (D8: enforcement mora no executor, não no anúncio): hook pre-write que, ao ver criação de `.py/.sh` em destino efêmero com basename genérico, injeta os 3 snippets mais próximos existentes (já há prova de conceito: o hook de hoje mostrou "ANN related: mede_base_js.py (37%)…" quando este minerador foi escrito — a capacidade existe, falta transformá-la em gate com veredito).
3. **Colheita real**: `touring run --harvest <slug>` promovido de flag esquecida a passo default quando o mesmo programa roda ≥2× (o journal `run_journal.jsonl` já sabe contar isso; KPI `code_mode_reuse` já existe com piso 0,20).
4. **Escada medida** (A13 já prevê): provisional ≥10 exec @≥90% → trusted ≥100 @≥95% — ligar a escada ao portfólio L3.

### 5.4 O que NÃO fazer

- Não criar um segundo sistema de catálogo fora da memory Touring (a faceta `#kind:snippet` já é o índice; falta volume, não estrutura).
- Não proibir inline/efêmero — 57% inline é code mode saudável para exploração; o alvo é o **repetido**, não o **exploratório** (E2: forçar programa no caso simples taxa o caso comum; o mesmo vale para forçar persistência no script de uso único).
- Não confiar em nudge sem gate: a lição `protocol-adherence-diagnosis` já mediu que persuasão falha; o gate mora no hook/executor.

---

## 6. Próximos passos propostos (para aprovação do Gabriel)

1. **P0** — Hook `script_portfolio_gate.py` (pre-write, sombra primeiro): ao criar script em destino efêmero, injeta top-3 snippets próximos por finalidade; mede em `events.jsonl` quantas criações teriam sido evitadas.
2. **P1** — Migrar os basenames campeões de recriação (`probe/mutantes/dump/verifica/fetch/build/show/recalc`) para `~/.claude/scripts-portfolio/` como snippets facetados, começando pelos de finalidade teste/verificação (30% do volume).
3. **P2** — Ligar `--harvest` ao journal: quando o mesmo programa (hash normalizado) roda ≥3×, o próprio `touring run` sugere `--harvest <slug>` no output.
4. **P3** — CLAUDE.md: 1 linha no Reflexo #8 apontando "consultar portfólio antes de criar" (respeitando REGRA #16 — a regra detalhada vai para `rules/` ou skill).

---

## Apêndice A — Reprodutibilidade

```bash
# mineração (2.420 transcripts, ~2min)
python3 docs/plans/2026-10-27-portfolio-scripts/scripts/transcript_script_miner.py
# detalhe (grupos de duplicata, basenames)
cat docs/plans/2026-10-27-portfolio-scripts/scripts/transcript_miner_detail.json | jq .
# contadores de adoção
rg -c 'command\\?":\\?"[^"]*touring run --lang' ~/.claude/projects | awk -F: '{s+=$NF} END {print s}'
rg -c 'command\\?":\\?"[^"]*--harvest' ~/.claude/projects | awk -F: '{s+=$NF} END {print s+0}'
```

Limites da medição: (a) `por_mes` tem 6.219 eventos sem timestamp (registros antigos sem o campo); (b) a classificação de finalidade é por palavra-chave (precisão ~0,8, não auditada exaustivamente); (c) "recriação semântica" usa basename como proxy — subestima recriações com nomes distintos e superestima coincidências de nome curto; (d) a contagem de `touring run` inclui comandos que aparecem em outputs ecoados, não só invocações (margem pequena, mesmo padrão `command":`).
