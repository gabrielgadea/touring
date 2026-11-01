---
type: Report
title: "TokenEconomizer × harness2 — o que o modus operandi do Claude Code + Touring deve adotar, adaptar ou corrigir"
description: "Análise offline com fontes fixadas por SHA, mapeamento mecanismo a mecanismo contra 7 subsistemas, 4 contrafactuais sobre 250 transcritos, 7 ADRs e backlog pré-registrado. Nada implementado: aguarda decisão do Gabriel."
timestamp: 2026-11-01T18:45:00-03:00
---

# TokenEconomizer × harness2 → modus operandi

**Fontes:** `microsoft/TokenEconomizer@c53a4a6dcdeb` (arXiv 2610.22732) e `google-research/harness2@1cfe254e8d25`
(arXiv 2610.14224), ambos Apache-2.0, clonados em `~/.claude/touring/ext/` e só LIDOS. Medição 100% offline
(AB34 armado até 08/11 00:10). Scripts e agregados: `medicao/`. Decisões: `adr/`. Plano: `BACKLOG.md`.

## Sumário executivo

1. **Lições:** o minerador guarda 9.367 lições, 90-94% delas vistas numa sessão só, e em 250 sessões entregou o bloco
   `Past failure` em 2. O TokenEconomizer guarda pouco, só o confirmado por 3 tarefas, e retira por contradição. → ADR-001.
2. **Seguimento sem consequência:** medimos `injection_follow_ratio` (0,598) e nunca o usamos para cortar a família que
   ninguém segue; o FIRE_GATE deles é isso. → ADR-002 (POTENCIALIZAR).
3. **Superfície morta:** 327 skills anunciadas, 7 usadas (1,93 MB de anúncio em 250 sessões); 28 agentes, 9 usados. A
   regra de poda deles erraria 18 vezes nos agentes — adotar a ideia, não a regra. → ADR-003.
4. **Espirais:** 45 de 47 turnos que o detector de trajetória deles marcaria já tinham recebido um deny nosso; os gates por
   chamada disparam e a trajetória segue (teto: 19,5% das chamadas depois do ponto de detecção). → ADR-004, em sombra.
5. **Custo:** medimos bytes, não tokens com cache; nenhuma das decisões acima tem desfecho em custo sem isso. → ADR-005,
   primeiro item do backlog.
6. **Método (harness2):** ablação por componente e ledger que reprova se as execuções não fecham com o plano. → ADR-006.
7. **Não instalar o plugin:** escreve no `settings.local.json`, faz side-calls com haiku (o paper usou o modelo do agente) e
   foi avaliado só em SWE-Bench. → ADR-007.

## Linha de base (método e denominador)

- `touring kpi -j` → `context_budget`, 1 transcrito (o KPI limita a varredura): 6.039 B injetados por turno (teto 2.000);
  seguimento 0,598 (piso 0,6, n=502); duplicação 0,039.
- Corpus da análise: 250 transcritos de sessão principal do projeto, 02/10–01/11/2026, 976 MB (subagentes fora).

## Matriz — TokenEconomizer

