---
type: Backlog
title: "Backlog — TokenEconomizer × harness2 aplicados ao modus operandi do Claude Code + Touring"
description: "Seis itens ordenados por (efeito esperado × confiança) ÷ custo, cada um com aceite mensurável e experimento pré-registrado. Nada aqui está aprovado: implementar exige decisão do Gabriel."
timestamp: 2026-11-01T18:30:00-03:00
---

# Backlog (proposta — aguarda aprovação)

Restrição comum: o AB34 está armado até 08/11 00:10. Antes disso só cabe o que é **só leitura e
offline** (B5 e a fase sombra offline de B4). Tudo o que muda hook, settings ou daemon começa depois.

Pontuação = efeito (1-3) × confiança ÷ custo (S=1, M=2, L=3).

| # | Item | Verbo | ADR | Efeito | Conf. | Custo | Pontos |
|---|---|---|---|---:|---:|---|---:|
| B5 | Custo em tokens (com cache) por mecanismo no `touring kpi` | APERFEIÇOAR | 005 | 3 | 0,85 | M | 1,28 |
| B2 | Gate por família pelo seguimento medido | POTENCIALIZAR | 002 | 3 | 0,75 | M | 1,13 |
| B1 | Ciclo de vida das lições mineradas (confirmar, retirar, chave normalizada) | INSPIRAR + CORRIGIR | 001 | 2 | 0,80 | M | 0,80 |
| B6 | Ledger de identidade + ablação leave-one-family-out | INSPIRAR / APROVEITAR (ideia) | 006 | 2 | 0,80 | M | 0,80 |
| B3 | Relatório de uso da superfície (skills, agentes, plugins, tools) | INSPIRAR | 003 | 2 | 0,70 | S | 1,40 → só depois de B5 |
| B4 | Detector de espiral por trajetória, em sombra | INSPIRAR + POTENCIALIZAR | 004 | 2 | 0,65 | L | 0,43 |
| B7 | `touring explore` exclui o próprio ledger da lente antistaleness | CORRIGIR | — | 1 | 0,95 | S | 0,95 |
| B8 | `adw --resume-run` invalida nó cuja definição mudou (digest da spec + hash do nó) | CORRIGIR | — | 2 | 0,9 | S | 1,80 |
| B9 | `claude -p` headless sem MCP alheio e sem gravar sessão | INSPIRAR | 005 | 2 | 0,7 | S | 1,40 |
| B10 | Crédito de memória com veredito de 4 classes (inclui "confundido") | INSPIRAR + POTENCIALIZAR | 001 | 1 | 0,7 | S | 0,70 |

Ordem de execução: **B8 → B7 → B9 → B5 → B3 → B2 → B1 → B10 → B6 → B4** (os consertos pequenos e de
defeito provado primeiro).

Restrições de desenho trazidas pela rodada 2 (valem para os itens indicados):
- **B2/B3:** mudança de nível de injeção ou de superfície só vale a partir da sessão seguinte (o
  TokenEconomizer espera o cache frio: `plugin/fold.py:43-52`); mudar o prefixo no meio da sessão
  quebra o cache e pode custar mais do que poupa.
- **B5:** modelo sem preço conhecido FALHA o cálculo em vez de chutar (`tokeneconomizer/pricing.py:135-160`).
- **B1/B2:** o gate mede seguimento real e com hold-out temporal; a implementação deles é vácua em
  parte (`microskills/distiller.py:292`, `envfacts.py:508`) e valida na janela de proposta
  (`distiller.py:663-670`). B3 pontua alto, mas sem B5 decide em bytes, e
bytes de prefixo cacheado custam ~1/10 (ADR-003).

---

## B5 — Custo por mecanismo (APERFEIÇOAR)

- **Entrega:** seção `context_cost` no `touring kpi -j` com tokens de entrada nova, leitura e escrita
  de cache, atribuídos a famílias de injeção, anúncio de superfície, saída de tools e overhead próprio
  (hooks, subagentes), lidos dos registros de uso do transcrito.
- **Aceite mensurável:** em 10 sessões sorteadas com semente fixa antes do sorteio, soma por mecanismo
  = custo total da sessão (registro `cost-state`) com erro ≤ 2%.
- **Experimento:** nenhum (instrumento). Regra: erro > 2% em qualquer das 10 → reprova e não serve de
  desfecho para B1-B4.

## B3 — Relatório de uso da superfície (INSPIRAR)

- **Entrega:** `touring kpi` → `surface_usage`: por skill, plugin, agente e tool adiada, anunciado ×
  usado (tool `Skill`, `/comando` digitado e `Agent.subagent_type`) × custo (B5).
- **Aceite mensurável:** reproduz o F4d (327/7 skills, 28/9 agentes, 265/23 tools em 250 sessões) com
  o uso por `/comando` incluído, e mostra o custo em tokens do anúncio por sessão.
- **Decisão humana:** o relatório propõe; desligar plugin ou skill é decisão do Gabriel, item a item.
- **Poda automática (fase 2, só se aprovada):** regra calibrada nos nossos dados, com agentes fora da
  poda. Pré-registro: aceitar só se, em replay cronológico, podas erradas ≤ 1 por 50 sessões e
  economia ≥ 30% do custo de anúncio (B5).

