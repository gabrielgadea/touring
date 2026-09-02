# Plano — Aprimoramento de Skills TACO-house + Proposta de Novas Skills

| | |
|---|---|
| **Solicitante** | Gabriel Gadea |
| **Data** | 01/09/2026 |
| **Modo** | `--ultrathink --sequential-thinking` |
| **Constraint operacional** | "Não quero baixar todas essas skills. Em primeiro lugar, aperfeiçoar as que já temos. Em segundo lugar, criar skill(s) novas." |
| **Escopo do plano** | Top-10 skills **TACO-house** (nossas), classificadas contra a taxonomia A→G de 7 mecanismos, com **cross-pollination** medido contra `obra/superpowers` e `mattpocock/skills` |
| **Tipo de entrega** | Diagnóstico + plano de melhorias (ranking) + 3 candidatas a NOVA skill + roteiro de execução |
| **Companion** | Persistência canônica em memória: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` (taxonomia A→G canônica + dataset obra + mattpocock) |

---

## 1. SUMÁRIO EXECUTIVO

> **Veredito sintético**: as 10 skills TACO-house são **excelentes em rigor (B+D), medianas em C, e UNIVERSALMENTE FRACAS em (E) RED-GREEN-REFACTOR**. Há uma lacuna cultural: nenhuma skill nossa **força** que código novo nasce vermelho. Isso é um achado, não um defeito isolado — é a mesma lição que valeu `taco-planning` o **sub-skill E** via obra `superpowers:test-driven-development`. Cinco melhorias prioritárias (ranking em §6) + três candidatas a NOVA skill (§7).

### 1.1 Os 7 mecanismos (referência canônica)

Da memória persistente `skill-structured-reasoning-2026-09-01.md` (canônica) + corpus do turno anterior:

| # | Mecanismo | Definição compacta | Onde mora o gate |
|---|---|---|---|
| **A** | HARD-GATE em prompt | MUST/NEVER/Iron Law literal no corpo da skill | Prompt ao LLM |
| **B** | Exit-code gate | Script Python devolve exit≠0 quando cláusula não passa (único fail-closed por construção) | Executor |
| **C** | Sequential phase protocol | Fases encadeadas com GATE HUMANO explícito entre | Workflow |
| **D** | Adversarial / blind critic | Painel separado do autor, sessão fresca, lentes distintas, quórum por código | Sessão |
| **E** | RED-GREEN-REFACTOR (TDD procedural) | Sem teste falhando, não há código; "fixed" é teste verde sob regressão | Executor + prompt |
| **F** | Human-in-the-loop checkpoint | Gate humano explícito antes de caro/irreversível | Prompt + workflow |
| **G** | Pre-conditional gate | Verifica condição *antes* de invocar sub-processo caro; falha cedo em vez de dentro | Executor (orchestrator) |

A taxonomia A→G **inclui (G)** por contribuição primária do `mattpocock/skills` (ver `tdd` verbatim: *"No test is written at an unconfirmed seam"*; `code-review`: *"A bad ref or empty diff should fail here, not inside two parallel sub-agents"*).

---

## 2. PARTE 1 — DIAGNÓSTICO: 10 SKILLS CONTRA A→G

### 2.1 Metodologia

Para cada skill abaixo foram lidas as `SKILL.md` por inteiro e classificadas nos 7 mecanismos. Cada célula usa:
- **✓** (presente, evidência citada verbatim) | **△** (parcial, evidência + gap citado) | **✗** (ausente)
- Evidência = citação verbatim entre aspas, ou nota de gap observacional

### 2.2 Tabela consolidada (visão geral, justificativa detalhada em §2.3-2.12)

| Skill | A (Hard-Gate) | B (Exit-code) | C (Sequential) | D (Adversarial) | **E (RED-GREEN)** | F (Human-gate) | G (Pre-conditional) |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Touring** (master, 498L) | ✓ | △ | ✓ | ✓ | **✗ GAP universal** | △ | ✓ |
| **loop-engineering** (355L) | ✓ | ✓ GOLD | ✓ | ✓ | **✗ GAP universal** | ✓ | ✓ |
| **TACO-cross-audit** (309L) | ✓ | ✓ | ✓ | ✓ | **✗ GAP universal** | ✓ | ✓ |
| **TACO-skilling** (252L) | ✓ | ✓ | ✓ | △ | **✗ GAP universal** | ✓ | ✓ |
| **TACO-subagent** (491L) | ✓ | △ | ✓ | ✓ | **✗ GAP universal** | △ | ✓ |
| **TACO-wt** (217L) | ✓ | ✓ | ✓ | △ | **✗ GAP universal** | △ | ✓ |
| **taco-planning** (315L) | ✓ | ✓ | ✓ | △ | **✗ GAP universal** | △ | ✓ |
| **analysis-loop** (≈700L) | ✓ | ✓ GOLD | ✓ | ✓ GOLD | **✗ GAP universal** | ✓ | ✓ GOLD |
| **briah** (≈150L) | ✓ | ✓ GOLD | ✓ | ✗ GAP | **✗ GAP universal** | ✓ | ✓ GOLD |
| **plan-amplifier** (≈300L) | ✓ | △ | ✓ | △ | **✗ GAP universal** | △ | △ |

**Achado universal**: **(E) RED-GREEN-REFACTOR é a maior lacuna**. Nenhuma das 10 skills **força** o ciclo procedural TDD. Elas mencionam testes em §6/§8 e nas Quality Gates — mas como COVERAGE TARGET (`≥80%`), não como **gating procedure** ("sem teste falhando, não há código"). É o gap que obra `superpowers:test-driven-development` endereça cirurgicamente.

### 2.3 Touring (master integration)

- **A ✓** "Always" / "Never" enumerado nos 11 Golden Rules (e.g., "**NEVER** ignore orphan symbols", "ALWAYS classificar refinement level")
- **B △** Hooks `pre-edit` (score≥0.8) + `loop_stop_guard.py` exit-code, mas exit 0 não é regra unificada na skill
- **C ✓** TACO phase level L0-L4 com `Phase` table (L0-L1 SOLO, L2 1→5→validate, L3 1→2→5→6→validate, L4+ todas)
- **D ✓** "Symbol Verification Table (Wave TRM 2026-05-02 — constitutional)" com 5 roles × `cited_symbols / symbol_verification / vgp_cross_verification / documented_symbols`; anti-padrões `BLOCKED_INVENTED_SYMBOL` etc.
- **E ✗** Citado em §"Best Practices by Category > TESTING" mas como coverage objetivo, não como gate procedural que bloqueia antes do código
- **F △** `/clear` é predito nas "Operating Principles > Context Management", mas não há explicit human-gate em fases irreversíveis além do REGRA #11 git
- **G ✓** Gold Rule #1 "File metadata first" pré-condição (cheap `touring ast meta` antes do caro `Edit`); VGP pré-condição antes do codegen

### 2.4 loop-engineering (iterate-until-converged engine)

- **A ✓** Regra de ouro 1 "Convergence is measured, never asserted"
- **B ✓ GOLD** `scripts/loop_converged.py --task <id> --scope <path>` exit 0 é O ÚNICO "pronto"
- **C ✓** 4 stacked loops L1/L2/L3/L4 + OUTER/INNER/CLOSE/META; checkpointer para resumability
- **D ✓** Crítico via fragmento `critic-panel` ("N blind critics com distinct lenses, quorum by code") — composição flow-portfolio, **não nativo no skill mas wired em ADW library**
- **E ✗** Test clause "cargo check + test + clippy → green" no gate — mas coverage/complexity/procedural gate ausentes
- **F ✓** "Human-in-the-loop — hybrid autonomy (3 gates)" listadas explicit: (1) Strategy → plan, (2) Before irreversible, (3) Final sign-off
- **G ✓** `--run-gates` mode do `analysis_converged.py` é **executor-side precondition**, contribuição direta para corrigir "gate que ninguém executava"

### 2.5 TACO-cross-audit (purpose-fidelity auditing)

- **A ✓** Hard rules 1-8 enumeradas; REGRA #0 potencializar nunca reduzir
- **B ✓** "verdict is an exit code... judge of record is `loop_converged.py`"
- **C ✓** 7 phases MAP→PURPOSE→DEBT→HARMONY→FIX→E2E→REPORT; "Never reorder"
- **D ✓** "critic-panel fragment is the remedy — N critics, each with a DISTINCT lens... quorum counted by code, never a narrative synthesis" — wired por composição
- **E ✗** "E2E tests are run, not just written" — mas regra procedural por sub-código, não gate universal
- **F ✓** Final sign-off; human-decision items flagged em §"ACTIONS"
- **G ✓** "A bad ref or empty diff should fail here" — aplicado, mas só no code-review (não cross-audit-wide)

### 2.6 TACO-skilling (meta-skill para criar/refinar skills)

- **A ✓** 4 Rules + Hard rules 1-9; "Discover before creating — dedup"
- **B ✓** `quality_gate.py` (structure + hygiene), `triggering_audit.py` (zero LLM verification) — scripts Python como gate
- **C ✓** CREATE 5-phase pipeline + REFINE 5-phase loop
- **D △** Mentioned but no blind critic; "triggers real session history" é proxy
- **E ✗** Não impõe TDD nas próprias skills que gera (só "every code change has a named test + error branch")
- **F ✓** "The user sees the diff. REFINE never applies changes blind"
- **G ✓** "Discover before creating" — `discover.py` antes de criar nova skill (cheap probe before expensive write)

### 2.7 TACO-subagent (sequential phase protocol v6.3)

- **A ✓** REGRA #15 Mandatory Symbol Verification Table; "output sem o campo = checkpoint REJECT, composite 0.0"
- **B △** Hard rule 4 "Exit 0: Never block user operations" — mas convergence gate não é uniforme
- **C ✓** 7 fases sequenciais (PERCEPTION/SCOUT/ARCHITECT/CONTEXT7/DECOMPOSE/ENGINEERS/CROSS-AUDIT/DOCS)
- **D ✓** Auditor tem de ser cego; "N críticos idênticos encontram o mesmo modo de falha N vezes" → `critic-panel` fragment
- **E ✗** Sem TDD procedural no subagent
- **F △** PHASE 0 health gate é system-side, não human checkpoint; "AGUARDAR" é o gate
- **G ✓** `touring shadow validate` antes de aplicar edits; `decompose claim` antes de spawnar

### 2.8 TACO-wt (wave template forense)

- **A ✓** 8 Hard Rules (Layer 3 over prose, dry-run by default, JSON contract, etc.)
- **B ✓** TOON checkpointing + blake2b hash chain + `cross_audit.py` composite score
- **C ✓** 4-phase anatomy (Imports/CLI parser/Pure scan/Optional mutation) + per-wave validator
- **D △** cross-audit é single-pass (não parallel com critic-panel)
- **E ✗** Validator per-sub-script mas sem RED-GREEN procedural
- **F △** Quality gates mas sem human checkpoint explícito entre waves
- **G ✓** `--apply` opt-in "default is dry-run; L9 is non-negotiable"

### 2.9 taco-planning (Pln2-grade plan authoring)

- **A ✓** Hard rules 1-8; Lessons L1-L7 (planning-specific)
- **B ✓** `plan_validator.py --strict`, `dimension_scorer.py`, `gap_detector.py --fail-on=P0` — todos os 10 scripts Python
- **C ✓** 4 stages (Ground truth → 9-dim analysis → Plan structure → Amplification check)
- **D △** gap_detector P0-P3, mas sem painel cego — plano valida-se sozinho, não é adversarial
- **E ✗** Even LESS — plan é estática, não roda TDD
- **F △** §5 Verification Protocol inclui 50-dim gate
- **G ✓** VGP ("Every cited symbol passes `touring index find`") por construto (Stage 1 + 2 + 4)

### 2.10 analysis-loop (graph-engineering analysis)

- **A ✓** Hard Rules 1-11; 14 clauses de convergência
- **B ✓ GOLD** `analysis_converged.py` exit-code único "pronto"; fail-closed on ausência
- **C ✓** OUTER/INNER/CLOSE; checkpointer, profile to ADW
- **D ✓ GOLD** PAINEL CEGO: "N críticos com a mesma lente compram uma opinião N vezes"; lenses vêm de falhas medidas; quorum by code
- **E ✗** Não roda TDD (análise não produz código de produção)
- **F ✓** "Gates humanos — três, e só três" (destino+mapa / irreversível / entrega)
- **G ✓ GOLD** `--run-gates` EXECUTA gates em vez de ler JSON hipotético (a "família fake_waiting")

### 2.11 briah (Mundos da Criação)

- **A ✓** Lei do rito (3 leis) + 7 operações formais + 4 anti-padrões
- **B ✓ GOLD** `cognicao_formal.py medir --arquivo` → ratio 1.0 (exit-code gate literal)
- **C ✓** Passo 0 → 7 ops → final artifact (criacao.md OKF)
- **D ✗ GAP** Sem painel cego; o "journal cognitivo" substitui mas é self-evaluation
- **E ✗** rito criativo, não codificação
- **F ✓** Gate humano (decisão d) "jamais entra sem o sim"
- **G ✓ GOLD** "medir ANTES de perguntar" — pre-condição exemplar

### 2.12 plan-amplifier (Pln² evolution)

- **A ✓** Quality Score Target ≥95/100; Rubric com 6 critérios ponderados
- **B △** Quality Score usado mas **não é exit-code Python** (depende de auto-avaliação)
- **C ✓** 5 phases (Dimensional → Synthesis → Amplification → Roadmap → Validation)
- **D △** 8 dimensões fan-out em paralelo, mas "mesmo orquestrador mesmíssimo classifica" — não é blind critic
- **E ✗** Não menciona TDD nos quality gates
- **F △** "Quality Gate 1/2/3 Score ≥ X" são thresholds, não human approve explícito
- **G △** "Validation hooks" como concept; implementação esparsa

### 2.13 Padrões transversais observados no diagnóstico

**Onde somos fortes**:
1. **Adversarial (D)** em 4 skills (loop-engineering, TACO-cross-audit, TACO-subagent, analysis-loop)
2. **Exit-code gate (B)** em 5 skills (loop-engineering, analysis-loop, briah já GOLD; TACO-cross-audit, TACO-skilling via scripts)
3. **Pre-conditional (G)** em todas 10 (file-metadata-first é universal; VGP é nativo em TACO-house)
4. **Sequential phase (C)** em 9 skills (9-fase TACO + 7-fase analysis-loop + 4-fase wave + 4-stage planning)

**Onde somos fracos** (gaps reais):
1. **(E) RED-GREEN-REFACTOR** — universal, exceto pela menção. Critério de cobertura não é gate procedural.
2. **(F) Human-approve checkpoint** fraco em TACO-wt, taco-planning, plan-amplifier (aceitam só "thresholds")
3. **Primitive reusável** — usamos 7+ scripts compartilhados (loop_diagnose, loop_converged) mas SEM o princípio de **"nomear primitive reutilizado por N skills"** (analogia direta ao `grilling` de mattpocock)
4. **`wait-what` recovery primitive** — não temos primitive nomeado para misfire humano↔IA

---

## 3. PARTE 2 — APRIMORAMENTOS PRIORITÁRIOS (RANKING A→G de impacto vs esforço)

### 3.1 Ranking final (top-5 pelo par impacto / esforço) — **v1.1 atualizado pós-sub-agent + Marcel Point do Gabriel**

> **Nota de versão**: o ranking abaixo é v1.1 (após cross-pollination analysis do sub-agent + correção do Marcel Point de Gabriel "se é gap de todas, deve constar em todas"). O ranking v1.0 original (Touring MUST (E) isolado, analysis-loop precondition, briah blind) foi SUBSTITUÍDO pelo sub-agent em v1.1 (ver §10.3 para o ranking v1.0→v1.1). **Use este §3.1, ignore o v1.0**.

| Rank | Skill + Ação | Mecanismo inserido | Esforço | Impacto | Cross-pollination source |
|:---:|---|:---:|:---:|:---:|---|
| **1** | **Touring master** — `verification-before-completion-taco` rule (per-claim evidence) | B + (A) | **S** | **ALTO** | obra `superpowers:verification-before-completion` |
| **2** | **loop-engineering** — `tdd-enforcer` integration INNER 12 (red-green pre-conditional) | E + (G) | **M** | **ALTO** | obra `test-driven-development` + mattpocock `tdd` |
| **3** | **TACO-cross-audit** — root-cause-before-fix em DEBT phase + 1 human pause antes de FIX | (G) + (F) | **S** | **ALTO** | obra `systematic-debugging` |
| **4** | **MUST (E) transversal em 7 skills** (Touring + loop-engineering + TACO-cross-audit + TACO-skilling + TACO-subagent + TACO-wt + taco-planning) — REGRA-ZERO-DRIFT idêntica, single commit | **E** universal gap (**transversal**) | **M×1** | **ALTO** | obra `test-driven-development` |
| **5** | **TACO-wt** — DOCUMENTAR exit codes 0/1/2/3 já existentes | B (doc-fix) | **S** | **MÉDIO** | verificação empírica 01/09 (red flag RF2) |

#### Marcel Point de Gabriel (01/09/2026)

> "Se é um gap de todas, deve constar em todas."

A classificação de Rank #4 mudou de **1 skill edit (Touring)** para **7 skills transversal** porque o gap (E) é **universal** — tocar só Touring deixaria 6 skills com o gap aberto. O MUST idêntico é inserido nas 7 skills que lidam com código de produção (Touring + loop-engineering + TACO-cross-audit + TACO-skilling + TACO-subagent + TACO-wt + taco-planning); analysis-loop / briah / plan-amplifier são meta/analíticas e isentas.

### 3.2 Detalhamento de cada aprimoramento

#### Rank #1 — Touring: adicionar (E) explícito

**Diagnóstico**: §"Best Practices by Category > TESTING" menciona testes como coverage target, mas não há MUST procedural. Um Edit que cria produção sem teste vermelho fere a skill.

**Ação concreta** (Edit em `~/.claude/skills/Touring/SKILL.md`):

1. Inserir em §"Three Mandatory Principles" um Quarto Princípio explícito: **"Test-First (E)"** com texto análogo ao "Golden Rule #1".
2. Adicionar MUST: "Para código de produção novo (.rs/.py/.ts/etc, fora de docs/tests), o teste RED deve existir antes do código GREEN. `Edit tool` em produção exige prova do teste RED em `tests/` ou equivalente."
3. Cross-link com `superpowers:test-driven-development` como referência canônica do procedimento.
4. Adicionar Hard Rule #12 com exemplo concreto:
   ```bash
   # WRONG (E violated): tour edit <src> sem tour verify <test_file>
   tour edit src/cache.py
   tour verify tests/test_cache.py
   # RIGHT: assert test fails FIRST
   tour verify tests/test_cache.py          # PASS expected=False
   tour edit src/cache.py                    # GREEN
   tour verify tests/test_cache.py          # PASS
   ```

**Critério de pronto**: novo MUST visível em skill, hook de pre-edit capaz de detectar "test file missing for production file" (gate computacional opcional — Phase 2).

**Métricas de validação**: rodar `triggering_audit.py` no log após 7 dias; almeja redução ≥50% em "código sem teste" no scope.

**Esforço**: **S** (uma Edit + testes; sem hook novo em V1; gate computacional é V2 opcional).

#### Rank #2 — loop-engineering: seam-confirmation (G)

**Diagnóstico**: o gate-engineering step (INNER 12) executa Edit/Write + touring-engineer sem precondition de seams. Matt Pocock `tdd` verbatim: *"Before writing any test, write down the seams under test and confirm them with the user. No test is written at an unconfirmed seam."*

**Ação concreta** (Edit em `~/.claude/skills/loop-engineering/SKILL.md` e no protótipo de INNER 12):

1. Inserir como sub-step de INNER 12 antes de Edit: **"Seam Confirmation"** — executar `touring ast overview <file>` + `--just-the-interfaces` e logar `seams = <list>` em `knowledge/<phase>.json`.
2. Adicionar MUST: "Nenhum Edit em código de produção sem `seams` registrado na phase abstract."
3. Cross-link com `mattpocock/tdd` como source do princípio.

**Critério de pronto**: `loop_phase_close.py --facts` valida que o abstract tem campo `seams` não-vazio para qualquer file path em `touched_files` (proof-script).

**Esforço**: **M** (Edit + 1 proof-script + integração com loop_phase_close).

#### Rank #3 — TACO-cross-audit: critic-panel nativo

**Diagnóstico**: §"Four ways an audit reaches the wrong verdict (2026-08-18/19, all observed)" já lista o problema ("auditor grading its own work"), e cita `critic-panel` como remedy — mas só via fragmento. Falta um script procedural dentro da skill (`scripts/critic_panel.py`) que produza o resultado.

**Ação concreta**:

1. Adicionar ao Layer-3 da skill: `scripts/critic_panel.py` (N críticos cegos, lenses distintas, sessão fresca, quórum por código).
2. Atualizar §"7 fases" inserindo 5.5 "Blind Critic Panel (Gate)" — entre FIX (5) e E2E PROOF (6) — invocável só se `--enable-blind-panel`.
3. Default: manter auditor single-pass atual como quick-mode; --enable-blind-panel como strict-mode.

**Esforço**: **M** (1 script + integração + testes).

#### Rank #4 — analysis-loop: exemplar precondition (G)

**Diagnóstico**: A própria skill já ensina o princípio ("A barra do painel vem do mapa, nunca do crítico" + "the gauntlet loop... before optimization the wrong thing"), mas FALTA o mecanismo executor: `_clause_painel` cita o pre-condition mas não tem script enforcement.

**Ação concreta**:

1. Em `scripts/analysis_converged.py`, o painel exige `--exemplar <file>` flag opcional; quando ausente E phase-com-painel aplicada → refuse-to-collect panel (exit 2, mensagem "deriveBar=false").
2. Adicionar `--bar-source` flags: `exemplar`, `derived-from-map`, `judged-as-novel` — pelo menos 1 deve ser setado ou panel is denied.
3. Documentar em `references/clausulas-de-convergencia.md` a classe de defeito coberta.

**Esforço**: **S** (1 flag + 1 doc + testes de regressão).

#### Rank #5 — briah: leitura cega da criacao.md (D)

**Diagnóstico**: briah é rito criativo, mas a transição Yetzirah (estratégia) acontece self-evaluated. A "sombra da criação" é o anti-padrão que mattpocock grilling-flagged.

**Ação concreta**:

1. Adicionar um gate de 2 críticos paralelos (sessões frescas, lenses distintos: "internal-coherence" e "external-valor") entre Ratio 1.0 e Yetzirah.
2. `cognicao_formal.py` ganha `--pre-yetzirah-check` que chama os críticos.
3. Memoriza resultado como `--mundo briah` no journal.

**Esforço**: **M** (sub-skill + integration + tests).

### 3.3 Aprimoramentos secundários (rank 6-10; não entravam no top-5)

| Rank | Skill | Ação | Mecanismo | Esforço |
|:---:|---|---|:---:|:---:|
| 6 | TACO-skilling | Adicionar MUST (E) para "tests-run-fail-first" no script `quality_gate.py` | E | S |
| 7 | taco-planning | Adicionar §"D Validator anti-self-bias" antes do Stage 4 | D | S |
| 8 | TACO-wt | Adicionar step "staged-validator" no `validate_W<N>.py` que rode um critic-panel mini (3 críticos) | D | M |
| 9 | TACO-subagent | Tornar PHASE 1.5 human-gate explícito (não "AGUARDAR", mas "██ GATE HUMANO ██") | F | S |
| 10 | plan-amplifier | Trocar "Quality Score" self-evaluator por Python exit-code (regex ou LLM-judge-as-script) | B | L |

---

## 4. PARTE 3 — NOVAS SKILLS CANDIDATAS (TOP-3)

> **Constraint de Gabriel**: "criar skill(s) novas" — apenas SE NECESSÁRIO. Após o diagnóstico, identifiquei **3 candidatas** que **NÃO são cobertas por aprimoramentos das skills existentes**.

### 4.1 Candidata #1 — `wait-what` (misfire recovery primitive)

**Origem externa**: mattpocock/skills. *"Fire this the moment a message doesn't land. The agent re-pitches it with the context you're missing, in plain English, using your CONTEXT.md vocabulary."*

**Gap coberto**: TACO-house tem detecção de misfire via Stop hook (Lei L3 do TACO) e via memória (`memory recall`)。**Mas não há primitive NOMEADO** que o modelo invoque PROACTIVAMENTE quando sente que o usuário "não entendeu o que quis dizer".

**O que faz**:
- Auto-trigger: invocada quando (a) o usuário corrigiu a última frase, ou (b) `touring memory recall "<user_prompt>"` retorna 0 hits relevantes E o usuário repetiu, ou (c) explicitamente `/wait-what`.
- Saída: reformulação da resposta anterior com vocabulário do `CONTEXT.md` / `.claude/CLAUDE.md`, 3-5 alternativas concretas, e pergunta de clarificação.

**Por que precisa ser nova skill e não aprimoramento**: é um primitive de CROSS-SKILL — `analysis-loop`, `TACO-cross-audit`, `Briah`, `Touring`, todos se beneficiariam, mas é incorreto distribui-lo em cada uma (duplicação por construção). Matt Pocock demonstrou o princípio com `grilling` (1 primitive, reusado por 5+ skills).

**Composição com a casa**:
- (A) MUST: "When triggered, ALWAYS reformulate the last response, do NOT proceed to new action."
- (F) Human-gate: o usuário decide qual reformulação é a correta — mas o primitive oferece 3-5 alternativas.

**T-shirt**: **S** (criação pura, sem dependência de tools novos; usa apenas `touring memory recall` + `CONTEXT.md`).

### 4.2 Candidata #2 — `grilling` (frontier-drain interview primitive)

**Origem externa**: mattpocock/skills + análise-loop `analysis_frontier.py`. O primeiro para uso interativo, o segundo para uso autônomo.

**Gap coberto**: TACO-house tem `Briah` que faz **uma passada** das 7 operações de criação, mas NÃO tem primitive **reutilizável** de "exhaust decision tree". `Touring`/`analysis-loop` rodam decisões em batch mas interagem com humano só em GATE HUMANO explícito (3 ao todo). Não há primitive mid-tier "entrevista focada para fechar sub-decisão".

**O que faz**:
- AI mapeia conversa como **decision tree** (cada decisão ramifica).
- Trabalha em **rounds**: a cada round, a frontier inteira de perguntas respondíveis é disparada.
- **Frontier rule**: pergunta só entra se todos pré-requisitos resolvidos.
- **Role split**: AI acha fatos via sub-agents filesystem/tools **sem perguntar**; usuário decide.
- Termina quando frontier vazia + nada importante assumido.

**Por que precisa ser nova**: é primitive cross-skill (Briah, TACO-subagent FASE 4, analysis-loop, Taco-planning têm lugar para ele). Tentar implementar como ramificação de `analysis_frontier.py --interactive` criaria acoplamento ruim; como skill, é composable.

**Composição com a casa**:
- (A) MUST: "AI NEVER assume decisão; always ask via frontier."
- (B) Exit-code: termina quando frontier vazia (script `grilling_converged.py`).
- (C) Pre-requisitos/lens-name dentro de cada round.
- (D) NEW primitive de painel cego para validar respostas registradas.
- (G) Pre-condição: question só dispara se prerequisites resolvidos.

**T-shirt**: **M** (precisa de subscripts JSONL de frontier state + `--mark-answered` integration).

### 4.3 Candidata #3 — `critic-panel` (blind-critic-as-orchestrator, não fragmento)

**Origem externa**: mattpocock `code-review` two-axis separation + analysis-loop PAINEL CEGO + obra `requesting-code-review`. Hoje no TACO-house vive como `adw fragment` (no library), mas **não como skill standalone** que pode ser invocada em qualquer contexto.

**Gap coberto**: TACO-cross-audit cita `critic-panel` como remedy, mas só via fragmento ADW — quem não roda ADW (e.g., engenheiro solo, sessão sem ADW) **não tem acesso**. Como skill, qualquer sub-agent, skill ou humano pode invocar `Skill: critic-panel --target <artifact>`.

**O que faz**:
- Persona config: N críticos, lenses distintos (correção · segurança · did-it-reproduce · valor-externo · ...)
- Sessão fresh obrigatória (não herdar contexto)
- Quórum por código (não narrativa)
- Output: VERDICT=PASS|REJECT|ESCALATE + findings endereçados (clé-usuarios de LEI L3 do TACO).

**T-shirt**: **M** (a infra `critic-panel` fragment já existe em `.touring/adw/` — promoção a skill é refactor + doc).

### 4.4 Candidatas rejeitadas (com justificativa)

| Candidato | Por que NÃO criar |
|---|---|
| `verification-before-completion` (port obra) | Já temos 6 BLOCK dims P0 + loops que medem convergence. **Duplicação**. |
| `grill-me` (port mattpocock) | briah já cobre interview criativo. **`grilling` primitive é suficiente**. |
| `two-axis-review` (port mattpocock) | Implementar dentro de `TACO-cross-audit` rank #3 é melhor. **Não criar**. |

---

## 5. PARTE 4 — ROTEIRO DE EXECUÇÃO

### 5.1 Sequência respeitando dependências

```
PHASE A — Aprimoramentos de baixo custo (semanas 1-2)
├─ A1 [S] Touring rank #1 — adicionar MUST (E) + Hard Rule #12
├─ A2 [S] TACO-skilling rank #6 — `quality_gate.py` testa red-first
└─ A3 [S] analysis-loop rank #4 — `analysis_converged.py` gain `--bar-source` flag

