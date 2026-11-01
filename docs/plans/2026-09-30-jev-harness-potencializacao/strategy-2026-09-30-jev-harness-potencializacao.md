---
type: Strategy
title: Jev × Touring — potencialização do harness com decisões tipadas (v3)
description: Com a chave TypeSafe criada (27/09) mas nunca carregada por processo, o passo racional é LIGAR → MEDIR → POTENCIALIZAR. Re-ranqueia os pontos de decisão com volumes frescos, incorpora os padrões que o ecossistema validou em 8 dias (30 fontes NotebookLM, docs oficiais relidas) e estende o plano G0–G5 com a instrumentação de decisão que o G4 precisa para ser lido.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [strategy, jev, typesafe, system-one, harness, intent, recall, skill-picker, professor-aluno]
timestamp: 2026-09-30T23:20:00-03:00
version: "3.0"
okf_version: "0.1"
---

# Jev × Touring — potencialização do harness (estratégia v3)

Parte do [bundle](/index.md). Sucede a v2 ([22/09](/../2026-09-22-jev-system-one/strategy-2026-09-22-jev-system-one.md))
e a wave N1–N6 ([23/09](/../2026-09-23-jev-touring-otimizacao/strategy-2026-09-23-jev-touring-otimizacao.md)).

Convenção: **[F]** fato medido/lido na fonte citada · **[I]** inferência (confiança) · **[E]** estimativa derivada.

Fontes desta rodada: NotebookLM JEV **exaustivo** (5 consultas sobre as 30 fontes — [research/](/research/)),
documentação oficial relida hoje (`api.md`, `cookbooks.md`, `models.md`, `sdk/python/changelog.md`),
Context7 (endpoint público; MCP aguarda OAuth do Gabriel), varredura interna do harness por sessão
colaboradora ([harness-scan-t02.md](/harness-scan-t02.md)) e o OUTER determinístico
(strategy-loop, 4 rodadas de explore, 36 findings — [diagnostics/](/diagnostics/)).

---

## 1. Veredito em uma tela

- **A chave existe e ninguém a vê.** `~/.config/typesafe/env` foi criado em 27/09 16:28, tem
  `TYPESAFE_API_KEY=` e **nenhum** rc/shell/daemon o carrega (0 variáveis `TYPESAFE*` no daemon vivo;
  `typesafe-sdk` não instalado; zero clientes no repositório). O R6 de 22/09 está resolvido no papel
  e aberto no fato. [F — harness-scan §1]
- **O plano G0–G5 segue certo; o que faltava era a torneira.** G0 ✅ (30.4.64), G1 ✅ (corpus rotulado:
  800 prompts + 486 memórias + 300 pares; concordância modelo×modelo 0,963/1,000/0,900; spot-check
  humano do Gabriel pendente). **G2 é o primeiro portão da chave e está por fazer.** [F]
- **Os volumes frescos confirmam o ranking e corrigem dois enquadramentos** [F — harness-scan §3]:
  1. **Intent/CILA é o #1** (191 prompts humanos/dia; a classe majoritária *analysis* é a que o
     keyword mais erra — os dois pedidos de pesquisa profunda de 22/09 saíram GENERAL);
  2. **Factory fica adiado de vez** (10 tickets/7d — volume não paga integração);
  3. **Recall: medir antes de otimizar** — o mirror SDK registra 5 recalls/dia enquanto 2.316
     memórias/dia são tocadas: o instrumento cobre o canal errado, e otimizar o que não se mede
     é a lição `instrumento-que-mente` já paga 5× neste acervo;
  4. **Tags: reframe** — 0 memórias sem tag; o alvo real é **corrigir** as ~34,6% em que o
     professor discorda da tag automática (≈ 6.800 de 19.815), não "preencher ausentes".
- **O ecossistema validou os padrões em 8 dias** (30 fontes vs 7 em 22/09): gateway/proxy mediu
  −41% a −57% de output tokens em bugfix; Skill Picker reduziu skill errada 17% → 7,3% (catálogo
  de 182); um retreino público do Laya com ~3k exemplos sintéticos em 32 min numa T4 atingiu 100%
  no domínio (Jev: 99,9%) — a rota professor→aluno deixou de ser hipótese. [F das fontes — research/]
- **Três trilhos, nesta ordem, cada um fechando por artefato medido:** **W0–W1 LIGAR**,
  **W2–W3 MEDIR**, **W4–W5 POTENCIALIZAR**. Nenhum modelo antes de instrumento.

