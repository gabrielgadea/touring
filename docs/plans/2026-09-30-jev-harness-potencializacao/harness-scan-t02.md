---
type: ScanReport
title: T02 — varredura do harness para decisões tipadas (Jev × Touring)
description: Estado da chave TypeSafe, o que falta para o G2 rodar, volumes frescos dos pontos de decisão candidatos e os achados do harness desde 23/09. Nenhum código foi editado.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [scan, harness, jev, decisoes-tipadas, g2]
timestamp: 2026-09-30T22:10:00-03:00
okf_version: "0.1"
---

# T02 — varredura do harness (analista interna)

Dona dos caminhos de leitura desta frente. **Nada foi editado fora deste arquivo**;
nenhum commit, nenhum restart de daemon. Marcação: **[F]** ato medido (comando ao
lado) · **[I]** inferência (confiança entre parênteses).

Método e instrumento primeiro, porque três dos números abaixo nasceram de um
instrumento que mentiu e foram corrigidos antes de virar número:
o `bash_outcomes` e o `run_journal` **não registram recall** (0 em 22.745 linhas
mesmo com 1.340 prompts humanos na semana), e o `sdk_signal_mirror` grava `ts` como
**epoch inteiro** — um parser ISO devolve zero sem erro. Zero aqui é falha de
instrumento, não ausência. Já é a lição `instrumento-que-lie` do acervo.

---

## 1. A chave existe — mas não está no caminho de nenhum processo

| Medida | Resultado | Fonte |
|---|---|---|
| `TYPESAFE_API_KEY` no ambiente desta sessão | **UNSET** | `[ -n "$TYPESAFE_API_KEY" ]` |
| `TYPESAFE_*` no ambiente do daemon vivo | **0** | `tr '\0' '\n' < /proc/$(cat /run/user/1000/touring-daemon.pid)/environ \| grep -c '^TYPESAFE'` |
| Arquivo com a chave | **existe**: `~/.config/typesafe/env` (126 B, `-rw-------`), criado **27/09 16:28** | `ls -la ~/.config/typesafe/` |
| Variável declarada nesse arquivo | `TYPESAFE_API_KEY` (só o nome lido; **valor nunca lido**) | `grep -oE '^[A-Za-z_][A-Za-z0-9_]*=' ~/.config/typesafe/env \| tr -d '='` |
| Algum rc faz `source` do arquivo | **nenhum** — `~/.zshrc`, `~/.bashrc`, `~/.profile`, `~/.config/omarchy/env`: 0 ocorrências de `config/typesafe` | `grep -rn "config/typesafe" …` |
| `typesafe-sdk` instalado | **não** — `WARNING: Package(s) not found` | `pip show typesafe-sdk` |
| Clientes Jev escritos no repositório | **2 arquivos, ambos falsos positivos** — `crates/touring-cli/src/cli/prove.rs` e `crates/touring-offensive/src/solver.rs` casam a string "typesafe" por outro motivo (a busca é `grep -ril` sobre `crates/ scripts/ ~/.claude/skills/`, `--include=*.rs\|*.py`) | `grep -ril typesafe crates/ scripts/ ~/.claude/skills/ --include=*.rs --include=*.py \| head -20` → 2 de 2 |