## B2 — Gate por família (POTENCIALIZAR)

- **Entrega:** a política de injeção desce uma família de nível (`full` → `brief` → `off`) quando o
  seguimento medido dela fica abaixo de τ com n ≥ N; volta quando a medição em sombra sobe.
- **Calibração (antes do experimento):** τ e N escolhidos no corpus de 250 sessões por validação
  temporal (calibrar em out, testar em nov), declarados aqui antes do teste.
- **Experimento pré-registrado:** A/B por dia, 8 dias, depois do AB34. Desfecho primário: tokens de
  injeção por turno (B5). Guarda: `injection_follow_ratio` global não pode cair mais de 0,05 e a taxa
  de deny repetido (G6) não pode subir. Regra: aceita se o primário cair ≥ 20% com IC-95 por bootstrap
  de sessões abaixo de 0 e as guardas passarem.

## B1 — Ciclo de vida das lições (INSPIRAR + CORRIGIR)

- **CORRIGIR primeiro (teste vermelho):** um teste em `transcript_miner` prova que a mesma falha em 3
  sessões gera UMA lição sem contagem de sessões (hoje), e passa a gerar contagem = 3 (depois).
- **Entrega:** chave normalizada (caminhos, números, hex), contador de sessões distintas, estado
  `candidate|active|retired`, retirada por contradição observável e por recência, injeção só de
  `active` e sob o gate de B2.
- **Aceite mensurável:** no corpus, a normalização reduz as chaves de 530 para ≤ 410 (F4a: 402) e a
  fração ativa fica entre 3% e 10%.
- **Experimento:** A/B por dia, desfecho primário = seguimento da família `past-lessons`; regra: aceita
  se ≥ 0,6 com n ≥ 30 blocos e bytes da família por turno não aumentarem.

## B6 — Ablação e ledger de identidade (INSPIRAR / APROVEITAR ideia)

- **Entrega:** (a) `assert_identity` nos runners de experimento (AB34 e sucessores) — o runner reprova se
  as células executadas não forem exatamente as pré-registradas, checando duplicata antes da contagem;
  (b) desenho leave-one-family-out para as famílias de injeção com n por célula calculado por poder
  antes.
- **Aceite mensurável:** um teste com uma célula duplicada e uma faltando reprova; o desenho declara n
  e o efeito mínimo detectável antes do dado.

## B7 — `touring explore` lê o próprio ledger (CORRIGIR, achado durante esta análise)

- **Defeito (FATO 1.0):** `lens_antistaleness` (`client/skills/Touring/scripts/explore_until_dry.py:274-283`,
  vivo em `~/.claude/skills/Touring/scripts/`) faz grep no escopo inteiro sem excluir o arquivo do
  ledger. Com o ledger dentro do escopo, cada rodada posterior acha as próprias linhas: nesta análise
  as rodadas 6 e 7 deram 11 achados "novos" cada, todos auto-eco (`echoes_suppressed: 0`). O ledger
  também grava caminhos absolutos, o que impede guardá-lo num repositório público.
- **Teste vermelho primeiro:** ledger dentro do escopo → a 2ª rodada não pode ter achado cujo caminho
  seja o próprio ledger.
- **Aceite mensurável:** repetir este tema com o ledger no bundle dá rodadas secas consecutivas sem
  achado do ledger; chaves gravadas relativas ao escopo.
- Custo S. Efeito: o critério de convergência deixa de depender de onde o ledger mora.

## B8 — Retomada do ADW com identidade de conteúdo (CORRIGIR, achado na rodada 2)

- **Defeito (FATO 1.0):** `exec_key = f"{node.name}#{visita}"` (`client/skills/Touring/scripts/adw.py:2443`)
  e o evento `run_started` guarda só `adw` e `run_id` (`adw.py:2406`); `_resume_state` (`adw.py:2336-2350`)
  carrega o artefato de toda `exec_key` concluída. Editar o prompt ou o comando de um nó e rodar
  `--resume-run` reaproveita o artefato da versão antiga como se fosse da nova. O harness2 tem o mesmo
  furo (`cli/run_sweep.py:209-235`).
- **Teste vermelho primeiro:** run interrompido → editar o `prompt` de um nó concluído → `--resume-run` deve
  reexecutar esse nó (hoje pula).
- **Entrega:** digest da spec no `run_started`; hash da definição do nó gravado no `node_completed`; na
  retomada, nó com hash diferente é reexecutado e o journal registra o motivo. Opcional: reaproveitar
  artefato entre runs pelo mesmo hash (cache por conteúdo, como `harness2/core/proposer.py:507-534`).
- **Aceite mensurável:** o teste vermelho passa; um nó não editado continua pulando.

## B9 — Headless sem MCP alheio e sem sessão gravada (INSPIRAR)

- **Fato (1.0):** nenhum `claude -p` do repositório passa `--strict-mcp-config` ou `--no-session-persistence`
  (busca vazia); o TokenEconomizer passa os dois em toda side-call (`tokeneconomizer/plugin/sidecall.py:140-161`).