---

## 2. Delta desta rodada (o que 22/09 não sabia)

| # | Delta | Consequência |
|---|---|---|
| D1 | Chave criada 27/09, **nunca carregada** por processo [F] | W0 vira o gate zero: adapter lê o arquivo ele mesmo — sem restart de daemon, sem tocar rc |
| D2 | Rate limit agora **100K tok/s + 40 req/s** (era 250K/20) [F — models.md] | Lotes de backfill/corpus ~2× mais rápidos |
| D3 | SDK Python **v0.7.2** (26/09): extra `http2`; sem breaking desde 0.7.0 [F — changelog] | Pin atualizado: `typesafe-sdk==0.7.2` + hash (R12 segue) |
| D4 | Modelo segue `jev-1.13.0`; aliases estáveis; 19 cookbooks sem novos [F] | Nada a re-aprender; thresholds calibrados continuam válidos |
| D5 | **Skill Picker** provado em produção alheia: erro 17% → 7,3%, cargas desnecessárias 9,8% → 4,0%; thresholds publicados (prob ≥ 0,50 + conf ≥ 0,60, opção "nenhuma") [F] | Candidato novo P8 — Touring tem ~180 skills e 75% dos gatilhos fora da description (medido 12/09) |
| D6 | **Gateway benchmarks** (5 tarefas reais): bugfix −41…57% output tokens, −25…39% tempo; features +14…27% [F] | Confirma o N5 já entregue ([purpose] class); estende a justificativa para rotear **tier de modelo** nos ADWs headless |
| D7 | **Laya retreinado por terceiro**: 3k sintéticos, 32 min, T4 15 GB → 100% no domínio; base crua fraca (36,9–72%) [F] | G3 des-riscado; o risco R15 (8 GB) contornável via Colab/Kaggle |
| D8 | Erro-confiante documentado (MS↔MSNBC): Jev errou com confiança onde pipeline com pré-filtro empatou com GPT-6 Astra a 1/200 do custo [F] | Todo ponto Jev nasce com **pré-filtro determinístico + fallback** — o desenho, não a exceção |
| D9 | Hooks ficaram **calados** ~1 semana (S31 religou); fila do daemon serializa hooks (S32b, ticket aberto) [F] | Latência de hook é variável compartilhada: Jev em hook só assíncrono/advisory, nunca no caminho síncrono |
| D10 | **Zero KPIs vivos** de decisão (intent/recall/factory) nas 12 famílias do `touring kpi` [F] | W3 (instrumentação) é pré-requisito do G4 — sem ele, "taxa de fallback" não tem onde ser lida |

---

## 3. Candidatos re-ranqueados (volume × custo do erro × corpus disponível)

Predicado de encaixe inalterado (5 cláusulas da v2). Ranking novo com dados de 24/09–01/10 [F]:

| # | Ponto | Volume/dia [F] | Corpus pronto | Trilho |
|---|---|---|---|---|
| **P1** | Intent → nível CILA (`prompt_enhance`) | **191** | ✅ 800 prompts (0,963) | W2→W4 |
| **P2** | Relevância de memórias no recall | real desconhecido (instrumento quebrado); 78,7% de ruído medido no G1 | ✅ 300 pares (0,900) | W3 (medir) → W5 |
| **P3** | Correção de tags facetadas | 321 memórias novas/dia; ~6.800 erradas em estoque | ✅ 486 memórias (1,000) | W5 (lote) |
| **P8** (novo) | Skill-picker no UserPromptSubmit | 191 prompts | parcial (catálogo existe) | W5 (experimento) |
| **P9** (novo) | Pré-triagem 7-Noul de cross-audit/review | por PR/fase | cookbook replicável | W5 (experimento) |
| P4 | Roteador do factory | 1,1 | ❌ | **adiado** (G5 já previa) |
| P5 | Dedupe scout/memórias | médio | parcial | W5 (lote, junto com P3) |
| P6 | Compaction-assist (PreCompact) | por sessão | ❌ | W5 (explorar) |
| P7 | AskJev-style rerank no `explore`/scout | por consulta | ❌ | W5 (explorar) |

