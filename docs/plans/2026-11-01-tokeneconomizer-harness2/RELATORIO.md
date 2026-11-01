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
8. **Rodada 2:** os números do TokenEconomizer valem menos do que parecem (gate de disparo em parte vácuo, fato validado na
   janela que o propôs, envelope do guard com o rótulo do avaliador, nenhum IC); e o harness2 revelou um defeito NOSSO — a
   retomada do ADW reaproveita artefato de nó editado (B8). Também: headless sem MCP alheio e sem sessão gravada (B9) e
   crédito de memória que não aprende com run confundido (B10).

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

## Rodada 2 — exploração exaustiva (01/11, 4 fatias, todos os arquivos rastreados)

Quatro extrações por fatia, sem sobreposição: núcleo e avaliação do TokenEconomizer; núcleo e
periferia do harness2. As afirmações que sustentam decisão foram reverificadas por leitura direta.

### O que muda a confiança nos números do TokenEconomizer (FATO 1.0, verificado)

| Defeito de método | Onde | Consequência para nós |
|---|---|---|
| Gate de disparo vácuo: `fired` cai para `scope`, logo "disparou" = "foi exercitado" e a taxa é 1,0 por construção | `tokeneconomizer/microskills/distiller.py:292`; `microskills/envfacts.py:508` (`fired = exercised`) | O FIRE_GATE do ADR-002 deve medir SEGUIMENTO real (o nosso `injection_follow_ratio` já mede), nunca a mera ocorrência |
| Fato validado na mesma janela que o propôs (sem hold-out) | `microskills/distiller.py:663-670` (`_build_fact`) | B1 calibra e confirma com hold-out temporal |
| Envelope do loop guard usa o rótulo do avaliador no benchmark e um proxy em produção | `experiments/te_bench/spiral_stage.py:100-106` × `tokeneconomizer/loopguard/turn.py:255-266` | O ganho declarado do guard é otimista; B4 fase 0 rotula à mão antes de qualquer número |
| Sem IC nem teste estatístico; seeds só medem não-determinismo; `usage_incomplete` excluído de modo inconsistente; `eval_timeout` conta como miss | `experiments/scripts/compare_arms.py:84-114`; `te_bench/mechanism_arms.py:232-235`; `te_harness/swebench.py:1510-1518` | Os −18,4%..−30,9% são ESTIMATIVA PONTUAL sem incerteza (INFERÊNCIA 0,7); o nosso pré-registro com IC continua obrigatório |
| CI não roda `experiments/tests` | `.github/workflows/tests.yml:33` | — |

### Recursos novos aproveitáveis

| Mecanismo externo (arq:linha) | Análogo nosso (arq:linha ou ausente + busca) | Verbo | Item |
|---|---|---|---|
| Side-call sem MCP: `--strict-mcp-config` + `--mcp-config` vazio e `--no-session-persistence` (570k → 14k tokens efetivos, medido pelo autor com `--tools ""`) (`tokeneconomizer/plugin/sidecall.py:140-161`) | `_claude_cmd` em `client/skills/Touring/scripts/adw.py` monta `claude -p` herdando todo MCP; **ausente** em todo o repo (busca `strict-mcp-config\|no-session-persistence`) | INSPIRAR | B9 |
| Mudança de superfície só no fim de sessão e à espera de cache frio (`plugin/fold.py:43-52,312-341`; "a list applies from a later session") | — (restrição de desenho) | REFINAR | regra em B2/B3 |
| Preço desconhecido FALHA em vez de chutar (`tokeneconomizer/pricing.py:135-160`) | — | REFINAR | regra em B5 |
| Veredito de efeito com 4 classes: ajudou / atrapalhou / sem efeito / **confundido** (`harness2/core/prompts.py:1105-1160`) | `memory credit --reward <num>` credita todo caso pendente com o mesmo número (`adw.py:2382`) | INSPIRAR + POTENCIALIZAR | B10 |
| Retomada revalida artefatos e fingerprint, mas a identidade do sweep não tem a versão do prompt (`harness2/core/schedule.py:350-421`; `cli/run_sweep.py:209-235`) | **O mesmo furo no nosso**: `exec_key = f"{node.name}#{visita}"` (`adw.py:2443`) e `run_started` sem digest da spec (`adw.py:2406`) — `--resume-run` reaproveita artefato de nó editado | CORRIGIR | B8 |
| Cache de sessão por hash de missão + conteúdo (`harness2/core/proposer.py:507-534`) | — | INSPIRAR | junto com B8 |

### Defeitos do harness2 que servem de checklist (já cobertos por doutrina nossa)