**[F] O bloqueio R6 de 22/09 ("ação externa do Gabriel") está RESOLVIDO no papel e
aberto no fato.** A chave foi criada em 27/09 — cinco dias depois do plano — e nunca
chegou a nenhum processo: nem shell, nem daemon, nem SDK. **[I] (0,95)** Nenhuma das
minhas shells vai vê-la sem `source ~/.config/typesafe/env` (ou um
`environment.d/`), porque o arquivo não é carregado por nenhum caminho de
inicialização; e o daemon, que é quem resolve as chamadas de hook, não a herdará
de uma sessão futura sem reinício explícito (REGRA #19: `touring daemon-ctl restart`).

**[I] (0,90) Nenhum cliente existe.** R12 da estratégia (pin `typesafe-sdk==0.7.1`
com hash, porque o `typesafe-client` do PyPI é placeholder de terceiro) continua
inteiro: não há nem sequer um import para pinar. O G2 vai escrever o primeiro
cliente do zero.

**Bloqueio duro para o G2 agora:** chave não exportada **e** SDK ausente. O G2
precisa de `typesafe-sdk` instalado; a chave só é preciso no momento de chamar a
API, não para montar o baseline — as três linhas de base não-Jev (keyword, Laya
zero-shot, professor) **não dependem da chave** e podem correr hoje.

---

## 2. G0–G5: o que falta para o G2 rodar AGORA

| Gate | Estado | Fonte |
|---|---|---|
| **G0** — filtro de payload automático | ✅ entregue e vivo (30.4.64) | `strategy-…-jev-system-one.md:621` |
| **G1** — corpus e rótulos | ✅ entregue; `passes_gate: true` na revisão modelo×modelo (prompt_intent 0,963 · memory_kind 1,000 · recall_relevance 0,900) | `experiments/g1-corpus/report.md:107-118` |
| **G2** — linhas de base | ⬜ **não iniciado**. Definição: "keyword atual, Laya zero-shot, professor; Jev só nos dados não sensíveis, se houver chave → tabela de acurácia, ECE e p99 por decisão" | `strategy-…:623` |
| G3 · G4 · G5 | ⬜ não iniciados (dependem do G2) | `strategy-…:624-626` |

**[F] Corpus disponível para o G2:** 800 prompts humanos + 486 memórias + 300 pares
consulta→memória, rotulados, fora do repo em `~/.claude/touring/g1-corpus/`
(`report.md:21-34`). A folha de revisão de 158 itens também está lá.

**[F] Split congelado treino/validação/teste: [I] (0,60)** — o `report.md` promete
o split na definição do gate (`:622`) mas **não encontrei o artefato do split**
(procurei em `experiments/`, `knowledge/` e na árvore do plano: só há `G0.json`,
`G0b.json`, `R4.json`, `S5.json`). O G2 não tem como reproducir a partição sem ela.

**[F] Bloqueio de gate ainda aberto no G1:** falta o spot-check **humano** do Gabriel
sobre as 6 divergências — `report.md:118-119` é explícito: "um segundo modelo
concordando com o primeiro não é evidência independente".

**O que falta, em ordem, para o G2 rodar hoje:**
1. **[F]** `typesafe-sdk` instalado com versão pinada (R12) — bloqueia só a linha
   Jev, não as outras três.
2. **[F]** Export da chave no caminho de quem roda o G2 — 1 linha de `source`, ou
   `~/.config/environment.d/typesafe.conf`. **[I] (0,90)** Sem isso, a chamada à API
   falha em tempo de execução, não em tempo de instalação.
3. **[F]** O artefato do split (se não estiver fora do repo).
4. **[I] (0,85)** O Laya zero-shot é a única das três linhas de base que depende de
   artefato externo ao Touring (pesos `laya-multilingual` + o fine-tune, R13/R15: projeto
   de 4 dias, issues abertas, e **nenhum relato de treino em GPU de 8 GB** — a RTX
   4060 da máquina é 8 GB). É a linha de base com maior risco de não existir.

**[F] O texto do plano já previa isto:** "Nenhum gate exige chave da TypeSafe até o
G2" (`strategy-…:628`). O G2 é literalmente o primeiro portão da chave — e ele está
inteiramente por fazer.

---

## 3. Volumes frescos (7 dias) — o ranking de 22/09 mudou

Todos os números com a fonte. Janela: **24/09 → 01/10**.

| Métrica | n | média/dia | Fonte |
|---|---|---|---|
| **Prompts humanos** (payload automático já filtrado) | **1.340** | **191,4** | varredura de 1.639 transcripts em `~/.claude/projects/**/*.jsonl`, `type=user`, filtrando os prefixos de `AUTOMATED_PAYLOAD_PREFIXES` |
| Entradas de memória novas | **2.250** | **321** | `select count(*) from memory_entries where created_at>=cut7` no `memory.db` do projeto |
| Entradas de memória **tocadas** (recall as alcançou) | **2.316** | **330,9** | `last_accessed_at` no mesmo banco |
| Acessos acumulados a memória | **109.315** (91,9% das 19.815 entradas já foram acessadas ≥1 vez) | — | `sum(access_count)` |
| **Recalls via SDK** (o único registro real de chamada) | **35** | **5,0** | `~/.claude/touring/sdk_signal_mirror.jsonl`, `hook_name=memory_recall` |
| **Tickets do factory** desde 23/09 | **10** (1 `scout-ticket`, 9 planos de trabalho) | 1,1 | `task_decompositions` do `knowledge.db` do projeto |
| Subtasks com `ticket_kind` | `implementation` 45 · `decision` 17 · sem kind 1.442 | — | `decomposition_subtasks` |
| Subtasks por status | completed 614 · pending 473 · done 410 · in_progress 5 | — | idem |
| Decisões do `judge` (subtasks da_cláusula cross-audit) | 45+17 = **62** | 8,9 | idem |

Distribuição diária dos prompts humanos **[F]** (`#` = 1 prompt):
`24/09 162 · 25/09 333 · 26/09 220 · 27/09 332 · 28/09 79 · 29/09 182 · 30/09 26 · 01/10 6`
→ **[I] (0,80)** o dia corrente sempre aparece truncado (a sessão ainda está no
começo); ler 30/09 e 01/10 como queda de uso seria erro de instrumento.

Por projeto **[F]**: `analise` 3.364 prompts (75,4%) · `touring` 938 · `Work` 142.

### 3.1 O que isso muda no ranking de 22/09

**[I] (0,90) O factory NÃO é mais candidato a decisão tipada.** A estratégia v2
colocou o factory em 1º com 9 tickets na vida (`:833-835`, lição de método: "ranqueou
pelo ganho sem medir o volume"). Hoje são **10 tickets em 7 dias** — o volume
cresceu, mas o total absoluto continua sendo da ordem das dezenas por semana, e o
`factory.py` só roteia o que o scout perpétuo emite. Com 62 subtasks de
`ticket_kind` decide-tipo na vida toda, um classificador de decisão tipada pago por
chamada (~$0,042/MTok, ~100 ms) **não se paga** neste ponto. O gate G5 do plano
("factory quando houver volume") continua correto ao adiar.

**[I] (0,85) O ponto de decisão de maior volume é o PROMPT, não o recall.** 191
prompts humanos/dia contra 5 recalls/dia por SDK. A entrada é a decisão de
*intenção + CILA*, no `prompt_enhance`, e ela roda 191×/dia. O G1 já mediu que essa
é a classe que o classificador por keyword **mais erra** (`report.md:67-69`: "o
grosso do trabalho é análise — a classe que o keyword classifier mais erra"; ele
mandou os dois pedidos de pesquisa profunda daquela sessão para GENERAL). **[I]
(0,80)** Com 191/dia e o erro medido, o ganho por chamada é ordens de grandeza maior
que o do recall — e é exatamente o G4 do plano ("Intent → CILA com gate de
confiança e fallback para o keyword").

**[F] O recall é 4× mais Chong do que o prompt é lento**, mas é 38× mais raro via
SDK: `memory_recall` = 35 chamadas em 7 dias no mirror, contra 133 no total
histórico (28 dias). Latência **[F]**: 100 das 133 abaixo de 50 ms, 20 entre
200–1000 ms, **11 acima de 1 s**. **[I] (0,75)** Uma chamada Jev de ~100 ms seria
invisível nas 120 rápidas e irrelevante nas 31 lentas — o gargalo do recall é
busca, não decisão.

**[I] (0,85) O `sdk_signal_mirror` é um piso, não o total.** Ele registra só o que
passa pelo SDK/hook: 35 recalls/7d contra 2.316 entradas **tocadas** em 7d. O recall
que importa para a estratégia — o injetado em contexto — não aparece ali. **A
medição de volume de recall que a estratégia precisa ainda não tem instrumento.**

### 3.2 Memórias: o alvo de "sem tag revisada" evaporou

**[F]** `memory_tags` tem 43.524 linhas sobre 19.815 entradas: `auto` 24.478 ·
`backfill` 15.825 · `explicit` 2.727 · `code_sync` 494. Facetas: `kind` 20.135 ·
`status` 20.130 · `domain` 1.868 · `process` 798 · `artifact` 291 · `lang` 213 ·
`purpose` 89.

- **[F] `sem_nenhuma_tag = 0`** — 0% das entradas estão sem tag.
- **[F] `entrada_com_tag_humana = 0`** — nenhuma tag tem `source` humano/manual/user.
  **Tudo é derivado ou automático.**
- **[F]** `superseded_by` preenchido: **4** entradas. `pinned`: **2**.

**[I] (0,95) O backfill de tags já rodou e estoqueou 15.825 tags automáticas sobre
2.250 memórias novas na semana.** A pergunta "quanta memória está sem tag revisada,
para o G2 rotular?" responde: **todas as 19.815** — não há uma cauda de sem-tag
para atacar, e o que existe é tag automática que o professor só concorda **65,4%**
das vezes (`report.md:77-79`). **[I] (0,80)** O ganho de decisão tipada aqui é
**corrigir tag errada**, não **preencher tag ausente** — e a strategy de 22/09
framava como ausência. Corrigir 34,6% de 19.815 ≈ 6.800 memórias é um alvo
**maior** e de natureza diferente. `superseded_by` em 4 entradas confirma que o
esquema de aposentadoria (30.4.65) praticamente não foi exercido.

---

## 4. Achados no harness desde 23/09 relevantes a decisões tipadas

**[F] 153 commits** desde 23/09 (`git log --oneline --since=2026-09-23 | wc -l`),
versão viva `touring 30.4.87` (git `25753268`), **18 releases** no período.

**[F] Distribuição por área (prefixo da mensagem):** release 18 · hooks 13 · style 6 ·
quality 6 · wiring 4 · client 3 · tests 2 · os demais 1 cada.

### 4.1 Hooks — o que mudou para quem decide

| Achado | Fonte | Relevância para decisão tipada |
|---|---|---|
| **S31: os 4 handlers touring voltaram a disparar** (um `if` por extensão) e foram para o settings do projeto | `c4a684f5` | **[I] (0,90)** Entre 23/09 e 31/09 os hooks ficaram **calados** — o `SessionStart` que injetou o contexto desta sessão cita "o campo `if` aceita **uma** regra só e falha aberto". Qualquer medição de recall/hook anterior a S31 mede um sistema desligado. |
| **S32: handlers religados param de injetar o que não existe ou não é do leitor** | `c3e6da8d`, `0f15aa90` | **[I] (0,85)** Corrigiu injeção indevida — ou seja, **o volume de injeção medido antes estava inflado por conteúdo errado**. |
| **S32b: a fila do daemon** (hooks de todas as sessões atendidos em série) — ticket aberto | `95d69cc9` | **[I] (0,80)** Latência de hook entrou na fila. Se um classificador de decisão for somado ao caminho do hook, ele **herda a fila** — o p99 do `memory_recall` já tem 11/133 acima de 1 s. |
| **Guarda D8: `statusMessage` de handler só-observação não promete gate** (S29) | `4be9f635` | **[I] (0,75)** Padrão institucional: a decisão que se anuncia tem de ser a que o executor aplica. Vale para qualquer Jev atrás de flag. |
| **As janelas de code mode passaram a ser por ator** (sessão + agente) | `4fdd54f2` | **[I] (0,70)** O subagente tem janela própria — a taxa de decisão que um subagente de factory vê não é a da sessão. Relevante se o Jev rodar em `_agent_claude`. |
| **O masker do `block_git` nunca rodou em produção** (o CC chama um symlink) | `18b4462d` | **[F]** Guard que eraorceito nunca executou. |
| **Sessão sintética é observada, nunca ensina** | `afd33505` | **[F]** Corrigido o desvio de treino. |
| **Seção do code mode no SessionStart: 4.096 B → 1.632 B** | `5f84631d` | **[F]** Direto: **4 KB devolvidos a cada abertura de sessão**. |

### 4.2 Classificadores e gates novos

| Achado | Fonte | Relevância |
|---|---|---|
| **S213b: uma SEQUÊNCIA só é inspeção se todo passo inspeciona** | `2adcc181` | **[I] (0,80)** Classificador de *forma de comando* — é o mais próximo de um "classificador tipado" já vivo no CEG, e foi corrigido 2 dias atrás. |
| **Detecção de código em Python/Rust** (`detect_code_mode`) só conta código, não mensagem nem tabela de padrões | `92b5282c` | **[I] (0,80)** Falso positivo de detector eliminado **242×** (lição `produto-avisa-o-que-comete`). |
| **F2.4 sem falso positivo**: regex de string de conexão, SRI, pacote npm com escopo, sintaxe de código | `8bb08fe0`, `0811525a`, `e391f36c` | **[F]** Quatro correções de FP em ~1 semana num único detector. **[I] (0,75)** O custo de um classificador é dominado pelo **FP**, e FP exige tratamento determinístico, não mais modelo. |
| **F2.1/F2.6**: harness de teste reconhecido por **nome de arquivo**; `0o777` só é CWE-732 quando **concede** o modo | `0bed3c6c`, `463e1660` | **[F]** Predicado único no lugar de duas cópias da lista. |
| **F3.1: o proxy Python pareia o módulo com os testes pytest** | `5d2027d9` | **[F]** "contar só `def test_` no próprio arquivo dava **0,000 por construção**" — outro caso de instrumento que produz zero honesto. |
| **F2.6: `0o777` world-writable só quando concede** | `463e1660` | **[F]** Distingue *ler bits* de *comparar valor*. |
| **S510: F2.5 e F4.5 ganham backend Python** (pip-audit/OSV, `uv lock --check`) | `6027f99c` | **[I] (0,70)** Fecha o gate em Python — a linguagem onde a estratégia quer labels. |

### 4.3 O que **não** apareceu

**[F]** `touring kpi -j` expõe hoje **12 famílias** — `ab`, `checks`,
`code_mode_adherence`, `code_mode_reuse`, `code_mode_signal_use`, `context_budget`,
`daemon_availability`, `hooks_complement`, `project_root`, `schema`,
`snapshot_date`, `summary`. **Nenhuma é de recall, prompt, intent, memória ou
factory.** `touring gate-metrics -j` expõe 196 chaves, nenhuma delas dessas.

**[I] (0,90) Ponto cego de instrumento, e é o mesmo do G2:** os KPIs que o plano
quer otimizar — acurácia da decisão, taxa de fallback, `curated_recall_share` — não
têm **nenhuma** métrica viva que os preceda. O gate G4 ("taxa de fallback, acurácia
amostrada em produção") não tem onde ser lido. **[I] (0,85)** isto repete a lição
`kpi-fonte-declarada-sem-resolvedor`: KPI declarado sem coletor é braço ausente.

---

## 5. Os três pontos de decisão candidatos, ranqueados por volume medido

| # | Ponto de decisão | Volume/dia **[F]** | Custo por erro **[I]** | Veredito |
|---|---|---|---|---|
| 1 | **Intent + CILA** no `prompt_enhance` | **191,4 prompts humanos** | **[I] (0,85) alto** — o keyword erra a classe majoritária (34% *analysis*), e o modo errado muda como a sessão inteira trabalha | **Candidato principal.** É o G4 do plano, e o volume o confirma |
| 2 | **Relevância no recall** ( Corte CILA) | 5 recalls/dia por SDK; 2.316 entradas/dia tocadas | **[I] (0,80) médio-alto** — 78,7% do recall é ruído (medido no G1: 21,3% útil) | **Candidato real, mas o volume por chamada é baixo e o gargalo é busca, não decisão** |
| 3 | **Roteamento do factory** | 1,1 tickets/dia; 62 subtasks decide-tipo na vida | **[I] (0,80) baixo** | **Adiado** — o gate G5 já diz "quando houver volume" |
| 4 | **Tags de memória** (corrigir as 34,6% erradas) | 321 memórias/dia, 0 revisadas por humano | **[I] (0,75) médio** — alvo de ~6.800 memórias | **Reformular**: de "preencher ausentes" para "corrigir erradas" |

**[I] (0,80) Uma observação de método que vale para a escolha:** o ponto 1 é
também o único em que o G1 **já tem rótulo** (800 prompts, concordância
modelo×modelo 0,963) e a folha de revisão pronta. O ponto 2 tem 300 pares
rotulados com 0,900. O factory não tem corpus nenhum. **Escolher por corpus
disponível é escolher por custo de avaliação, e é o primeiro critério a considerar
depois do volume.**

---

## 6. Bloqueios (reportados, não contornados)

1. **[F] A chave não está no caminho de nenhum processo** — `~/.config/typesafe/env`
   existe (27/09) mas nenhum rc o carrega, e o daemon vivo tem 0 variáveis
   `TYPESAFE*`. Não exportei nada nem reiniciei o daemon: é ação de estado, e a
   instrução desta frente é leitura.
2. **[F] `typesafe-sdk` não está instalado.** R12 (pin com hash) segue inteiro.
3. **[F] Não encontrei o artefato do split congelado do G1** que o gate promete
   (`strategy-…:622`). Se ele estiver fora do repo, o G2 precisa do caminho.
4. **[F] Landlock nega leitura fora do workspace dentro do `touring run`** — o
   primeiro `touring run` desta frente devolveu `recalls_linhas_total=0` com
   1.639 transcripts presentes em disco. **Um zero de dentro do sandbox não prova
   ausência**: é a lição já escrita na estratégia (`:829-832`) e o instrumento me
   tentou repetir. Todos os números de recall deste relatório vieram de fora do
   sandbox; nenhum "0" aqui veio do sandbox.

---

## Apêndice A — W2.0: o split congelado do G1 (ausência provada)

Pedi para achar o artefato do split e, não achando, **provar a ausência**. Feito em
cinco frentes independentes. Resultado: **o split nunca foi produzido.**

**[F] A1 — As chaves dos artefatos não têm partição.** Campos reais de cada arquivo:

| Arquivo | Chaves |
|---|---|
| `prompts.jsonl` | `id`, `project`, `task`, `text` |
| `memories.jsonl` | `id`, `key`, `text`, `task`, `weak_label`, `derived_tag`, `entry_type` |
| `pairs.jsonl` | `id`, `query`, `candidate_key`, `channel`, `task`, `text` |
| `labels_*.jsonl` (os 3) | `id`, `label`, `confidence`, `channel`, `project`, `weak_label` |

`task` é a **tarefa de rotulagem** (`prompt_intent` / `memory_kind` /
`recall_relevance`), não uma partição. Não há `split`, `fold`, `partition`, `subset`
nem `held_out` em nenhum dos 6 arquivos. Os 3 rótulos são **universais** — os mesmos
800/486/300 que o professor produziu, sem uma fatia reservada.

**[F] A2 — O produtor não particiona.** `g1_sample.py` (11.901 B, versionado) tem
exatamente **2** ocorrências de `split`, ambas do método `str.split` do Python
(`:90` e `:134`) — nem uma é partição. Ele amostra por **projeto** (prompts) e por
**classe do prefixo da chave** (memórias), estratificação de cobertura, não de
conjunto reservado. O `sample_stats.json` confirma: só há `seed`, `n` e as
distribuições.

**[F] A3 — O rotulador não particiona.** `g1_label.py` (11.960 B) tem 1 ocorrência
de `split`, e é o `str.splitlines()` de ler o arquivo.

**[F] A4 — A folha de revisão não é split.** `g1_review_sheet.py` tem 5
ocorrências de `split`, todas do método `.split()` sobre texto, mais `BANDS` que
particiona por **confiança** (`baixa` <0,6 / `média` / `alta` ≥0,8). A estratificação
da folha é por faixa de confiança, e o `report.md:88-90` é explícito: "158 itens
estratificados por faixa de confiança, com metade na faixa duvidosa" — é revisão de
concordância, não holdout.

**[F] A5 — A única menção a split em todo o plano é a linha do gate.**
`strategy-…:622` promete "split congelado treino/validação/teste". As outras
ocorrências de `split` nos dois planos Jev: `digest-jev-r2.md:303` (o autor do Laya
é de Split, Croácia), `digest-cookbooks.md:381` ("você dividiria em chunks"), e
`experiments/laya-local/probe.py:77` (`re.split` num prefixo de chave). **Nenhuma é
sobre o corpus.**

**[F] A6 — Não há em `.remember/`, nem no plano de 23/09, nem no git.** Zero
ocorrências combinando split com jev/g1/corpus/treino; zero no plano N1-N6; os
commits que casam `--grep=split` desde 20/09 são todos "stderr"/"split" em outro
sentido (o S224, o S52 do F2.4).

**[F] A7 — `g1-corpus/` inteiro, 12 arquivos na raiz** (listados em A1 mais
`cache/`, `label_stats.json`, `sample_stats.json`, `intent_distribution_filtered.json`,
`review_score.json`, `review_sheet.md`). Os únicos JSON de estatística são
`sample_stats` (n e estratos) e `label_stats` (n, lotes, distribuição, confiança
média). Nenhum deles carrega partição.

### Consequência para o G2 — e o que ela muda de verdade

**[I] (0,95) O split não é um artefato perdido: ele nunca foi escrito.** Não há
arquivo corrompido nem caminho errado — o produtor não produz. Isso é diferente de
"não achei", e muda o remedy: não é procurar, é **criar**.

**[I] (0,90) O gate G1 foi avaliado sem o split, e isso é coerente com o gate que
ele realmente mede.** O gate pede concordância professor × humano ≥ 0,85 numa
amostra revisada de ~10% — a revisão de 158 itens é uma **concordância de rótulo**,
não uma avaliação de modelo treinado. Para medir se o professor acerta, nenhuma
partição é necessária. **[I] (0,90)** O split foi escrito no G1 porque o G1 é a
fonte de dados do G3 (fine-tune): é lá que a partição vira necessária, e o G3 está
**dois gates adiante** (`:624` — "temperatura por (tipo, nº de opções) no held-out").

**[F] O plano consome o split em dois lugares, ambos G3:** a árvore de decisão em
`:538` ("temperatura por tipo e nº de opções (held-out)") e o gate do G3 em `:624`.
O G2, tal como escrito (`:623`), pede "tabela de acurácia, ECE e p99 por decisão" —
**ECE e p99 não exigem holdout**; ECE é calibração sobre o conjunto de teste, e o
"conjunto de teste" pode ser um recorte do próprio corpus, já que nenhuma linha de
base do G2 foi **treinada** nele: keyword atual não treina nada, Laya zero-shot não
treina nada, e o professor é o rótulo de referência, não um modelo avaliado.

**[I] (0,85) O que falta de verdade, para o G2, é um recorte nomeado de TESTE — não
um split de três.** E ele se faz em minutos, sobre dados que já existem: os IDs são
estáveis e o seed é registrado (`sample_stats.json` → `"seed": 20260922`).
**[I] (0,80)** Uma partição determinística por `id % 3` reproduz, e o G2 pode declarar
`test` = um terço. O split de treino só se justifica quando o G3 fine-tunar — e aí
ele precisa ser criado **antes** do fine-tune, nunca depois (fine-tunar em cima do
conjunto que se vai avaliar invalida o held-out sem nenhuma falha visível).

**[F] Nenhuma destas conclusões depende da chave TypeSafe.** O W2.0 é inteiramente
read-only e não tocou o HUMAN GATE.

---

## 8. Apêndice VGP (plano W0–W5) — evidência `file:line`

Coleta para o plano, a pedido de `touring-a7` (01/10). **Read-only**: nada editado
além deste arquivo, nenhum commit, HUMAN GATE intocado. **[F]** em tudo que é citação
literal; **[I]** para a leitura que eu faço do código.

### 8.1 `prompt_enhance.rs` — o ponto onde intent→CILA é decidido

`touring ast meta --depth summary -j`:
`blast_radius: 46` · `line_count: 1755` · `quality_score: 0.709` ·
`pub_symbol_count: 14` · `integration_score: 1.0` ·
notes: "High complexity: keyword_matches(CC=11), action_directives(CC=19)"

**A v2 acertou a linha 253.** O arquivo tem 1.891 linhas (o `ast meta` reporta
1.755 — **[I] (0,9)** o meta conta linhas de código, não físicas).

**[F] A classificação é match de keywords, e a linha 253 é o início dela**
(`crates/touring-hook-runtime/src/prompt_enhance.rs:253-292`):

```rust
/// Classify a prompt into one of 8 intent categories.
///
/// Uses weighted keyword scoring with tiebreaking by intent priority.
/// Bilingual: English + Portuguese (Brazilian).
pub fn classify(prompt: &str) -> ClassifyResult {
    let lower = prompt.to_lowercase();
    let mut scores: HashMap<Intent, f64> = HashMap::new();
    for intent in Intent::all() {
        let keywords = keywords_for(intent);
        let mut score = 0.0;
        for kw in keywords {
            if keyword_matches(&lower, kw.keyword) { score += kw.weight; }
        }
        if score > 0.0 { scores.insert(*intent, score); }
    }
    if scores.is_empty() {
        return ClassifyResult { intent: Intent::General, confidence: 0.0 };
    }
    let (best_intent, best_score) = scores.iter().max_by(|(ia, sa), (ib, sb)| {
        sa.partial_cmp(sb).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| ia.priority().cmp(&ib.priority()))
    }).map(|(i, s)| (*i, *s))
      .expect("scores is non-empty — guaranteed by is_empty guard above");
    ClassifyResult { intent: best_intent, confidence: best_score }
}
```

**[F] E o nível CILA sai numa função separada, uma tabela total** (`:168-176`):

```rust
pub fn intent_to_cila(intent: &Intent) -> u8 {
    match intent {
        Intent::General   => 0,
        Intent::Code      => 1,
        Intent::Debug | Intent::Refactor | Intent::Test => 2,
        Intent::Analysis | Intent::Plan => 3,
        Intent::Creative  => 4,
    }
}
```

**[F] `classify_with_details` (`:183-193`) é o consumidor único dos dois** — junta
intent, CILA, técnicas e confiança numa chamada.

> **[I] (0,90) Isto é a moldura exata do G4, e é melhor do que a v2 supunha.** O G4
> pede "Intent → CILA com gate de confiança e fallback para o keyword". Mas
> **`classify` já produz `confidence` e mesmo assim nunca recusa**: `score > 0.0`
> basta, não há limiar. A confiança é calculada e **ignorada** na decisão
> (`:270-289`). **[I] (0,85)** O gate de confiança não precisa ser *inventado* — é um
> `if best_score < T` em um lugar, e `intent_to_cila` é uma função **total e pura** de
> 6 linhas, o que torna o ponto de inserção barato e reversível por uma flag.
> **[I] (0,80)** A sombra de qualidade (`quality_score` 0,709) e o
> `blast_radius: 46` explicam por que a estratégia deve entrar por flag default-OFF:
> 46 dependents, e `intent_to_cila` decide o roteamento de fase do TACO inteiro
> (o doc-comment em `:163-167` mapeia L0-L1 → solo, L4+ → todas as 9 fases).

### 8.2 `tags.rs` — `derive_tags` e a origem da tag

`ast meta`: `blast_radius: 54` · `line_count: 1002` · `quality_score: —` (ausente no
summary) · `integration_score: 0.821` · `pub_symbol_count: 32`.

**[F] `derive_tags` está em `:147`** (a v2 não citou linha). Assinatura e corpo
(`:147-179`):

```rust
pub fn derive_tags(key: &str, entry_type: &str, file_path: Option<&str>) -> Vec<ParsedTag> {
    let mut out: Vec<ParsedTag> = Vec::new();
    let push = |facet: Facet, value: String, out: &mut Vec<ParsedTag>| { … dedup por full_tag … };
    if let Some(lang) = file_path.and_then(lang_from_path) { push(Facet::Lang, lang.to_string(), &mut out); }
    let et = entry_type.trim().to_ascii_lowercase();
    if let Some(kind) = kind_from_entry_type(&et) { push(Facet::Kind, kind.to_string(), &mut out); }
    else { let prefix = key.split(':').next().unwrap_or("").to_ascii_lowercase();
           if Facet::Kind.seed_values().contains(&prefix.as_str()) { push(Facet::Kind, prefix, &mut out); } }
    let hay = format!("{} {}", file_path.unwrap_or(""), key).to_ascii_lowercase();
    domains_in(&hay, &mut |f, v| push(f, v, &mut out));
    push(Facet::Status, "stable".to_string(), &mut out);
    out
}
```

**Entrada**: `key`, `entry_type`, `file_path` — três strings, sem estado, sem I/O.
**Saída**: `Vec<ParsedTag>` dedup por `full_tag`, com 3 facetas derivadas
(`lang` ← extensão, `kind` ← `entry_type` ou prefixo da chave, `domain` ← match de
texto) e **uma constante**: `Facet::Status, "stable"` (`:179`).

> ### ⚠️ DIVERGÊNCIA 1 — `derived_by` **não existe**. A coluna real é `source`.
>
> A v2 (`:639`, plano F4) e o gate G4 (`:625`) falam em "grava só com confidence alta,
> com `derived_by`". **[F] Não existe coluna, campo nem constante `derived_by` em
> `tags.rs`** (1.002 linhas, 32 símbolos públicos). O que existe é **`TagSource`**
> (`:506-514`):
>
> ```rust
> pub enum TagSource {
>     /// Derived offline over pre-existing entries (lowest trust).
>     Backfill,
>     /// Derived deterministically at store time (lang←extension, kind←type).
>     Auto,
>     /// Re-harvested from an in-code `#tags:` anchor by the snippet indexer.
>     CodeSync,
>     /// Stated by the author/LLM at creation (highest trust).
>     Explicit,
> }
> ```
> persistido em `memory_tags.source` (`:518-526`, `as_str` → `"backfill"`, `"auto"`,
> `"code_sync"`, `"explicit"`), e escrito por `upsert_tag(…, source: TagSource, …)`
> (`:603-607`).
>
> **[F] Isto é BOM para o plano, não um problema.** O enum já é um nível de confiança
> ordenado e commentado — `Backfill` é literalmente "lowest trust" e `Explicit` é
> "highest trust". **[I] (0,90)** Um `TagSource` novo para decisão do modelo é uma
> variante de enum com o `as_str` correspondente; o caminho de gravação e a
> ordenação de confiança já existem. **[I] (0,80)** A divergência é de **nome**, e o
> plano que a carrega diz `derived_by` — se o executor procurar esse campo, não
> acha. Corrigir o nome no plano (não no código) é o caminho barato.

### 8.3 `factory.py` — rotas (as duas linhas da v2 conferem)

**[F] `deterministic_route` em `:77`, `llm_route` em `:85`, `route_ticket` em `:103`**
(243 linhas no total). A v2 acertou exatamente.

```rust
// factory.py:77
def deterministic_route(ticket: str) -> tuple[str, str] | None:
    lowered = ticket.lower()
    for adw, pattern, why in RULES:
        if re.search(pattern, lowered): return adw, why
    return None

// factory.py:85 — docstring: "Ambiguous tickets only: one light-tier call, forced-choice JSON."
def llm_route(ticket: str) -> tuple[str, str]:
    names = [r[0] for r in RULES]
    prompt = (f"Route this ticket to exactly one workflow of {names}. "
              f'Ticket: "{ticket}". Reply ONLY with JSON {{"adw": "<name>", "why": "<short>"}}.')
    try:
        proc = subprocess.run(["claude", "-p", prompt, "--output-format", "json", "--model", "haiku"],
                              capture_output=True, text=True, timeout=120)
        result = json.loads(json.loads(proc.stdout).get("result", "{}"))
        adw = result.get("adw", "")
        if adw in names: return adw, f"llm-router: {result.get('why', 'no rationale')}"
    except Exception: pass
    return "explore-plan", "llm-router unavailable/ambiguous → safest default is discovery"
```

**[F] A cascata completa está em `route_ticket` (`:103-127`)**: regex → se `None`,
`llm_route` (haiku) → se falhar, `"explore-plan"`. Depois chama 4 sondas
(`classify-intent`, `route`, `predict-action`, `investigate --brief`) e monta o
payload, com `solo = adw == "chore" or level <= 1`.

> **[I] (0,90) O factory JÁ É um LLM router, e é a prova executada do padrão que o
> Jev propose.** `llm_route` é literalmente uma **Choice forçada em JSON com
> fallback determinístico** — o mesmo desenho do G2, já em produção, com haiku em vez
> do Jev. **[I] (0,85)** Isso é um *controle* para o plano, não só um candidato: a
> troca haiku→Jev aqui é A/B no código existente (`router` já grava `"deterministic"`
> vs `"llm"` no payload, em `:120`), o que dá **journal de verdade** sem instrumentar
> nada novo. **[F]** E o `timeout=120` + `except Exception: pass` significa que
> hoje essa chamada é a **latência dominante e silenciosa** do roteamento: um
> roteador de ~100 ms (o número do Jev) seria invisível contra um `timeout` de 2 min.

### 8.4 `loop_outer_arm.py` — `is_default_work`: **aposentada por decisão do Gabriel**

**[F] A função existe em `:117` e está intacta** (`loop_outer_arm.py`, 324 linhas):

```python
def is_default_work(prompt: str) -> bool:
    """True when the prompt reads as a substantive engineering request. …"""
    text = prompt or ""
    if os.environ.get("TOURING_WORK_OUTER_DISABLED") == "1": return False
    if len(text.split()) < MIN_WORK_WORDS: return False
    return bool(_WORK_VERB.search(text))
```

**[F] Mas `detect_flow` (`:132+`) registra que ela NÃO decide mais nada** —
comentário literal do código:

> *"Opção D (Gabriel, 03/09/2026) retired the text-heuristic arm. `is_default_work`
> above is kept — it still documents the old contract and its tests still pin the
> behaviour — but `work-outer` is no longer armed from prompt wording. Measured on
> the session that made this change, the heuristic scored **0/2** on the turns that
> mattered:*
> *`"estou achando que a estratégia de escrever um .md…"` (a QUESTION) → armed;*
> *`"Execute D5 e D6 …"` (the turn that wrote ~10 files) → silent.*
> *Both errors are structural, not tuning: a prompt is a CLAIM about the work…"*

> ### ⚠️ DIVERGÊNCIA 2 — `is_default_work` **não é um ponto de inserção**; é uma
> cicatriz de um experimento que já falhou.
>
> A v2 a cita (`:893`) como código a referenciar. **[F]** O código está lá, mas
> **o Gabriel já o desligou com medição** porque a heurística de texto armava em
> pergunta e calava na ordem de trabalho real — 0/2 no turno que importava.
> **[I] (0,90)** Isto é **evidência a favor do Jev, e forte**: o motivo de desligar
> foi "um prompt é uma DECLARAÇÃO sobre o trabalho, e heurística sobre a declaração
> erra estruturalmente" — literalmente o caso d'uso do System One, que julga o
> **estado**, não a frase. **[I] (0,80)** O plano deve citar `detect_flow` e o
> comentário da Opção D como **prova de que a decisão tipada foi tentada por outra
> via e reprovada por medição** — é o argumento mais forte que existe contra
> "mais um classificador de prompt", e a favor de "julgar o estado".

### 8.5 `loop_snapshot.py` — o que o PreCompact grava hoje (candidato P6)

`loop_snapshot.py`, 125 linhas. **[F] Dois ramos, por fase do loop:**

**Ramo OUTER** (`:40-68`) — `snapshot_outer`: roda `loop_outer_gate.py --json
--no-emit` em subprocesso, tira `missing` e `next_action` do relatório, e grava
**uma linha de memória**:

```python
key = state_key(marker)      # determinístico (REGRA #17) — nunca abs(hash(cwd))
snap = (f"OUTER flow={marker.get('flow')} cwd={cwd} missing={missing} "
        f"next={nxt} bundle={marker.get('bundle')} snapshot_at={ts}")
subprocess.run(["touring", "memory", "store", key, snap, "--tier", "semantic", "--type", "reference"], …)
```

**Ramo com DAG** (`:88-120`) — grava `loop-state pending=[…8 primeiros…] bundle=… scope=…
Resume: touring decompose ready {task}` e **acrescenta** uma seção `## {ts} — PreCompact
snapshot` no `log.md` do bundle.

> **[F] O `except Exception: pass` aparece nos dois ramos**, com o comentário
> *"snapshot must never block compaction"*.
> **[I] (0,85)** Para o P6 (compaction-assist), o **fato bruto** é que o PreCompact
> hoje grava **estado de artefato pendente e subtasks**, e nada sobre **o que se
> perde**: o que foi decidido, o que foi tentado e falhado, o que está em aberto na
> prosa. É a definição de uma *decisão tipada de triagem de contexto* — "o que
> sobrevive à compactação" é uma `Choice` sobre candidatos, com o estado (ledger de
> artefatos, subtasks, `next_action`) como entrada, exatamente o padrão `{state,
> question}` dos cookbooks. **[I] (0,75)** E o `state_key` determinístico (REGRA #17)
> já dá a chave estável que uma chamada idempotente exigiria.

### 8.6 Contagens reais (P8 skill-picker, e o volume de specs ADW)

**[F]**

| Medida | n | Comando |
|---|---|---|
| Specs ADW no projeto | **25** `.toml` | `ls .touring/adw/*.toml \| wc -l` |
| (inclui `fragments/`, os blocos reutilizáveis) | **27** | `find .touring -name "*.toml" -path "*adw*"` |
| **Skills globais** | **187** entradas em `~/.claude/skills/` | `ls -1 ~/.claude/skills/ \| wc -l` |
| …das quais são diretórios reais com `SKILL.md` | **84** | `find ~/.claude/skills/ -maxdepth 2 -name SKILL.md` |
| …das quais são **symlinks** (espelho de outro repo) | **102** | `find ~/.claude/skills/ -maxdepth 1 -type l` |
| Specs na biblioteca `adw from-template` instalada | **0** | `find ~/.claude -path "*adw*library*" -name "*.toml"` |

**[F]** Os 25 specs (lista): `adw-curation, bugfix, chore, code-mode-adherence,
converge-close, cross-audit, error-teach, exercise-idle-infra, explore-plan,
family-fix, feature, freshness-audit, guard-sweep, herdr-fanout-demo,
instrument-first, memory-curation, omarchy-audit-2, plan-excellence, release-gate,
scout-perpetuo, skill-refine, strategy-loop, xaudit-gates, xaudit-lintscan, zte-probe`.

> **[I] (0,90) O P8 tem dois catálogos com formas diferentes, e a diferença importa.**
> O de ADW é **25 opções fechadas e curtas** (nomes + `[purpose].when_not_to_use`) —
> forma ideal para `Choice` tipada, e a própria estratégia já exige o
> `when_not_to_use` para o portfólio *descartar* em vez de recomendar o mais próximo.
> O de skills é **187 nomes soltos sem descrição estruturada** — `Choice` sobre 187
> opções é precisely o caso que os cookbooks de `progressive disclosure` tratam
> (top-3 com texto completo, janela de linhas acima de 255). **[I] (0,85)** Se o
> skill-picker virar `Choice`, ele precisa antes de um **catálogo com
> `when_not_to_use`** por skill; sem isso o modelo tem 187 opções e nenhum sinal de
> qual é o "não usar" — que é exatamente a informação que a estratégia do portfólio
> já mostrou ser decisiva.
> **[F]** A contagem de 102 symlinks entre 187 confirma a memória do acervo
> (`skills-globais-sao-symlinks`) — **[I] (0,80)** o "censo" de skills tem 54% de
> entradas que são o *mesmo* arquivo por dois caminhos; um picker que conte por
> entrada sem desduplicar mede a coisa errada.

### 8.7 Item 7 (W2.0, split congelado) — já provado, ver Apêndice A

**[F]** Não refiz a busca. O resultado está no **Apêndice A** (seção acima), com
ausência provada em 5 frentes: nenhuma chave de partição nos 6 `.jsonl` de
`~/.claude/touring/g1-corpus/`; `g1_sample.py` e `g1_label.py` sem partição
(só `str.split`); `g1_review_sheet.py` estratifica por **confiança**, não por item; a
única menção a split no plano 22/09 é a própria linha do gate (`:622`); nada em
`.remember/`, no plano 23/09 ou no git. **[I] (0,85)** O que falta para o G2 é um
recorte nomeado de **teste** (fazível por `id % 3`, seed 20260922 registrado), não
um split de três — o split de treino só é exigido pelo G3 (fine-tune) e precisa
existir **antes** do fine-tune.

---

## 7. Fontes

- `docs/plans/2026-09-22-jev-system-one/strategy-2026-09-22-jev-system-one.md` (893 linhas) — `:621-628` gates, `:833-835` a lição de volume, `:829-832` o gotcha do Landlock
- `docs/plans/2026-09-22-jev-system-one/experiments/g1-corpus/report.md` — corpus, concordâncias, o 21,3% de recall útil, os 65,4% de concordância de tag
- `docs/plans/2026-09-23-jev-touring-otimizacao/` — N1-N6 entregues
- `~/.claude/projects/**/*.jsonl` (1.639 transcripts) — contagem de prompts humanos
- `~/.claude/touring/{memory,knowledge,sdk_signal_mirror}` e `projects/touring/.claude/touring/*` — volumes
- `touring --version` 30.4.87 · `touring kpi -j` (12 famílias) · `touring gate-metrics -j` (196 chaves)
- `git log --oneline --since=2026-09-23` (153 commits)