**Anti-usos reafirmados e estendidos** [F para limites do modelo; I (0,85) para o mapeamento]:
CEG/gates de segurança · wiring/imports (compilável) · juiz de convergência (reprodutibilidade) ·
contagem/datas/números · hooks síncronos ~1 ms · geração de texto · **redução de FP em detectores
de segurança** (F2.4 aprendeu na semana passada: FP se trata com predicado determinístico — 4
correções, nenhuma com modelo) · **decisões no caminho síncrono do hook enquanto S32b estiver aberto**.

---

## 4. Plano W0–W5 (cada wave fecha por artefato medido)

### W0 — LIGAR (meio dia; sem modelo, sem daemon)

1. `pip install "typesafe-sdk==0.7.2"` com hash (R12; `typesafe-client` no PyPI é placeholder de terceiro).
2. Adapter fino `scripts/jev_client.py` (ou `crates/` se chegar ao daemon): lê
   `~/.config/typesafe/env` **ele mesmo** (fallback `TYPESAFE_API_KEY` do ambiente), cache por hash
   de (state, perguntas, modelo), journal JSONL (`request_id`, modelo pinado, decisão,
   probabilities, confidence, latência, tokens), timeout curto, **fail-open sempre**.
3. Smoke test: 1 Noul ("Is `touring` a Rust workspace?" sobre 3 linhas de state) — custo [E] < US$ 0,0001.
4. Guard: o adapter nunca vaza a chave em log (SDK 0.7.1+ já a tira das exceções; debug log não redige corpos).

**Gate W0:** smoke test verde + journal gravado + `touring memory store` do custo/latência medidos.

### W1 — primeira decisão real (meio dia; prova de vida end-to-end)

Re-implementar **uma** decisão viva atrás de flag default-OFF, escolhida pelo corpus pronto:
o **backfill de tags de UMA faceta** (`kind`) sobre amostra de 100 memórias — lote, fora de caminho
crítico, reversible (`derived_by=jev-1.13.0`), e diretamente comparável ao rótulo G1 (concordância 1,000).
**Gate W1:** curva confidence×acurácia da amostra + decisão go/no-go do Gabriel para o W2.

### W2 — MEDIR: G2 baselines (1–2 dias)

No split congelado do G1 (**reconstruir com seed registrada se o artefato não for achado** — W2.0):
keyword × Laya zero-shot × professor (Claude via `system-one-adapter`) × **Jev** (só dados não
sensíveis — touring é repo público; analise fica de fora). Saída: tabela acurácia/ECE/p99 por decisão.
**Gate G2 (inalterado):** Jev ≥ haiku com ≥ 90% acima do threshold de automação, ou a trilha hospedada
morre aqui e o aluno fica com o professor Claude.

### W3 — MEDIR: instrumentação de decisão (1 dia; pré-requisito do G4)

Hoje `touring kpi` não tem família nenhuma de decisão [F — harness-scan §4.3]. Antes de ligar
qualquer flag: emitir `intent_distribution`, `keyword_fallback_rate`, `recall_curated_share`,
`jev_calls_total/latency/confidence_hist` (este último já nasce no adapter W0). Sem isso, o gate G4
("taxa de fallback, acurácia amostrada em produção") **não tem onde ser lido** — é a lição
`kpi-fonte-declarada-sem-resolvedor` pela terceira vez.
**Gate W3:** as 4 métricas vivas em `touring kpi -j` por 3 dias de tráfego real.

### W4 — POTENCIALIZAR: aluno + intent→CILA (2–4 dias)

- **G3**: professor (Jev no não-sensível + Claude no sensível) rotula → fine-tune do
  laya-multilingual (RTX 4060 com gradient checkpointing; Colab/Kaggle como plano B — D7
  des-riscou). Temperatura por (tipo, nº de opções) em held-out.
- **G4**: intent→CILA atrás de flag, aluno decide ≥ τ, keyword como fallback, journal por decisão.
**Gates:** aluno ≥ keyword e ≥ 0,85× professor, ECE ≤ 0,10, p99 < 30 ms; em produção: taxa de
fallback e acurácia amostrada lidas nas métricas do W3, zero regressão nos testes do hook.

### W5 — POTENCIALIZAR: portfólio de experimentos (paralelo, cada um atrás de flag)

Ordem por (valor esperado × prova externa):

1. **P8 Skill-picker** — cascata de 2 estágios do cookbook (182→top-3→rejulgado), gate de silêncio,
   override manual `!skill`. Alvo: 75% dos gatilhos hoje fora da description.
2. **P3 Correção de tags em lote** — Choice por faceta, grava só conf ≥ τ com `derived_by`,
   amostra auditada à mão. Estoque: ~6.800. Custo [E] ≈ US$ 0,7.