PHASE B — Aprimoramentos de médio custo (semanas 2-4)
├─ B1 [M] loop-engineering rank #2 — seam-confirmation (G) + integration
├─ B2 [M] TACO-cross-audit rank #3 — `scripts/critic_panel.py` nativo
├─ B3 [M] TACO-wt rank #8 — staged-validator no validate_W<N>.py
├─ B4 [M] briah rank #5 — pré-Yetzirah cego
└─ B5 [M] Criar skill nova `wait-what` ←── paralela, depende de A apenas

PHASE C — Aprimoramentos adicionais (semanas 4-6)
├─ C1 [M] Criar skill nova `grilling`
├─ C2 [M] Criar skill nova `critic-panel` (T-shirt M; promoção de fragment)
└─ C3 [S] TACO-subagent rank #9 — PHASE 1.5 human-gate explícito

PHASE D — Validação cruzada (semana 6-8)
├─ D1 Run `triggering_audit.py` em cada skill modificada
├─ D2 Run `mine_transcripts.py --skill <name>` por 7 dias coletando uso real
├─ D3 Run quality_gate em todas as 13 (10 originais + 3 novas) skills
└─ D4 Comparar composite antes/depois — alvo: ≥0.80 Gold em todas

PHASE E — Documentar entrega (semanas 8-10)
├─ E1 Update `~/.claude/skills/MEMORY.md` com versões V2
├─ E2 Update `~/projects/touring/docs/CONSTITUTION-v8.md` (se rank #1 mudar golden rule)
├─ E3 Store skills-curation memory no traveling memory store
└─ E4 Run `touring portfolio "<intent>"` para verificar n+1 saturação
```

### 5.2 Tabela com T-shirt + dependências + responsável

| ID | Item | Skill/esforço | T-shirt | Depende de | Entrega atômica? |
|:---:|---|:---:|:---:|:---:|:---:|
| A1 | Touring rank #1 (E explícito) | Touring/S | **S** | — | SIM |
| A2 | TACO-skilling rank #6 (gate red-first) | TACO-skilling/S | **S** | — | SIM |
| A3 | analysis-loop rank #4 (bar-source flag) | analysis-loop/S | **S** | — | SIM |
| B1 | loop-engineering rank #2 (seam-confirmation) | loop-engineering/M | **M** | A1 | SIM |
| B2 | TACO-cross-audit rank #3 (critic_panel.py) | TACO-cross-audit/M | **M** | A1, B1 | SIM |
| B3 | TACO-wt rank #8 (staged-validator) | TACO-wt/M | **M** | A3 | SIM |
| B4 | briah rank #5 (pré-Yetzirah cego) | briah/M | **M** | A1 | SIM |
| B5 | NEW skill `wait-what` | nova skill/S | **S** | (independente) | SIM |
| C1 | NEW skill `grilling` | nova skill/M | **M** | B4 (ou independente) | SIM |
| C2 | NEW skill `critic-panel` | promotion de fragment/M | **M** | B2 | SIM |
| C3 | TACO-subagent rank #9 (human-gate) | TACO-subagent/S | **S** | A1 | SIM |
| D1 | triggering_audit em modificadas | tooling/S | **S** | B-fim | — |
| D2 | mine_transcripts × 7 dias | tooling/M | **M** | D1 | — |
| D3 | quality_gate nas 13 | tooling/S | **S** | A-B-C-fim | SIM |
| D4 | comparativo composite | tooling/S | **S** | D3 | SIM |
| E1-E4 | docs + memory + portfolio | tooling/S | **S** | D-fim | SIM |

### 5.3 Verificação de exit (gate de TACO-house)

Cada skill modificada deve passar:
1. `quality_gate.py` (estrutura + hygiene + REGRA #13)
2. `touring-quality check --gate <dim>` em 6 BLOCK dims
3. `touring-quality score <skill> --fail-below 0.80`
4. `triggering_audit.py` sobre session history (zero LLM)
5. **Composite ≥ 0.80 (Gold)** para marcar a skill como "entregue".

---

## 6. PARTE 5 — RISCOS + MITIGAÇÕES

### 6.1 Tabela de riscos classificados

| # | Risco | Categoria | Probabilidade | Impacto | Mitigação |
|:---:|---|:---:|:---:|:---:|---|
| R1 | Rank #1 (Touring MUST (E)) causa **fadiga de testes** em mudanças triviais de doc | Over-blocking | **MÉDIA** | **MÉDIO** | MUST só para `.rs/.py/.ts`; arquivos `.md/.txt/.json` isentos. Limitar a "código de produção novo" (não Edit sobre Edit). |
| R2 | Skill `wait-what` pode **roubar contexto** do Stop hook (misfire detection paralelo) | Colliding guards | **ALTA** | **BAIXO** | wait-what é `model-invocable` apenas; não vira pre-tool hook. |
| R3 | seam-confirmation (G) em loop-engineering adiciona latência em loops rápidos | Diminishing returns | **MÉDIA** | **BAIXO** | Aplicar só em waves W>N onde N=t-shirt; loops de recovery continuam sem seams. |
| R4 | critic-panel promotion para skill cria 2 implementações divergentes (skill + fragment) | Reimplementação | **MÉDIA** | **ALTO** | Adopt `critic-panel` skill como source-of-truth e **delete fragment** (REGRA #3 composable-not-custom). |
| R5 | Augmentar (E) em 10 skills simultaneamente causa **transient skill-pollution** (Novas MUSTs conflitando) | Concurrent updates | **MÉDIA** | **MÉDIO** | Fazer PHASE A (3 S) antes de B e C; rodar `quality_gate` em cada. |
| R6 | Cross-pollination errado — mattpocock `grilling` é incompatível com `analysis-loop`'s analysis_frontier em semântica | Semantic collision | **BAIXA** | **ALTO** | Grillin="como entrevistar"; analysis_frontier="como descobrir sozinha". São complementares; documentar a fronteira claramente. |
| R7 | Triggering-audit detecta que ranked #1 (E em Touring) **diminui** triggering da skill (modelo evita Touring) | Adoption collapse | **BAIXA** | **ALTO** | Run audit antes/depois; se colapso, mover MUST para `TACO-engineer`/`TACO-cross-audit` ao invés de Touring-master. |
| R8 | Augmentar briah com pré-Yetzirah cego **puxa tempo** que o rito não tem (compactação) | Latência crítica | **MÉDIA** | **BAIXA** | O blind-panel é OPT-IN (`--enable-blink-panel` flag); default continua single-pass. |

### 6.2 Critério de abortar um item

Se durante PHASE A/B um item:
- Causa ≥30% regressão em triggering_audit (R7), OU
- Quebra cargo check / clippy ou quality_gate por ≥3 ciclos consecutivos, OU
- Aumenta SKILL.md body >500L (REGRA #13 hygiene violation),

ENTÃO: voltar atrás, registrar "rolled back" em memory, e pular para próximo item do roadmap.

### 6.3 Política de "fail-closed" para dependências

Cada item **DENTRE** de uma phase depende do anterior (T-shirt sequencing). Se B1 falhar, B2 (que depende de B1) **NÃO INICIA**. Recuo a re-design com `cognicao_formal.py medir --dominio` e reconsideração.

---

## 7. APÊNDICE A — CROSS-POLLINATION COMPLETO OBRA ⇄ MATT POCOCK ⇄ TACO-HOUSE

### 7.1 Matriz de equivalências cruzadas

| obra/superpowers | mattpocock/skills | TACO-house atual | Mecanismo |
|---|---|---|---|
| `brainstorming` (Socratic + HARD-GATE) | `grill-me` (frontier-drain) | `TACO-subagent` PHASE 0 + `briah` 7 ops | C + F |
| `systematic-debugging` (4 fases, Iron Law) | `diagnosing-bugs` (red→minimise→hypothesise) | `analysis-loop` (14-clause), nativamente | C + D |
| `verification-before-completion` (5-step) | (covered by `tdd`) | 50-dim P0 BLOCK (6 dims) | B + E |
| `test-driven-development` (red-green-refactor) | `tdd` (seams-first + refactor-out) | **`GAP (E) universal`** | **E** |
| `writing-plans` (markdown, 2-5 min, self-review) | `to-tickets` (tracer-bullet) | `taco-planning` (Pln2-grade) | C + F |
| `executing-plans` (inline batches + checkpoints) | `implement` (spec → tdd → code-review) | `loop-engineering` INNER 12-14 | C + F + G |
| `dispatching-parallel-agents` (concurrent subagents) | (implícito em `tickets` array) | `TACO-subagent` "GRUPO PARALELO 1" + ADW `parallel` | C |
| `requesting-code-review` | `code-review` (two-axis) | `TACO-cross-audit` PHASE 6 | D + F |
| `receiving-code-review` | (implícito) | (GAP parcial — 50-dim comments accepted) | (n/a) |
| `using-git-worktrees` | — | (manual hoje, seria bom adicionar) | (n/a) |
| `finishing-a-development-branch` | `resolving-merge-conflicts` (nunca `--abort`) | (corpo de CLAUDE.md — REGRA #11 v2) | (A) |
| `subagent-driven-development` | `ask-matt` (router) | `TACO-subagent` 9 fases | C |
| `writing-skills` | `writing-for-agents` | `TACO-skilling` CREATE 5-phase | C |
| `using-superpowers` | (none — meta-skill implícita) | `TACO-cross-audit`+ `Touring`+ composition | A |
| **—** | **`grilling` (decision-tree interview primitive)** | briah (1× use) + analysis-loop (autônomo) | **D + F** |
| **—** | **`code-review` two-axis separation** | `TACO-cross-audit` PHASE 6 (referenced but not explicit two-axis) | **D** |
| **—** | **`wait-what` (misfire recovery primitive)** | **(GAP real — primitive não nomeado)** | **F + D** |
| **—** | **`domain-modeling` (active glossary sharpening)** | briah's "Fronteira" + Touring's CONTRACT.md | (C + F) |
| **—** | **`to-spec` + `wayfinder` (decision-tracker blocking)** | `analysis-loop` `analysis_frontier.py` (autônomo) + `taco-planning` plan.md | **C + F** |
| **—** | **`harness` / `wizard` (bash wizard for human-only steps)** | (GAP — não temos primitive pra credentials/CI setup) | (n/a) |

### 7.2 Mapa de "primitive reuse" inspirado pelo mattpocock

mattpocock nomeia **4 primitives reutilizáveis** (grilling, tdd, code-review, domain-modeling). TACO-house TEM **vários primitives** mas estão **espalhados**:

| Primitive (espalhado) | Onde mora | O que faz | Quantas skills poderiam usar |
|---|---|---|---|
| `analysis_frontier.py` | analysis-loop | Mapeia decisões pesquisáveis | 3+ (Briah, taco-planning, TACO-subagent) |
| `judge_attest.py` | loop-engineering | Anexa SHA256 do grader | 2+ (TACO-cross-audit, analysis-loop) |
| `cognicao_formal.py` | loop-engineering | Mede ratio de presença (7 ops) | 2+ (Briah, plan-amplifier) |
| `pre_edit_gate.py` | Touring/scripts | Composite score gate (incl VGP) | 4+ (Touring, loop-engineering, TACO-cross-audit, TACO-subagent) |
| `SEAM-CONFIRMATION` (proposta) | loop-engineering (Rank #2) | Confirma seams antes de Edit | 5+ (skills que editam código) |
| `CRITIC-PANEL` (proposta como skill) | ADW fragment → promoted | N cegos, fresh session, quorum | 4+ (TACO-cross-audit, TACO-wt, analysis-loop, Briah) |

**Insight**: o PRINCÍPIO de mattpocock é nomear primitive + reusar; o que falta na nossa casa é o NOME e a REUTILIZAÇÃO FORMAL. Extrair `pre_edit_gate.py` como **um primitive nomeado em skill** (`touring-code/SKILL.md`?) é um exercício simples.

---

## 8. APÊNDICE B — VEREDITO CANÔNICO EM 1 FRASE

> **As 10 skills TACO-house são 9 sobre `analysis_loop` (B+C+D+G) + 1 sobre criação (Briah). Falta universalmente (E) RED-GREEN-REFACTOR. Cinco melhorias prioritárias + três candidatas a NOVA skill (`wait-what` primal, `grilling` primitive, `critic-panel` skill) integram o melhor de obra e mattpocock sem baixar nada, com roadmap de 8-10 semanas e composite ≥0.80 em todas.**

---

## 9. CHECKLIST DE ENTREGA

- [ ] Cada item PHASE A (A1, A2, A3) editado + quality_gate 0.80+
- [ ] Cada item PHASE B (B1-B5) editado + `triggering_audit` antes/depois
- [ ] Cada item PHASE C (C1, C2, C3) criado + cross-checked contra fragment existente
- [ ] D1-D4: composite >= 0.80 em todas as 13 skills (10 originais + 3 novas)
- [ ] E1: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/` persistence das 3 skills novas
- [ ] E2: `MEMORY.md` line por skill V2
- [ ] E3: constitution update (só se Rank #1 mudar Golden Rule — improvável)
- [ ] E4: `touring portfolio "<intent>"` para detectar saturação
- [ ] **Composite final ≥ 0.95 (Diamond)** como bonus, não como critério
- [ ] **Anexo v1.1** — correções pós-verificação empírica + candidatas extras (ver §10)

---

## 10. ANEXO v1.1 — CORREÇÕES PÓS-VERIFICAÇÃO + CANDIDATAS EXTRAS

Após a escrita inicial, dispatchei sub-agent de cross-pollination analysis (general-purpose, async) que retornou **3 correções factuais** + **5 candidatas NOVAS extras**. Verificação empírica posterior confirmou 1 correção (TACO-wt B-mechanism).

### 10.1 Correções factuais (sub-agent + verificação por execução)

| # | Onde | Erro original | Correção verificada | Como verificar |
|:---:|---|---|---|---|
| 1 | §2.8 TACO-wt (B) | Marcado △ | **Mover para ✓** — `cross_audit.py` exit codes documentados em `--help`: `0 PASS / BASELINE · 1 WARN · 2 FAIL · 3 structural error`. `forensic_runner.py` herda via subprocess invocation. B-mechanism **é nativo e não-declarado na SKILL.md** — gap é de DOCUMENTAÇÃO, não de implementação. | `timeout 15 python3 ~/.claude/skills/TACO-wt/scripts/cross_audit.py --help` (executado 01/09) |
| 2 | §2.2 Touring (D) | △ em vez de ✗ confirmado | **Manter ✓** — Symbol Verification Table com 5 roles × `BLOCKED_INVENTED_SYMBOL` anti-patterns já é D-mechanism nativo |
| 3 | §2.5 TACO-cross-audit (B) | △ | **Mover para ✓** — `loop_converged.py` é judge of record, exit-code já é a regra |

### 10.2 Candidatas NOVAS extras (5 adicionadas; total 8)

| # | Nome | T-shirt | Origem primária | Por que NÃO apenas aprimoramento |
|:---:|---|:---:|---|---|
| 4 | `verification-before-completion-taco` (per-claim evidence) | S | obra `verification-before-completion` | Hard-gate explícito em Touring master — cobre per-claim discipline (B-mechanism hygiene) |
| 5 | `tdd-enforcer` (red-green pre-conditional) | M | obra `test-driven-development` + mattpocock `tdd` | G-mechanism procedural universal |
| 6 | `adversarial-critic` (primitive reusable cross-skill) | M | TACO-cross-audit + mattpocock `code-review` (two-axis) + obra `systematic-debugging` | Re-uso entre 5 skills; fragment sozinho é fragment |
| 7 | `intent-grilling` (intent-clarification VGP-style) | S | obra `brainstorming` + mattpocock `grilling` + taco-planning VGP | Cobre intent-verification paralela a symbol-verification |
| 8 | `human-pause-gate` (portable F-mechanism) | M | obra `writing-plans` + analysis-loop (3 human gates) | Promote F-mechanism próprio — hoje apenas analysis-loop tem F declarado |

**Pós-filtragem**: mantemos **5 top candidatas** (`wait-what`, `grilling`, `critic-panel-as-skill`, `tdd-enforcer`, `human-pause-gate`); #4 e #7 integradas ao roadmap Rank #1 e Rank #3.

### 10.3 Ranking top-5 atualizado (sub-agent validado + cruzamento)

| Rank | Skill + Ação | Mec | T-shirt | Origem |
|:---:|---|:---:|:---:|---|
| **1** | Touring master — `verification-before-completion-taco` rule (per-claim evidence) | B + A | **S** | obra `verification-before-completion` (#4) |
| **2** | loop-engineering — `tdd-enforcer` integration no INNER 12 (engineer step) | E + G | **M** | obra `test-driven-development` + mattpocock `tdd` (#5) |
| **3** | TACO-cross-audit — root-cause-before-fix em DEBT + 1 human pause antes de FIX | G + F | **S** | obra `systematic-debugging` (#3) |
| **4** | Touring master — MUST (E) explícito para código de produção | **E** (universal gap) | **S** | obra `test-driven-development` |
| **5** | TACO-wt — DOCUMENTAR exit codes 0/1/2/3 já existentes | B (correção) | **S** | verificação empírica 01/09 |

### 10.4 Roadmap atualizado (integração das 5 candidatas)

```
PHASE A — Aprimoramentos de baixo custo (semanas 1-2)
├─ A1 [S] Touring rank #1 — verification-before-completion-taco (per-claim evidence)
├─ A2 [M×1] Rank #4 transversal — MUST (E) idêntico em 7 skills (Touring + loop-engineering + TACO-cross-audit + TACO-skilling + TACO-subagent + TACO-wt + taco-planning) — single commit
├─ A3 [S] TACO-wt rank #5 — DOCUMENTAR exit codes 0/1/2/3
└─ A4 [S] TACO-cross-audit rank #3 — root-cause-before-fix gate + human pause

PHASE B — Aprimoramentos de médio custo (semanas 2-4)
├─ B1 [M] loop-engineering rank #2 — tdd-enforcer integration INNER 12
├─ B2 [M] TACO-subagent — HUMAN-PAUSE-GATE (entre SCOUT/ARCHITECT, DECOMPOSE/ENGINEERS)
├─ B3 [M] TACO-skilling — skill-eval-benchmark primitive (REFINE → script-graded)
├─ B4 [M] Criar skill nova `wait-what` ←── paralela
└─ B5 [M] Criar skill nova `tdd-enforcer` ←── paralela a B1

PHASE C — Aprimoramentos adicionais (semanas 4-6)
├─ C1 [M] Criar skill nova `grilling`
├─ C2 [M] Criar skill nova `critic-panel-as-skill` (promoção de fragment existente)
└─ C3 [M] Criar skill nova `human-pause-gate` (F-mechanism portable)

PHASE D — Validação cruzada (semana 6-8)
├─ D1 Run `triggering_audit.py` em cada skill modificada
├─ D2 Run `mine_transcripts.py --skill <name>` por 7 dias
├─ D3 Run quality_gate em todas as 15 skills (10 originais + 5 novas)
├─ D4 Run `loop_converged.py` final composite check
└─ D5 Red flags revalidados (RF1-RF8 abaixo)
```

### 10.5 Red flags adicionais que o sub-agent levantou

| # | Red flag | Implicação |
|:---:|---|---|
| RF1 | Tourism master D △ não ✗ | Confirmado — Symbol Verification Table já é D-mechanism |
| RF2 | TACO-wt B △; verificação empírica mostra ✓ | Scripts exit codes existem; gap é documentação |
| RF3 | TACO-cross-audit B △ confirmado ✓ | `loop_converged.py` é judge of record |
| RF4 | analysis-loop primitives pode não transferir para code-refactor | Cross-pollination entre analysis-loop e TACO-subagent deve ser TESTED |
| RF5 | TACO-skilling evidence type underspecified | REFINE loop tem gap D-meta |
| RF6 | "Two-implementations" risk se `critic-panel` for promovido a skill sem deletar fragment | Se C2 executar, deletar fragmento ADW correspondente |
| RF7 | TACO-subagent A-mechanism pode ser soft se runtime checker do rule for bypassed | Audit empírico |
| RF8 | Mattpocock v1.2.0 ago/2026 — primitives podem ter migrado | Verificar antes de citar verbatim |

### 10.6 Verificação adicional recomendada (não feita, listada para próximo turno)

| Item | Comando | Por quê |
|---|---|---|
| Confirmar critic-panel fragment shape | `cat /home/gabrielgadea/projects/touring/client/skills/Touring/adw-library/fragments/critic-panel.toml` | Decidir se promoção fragment→skill é trivial (1:1) |
| Confirmar loop_converged.py exit semantics | `python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --help` | Saber se 0/1 binário ou mais granular |
| Confirmar dimension_scorer exit code | `python3 ~/.claude/skills/taco-planning/scripts/dimension_scorer.py --help` | Já citado como B-mechanism |

---

*Documento versionado em `docs/plans/2026-09-01-skill-aprimoramento/plan.md` (v1.1 com correções pós-verificação). Correspondente memória persistente: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` (taxonomia A→G canônica). Gerado em 01/09/2026 sob comando explícito de Gabriel Gadea com constraints: (i) aperfeiçoar skills já carregadas; (ii) criar skills novas se necessário; (iii) plano profundo em .md.* Correspondente memória persistente: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` (taxonomia A→G canônica). Gerado em 01/09/2026 sob comando explícito de Gabriel Gadea com constraints: (i) aperfeiçoar skills já carregadas; (ii) criar novas skills se necessário; (iii) plano profundo em .md.*

---

## ANEXO Gabriel (01/09/2026, mesmo turno) — 4 ajustes finos

> **Contexto**: Gabriel fez 4 ajustes ao plano v1.1 entregue. Cada ajuste vem com justificativa operacional explícita. Este anexo documenta o estado pós-ajuste; v1.1 permanece como base, ajustes SOBREVÕEM-no-anexo.

### G.1 — EXCLUIR Candidata E original `human-pause-gate` e SUBSTITUIR

**Original** (v1.1): Candidata E = `human-pause-gate` (portable F-mechanism).

**Substituição** (v2.0): Candidata E vira **`decision-canvas`** (decision-explainer primitive).

| Aspecto | original | substituto |
|---|---|---|
| Função | Pausa humana genérica antes de irreversível | Quadro rico e completo de subsídios sempre que o agente apresenta decisão pendente |
| Trigger | "before we deploy ..." / explicit `/human-pause` | Auto-trigger em **toda vez** que o agente diz "preciso decidir X" ou apresenta ≥1 opção |
| Output | Pausa com 3-5 alternativas | Artefato `decision_canvas.md` com **9 seções** (o que / por que surgiu / o que decidir / alternativas / significado / implicações / recursos / plano operacional / o que mais surge) |
| Mecanismos A→G | F + A + G (leve) | **A + F + D** (mais forte — força explicação estruturada) |
| T-shirt | M | **M** (mesmo) |

Ver `explanation.md` Candidata E-NOVA para a spec completa.

### G.2 — `grilling` promovido a TRANSVERSAL (Marcel Point #2)

**Gabriel**: *"esse processo de `Interview the user relentlessly about a plan, decision, or idea until every branch of the design tree is resolved` precisa existir em todas as skills que elaboram planos e estratégias, como a própria loop-engineering."*

| Skill TACO-house que elabora plano/estratégia | Invocação grilling no |
|---|---|
| **loop-engineering** | Antes do `██ GATE HUMANO ██` strategy → plan (passo 9 vs 10) |
| **taco-planning** | Antes do §5 Verification Protocol (Stage 3 vs 4) |
| **TACO-subagent** | ENGINEERS phase após DECOMPOSE (Phase 4 vs 5) |
| **analysis-loop** | Loop passo 6-12 (PAINEL... já tem cego; grilling é interactive-zen) |
| **Briah** | Entre Passo 0 (medir) e Passo 7 operações (cada operação = round grilling) |

**T-shirt agregado**: M (criar primitive `grilling` standalone) + 5×S (MUST de 1 linha por skill) = **M×1 single commit**, paralelo ao Marcel Point #1 (Rank #4 E em 7 skills).

**Critério de verificação**:
```bash
grep -l "grilling" ~/.claude/skills/{loop-engineering,taco-planning,TACO-subagent,analysis-loop,briah}/SKILL.md | wc -l
# Expected: 5
```

### G.3 — critic-panel-as-skill: decisão de migração fica com Gabriel

**Crítica do Gabriel**: *"não estou convencido de que é necessário deletar `client/skills/Touring/adw-library/fragments/critic-panel.toml` no ato da promoção. Manter ambos = divergência garantida."*

A justificativa hard-rule-#1 original citava lesson 25/07/2026 (shell-injection chegou ao mirror mas não ao deployed `adw from-template`). **Mas**: aquela lesson era sobre **cópia física via `from-template`**, não sobre **referência runtime**. Cenários onde manter fragment temporariamente é defensável:

- Phase de transição (consumers ainda usam `--use critic-panel:panel`)
- Compatibilidade retroativa (ADW library pode preferir fragment)
- Bridge deprecation (flag `deprecated = true` por N versões)

**Posição revisada**: a decisão é de Gabriel, não minha. Apresento 3 opções:

| Opção | Custo | Benefício | Risco |
|---|---|---|---|
| **A** — Hard-rule-#1 strict: deletar fragment no ato | Consumers migram imediatamente | Single source-of-truth | ADW library pode quebrar consumers em voo |
| **B** — Pragmática dual: manter fragment com `deprecated=true` por 1 minor version | Manutenção dual temporária | Bridge seguro | Dívida técnica explícita por ~3 meses |
| **C** — Third way: fragment vira thin-wrapper sobre a skill (forward), deletar depois | Refactor mínimo do fragment | Compatibilidade + DRY eventual | Custo inicial de refactor do fragment |

**Status**: **C3 (Candidata C de v1.1 roadmap) bloqueia decisão do Gabriel** antes de executar. Não vou assumir.

### G.4 — Roadmap atualizado (PHASE C rebalanceado)

```
PHASE A (sem 1-2) — Tour master: A1 → A2 (transversal E) → A3 → A4
PHASE B (sem 2-4) — M items: B1 ‖ B2 ‖ B3 ‖ B4 (paralelos)
PHASE C (sem 4-6) — 4 candidatas novas + 1 transversal:
  ├─ C1 [S]   wait-what
  ├─ C2 [M]   tdd-enforcer (cross-cuts B1)
  ├─ C3 [M]   decision-canvas (E-NOVA substitui human-pause-gate)  ← NEW
  ├─ C4 [M]   grilling primitive standalone (M base da transversal)
  └─ C5 [M×1] grilling transversal: MUST em 5 skills (loop-engineering + taco-planning + TACO-subagent + analysis-loop + briah) — single commit  ← NEW
PHASE D (sem 6-8) — Validação: triggering_audit + mine_transcripts + quality_gate + loop_converged
```

**Total skills ao final da execução**: 10 originais + 5 candidatas (wait-what + tdd-enforcer + decision-canvas + grilling + critic-panel) = **15 skills finais** (era 13 em v1.1; +2 por decision-canvas e grilling; critic-panel já existia).

### G.5 — Lições pedagógicas adicionais

| # | Lição | Origem |
|:---:|---|---|
| 8 | **Se é gap de todas, deve constar em todas** | Marcel Point #1 do Gabriel (v5) |
| 9 | **Acknowledging classification error publicly** | v5 |
| 10 | **Substituir candidato por análogo melhor — usar descrição operacional, não nome herdado** | G.1 (human-pause-gate → decision-canvas) |
| 11 | **Quando justificativa tem lição histórica mas contexto diferente, admitir dúvida** | G.3 (critic-panel: hard-rule-#1 literal vs dual pragmatic) |
| 12 | **Transversal é o terceiro modo entre unit-tratamento e "todo-o-sistema"** | G.2 (grilling em 5 skills — nem unit, nem tudo) |

### G.6 — Decisões pendentes de Gabriel

| # | Decisão | Bloqueia |
|:---:|---|---|
| 1 | **C3 — critic-panel-as-skill: Opção A / B / C?** | Execução de C3 (criar skill + decidir destino do fragment) |
| 2 | **Grilling transversal: 5 skills é o conjunto certo?** (loop-engineering + taco-planning + TACO-subagent + analysis-loop + briah — ou faltou alguma?) | Execução de C5 |


---

## ANEXO Gabriel — DECISÃO C3 (Opção C — wrapper-forward) — 01/09/2026

> **Gabriel escolheu**: Opção C — `critic-panel-as-skill` vira thin-wrapper que **delega para a skill standalone**. Fragment reduz de 50L (3 personas, parallel fanout, quorum) → 5L wrapper com `type = "code"` + `command = "Skill: critic-panel-as-skill ..."`. Consumers (audit-pack, worker-critic-pair, fanout-lenses, cross-audit.toml:115) **não mudam**.
>
> **Confidence na decisão**: 0.92 (verificada por `grep` — 6/18 fragments ADW já usam `type = "code"`, pattern estabelecido).
>
> **Status**: DECIDIDO. Aguardando Gabriel sinalizar "execute".

### 1. Objective

**O que**: Migrar `client/skills/Touring/adw-library/fragments/critic-panel.toml` de fragment de 50L para wrapper de 5L que delega para uma nova skill `critic-panel-as-skill`. **T-shirt M agregado** (1 Edit fragmento + 1 Write skill + integration test + memory log).

**Por que**: (a) Consumers ADW referenciam critic-panel por nome lógico (`module = "critic-panel"`) e não devem mudar; (b) 6/18 fragments paralelos já usam `type = "code"`, então wrapper é pattern provado (não-inovação); (c) Lesson 25/07/2026 (shell-injection chegou ao mirror mas não ao deployed library) era sobre **cópia física**, este caso é **referência runtime via ADW runner** — contextos diferentes, hard-rule-#1 não se aplica literalmente.

### 2. Deliverables (numerados, atômicos, shippable independentemente)

#### D1 — Editar o fragmento `critic-panel.toml` para wrapper

**Local**: `client/skills/Touring/adw-library/fragments/critic-panel.toml`
**Ação**: Substituir 50L do fragment por 5L wrapper conforme diff abaixo.
**T-shirt**: **S** (1 Edit, conteúdo bem especificado).
**Critério de pronto**:
- Arquivo final tem ≤10L (era 50L+);
- Contém `type = "code"` com `command = "Skill: critic-panel-as-skill ..."`;
- Contém field `deprecated = "2026-09-01"` com explanation text;
- Mantém `[fragment]`, `inputs`, `entry` headers;
- Não referencia mais nenhum sub-node `[node.X]` (personas migraram para skill).

**Diff concreto**:
```toml
# Antes (50L+):
# Fragment `critic-panel` — N blind critics with DISTINCT lenses; code counts the quorum.
[fragment]
description = "Blind adversarial critic panel with distinct lenses and a code-counted quorum"
inputs = ["artifact", "bar", "quorum"]
entry = "panel"
[node.panel]
type = "parallel"
branches = ["correctness", "security", "reproducibility"]
...50L de personas...

# Depois (10L):
# Wrapper — delegates to Skill: critic-panel-as-skill (added 2026-09-01).
# Original personas (correctness, security, reproducibility) are now baked
# into the skill. ADW consumers continue pointing at "module = 'critic-panel'"
# without any change. See docs/plans/2026-09-01-skill-aprimoramento/plan.md
# (anexo G.7) for the decision rationale.
[fragment]
description = "DEPRECATED wrapper — delegates to Skill: critic-panel-as-skill"
deprecated = "2026-09-01"
inputs = ["artifact", "bar", "quorum"]
entry = "panel"
[node.panel]
type = "code"
command = "Skill: critic-panel-as-skill --artifact {{inputs.artifact}} --bar {{inputs.bar}} --quorum {{inputs.quorum}}"
```

#### D2 — Criar a skill `~/.claude/skills/critic-panel-as-skill/SKILL.md`

**Local**: `~/.claude/skills/critic-panel-as-skill/SKILL.md` (~150L).
**Ação**: Write com frontmatter + body.
**T-shirt**: **M** (~150L de conteúdo crítico + 3 personas embutidas).
**Critério de pronto**:
- Frontmatter tem `name: critic-panel-as-skill`, `description: <150 chars>` (REGRA #13 hygiene);
- Body ≤ 500L (REGRA #13);
- 3 personas do fragmento original (correctness, security, reproducibility) **incorporadas como persona blocks** dentro do skill body;
- Hard rules 1-4 do fragmento original preservados verbatim (fresh session per critic, different lenses, code-counted quorum, ESCALATE routes aside);
- Cross-link para o fragmento deprecated via nota `'promoted from fragments/critic-panel.toml (2026-09-01)'`;
- Trigger conditions explícitos (description-driven + "review this artifact", "panel the diff", etc.).

**Conteúdo mínimo esperado** (~150L):

```markdown
---
name: critic-panel-as-skill
description: Run a blind critic panel on an artifact (diff, design, plan, criacao.md).
N critics with DISTINCT lenses (correctness · security · reproducibility),
each in a fresh session, verdict counted by code (quorum: N/2 + 1 to PASS).
Use when an artifact needs adjudication that single-actor review cannot provide.
Triggers: "review against the bar", "panel the diff", "two-axis review",
"adversarial review", "/critic-panel".
---

# Critic Panel — Blind Adversarial Verdict

> **Migration note**: Promoted from `client/skills/Touring/adw-library/
fragments/critic-panel.toml` on 2026-09-01. The TOML fragment is now a
5-line wrapper (`type = "code"` + `command = "Skill: ..."`) that forwards
to this skill. ADW consumers continue to use `module = "critic-panel"`
unchanged.

## Hard rules

1. **Fresh session per critic** — inherited context contaminates the verdict
   this panel exists to distrust.
2. **Different lenses** — N critics with the same lens are N copies of one
   opinion (the lint rejects a panel whose personas share one lens).
3. **Code-counted quorum** — verdict is arithmetic over parseable outputs,
   never a model's summary of what the others said.
4. **ESCALATE routes aside WITHOUT spending a retry** — existence of
   doubt is not a retry trigger; a check that could not run is not
   something more attempts will fix.

## Personas (3 distinct lenses)

### Persona 1 — Correctness
[verbatim from fragment, ~25L persona block]

### Persona 2 — Security
[verbatim from fragment, ~25L persona block]

### Persona 3 — Reproducibility
[verbatim from fragment, ~25L persona block]

## Quorum arithmetic
[N/2 + 1 explanation, ~20L]

## Cross-link
- mattpocock/skills:code-review (two-axis separation inspiration)
- analysis-loop: PAINEL CEGO (this is its compositional cousin at .skills level)
- Adw fragment wrapper: `client/skills/Touring/adw-library/fragments/critic-panel.toml`
```

#### D3 — Integration test (`touring adw test cross-audit --dry-run`)

**Local**: shell command em touring workspace.
**Ação**: Bater `touring adw test cross-audit` em modo dry-run para validar que:
- ADW runner resolve `module = "critic-panel"` via wrapper;
- Consumers (`cross-audit.toml:115`) executam sem alteração;
- Verdict (PASS/REJECT/ESCALATE) chega ao cross-audit flow.

**T-shirt**: **S** (1 command, output JSON).

**Critério de pronto**:
- `touring adw test cross-audit --dry-run` exit 0;
- Output JSON mostra `node.panel.type = "code"` resolvido para `Skill: critic-panel-as-skill`;
- Latência do wrapper ≤ 100ms (ADW runner overhead).

#### D4 — `quality_gate.py` valida a skill nova

**Local**: `~/.claude/skills/critic-panel-as-skill/` (skill criada em D2).
**Ação**: `python3 ~/.claude/skills/TACO-skilling/scripts/quality_gate.py ~/.claude/skills/critic-panel-as-skill/SKILL.md`.
**T-shirt**: **S**.
**Critério de pronto**:
- quality_gate exit 0;
- 0 erros de REGRA #13 (name ≤ 64, description ≤ 1024, body < 500L);
- scoring ≥0.80 Gold;
- 6 BLOCK dims P0 verdes (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5 — não-aplicável a markdown, então 0).

#### D5 — `triggering_audit.py` na skill nova

**Local**: scan session history real.
**Ação**: `python3 ~/.claude/skills/TACO-skilling/scripts/triggering_audit.py critic-panel-as-skill`.
**T-shirt**: **S**.
**Critério de pronto**: triggering_audit confirma descrição tem alta precisão (sem falsos positivos observados em ≥50 sessões reais de teste).

#### D6 — Persistir a decisão em memory (v7 — Opção C chosen)

**Local**: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` (append v7).
**T-shirt**: **S** (heredoc append).
**Critério de pronto**: v7 contém `OPÇÃO C CONFIRMADA pelo Gabriel 01/09/2026` como primeira linha do append.

#### D7 — Update `MEMORY.md` line por skill v2 + Opção C

**Local**: `~/.claude/.../MEMORY.md`.
**T-shirt**: **S**.
**Critério de pronto**: linha reflete 6 fases (v1→v2→v3→v4→v5→v6→v7) e menciona explicitamente "Opção C escolhida pelo Gabriel para C3".

### 3. Timeline (sequenciado por dependências, all deps explicit + acyclic)

```
Sequência obrigatória (cada D depende do anterior):

  D1 (Edit fragmento)
    ↓ (consumer stability)
  D2 (Criar skill)
    ↓ (skill must exist before wrapper can reference it)
  D3 (Integration test `touring adw test cross-audit`)
    ↓ (verify wrapper + skill + consumers end-to-end)
  D4 (quality_gate.py)
    ↓ (sanity check before skill ships)
  D5 (triggering_audit.py)
    ↓ (description precision validation)
  D6 (Memory v7 append)
    ↓ (final canonical record)
  D7 (MEMORY.md line update)
```

Estimativa de execução (não em wall-clock, ordem de grandeza):

| Item | T-shirt | Estimativa horas | Notes |
|---|---|---|---|
| D1 | S | ~0.5h | Edit curto, conteúdo já bem especificado |
| D2 | M | ~2h | ~150L de skill com 3 personas verbatim |
| D3 | S | ~0.5h | 1 command + parse output |
| D4 | S | ~0.25h | quality_gate pode pegar hygiene issues |
| D5 | S | ~0.25h | triggering_audit precisa ≥50 sessões (provavelmente skip por agora — descrição é simples) |
| D6 | S | ~0.25h | heredoc append |
| D7 | S | ~0.25h | 1 Edit |
| **Total** | | **~4h** | single work session cabe inteiro |

### 4. Risks + Mitigations (probabilidade + impacto, per PLAN MODE self-validation)

| # | Risco | Probabilidade | Impacto | Mitigação |
|:---:|---|:---:|:---:|---|
| R1 | `[node.X.type = "code"]` fragment não executa `Skill:` tool (runner só aceita shell/CLI) | LOW (6 fragments já usam) | HIGH (consumers quebrariam) | Verificação empírica no D3: ver output JSON de `touring adw test cross-audit --dry-run`; se type=code não executar Skill, fallback para `command = "echo 'Skill: critic-panel-as-skill --artifact {{...}}'"` (output textual + executor) |
| R2 | Wrapper fragment quebra JSON parsing do ADW runner (5L tem sintaxe diferente) | LOW (skill-creator packs usaram type=code antes) | MED (D4 catches) | D4 valida estrutura; lint adw lint detecta fragment syntax |
| R3 | Skill description não triggera corretamente porque fragmento legacy já estava no contexto | LOW (fragment é curto, ADW runner carrega só headers) | MED (false negatives) | D5 triggering_audit; ajustar description-driven keywords se preciso |
| R4 | Consumers ADW (audit-pack, worker-critic-pair, fanout-lenses) esperam output estruturado específico do fragment | LOW (wrapper é `type=code`, output é capturado pelo `[node.X].result` no consumer) | LOW (D4 catches) | D3 valida output; se quebrar, ajustar `command` do wrapper para producer output contrato-específico |
| R5 | Migration confunde `Skill:` tool vs `touring adw run --use critic-panel:panel` (dois modos invocação) | MED (Gabriel pode não entender a coexistência) | LOW (docs resolvem) | Documentation no MEMORY.md note + brief explanation no D6 |
| R6 | D5 (triggering_audit) requer ≥50 sessões reais; corpus ainda pequeno | MED (corpus tem <50 sessões para esta skill) | LOW (D5 é sanity check, não bloqueante) | Marcar D5 como "best-effort, skip se corpus <50" no critério de pronto |
| R7 | Resistência a Opção C — alguém (humano ou agente em outro turno) pode tentar Opção A por "speed" sem ver evidência de 4+ consumers | LOW (esta decisão está documentada com 4 referências verbatim) | MED (regressão à decisão) | Risco mitigado pelo anexo G.7 — qualquer revisão tem rastro completo (F1 verbatim + F2 verbatim) |
| R8 | Cross-audit production runner tem cache de fragmento (HMR off) que ainda referencia o fragmento antigo após Edit | LOW (ADW runner é journaled, não cache-heavy) | MED (consumers veem "old" critic-panel) | D3 inclui verificação que wrapper foi aplicado; journal re-read forçado |
| R9 | Decision tem lição histórica (25/07/2026) mas contexto é diferente; alguém cita a lição errada | LOW (anexo G.7 cita lição explicitamente) | LOW (cite-anexo) | G.7 contém nota explícita "lesson 25/07 era cópia física, este caso é referência runtime — contextos diferentes" |

**Risco global** (composição de R1+R4+R8): **LOW-MED** — wrapper é trivial, ADW runner já tem pattern estabelecido, integration test detecta qualquer drift.

### 5. Self-Validation (per PLAN MODE self-validation rules)

| Regra | Avaliação |
|---|---|
| (1) Cada deliverable é atômico e independently shippable | ✓ — D1 é edit de 1 arquivo; D2 é write de 1 arquivo; D3-D7 são verification + logging, cada um commit-point próprio |
| (2) Dependências são explicit e acyclic | ✓ — sequência linear D1→D2→D3→D4→D5→D6→D7 (acíclico) |
| (3) Estimativas são realísticas | ✓ — total ~4h (cabe em 1 work session); T-shirts calibrados com skill-creator's prior `quality_gate.py` cycle times |
| (4) Riscos têm mitigações | ✓ — 9 riscos; mitigações concretas (R1→fallback command shape; R2→D4 catches; R3→D5 validation; etc.) |

### 6. Verification final (composite gate)

```bash
# Após D1-D7 executados:
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py \
  --task skill-aprimoramento-c3-opcC \
  --scope ~/.claude/skills/ \
  --rust-full
# exit 0 é o ÚNICO "pronto". LEI L2: sinal ausente ≠ zero.
```

### 7. Status final da decisão C3

| Aspecto | Status |
|---|---|
| Decisão | **Opção C — Wrapper-forward** (CONFIRMADA por Gabriel 01/09/2026) |
| Confidence | 0.92 (VGP-validada: 6/18 fragments já usam type=code) |
| Aguardando | Sinal de execução de Gabriel |
| Próximo passo (se Gabriel der "execute") | D1 → D2 → D3 → D4 → D5 → D6 → D7 (sequential) |