Assimetria seletor×juiz — o seletor lê 60k, o juiz lê inteiro (`benchmarks/lab.py:330-339`) → nossa lição
"verificador usa menos que o extrator"; "sem saída" tratado de 3 formas (`jobbench.py:1044-1068`,
`lab.py:803`, `workbuddy.py:991-1004`) → Lei L2 "sinal ausente ≠ zero"; quarentena com descarte silencioso
(`benchmarks/wb_render.py:94-135`) → "falhe loud"; rollout cortado pelo provedor pontuado como completo
(`workbuddy.py:991-992`); split que ignora tarefas novas e manda domínio de 1 tarefa para o teste
(`benchmarks/tasks.py:88`, `scripts/split_tasks.py:157`) → cuidado na calibração temporal de B2; Codex
rodando com `--dangerously-bypass-approvals-and-sandbox` (`substrate/codex.py:149-174`) → nosso CEG é a
contenção que eles não têm.

## Rodada 3 — google/elephant-goldfish-model@403d8881d620 (Apache-2.0), pedido do Gabriel

**O modelo (FATO, `docs/designs/architecture.md:41-45`, `docs/designs/review.md:27-38`):** o *elefante* é o agente de
contexto profundo que redige; o *goldfish* é um revisor novo, sem histórico, que lê só o alvo, as referências e o
padrão. O humano decide, o goldfish acha (Blocker/Major/Nit), um sintetizador deduplica e o elefante faz a triagem.
Motivo declarado: viés de confirmação e alucinação mascarada. **Sem medição de eficácia** no repositório
(`docs/faq.md:39-46` admite; o "eval runner" citado em `tests.yml:16` não existe).

**O que é enforcement de verdade (host/build) e o que é prosa:** o isolamento do revisor é do host —
`tools: [Read, Grep, Glob]` + `omitClaudeMd: true` no frontmatter (`.claude-plugin/agents/*.md:5-8`; o campo existe
no Claude Code 2.1.316, 22 ocorrências no binário) e `disable-model-invocation: true` nas skills
(`skills/*/SKILL.md:4`). Convergência por "rodada limpa", cardinalidade de perguntas e o gate de >15 diretórios são
prosa decidida pelo modelo (`egm-review-loop/SKILL.md:151-171`) — é aí que o ADW pode ser mais forte, com o runner
como árbitro (Lei L2).

| Mecanismo EGM (arq:linha) | Lacuna no ADW | Campo/ação | Verbo |
|---|---|---|---|
| Revisor goldfish só-leitura sem CLAUDE.md (`.claude-plugin/agents/*.md:5-8`) | crítico isolado só por `blind_to` | `persona.isolation = "goldfish"` (E1) | APROVEITAR ideia |
| "Revisor que não leu não revisou" (`docs/claude.md:61-62`) | nada prova a leitura | `must_read` com recibo sha256 verificado pelo runner (E2) | INSPIRAR (mais forte que o original, que é prosa) |
| Achados com severidade/escopo/motivo (`agents/synthesizer.md:42-62`) | só PASS/FAIL | `findings_contract`, `max_major` (E3) | INSPIRAR |
| Convergência com churn, teto 5 (`skills/egm-review-loop/SKILL.md:151-173`) | loop sem churn | `churn_limit` (E4) — árbitro é o runner | INSPIRAR |
| Triagem CONTEST/ESCALATE/FIX/DECLINE (`skills/egm-resolve/SKILL.md:56-82`) | autor não responde achado a achado | `triage_of` (E5) | INSPIRAR |
| Escolha única com zero ferramentas (`skills/egm-resolve/SKILL.md:132-134`) | — | `tools = "none"` (E6) | INSPIRAR |
| Design doc: Goals/Non-Goals/Open Questions/Verification Plan (`skills/egm-draft/references/design_doc.md`) | `[purpose]` com 3 campos | `goals`, `non_goals`, `open_questions`, `verification` (E7) | APERFEIÇOAR |
| Retorno tipado de worker (`skills/egm-bootstrap/references/worker-return.schema.json`) | saída de agente em prosa | `output_schema` validado pelo runner (E8) | INSPIRAR |
| `disable-model-invocation: true` (`skills/*/SKILL.md:4`) | 327 skills anunciadas, 7 usadas (F4d) | marcar as skills só-manuais (B12) | POTENCIALIZAR |
| Sintetizador devolve só contagem + link (`docs/designs/synthesizer.md:104-110`) | hooks injetam conteúdo inteiro | padrão "contagem + ponteiro" nas famílias de injeção (dentro de B2) | INSPIRAR |

**O que temos e o EGM não:** retomada durável com identidade de conteúdo (B8), orçamento por nó, journal fsync,
verificação dentro do loop (VGP, gates P0) e veredito por código. O EGM declara convergência e leitura em prosa.

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