| Mecanismo externo (arq:linha) | Análogo Touring/CC (arq:linha ou ausente + busca) | Delta | Verbo | Efeito esperado · métrica · como medir | Risco / reversibilidade | Tam. | Conf. |
|---|---|---|---|---|---|---|---|
| Micro-skills: candidato→ativo com K=3, γ=0,8, λ=5, hard-kill, FIRE_GATE 0,40/4 (`microskills/lifecycle.py:15-21`); teto 4 fatos/150 tokens (`config.py:148-151`) | `ingest/transcript_miner.rs:421,499-506` (1 episódio = lição; recorrência descartada como duplicata); injeção `cli_suggester.rs:2063`. Retirada: **ausente** (busca `contradict\|superseded_by\|invalidate_lesson\|stale\|expire`) | Eles confirmam e retiram; nós acumulamos sem contar sessões. F4a: 94% sessão única | INSPIRAR + CORRIGIR | Seguimento da família `past-lessons` ≥ 0,6; A/B por dia (B1) | Baixo; sombra possível | M | 0,8 |
| FIRE_GATE: injetar só o que é seguido (`lifecycle.py:20-21`) | `injection_follow_ratio` e `by_family` no `touring kpi`; níveis em `client/hooks/injection_policy.py` — **sem ligação** | Medimos e não decidimos | POTENCIALIZAR | Tokens de injeção/turno −20% com guarda de seguimento (B2) | Baixo; kill = `full` | M | 0,75 |
| Poda de tools p=10/m=2/s=5, alto risco ×3 (`plugin/fold.py:97-106`, `config.py:153-155`) → deny list no `settings.local.json` (`plugin/denylist.py`) | Fachada MCP estática `server/mod.rs:73` (`CURATED_TOOLS`) + descoberta `touring_search`; `client/hooks/skill_picker_hook.py` sugere skills. Poda por uso: **ausente** | Curadoria à mão × evidência; F4d: 7/327 skills usadas, regra deles erra 18× em agentes | INSPIRAR (relatório antes de poda) | Custo de anúncio/sessão (B5) e podas erradas ≤ 1/50 sessões (B3) | Médio (item necessário podado); recuperação por busca | S→M | 0,7 |
| Loop guard: conjunção ≥20 turnos, churn, ≥6 erros, ≥20% repetidos, envelope p90×1,25 (`loopguard/rules.py:20-43`); juiz ≥0,7 a cada 5→20 chamadas (`turn.py:191-195`); r=1 (`turn.py:334`); backstop 2,5× | G6/G1/G10 por chamada (`cli_suggester.rs`), A14 (`client/rules/code-elaboration-directives.md:24`; `adw.py:1239`), `loop_stop_guard.py` (artefatos). Trajetória só no ADW opt-in (`adw.py:2800-2806`); sessão: **ausente** (busca `spiral\|edit_churn\|stagnation`) | Granularidade: chamada × trajetória. F4c: 45/47 marcados já tinham deny | INSPIRAR + POTENCIALIZAR (com memória) | Fase 0: precisão ≥ 0,7 / recall ≥ 0,6 vs rótulo (B4) | Médio: paper mostra Pass@1 menor no Opus; sombra primeiro | L | 0,65 |
| Ledger/pricing com side-calls no custo (`ledger.py`, `pricing.py`, `plugin/accounting.py`) | `status_monitor.sh:112`, `adw.py:2239` (total); `context_budget` em bytes; `cost-state` no transcrito sem leitor | Sem custo por mecanismo nem overhead próprio | APERFEIÇOAR | Soma por mecanismo = fatura ± 2% (B5) | Nenhum (leitura) | M | 0,85 |
| RepoMap (desligado por padrão, `config.py:162`, `plugin/repomap.py`) | `instructions_loaded.rs:367` (`push_repo_map_parts`) — **já ligado** | Nós temos; eles não ligam | — (Touring melhor) | — | — | — | 0,9 |
| Side-calls de modelo, `haiku` padrão (`plugin/sidecall.py:164`) | Hooks determinísticos, sem modelo no caminho quente; exceção: sugestão de skill tipada (`skill_picker_hook.py`) | O custo deles inclui modelo; o nosso não | — (Touring melhor; ADR-007) | — | — | — | 0,85 |

## Matriz — harness2 (lentes não cobertas pela rodada 4)