- **Dois efeitos esperados:** (a) menos tokens de entrada por nó headless — o 570k→14k deles é com
  `--tools ""`, que desliga o ToolSearch; nós usamos `--allowedTools`, que mantém tools MCP adiadas,
  então o ganho é INFERÊNCIA a medir; (b) execuções headless deixam de entrar no histórico do usuário e
  no corpus de transcritos que os KPIs varrem (hoje contaminam as duas coisas).
- **Experimento pré-registrado (custa crédito pequeno, exige aprovação):** o mesmo nó trivial 5× com e
  5× sem as flags; desfecho = tokens de entrada (`usage` do JSON); regra: adotar se a mediana cair ≥ 20%
  e o nó produzir o mesmo artefato. Exceção declarada: nós que precisam de um MCP específico recebem
  `--mcp-config` só com ele.

## B10 — Crédito de memória com veredito de 4 classes (INSPIRAR + POTENCIALIZAR)

- **Hoje (FATO 1.0):** `deposit_run_outcome` credita TODOS os casos pendentes com o mesmo número do run
  (`adw.py:2382`, `memory credit --all-pending --reward`).
- **Externo:** o harness2 classifica o efeito de cada mudança como ajudou / atrapalhou / sem efeito
  observado / confundido (`harness2/core/prompts.py:1105-1160`).
- **Entrega:** a classe "confundido" (run com mais de uma intervenção nova, ou falha de ambiente)
  não credita nem debita; a memória deixa de aprender correlação espúria.
- **Aceite:** num run marcado como confundido, nenhuma memória muda de score (teste).

## B11 — Onda EGM no ADW (E1-E8) — EM EXECUÇÃO (01/11)

Campos novos da Spec, cada um com lint, comportamento no runner e teste (detalhe em RELATORIO §Rodada 3):
E1 `persona.isolation = "goldfish"` · E2 `must_read` com recibo sha256 · E3 `findings_contract`/`max_major` ·
E4 `churn_limit` · E5 `triage_of` · E6 `tools = "none"` · E7 `[purpose]` `goals`/`non_goals`/`open_questions`/
`verification` · E8 `output_schema`. Aceite: test_adw verde, ruff limpo, as 23 specs da biblioteca válidas, doc de
referência da Spec atualizada. O efeito do `omitClaudeMd` exige um run ao vivo — depois de 08/11.

## B12 — Skills só-manuais fora do anúncio (POTENCIALIZAR, depois de 08/11)

- **Fato:** 327 skills anunciadas, 7 usadas pela tool `Skill` em 250 sessões; 1,93 MB de `skill_listing` (F4d).
  O EGM marca as suas com `disable-model-invocation: true` (`skills/*/SKILL.md:4`), que tira a skill do anúncio ao
  modelo e a mantém chamável por `/comando` (INFERÊNCIA 0,8 sobre o efeito no anúncio — confirmar na doc oficial).
- **Entrega:** lista proposta (B3) das skills sem invocação pelo modelo em N sessões → Gabriel aprova → frontmatter.
- **Aceite:** bytes de `skill_listing` por sessão caem; nenhuma skill marcada é chamada pelo modelo nas 2 semanas
  seguintes sem o usuário pedir (falso positivo = 0).

## B13 — Isolamento goldfish nos agentes críticos (INSPIRAR, depois de 08/11)

`omitClaudeMd: true` + tools só-leitura no frontmatter dos agentes revisores (`~/.claude/agents/touring-auditor.md`
e afins), como `.claude-plugin/agents/*.md:5-8`. Aceite: um run ao vivo mostra o revisor sem o CLAUDE.md no
contexto e sem Edit/Write/Bash; os achados do revisor não caem de qualidade (amostra rotulada).

## Instrumento existente para B1-B4 (POTENCIALIZAR)

`eval/swe_bench/harness.py` (junho/2026, `eval/swe_bench/README.md`) mede `resolved%` e custo em
tokens, é determinístico e não gasta crédito sozinho; o solver com modelo roda fora de banda **com
autorização explícita de crédito**. É o único caminho para replicar no nosso ambiente a medição do
paper (custo × Pass@1) em vez de só sessões reais. Usar exige a decisão do Gabriel sobre crédito.

## B4 — Detector de espiral, em sombra (INSPIRAR + POTENCIALIZAR)

- **Fase 0 (offline, permitida antes de 08/11):** rotular à mão uma amostra pré-sorteada de 40 turnos
  longos (20 marcados pelo detector, 20 não) como espiral ou trabalho produtivo.
- **Aceite da fase 0:** precisão ≥ 0,7 e recall ≥ 0,6 contra o rótulo; abaixo disso, recalibrar ou
  arquivar.
- **Fase 1 (sombra ao vivo, depois de 08/11):** registra a assinatura por turno sem injetar.
- **Fase 2 (aviso):** no máximo 1 aviso por turno, carregando a lição da memória para o erro repetido.
  A/B por dia; regra: aceita se chamadas por turno longo caírem ≥ 10% sem aumento de retrabalho
  (mesmo arquivo reeditado no turno seguinte).