3. **P9 Pré-triagem de review** — 7 Nouls sobre o diff antes do cross-audit completo (o funil
   que a Sentry mediu > 5× mais barato). Mapeia sobre o INNER 13 do loop-engineering como
   **advisory** — nunca cláusula do juiz.
4. **P2 Relevância no recall** — Noul por candidato na shortlist (candidata dentro das
   instructions, padrão anti-context-rot), depois do instrumento W3 medir o recall real.
5. **P5 Dedupe** — Noul por par candidato; sinal, não verdade.
6. **P6/P7 exploração** — compaction-assist e rerank de arquivos, só se W2/W4 provarem a trilha.

---

## 5. Decisões que só Gabriel pode tomar (HUMAN GATE)

> **Estado real em 02/10/2026** (verificação de findings). O gate foi **aprovado e
> executado**, mas este § nunca foi marcado — ver `FINAL-REPORT.md:102-112`.
> **1, 2 e 4: EXECUTADOS · 3: ABERTO.**

1. **W0 autorizado?** — ✅ **EXECUTADO**. SDK pinado instalado (0.7.2 verificado no PyPI),
   `scripts/jev_client.py` criado, smoke test rodado com < US$ 0,0001 (1ª chamada real da
   API: 5/5 checks, 395,9 ms, US$ 0,0000167, jev-1.13.0). Sem edição em daemon, rc ou settings.
2. **Política de dados confirmada?** — ✅ **EXECUTADO**. touring (público) → Jev liberado;
   analise (processos ANTT) → professor Claude apenas; nada sensível sai sem aditivo LGPD
   (R10 segue inteiro). **Astrumentado**: o corpus G1 passa a ser lido do banco do **projeto**
   (`--db`) — o `memory.db` global é disjunto do corpus.
3. **Context7**: — 🔴 **ABERTO (nunca autorizado)**. O endpoint público já cobriu o essencial
   desta rodada, então nada travou; a busca de bibliotecas segue inacessível. Autorizar em
   https://clerk.context7.com/oauth/authorize?response_type=code&client_id=BI4KjV5lrwjpyMii&code_challenge=ZzFcmkFK0FxQa_9FbgXKisgKgMeXfNeCoJXguUGQaGQ&code_challenge_method=S256&redirect_uri=http%3A%2F%2Flocalhost%3A65085%2Fcallback&state=1W89XPwTNrhTfdIanOxQM9vAXO4-eHbI-U1CtIAlNkA&scope=profile+email+offline_access&prompt=consent&resource=https%3A%2F%2Fmcp.context7.com%2F
4. **Sequência**: — ✅ **SEGUIDA (W0→W1→W2)**, como recomendado. A alternativa (pular para
   W4 sem W3) foi descartada: G4 sem W3 é flag sem réguas, e a medição veio primeiro.

## 6. Riscos herdados (v2) — status

R1 pt-BR (instruções em inglês, medir) · R2 dados (resolvido por projeto na decisão 2) ·
R3 maturidade SDK (pin 0.7.2) · R4 calibração de grupo (thresholds por pergunta) ·
R6 ~~sem chave~~ → **chave existe; W0 a liga** · R10 LGPD (inalterado) ·
R12 supply chain (inalterado) · R13–R15 Laya (D7 mitigou R15).
Novos: **R16** — erro-confiante (D8): todo ponto nasce com pré-filtro + fallback + revisão por
amostra. **R17** — hooks serializados (D9/S32b): Jev nunca no caminho síncrono até a fila ser
endereçada.

## Cadeia causal (delta v3)

1. Se a chave existe mas nenhum processo a vê, o primeiro trabalho é **wiring, não modelo** (W0).
2. Se o instrumento de recall cobre o canal errado, otimizar o recall agora é otimizar no escuro
   — **medir antes** (W3) é o pré-requisito, não o luxo.
3. Se o aluno local já foi provado por terceiro no domínio dele (D7), o risco do G3 migrou de
   "será que funciona" para "executar com nossos dados".
4. Se 191 prompts/dia erram a classe majoritária por keyword, o ganho esperado de intent→CILA
   supera qualquer outro ponto por ordem de grandeza — **P1 primeiro, sempre**.
5. Se cada wave fecha por artefato medido e flag default-OFF, o custo de estar errado sobre o
   Jev inteiro é o preço do W2 — e mais nada.