| Mecanismo externo (arq:linha) | Análogo Touring/CC | Delta | Verbo | Efeito · métrica | Risco | Tam. | Conf. |
|---|---|---|---|---|---|---|---|
| Superfície editável do harness: 8 componentes (prompt, guardrails, memória, tool guidance, subagentes, skills, scripts, plugins) (`harness2/substrate/opencode.py:48-90`) | Skills/hooks/rules versionados (`client/`), specs ADW (`client/skills/Touring/adw-library/`) | Equivalente; eles otimizam a superfície por busca automática, nós à mão | INSPIRAR (via B6) | — | — | — | 0,7 |
| Ablação por componente (paper Tab. 3 e 6; uma execução por célula, sem n/IC) | AB34 fatorial 2×2 por dia (`docs/plans/2026-10-31-ab34-compactador-eviccao/`); famílias de injeção nunca ablacionadas | Nós temos pré-registro e n; falta ablação por família | INSPIRAR (B6) | Desenho com n por poder | Nenhum | M | 0,8 |
| Ledger de orçamento R=4k/P=3k com `assert_identity` (`core/ledger.py:24-31,89`) | `ab34_arm.py:215` valida a composição dos blocos no PLANO; execução não auditada | Plano × execução | APROVEITAR a ideia (B6) | Teste com célula duplicada/faltante reprova | Nenhum | S | 0,85 |
| Seleção por LLM com IDs anonimizados e semente estável (`core/selection.py:71,268`) | Críticos cegos no ADW (`adw.py:174,1514`); `adw race` (`adw.py:3917`) | Equivalente na revisão; falta anonimizar condições de experimento para juiz | INSPIRAR (B6c) | — | Nenhum | S | 0,7 |
| Reflexão pós-adoção grava lição por domínio (`core/selection.py:402`, `core/library.py:58`) | `memory store` + tags facetadas; `loop_phase_close.py` | Equivalente; ver ADR-001 (o nosso não retira) | (coberto por B1) | — | — | — | 0,75 |
| Retry do estágio com a mensagem de erro, `STAGE_ATTEMPTS=2` (`core/recursion.py:36,108`) | A14 = 3 e a 4ª muda de estratégia (`adw.py:1239`); gate que ENSINA a correção (CLAUDE.md §8b) | Equivalente; o nosso tem regra de mudança de estratégia | — | — | — | — | 0,8 |
| Executor e melhorador em modelos separados (`config.py:174-178`) | `agent_model_gate.py` (matriz busca→haiku, implementação→sonnet) | Equivalente | — | — | — | — | 0,8 |
| Juiz por critério só como medição pós-escolha; nenhum verificador no loop (`core/judge.py:1-5`) | VGP, `loop_converged.py`, gates P0 dentro do loop | **Touring melhor**: verificação no loop | — | — | — | — | 0,85 |
| Paralelo 2k candidatos + k composições (`core/recursion.py:343-450`) | `adw race`, fan-out `parallel` | Equivalente | — | — | — | — | 0,7 |

## Convergência da exploração

`touring explore tokeneconomizer-harness2` convergiu em 9 rodadas (8 e 9 secas; lente `external`
marcada como visitada com as duas fontes): 36 achados, dos quais **22 são auto-eco** do próprio ledger
(defeito registrado como B7). Os achados reais do acervo: a análise do harness2 de 26/10, um harness
SWE-bench nosso de junho (`eval/swe_bench/harness.py`, instrumento para B1-B4) e a lacuna de portfólio
("nenhum artefato conhecido cobre o tema", 11.502 registros varridos). O ledger fica fora do repositório
público (`~/.claude/touring/explore/tokeneconomizer-harness2.json`) porque grava caminhos absolutos.

## O que o Touring faz melhor (candidatos a contribuição)

- **Verificação dentro do loop** (VGP, gates P0, juiz de convergência) — o harness2 só mede depois (`core/judge.py:1-5`).
- **Repo map ligado e determinístico** (`instructions_loaded.rs:367`) — no TokenEconomizer vem desligado.
- **Caminho quente sem modelo**: hooks em Rust/Python determinístico; o TokenEconomizer paga side-calls.
- **Pré-registro com n e regra de decisão** (AB34) — a ablação do harness2 é n=1 por célula, declarado pelos autores.
- **Regra de mudança de estratégia na 4ª tentativa** (A14) — mais forte que o retry fixo de 2.

## Autocrítica — cumpre a intenção (mais barato e mais eficaz, medido)?

- **Cumpre em parte.** Os deltas estão provados nos dois lados com arquivo:linha, e três contrafactuais dão tamanho ao
  problema. **Nenhum número de custo em tokens** foi produzido: bytes não são custo quando o prefixo é cacheado. Por isso
  B5 vem primeiro e nenhuma economia é prometida aqui.
- **Ficou de fora:** (a) apêndices B, E, F do paper do TokenEconomizer e as Tabelas 1-2/Figura 6 do harness2 (o PDF não
  foi lido; os números do harness2 vêm de extração por modelo do HTML — INFERÊNCIA 0,8); (b) o loop interno do
  OpenCode/Codex, que não mora no harness2; (c) contagem de uso por `/comando` digitado (subestima uso de skill);
  (d) envelope de tarefas resolvidas no F4c (sem rótulo offline → teto, não desperdício).
- **Risco de leitura errada:** o 45/47 do F4c pode refletir que turnos longos e produtivos também recebem denies; só o
  rótulo humano da fase 0 de B4 separa as duas leituras.
