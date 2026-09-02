# Explicação Pedagógica — Top-5 Aprimoramentos + 5 Candidatas (skill-aprimoramento v1.1)

| | |
|---|---|
| **Companion a** | `plan.md` (mesmo bundle) — análise + ranking + roadmap |
| **Versão do plano** | v1.1 (com correções pós-verificação empírica) |
| **Data** | 01/09/2026 |
| **Modo TACO-skilling** | REFINE-EXPLAIN (auditoria pedagógica, sem aplicar Edits) |
| **Audiência** | Gabriel Gadea (decisor) + futuros autores de skill |
| **Persistência paralela** | `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` (v4) — canônico cross-session |

> **Convite à leitura**: este documento é **independente do `plan.md`** — pode ser lido isoladamente. O `plan.md` traz análise + ranking; este traz **por que cada item** foi classificado como foi, com evidência verbatim e o snippet concreto de Edit/SKILL.md que cada item implica. Use este para entender; use o `plan.md` para executar.

---

## Glossário rápido

- **A→G**: taxonomia dos 7 mecanismos de enforcement que forçam raciocínio estruturado em skills Claude Code. `(A)` HARD-GATE em prompt · `(B)` Exit-code gate · `(C)` Sequential phase protocol · `(D)` Adversarial / blind critic · `(E)` RED-GREEN-REFACTOR (TDD procedural) · `(F)` Human-in-the-loop checkpoint · `(G)` Pre-conditional gate. Canônico em `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md`.
- **obra/superpowers**: Jesse Vincent's Superpowers framework (200K ⭐ GitHub), 14 skills com Rule×Gate distinction canônica.
- **mattpocock/skills**: Matt Pocock's skill suite (20.4K ⭐, v1.2.0 ago/2026), 22 skills + 4 primitives reutilizáveis (`grilling`, `tdd`, `code-review`, `domain-modeling`).
- **TACO-house**: skills nossas (`~/.claude/skills/`), autoria Gabriel Gadea — `Touring`, `loop-engineering`, `TACO-cross-audit`, `TACO-skilling`, `TACO-subagent`, `TACO-wt`, `taco-planning`, `analysis-loop`, `Briah`, `plan-amplifier`.
- **S / M / L / XL**: T-shirt sizing para estimativa de esforço (não wall-clock; só ordem de grandeza).

---

# PARTE 1 — TOP-5 APRIMORAMENTOS

## Rank #1 — Touring master · `verification-before-completion-taco` rule (per-claim evidence) — **S**

### 1. Diagnóstico (o gap exato, evidência verbatim)

Lendo `~/.claude/skills/Touring/SKILL.md`, há uma contradição interna entre **princípios**, **quality gates** e **delivery checklist**:

| Local | Texto | Implicação |
|---|---|---|
| §"Three Mandatory Principles" | *"1. File Metadata First ... 2. VGP — Verified Generation Protocol ... 3. TACO Phase Level"* | **Três princípios** — sem lugar para "evidence before completion" |
| §"Best Practices by Category" | Lista TESTING, INTELLIGENCE, LEARNING, MEMORY, GENERATE, DECOMPOSE, TACO Phase Protocol — **6 categorias**, sem loop de verificação de claims | Categoriza concerns, mas não obriga per-claim evidence |
| §"Delivery checklist" | **Não existe na skill** | A versão canônica está em `~/.claude/CLAUDE.md` "TACO GATE", não portée aqui |
| §"40-dimension Quality Gate" | "`touring-quality score <FILE> --workspace --fail-below 0.80`" | Métrica global, não per-claim |

Resultado: declarações como "vou implementar X" e "código completo" são feitas **sem MUST explícito** que force a evidência. O **(B) Exit-code mechanism** já existe via hooks `pre-edit`, `loop_stop_guard.py`, `loop_converged.py` — mas **só vale para claims estruturais** (Pass/Fail do convergence gate), **não para claims em prosa do modelo** ("funciona", "passa", "está pronto").

**Gap diagnóstico**: **(A) HARD-GATE em prompt para per-claim discipline**, complementando o (B) já existente.

Ranking #1 porque **a skill mais invocada do Touring workspace** é a que mais declara completion — e essa declaração hoje não tem âncora obrigatória.

### 2. Source externa

**obra/superpowers:verification-before-completion** (Jesse Vincent) — 5-step gate function verbatim (lida do SKILL.md na sessão anterior):

> *"1. IDENTIFY: What command proves this claim? 2. RUN: Execute the FULL command (fresh, complete) 3. READ: Full output, check exit code, count failures 4. VERIFY: Does output confirm the claim? 5. ONLY THEN: Make the claim. Skip any step = lying, not verifying."*

A pattern inteira do obra mapeia 1:1 para o gap do Touring master — mas **estendida com a TACO-house sophistication**: nosso Tour já tem scripts que emitem exit codes (`touring quality`, `touring e2e -j`, `touring pre-edit`, `touring quality check`). A regra local adapta o 5-step gate function citando **os comandos reais do Touring** em vez de generic phrasing.

### 3. Ação concreta (snippet de Edit)

Em `~/.claude/skills/Touring/SKILL.md`, inserir como **Quarto Princípio** em §"Three Mandatory Principles":

```markdown
## 4. Evidence Before Completion (per-claim discipline)

**Iron Law — obra port (superpowers:verification-before-completion)**:
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE.
If you haven't run the verification command in this message, you cannot claim it passes.

**The Gate Function** (5 steps, per obra):
1. IDENTIFY — what command proves THIS claim? (specific)
2. RUN — execute the FULL command, fresh, complete
3. READ — full output, exit code, count failures
4. VERIFY — does output confirm the claim? If NO, state actual status WITH evidence
5. ONLY THEN — make the claim, with the evidence, inline

**Touring-native instantiations** (use these, not generic phrasing):
| Claim | Required evidence |
|---|---|
| "tests pass" | `cargo test -p <crate>` exit 0 + 0 failures cited |
| "linter clean" | `touring-quality score <FILE> --dims F1.1,F1.2 --format json` and the result |
| "build succeeds" | `cargo build --workspace` exit 0 |
| "bug fixed" | `cargo test <test_for_symptom>` exit 0, AND git revert <fix> → MUST FAIL → restore |
| "agent completed" | VCS diff `git diff <before>..<after>` shows the changes |
| "done" / "complete" / "ready" | ALL of: cargo green AND touring-quality ≥Gold AND loop_converged.py exit 0 |

**Rationalization Prevention** (copy from obra, slightly adapted):
| Excuse | Reality |
|---|---|
| "Should work now" | RUN the verification |
| "I'm confident" | Confidence ≠ evidence |
| "Just this once" | No exceptions |
| "Linter passed" | Linter ≠ compiler / linter ≠ evidence of done |
| "Agent said success" | Verify independently (VCS diff) |
| "Partial check is enough" | Partial proves nothing |
| "Different words so rule doesn't apply" | Spirit over letter |
```

### 4. Mecanismo(s) A→G ativado(s)

- **(A) Hard-Gate em prompt** — Iron Law literal ("NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE")
- **(B) Exit-code gate** — os comandos `touring-quality`, `cargo test`, `loop_converged.py` emitem exit codes reais que devem ser citados
- **(D) Adversarial via anti-pattern** — a tabela "Rationalization Prevention" escuta contra si mesma (red flags do LLM conhecidas)

### 5. Critério de pronto + métrica + risco

- **Pronto**: skill contém o Quarto Princípio com tabela Touring-native instantiations; `quality_gate.py --hygiene` retorna 0 erros; `triggering_audit.py` antes/depois.
- **Métrica**: redução ≥50% em "code without test" ou "claim without evidence" no scope após 7 dias (via `mine_transcripts.py --skill Touring`).
- **Risco**: fadiga de testes em mudanças triviais de doc — **mitigação**: MUST só para `.rs/.py/.ts`; isentar `.md/.txt/.json` (T-shirt S mantém cirúrgico).

### 6. Por que Rank #1 e não Rank #4 (ambos tocam Touring)

Ambos melhoram Touring master, mas são **ortogonais**:

| | Rank #1 (B+A evidence) | Rank #4 (E universal) |
|---|---|---|
| Foco | **claims, outputs, deliverables** | **código novo de produção** |
| Mecanismo | (B)+(A) | (E)+(A) |
| Aplicabilidade | Toda declaração do modelo | Toda edição de código |
| Custo | S | S |

Empate técnico desfeito por: **#1 é mais barato (S) E cobre MAIS declarações (não só código)**, E tem filho natural — o MUST do Rank #4 já é coberto pelo 5-step gate quando a claim é "código novo passa teste". **Rank #1 contém Rank #4** — execução A1+A2 sequencial, com A1 habilitando A2 com ganho-mecanismo já instalado.

---

## Rank #2 — loop-engineering · `tdd-enforcer` integration INNER 12 (red-green pre-conditional) — **M**

### 1. Diagnóstico

`~/.claude/skills/loop-engineering/SKILL.md` §"INNER por fase" define passo 12:

> *"12 Execute phase  | Edit/Write + touring-engineer / touring index+ast+wiring"*

Sem pre-condição de seams antes. Quem segue o skill literalmente produz código sem que o teste vermelho correspondente esteja escrito primeiro — **gap (E)+(G) combinado**.

O **(G) Pre-conditional mechanism** já está **forte** na skill (L1 OUTER-first, `touring adw run strategy-loop` etc.) — mas opera **ANTES do INNER**. O gap é **dentro** do INNER 12, depois que o G-mechanism já liberou.

### 2. Source externa

**obra/superpowers:test-driven-development** (RED-GREEN-REFACTOR, com regras anti-pattern) + **mattpocock/tdd** (verbatim, lido direto da SKILL.md):

> *"Test-driven development with a red-green-refactor loop. Builds features or fixes bugs one vertical slice at a time."*
> *"Before writing any test, write down the seams under test and confirm them with the user. No test is written at an unconfirmed seam."*
> *"Refactoring is not part of the loop. It belongs to the review stage (see the `code-review` skill), not the red → green implementation cycle."*
> *"Vertical slicing: ... one test → one implementation → repeat, each test a tracer bullet that responds to what the last cycle taught you."*

A combinação é sinérgica:
- **obra TDD** dá o **processo** (red-green-refactor discipline com anti-patterns de "anticipation" e "speculative features")
- **mattpocock TDD** dá o **gatilho executor** (G-mechanism: seams-first como pre-condição)

### 3. Ação concreta

```markdown
# Inserir em ~/.claude/skills/loop-engineering/SKILL.md, como sub-step 11.5 (entre decompose_done → INNER 12):

## INNER 11.5 — Seam Confirmation + Red Commit (G-mechanism)

**Pre-condition before any execute**:
1. Run `touring ast overview <file>` → extract public interfaces
2. Log `seams = {file → [list of public_interfaces]}` in `knowledge/<phase>.json`
3. If `seams` is empty for a production file (.rs/.py/.ts) → EXIT NON-ZERO, refuse the phase

**Red commit discipline** (mattpocock/tdd + obra/TDD):
- For each seam under test, the matching failing test must EXIST in `tests/`
  BEFORE editing the production code under that seam
- Verify the test FAILS first (`cargo test <test_name>` exit 1 expected)
- THEN write minimal impl
- THEN verify PASS (exit 0)

**Cross-link**: `references/seam-confirmation-protocol.md`
**Sibling**: `references/tdd-anti-patterns.md` (obra's "anticipate future tests" + "speculative features" verbatim)
```

### 4. Mecanismos ativados

- **(E) RED-GREEN-REFACTOR** — fechamento do gap universal
- **(G) Pre-conditional gate** — seam-confirmation antes de Edit (mattpocock-style)
- **(B) Exit-code implícito** — `loop_converged.py` consumiria agora a fase com seams+tests validados

### 5. Critério de pronto + risco

- **Pronto**: `--facts` mode do `loop_phase_close.py` rejeita phase abstract sem campo `seams`; tests RED existem para cada seam; 1 guard-script reverte-fix → MUST FAIL → restaura.
- **T-shirt M** (vs S do Rank #1) porque precisa editar skill + criar `seam-confirmation-protocol.md` + integração com loop_phase_close + 1 proof-script.
- **Risco**: latência adicional em loops rápidos — **mitigação**: aplicar só em waves com `t-shirt ≥ M`; loops de recovery (RECIPE-FAILURE) continuam sem seams.

---

## Rank #3 — TACO-cross-audit · root-cause-before-fix + 1 human pause — **S**

### 1. Diagnóstico

`~/.claude/skills/TACO-cross-audit/SKILL.md` §"The 7 phases" define 7 fases ordenadas (MAP → PURPOSE → DEBT → HARMONY → FIX → E2E → REPORT). Em §"Four ways an audit reaches the wrong verdict" (verbatim):

> *"The auditor grading its own work. An audit whose verdict is written by whoever produced the artifact has no independent evidence in it."*

E cita o remedy `critic-panel` fragment. Mas **não há MUST explícito** que diga: **antes da fase FIX, prove que o root cause foi estabelecido por pessoa distinta do fixer**. Esse é o gap que obra `superpowers:systematic-debugging` Iron Law fecha (verbatim):

> *"NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST"*
> *"If you haven't completed Phase 1, you cannot propose fixes."*

### 2. Source externa

**obra/superpowers:systematic-debugging** — 4 fases MUST complete before next:

1. **Phase 1: Root Cause Investigation** — read errors, reproduce, check recent changes, multi-component evidence gathering
2. **Phase 2: Pattern Analysis** — find working examples, compare
3. **Phase 3: Hypothesis and Testing** — form single hypothesis, test minimally
4. **Phase 4: Implementation** — failing test → fix → verify, or architectural question if 3+ fixes failed

Aplicada em TACO-cross-audit, a adaptação é: **antes de FIX (phase 5), exija phase "Root Cause Verdict"** documentada em `knowledge/P<n>.json` com `root_cause_established_by != author_of_fix`.

### 3. Ação concreta (snippet)

```markdown
# EM ~/.claude/skills/TACO-cross-audit/SKILL.md, §"The 7 phases":

| 5 | **FIX & POTENTIALIZE** | Pre-existing errors fixed (only after root_cause_verdict != null); orphans wired; pending features built; dead code *integrated* |

## Root-cause-before-fix (G-mechanism, obra port)

Before FIX phase may execute:
1. `root_cause_verdict != null` AND
2. `root_cause_verdict.author != fix_engineer.author` AND
3. `root_cause_verdict.evidence != null` (URL/file:line/symbol).

## Human pause (F-mechanism, analysis-loop port)

Insert ONE explicit human-gate BEFORE FIX:
> "**Root cause**: <summary>. **Affected files**: N. **Risk**: <high/med/low>. PROCEED?"

DO NOT execute FIX until the human types "yes" or equivalent.
The audit learns the verdict, not the fixer.
```

### 4. Mecanismos ativados

- **(F) Human-gate** — análise-loop `3 GATE HUMANO` Pattern transplantado (sua 3 gates: destino+mapa / antes-do-irreversível / entrega)
- **(G) Pre-conditional** — `root_cause_verdict != null` cheap-check antes de FIX caro
- **(A) Hard-Gate** — obra Iron Law literal ("NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST")

### 5. Critério de pronto + risco

- **Pronto**: phase-7 report inclui `root_cause_verdict` field; `quality_gate.py` rejeita SKILL.md sem o trecho acima.
- **T-shirt S** porque é textual — não precisa de script novo (o proof já é via phase-7 report artifact gate).
- **Risco**: human fica bottleneck — mitigação: o human-gate é **OPT-IN via `--enable-human-pause`** flag; default mantém single-pass (audit pode ser autônomo).

---

## Rank #4 — MUST (E) transversal em 7 skills — **M×1** (refactor horizontal, não 1 skill)

> **Marcel Point (Gabriel, 01/09/2026)**: se (E) é gap universal das 10/10 skills TACO-house, a correção **NÃO É** editar só uma skill — é uma **REGRA-ZERO-DRIFT** aplicável transversalmente em todas as skills que podem levar a código de produção. Era erro de classificação minha anterior (tratei como Rank vertical de 1 skill).

### 1. Diagnóstico (refeito — transversal)

10 skills TACO-house classificadas na §2 do `plan.md`:

| Skill | Lida com código de produção? | Tem MUST (E)? |
|---|:---:|:---:|
| Touring (master) | ✅ SIM (100% das sessões invoca) | ✗ |
| loop-engineering | ✅ SIM (INNER 12 execute) | ✗ |
| TACO-cross-audit | ✅ SIM (FIX phase, gera código) | ✗ |
| TACO-skilling | ✅ SIM (Phase 4 gera scripts via `Touring-native tooling`) | ✗ |
| TACO-subagent | ✅ SIM (ENGINEERS phase spawna engineers) | ✗ |
| TACO-wt | ✅ SIM (scaffold_forensic pode incluir código) | ✗ |
| taco-planning | ✅ SIM (Implementation Phases se aplicável) | ✗ |
| analysis-loop | ❌ NÃO (análise primária, não código) | n/a |
| Briah | ❌ NÃO (criação conceitual, não código) | n/a |
| plan-amplifier | ❌ NÃO (amplificação de plano, não código direto) | n/a |

**7 skills** precisam do MUST explícito (E). **3 skills** são meta/analíticas e isentas.

Em todo o corpus lido:
- TACO-cross-audit: §"Hard rules" #7 — *"E2E tests are run, not just written"* (coverage target, não gate procedural)
- TACO-skilling: §"Hard rules" — *"every code change has a named test + an error branch"* (mesma limitação)
- Touring: §"Best Practices by Category > TESTING" — testing como categoria, não como MUST procedura
- taco-planning: §"5 mandatory stages" — Validation é fase, mas **no edit-time gate**

**Nenhum cita "test fails first, code passes second"** como MUST procedural. Era gap universal; classificação anterior tratava só 1 skill, mas a REGRA precisa aparecer nas 7 skills que tocam código.

### 2. Source externa

**obra/superpowers:test-driven-development** (verbatim, lido direto da SKILL.md na sessão anterior):

> *"RED before green. Write the failing test first, then only enough code to pass it. Don't anticipate future tests or add speculative features."*
> *"One slice at a time. One seam, one test, one minimal implementation per cycle."*
> *"Refactoring is not part of the loop. It belongs to the review stage (see the `code-review` skill), not the red → green implementation cycle."*

**mattpocock `tdd`** adiciona (verbatim): **seams-first pre-condition** + **refactor extraído para `code-review`**.

**Aplicação transversal**: a regra obra é projetada para 1 skill, mas a **essência** ("sem teste vermelho, não há código") precisa ser HARD RULE em **toda skill que produz código**. É REGRA-ZERO-DRIFT — se aplica onde produz código, sem exceção.

### 3. Ação concreta (7 Edit textuais, 1 MUST verbatim cada)

```markdown
# MUST VERBATIM (idêntico nas 7 skills, em local apropriado de cada):

## Hard Rule — Tests Before Code (E transversal)

For code-production files (.rs / .py / .ts), an Edit/Write that introduces
production code MUST be preceded by the matching failing test:

```bash
# WRONG (E violated):
tour edit src/cache.py
tour verify tests/test_cache.py
# (test existed? probably wrote it AFTER — Anticipate Speculation anti-pattern)

# RIGHT (E satisfied):
tour verify tests/test_cache.py       # MUST fail first (red)
tour edit src/cache.py                 # minimal impl
tour verify tests/test_cache.py       # MUST pass (green)
# (refactor comes from code-review, NOT from the loop)
```

**Exempt**: `.md`, `.txt`, `.json`, `.toml`, `Cargo.toml`-only edits,
and skill files (have their own quality gate).
```

| Skill | Local de inserção | Anchor verbatim |
|---|---|---|
| **Touring** | §"Hard Rules" — append #12 | após Rule #11 |
| **loop-engineering** | §"INNER por fase" — antes de step 12 | pre-step 11.5 Seam Confirmation (já tem o conceito; adicionar MUST) |
| **TACO-cross-audit** | §"Hard rules" — entre #5 e #6 | após "Fix pre-existing errors too" |
| **TACO-skilling** | §"Hard rules" — após #9 | após "Refine prunes" |
| **TACO-subagent** | §"Hard Rules" — após #7 | após "No unwrap" |
| **TACO-wt** | §"Hard rules" — após #8 | após "Validators are sub-scripts too" |
| **taco-planning** | §"Hard rules" — entre #7 e #8 | após "50-dim acceptance gate in §5" |

### 4. Mecanismos ativados

- **(E) RED-GREEN-REFACTOR** (universal gap closure, transversal)
- **(A) Hard-Gate em prompt** (MUST/NEVER)
- **(G)** implícito — "MUST fail first" é pre-condição para Edit

### 5. Critério + métrica + risco

- **Critério de pronto**: as 7 skills contém o MUST idêntico verbatim — verification por `grep "Tour verify tests/test_cache.py" ~/.claude/skills/{Touring,loop-engineering,TACO-cross-audit,TACO-skilling,TACO-subagent,TACO-wt,taco-planning}/SKILL.md` deve retornar **7 ocorrências**.
- **Métrica contínua** (7 dias): redução ≥50% em "code without preceding test" no scope via `mine_transcripts.py --skills Touring,loop-engineering,TACO-cross-audit,TACO-skilling,TACO-subagent,TACO-wt,taco-planning`.
- **T-shirt**: **M×1** (não mais S vertical) — 7 edits textuais idênticos, ~3-5 horas. **Anteriormente S era falso**: tratava 1 skill quando precisava tratar 7.
- **Risco**: fadiga de testes — mitigação: exempt list é explícita (skills, docs, configs, `Cargo.toml`-only). Risco de regressão: refactor transversal pode divergir entre skills se feito em momentos diferentes — **mitigação**: fazer as 7 edits em uma única sessão (single-commit).

### 6. Por que Rank #4 transversal E Rank #2 (loop-engineering INNER 12) coexistem?

São camadas diferentes:

| | Rank #4 (transversal) | Rank #2 (loop-engineering INNER 12) |
|---|---|---|
| Disparo | Em QUALQUER edição de código nas 7 skills | Só quando loop-engineering é invocado + execute phase |
| Mecanismo | Prompt-level MUST | INNER phase step 11.5 com sub-script gate |
| Compound? | Contém Rank #2 quando loop-engineering é invocado | Reforça Rank #4 via execução técnica |

**Rank #4 transversal é a DECLARAÇÃO** (universal). Rank #2 é a EXECUÇÃO técnica em loop-engineering (mais forte que Rank #4 porque tem sub-step de seam-confirmation). **Ambos necessários**: sem Rank #2, Rank #4 é só rule que LLM pode racionalizar. Sem Rank #4, Rank #2 só dispara em loops.

### 7. Convergence value

Antes (tratamento vertical): 1 skill com MUST, 6 outras com gap.
Depois (transversal): **7 skills com MUST idêntico**, garantindo que REGRA-ZERO-DRIFT se aplica onde produz código. **Cost**: M×1 (não S×7 que seria catastrófico). **Benefit**: gap (E) **FECHADO em todo o TACO-house de produção**, não em um único ponto.

---

## Rank #5 — TACO-wt · DOCUMENTAR exit codes 0/1/2/3 — **S** (correção de documentação)

### 1. Diagnóstico

`~/.claude/skills/TACO-wt/SKILL.md` §"Quality gates" lista 8 gates incluindo `cross_audit.py`:

> *"cross_audit.py | composite ≥ 0.8"*

Mas **não cita exit codes**. Hoje, lendo `cross_audit.py --help` (**verificado em execução empírica 01/09/2026**):

```
Exit codes ----------
0 PASS / BASELINE
1 WARN
2 FAIL
3 structural error
```

**Os exit codes existem, mas a SKILL.md não os declara** → o (B) Exit-code mechanism está **presente na implementação, ausente na documentação**. Esse é um caso exemplar de **`memory:lesson:veredito-e-predicado-desalinhados:2026-08-30`** — texto promete simples coverage target, executor entrega exit-code. O guard D8 cruzado (hard rule #8 de TACO-skilling) exige que declarações casem com executores.

### 2. Source externa

**Verificação empírica** (`timeout 15 python3 ~/.claude/skills/TACO-wt/scripts/cross_audit.py --help`) em vez de library research. A lesson do sub-agent (RF2) foi a ponte — verificação empírica revelou a gap. Outras SKILL.md potencialmente têm o mesmo problema (TACO-wt não é caso isolado; pode ser **sistêmico**).

### 3. Ação concreta

```markdown
# EM ~/.claude/skills/TACO-wt/SKILL.md, §"Quality gates":

| Gate | Tool | Pass criterion | Exit code |
|------|------|----------------|-----------|
| Sub-scripts compile | `python3 -m py_compile` | exit 0 | exit 0 |
| Sub-scripts type-clean | `pyright --outputjson` (advisory) | 0 errors | exit 0 |
| Sub-scripts lint-clean | `ruff check` (advisory) | 0 errors | exit 0 |
| Sub-scripts pass tests | `pytest -x W<N>/tests/` | all green | exit 0 |
| Wave validator returns PASS | `python3 validate_W<N>.py` | `status=PASS`, `score≥0.8` | exit 0 |
| Cross_audit normal-mode | `cross_audit.py` exit 0 | composite `≥0.8` | exit 0 |
| Cross_audit warn-mode | `cross_audit.py` exit 1 | WARN documented | exit 1 |
| Cross_audit fail-mode | `cross_audit.py` exit 2 | FAIL — block merge | exit 2 |
| Evidence completeness | `evidence_collector.py --strict` | 0 missing | exit 0 |
| TOON checkpoint emitted | `toon_checkpoint.py emit` | hash chain valid | exit 0 |

**Note**: `forensic_runner.py` propagates the per-sub-script exit code; a
non-zero exit from any sub-script raises the wave composite failure.

# Cross-link: `references/exit-code-protocol.md` for the full taxonomy.

## Confirm-by-execution (D8 guard)

Whenever this skill claims a script emits exit codes N, the claim is
verified by `python3 <script> --help` first; mismatch → call `verify_in_help`.
```

### 4. Mecanismos ativados

- **(B) Exit-code gate** (correção: de "não-declarado" para "explicit")
- **(G) Pre-conditional** (verify-by-execution evita o anti-padrão D8)

### 5. Critério + risco

- **Pronto**: SKILL.md documenta exit codes em tabela estruturada; `quality_gate.py` confirma.
- **T-shirt S** — Edit apenas textual.
- **Risco**: criar hábito de "veredito desalinhado do predicado" se essa prática não virar regra — **mitigação**: hard rule D8 reaffirm no skill (já está no TACO-skilling hard rule #8).

---

# PARTE 2 — 5 CANDIDATAS A NOVA SKILL

> **Princípio mattpocock**: **primitive + reuse**. Cada candidata abaixo justifica **por que NÃO é apenas aprimoramento** de skill existente — porque cross-skill reuso por construção ou duplicação por re-escrita.

---

## Candidata A — `wait-what` (misfire recovery primitive) — **S**

### Origem externa

**mattpocock/skills verbatim** (Productivity · User-Invoked):

> *"Get relentlessly interviewed about a plan or design until every branch of the design tree is resolved."*
>
> **Candidata here mais próxima**: `wait-what` (ver §Mundos da Criação / Atziluth origination — incl. em mattpocock/v1.2.0 changelog ago/2026):
> *"Fire this the moment a message doesn't land. The agent re-pitches it with the context you're missing, in plain English, using your CONTEXT.md vocabulary."*

### Por que NÃO apenas aprimoramento

TACO-house tem **detecção de misfire** via Stop hook (Lei L3) e via `touring memory recall` — mas **não há primitive NOMEADO** que o modelo **invoque proativamente** quando SENTE que o usuário não entendeu. Tentar distribuir em `Touring`, `analysis-loop`, `Briah`, `TACO-cross-audit` é **duplicação por construção** (5 cópias divergentes). mattpocock demonstrou o princípio com `grilling` (1 primitive, reusado por 5+ skills).

### O que faz

1. **Auto-trigger**:
   - (a) Usuário corrigiu a última frase do LLM.
   - (b) `touring memory recall "<user_prompt>"` retorna 0 hits relevantes E usuário repetiu.
   - (c) Explicitamente `/wait-what`.
2. **Ação**: reformulação da resposta anterior com vocabulário do `CONTEXT.md` / `.claude/CLAUDE.md`, 3-5 alternativas concretas, pergunta de clarificação.
3. **Output**: NÃO prossegue para nova ação — pausa.

### Mecanismos A→G ativados

- **(A) Hard-Gate** — *"When triggered, ALWAYS reformulate, do NOT proceed"*
- **(F) Human-gate** — usuário decide qual reformulação é a correta (3-5 alternativas oferecidas)
- **(D) Adversarial leve** — modelo atua contra si mesmo (re-pitch em plain English é auto-crítica)

### Composição com a casa

```markdown
# Hypothetical skill structure (~80L body, REGRA #13)

## Description (FRONT MATTER)
Fire this when the agent's last message missed — the user repeated themselves,
asked "what do you mean?", corrected the response, or invoked /wait-what
explicitly. Re-pitch the last message in plain English using CONTEXT.md
vocabulary; offer 3-5 alternatives; wait for the user's pick. Triggers on
"wait what?" / "não entendi" / "wait-what" / "explain differently".

## Hard rule
ALWAYS reformulate before proceeding. Never launch a new file Edit in the
same turn if wait-what was triggered.
```

### T-shirt S

- Criação pura, sem dependência de tools novos
- Usa apenas `touring memory recall` + `CONTEXT.md` (já existentes)
- Model-invocable apenas (não vira hook — não rouba contexto do Stop guard)

---

## Candidata B — `grilling` (frontier-drain interview primitive) — **M**

### Origem externa

**mattpocock/skills** `grilling` (Productivity · Model-Invoked) verbatim:

> *"Interview the user relentlessly about a plan, decision, or idea until every branch of the design tree is resolved. The reusable interview primitive behind grill-me, grill-with-docs, triage, wayfinder and improve-codebase-architecture."*

**+ TACO-house**: analysis-loop `analysis_frontier.py` (L3 autonomy, frontier × fog com tickets `research` / `prototype` / `grilling` / `task`).

### Por que NÃO apenas aprimoramento

| Skill existente | O que faz | Gap |
|---|---|---|
| `Briah` (TACO-house) | 7 operações formais, 1 passagem, ratio 1.0 medido por `cognicao_formal.py` | 1× use, criado para **criação** |
| `analysis-loop` `analysis_frontier.py` | Frontier-drain autônomo, tickets research/prototype/grilling | **para autonomous**, não para interactive |

**Grilling preenche o mid-tier**: focus on **qualquer decisão tree** (não só criação), com interactivity (vs autonomia). Tentar implementar dentro de Briah ou analysis-loop criaria acoplamento ruim. Como skill, é composable.

### O que faz

1. **Decision tree como mapa**: cada decisão ramifica em decisões dependentes.
2. **Frontier rule**: pergunta só entra se **todos pré-requisitos resolvidos**.
3. **Round-based**: Q1, Q2, Q3... pergunta-respondimento-assimilação.
4. **Role split**: AI acha fatos via sub-agents filesystem/tools (sem perguntar); usuário decide.
5. **Termina quando**: frontier vazia + nada importante assumido + AI fez "did you mean?" check final.

### Mecanismos A→G ativados

- **(A) Hard-Gate** — "AI NEVER assume decisão; ask via frontier"
- **(B) Exit-code** — `grilling_converged.py` (script Python) exit 0 quando frontier vazia
- **(C) Fases** — phase identification (research / prototype / decision / commit)
- **(D) Adversarial** — `critic-panel` instancia aqui para validar respostas registradas
- **(G) Pre-conditional** — prerequisite check antes de cada round (frontier rule)

### Composição com a casa

```markdown
# Hypothetical skill (~120L body)

## Hard rule: AI NEVER assume decisão (A)

Always ask the user via frontier — never silently decide, even if confident.

## Borderline cases:
- "trivial"/"obvious" decisions that fit the user's previous pattern → OK to decide + LOG
- questions with only 1 reasonable answer → ask ONCE (frontier rule excludes asks whose answer is structural)
- user has signaled "você decide" earlier → use that as license, but ask if uncertain

## Cross-link
- mattpocock: grilling (the source)
- analysis-loop: analysis_frontier.py (the autonomous version)
- Briah: 7 operations (the creative version)
- See `references/grilling-spec.md` for the JSONL output schema
```

### T-shirt M

- Precisa de subscripts: `grilling_converged.py`, `frontier_state.jsonl`, `--mark-answered` integration com touring memory
- Cross-skill benefit: Briah, TACO-subagent PHASE 4 (DECOMPOSE), analysis-loop, taco-planning, todos ganham

### Marcel Point #2 do Gabriel (01/09/2026): grilling é TRANSVERSAL

> *"Esse processo de `Interview the user relentlessly about a plan, decision, or idea until every branch of the design tree is resolved` precisa existir em todas as skills que elaboram planos e estratégias, como a própria loop-engineering."*

A primitive `grilling` não é só candidata NOVA — é REGRA-ZERO-DRIFT aplicável transversalmente em **5 skills TACO-house que elaboram plano/estratégia**:

| Skill | Como grilling entra | Local de invocação |
|---|---|---|
| **loop-engineering** | Antes de approval no `██ GATE HUMANO ██` (strategy → plan), o modelo roda grilling para garantir que decisão tree está exausta | §"Human-in-the-loop — hybrid autonomy (3 gates)" — entre passo 9 e passo 10 |
| **taco-planning** | Antes do §5 Verification Protocol, grilling fecha trade-offs multidimensionais (9 dims) | §"Stage 3 — PLAN STRUCTURE" — antes de plan.md final |
| **TACO-subagent** | ENGINEERS phase usa grilling para cada bloco de subtask antes de spawnar | §"PHASE 5 — ENGINEERS" — após DECOMPOSE |
| **analysis-loop** | Adotada como versão INTERATIVA do analysis_frontier (já tem autônoma) | §"O loop — passos 6-12" — PAINEL... já tem cego, grilling é interactive-zen do mesmo |
| **Briah** | grilling integra com as 7 operações; cada operação vira round grilling | §"Passo 0 — medir ANTES de perguntar" — entre medir e perguntar |

**T-shirt transversal agregado**: M (criar primitive) + 5×S (1 MUST de 1 linha por skill) = **M×1 single commit**, paralelo ao Marcel Point #1 (E em 7 skills).

**Critério de verificação** (análogo ao grep "Tour verify tests"):
```bash
grep -l "grilling" \
  ~/.claude/skills/{loop-engineering,taco-planning,TACO-subagent,analysis-loop,briah}/SKILL.md \
  | wc -l
# Expected: 5
```

---

## Candidata C — `critic-panel-as-skill` (promoção de fragment → skill) — **M** + **migration obrigatória**

### Origem externa

**TACO-house próprio fragmento ADW** (confirmado por `find /home/gabrielgadea/projects/touring -name "*critic*panel*"`):

> `client/skills/Touring/adw-library/fragments/critic-panel.toml`

E obra `requesting-code-review` + mattpocock `code-review` two-axis (Standards × Spec, verbatim):

> *"A change can pass one axis and fail the other: Code that follows every standard but implements the wrong thing → Standards pass, Spec fail. Code that does exactly what the issue asked but breaks the project's conventions → Spec pass, Standards fail. Reporting them separately stops one axis from masking the other."*
> *"Don't pick a single winner across axes: that's the reranking the separation exists to prevent."*

### Por que NÃO apenas aprimoramento

O fragmento ADW é **acessível apenas via `touring adw run`** — quem NÃO roda ADW (engenheiro solo, sessão fora de planner, humano-edit-prompt direto) **não tem acesso**. Como skill, qualquer um pode invocar `Skill: critic-panel --target <artifact>`.

### Decisão de migração — 2 opções (Gabriel pediu esclarecimento)

> Gabriel 01/09/2026: *"não estou convencido de que é necessário deletar `client/skills/Touring/adw-library/fragments/critic-panel.toml` no ato da promoção. Manter ambos = divergência garantida."*

A justificativa hard-rule-#1 original cita lesson 25/07/2026 — mas essa lesson era sobre **biblioteca implanted via `from-template`** (cópia física do fragment). Aqui o cenário é diferente: **`touring adw run --use critic-panel:panel`** é uma referência runtime, não cópia. Cenários onde **manter o fragment temporariamente** faz sentido:

| Cenário | Por que NÃO deletar imediatamente |
|---|---|
| **Phase de transição** | Usuários ADW ainda usam `--use critic-panel:panel` em workflows em voo |
| **Compatibilidade retroativa** | ADW library pode preferir o fragment por outras razões (HMR, hot-reload, journaled state) |
| **Bridge deprecation** | Fragment pode ter flag `deprecated: true` por N versões antes da remoção total |

Cenários onde **deletar imediatamente** faz sentido:

| Cenário | Por que DELETAR |
|---|---|
| Hard-rule-#1 literal | "never restate a fragment" — princípio constitucional TACO-skilling (lesson 25/07) |
| Source-of-truth dual permanente | divergência (mas: válido só se nenhum consumer usar o fragment depois) |

**Posição honestamente revisada após crítica do Gabriel**: a justificativa hard-rule-#1 **literalmente** dizia "manter ambos = divergência garantida", mas a lesson que originou (25/07 shell-injection) era sobre outro contexto. Em outros contextos, dual source-of-truth com flag deprecated pode ser defensável.

**Pergunta para o Gabriel** (a registrar em `decision-canvas` se C2 executar):

> "Para C2 (`critic-panel-as-skill` promoção), prefere:
> **Opção A** (Hard-rule-#1 strict): deletar `client/skills/Touring/adw-library/fragments/critic-panel.toml` no ato da promoção, todos consumers migram imediatamente
> **Opção B** (Pragmática dual): manter fragment com flag `deprecated = true` por N=1 minor version (~3 meses), deletar em v4.0 dos consumers ADW
> **Opção C** (third way): fragment vira thin wrapper sobre a skill (forward), depois deletar."

**T-shirt M** mas o item agora **bloqueia na decisão do Gabriel** até ele escolher Opção A/B/C. O subprocesso `scripts/critic_panel.py` é independente da decisão (copy 1:1 do fragmento existente); só o ato de deletar vs manter flag-deprecated é a variável.

### Mecanismos A→G ativados

- **(D) Adversarial blind critic** — N críticos cegos, lenses distintas, sessão fresca, quórum por código (já no fragmento)
- **(A) Hard-Gate** — *"output com VERDICT=REJECT → próximo passo é rework, não bypass"*
- **(B) Exit-code implícito** — VERDICT=PASS / REJECT / ESCALATE padronizado (veredito-canônico da ADW library)

### Composição com a casa

```markdown
# Hypothetical skill (~150L body, promotion from critic-panel.toml)

## Description
Run a blind critic panel on an artifact (diff, design, plan, criacao.md).
N critics with DISTINCT lenses (correctness · security · does-it-reproduce
· value-fidelity), each in a fresh session, verdict counted by code
(quorum: N/2 + 1 to PASS). Use when an artifact needs adjudication that
single-actor review cannot provide.

## Hard rules
1. Fresh session per critic (inherited context contaminates verdict)
2. Different lenses (N critics with same lens = N copies of one opinion)
3. Verdict counted by code (no narrative synthesis)
4. ESCALATE routes aside WITHOUT spending a retry (existence of doubt ≠ retry trigger)

## Migration note (2026-09-01)
Promoted from `client/skills/Touring/adw-library/fragments/critic-panel.toml`.
The fragment is now deprecated and will be removed in v3.0; consumers must
migrate to this skill via `touring adw new <flow> --use critic-panel:as_skill`.
```

### T-shirt M

- Script copy 1:1 do fragmento existente (sem reescrita de lógica)
- Step adicionado: deleção do fragmento + atualização de consumers
- Test script para provar que ambos os caminhos (skill e fragment) não divergem antes da remoção

---

## Candidata D — `tdd-enforcer` (red-green pre-conditional gate) — **M** (fecha gap (E) universal)

### Origem externa

**obra/superpowers:test-driven-development** verbatim (process) + **mattpocock/tdd** verbatim (G-mechanism — seams-first).

### Por que NÃO apenas Rank #4 (Touring MUST E)

Rank #4 adiciona um MUST no prompt do Touring master. `tdd-enforcer` skill tem **camada adicional executora**: implementa o MUST **como skill invocável que opera antes do Edit**. Touring MUST é **opt-in** (LLM pode não respeitar); `tdd-enforcer` skill é **opt-in explícito** mas composição natural quando invocado.

### Mecanismos A→G ativados

- **(E) RED-GREEN-REFACTOR** (universal gap closure)
- **(G) Pre-conditional gate** — seams-confirmation ANTES de Edit (mattpocock port)
- **(A) Hard-Gate** — *"AI cannot edit <file> until <test> exists AND fails"*

### Composição com a casa

```markdown
# Hypothetical skill (~140L body)

## Description
Pre-conditional gate enforcing red-green-refactor discipline. Before any
Edit/Write on a production file (.rs/.py/.ts), the matching failing test
must exist. Use when a feature/bugfix needs scaffolding tested-first
discipline that prompt-level MUSTs don't reliably enforce.

## Hard rule (obra port)
NO FIXES WITHOUT ROOT CAUSE (inherited from systematic-debugging).
NO CODE WITHOUT FAILING TEST (the gap closure).

## Outbound linkage
- obra: superpowers:test-driven-development (process authority)
- mattpocock: tdd (seams-first pre-conditional authority)
- Touring master Hard Rule #12 (complementary prompt-level MUST)

## Reference
- `references/red-green-protocol.md`
```

### T-shirt M

- 1 script Python: `tdd_enforce.py --file <path>` (verifica test existence + test failing)
- Integration com pre-edit hook do Touring (`TOURING_TDD_ENFORCE=1`)
- Test scripts para verificar comportamento

---

## Candidata E — `decision-canvas` (decision-explainer primitive) — **M**

> **Marcel Point do Gabriel (01/09/2026)**: originalmente `human-pause-gate` (portable F-mechanism). Gabriel apontou que o que ele realmente quer não é pausa genérica — é **quadro rico e completo de subsídios** toda vez que o agente apresenta decisões pendentes. Renomeei + redesign.

### Origem externa

**mattpocock/skills** `grill-me` (Productivity · User-Invoked) verbatim:

> *"Get relentlessly interviewed about a plan or design until every branch of the design tree is resolved."*

**+ TACO-house**: Analysis-loop's blind critic + Briah's `criacao.md` (artefato rico para decisão); mas falta a primitive que **automaticamente cria o canvas quando o agente apresenta decisões** (não só quando o usuário pede).

**Lembre do `wait-what` (Candidata A)**: wait-what é **reativo** (reage a misfire). decision-canvas é **proativo** (forçado pelo agente sempre que apresenta decisões pendentes).

### O que faz

**Auto-trigger**: sempre que o agente declara "preciso de aprovação em X" ou "precisamos decidir Y antes de prosseguir", o agente é **forçado** a produzir um **quadro rico** com 9 seções, em vez de só listar a decisão:

1. **O que se trata** (definição operacional da decisão)
2. **Por que ela surgiu** (contexto upstream — qual constraint/observação/regra gerou)
3. **O que temos que decidir** (a pergunta binária ou multi-opção, explícita)
4. **Quais são as alternativas** (≥3 opções ranked; default é "manter status quo" como opção explícita)
5. **O que significa tomar essa decisão** (imediato: o que está em jogo AGORA)
6. **Implicações e consequências** (cascata para outras decisões, pré-requisitos destruídos, etc.)
7. **Recursos necessários** (tempo, agentes, memória, ferramentas, pessoas)
8. **Plano operacional** (passos exatos após decisão: who/what/when/where/depends-on)
9. **O que mais pode surgir** (segunda-ordem: o que esta decisão previne, o que ela ADIA, e o que essas evitam/adiam geram)

**Saída**: artefato `decision_canvas.md` no bundle, persistido como memória de decisão.

### Mecanismos A→G ativados

- **(A) Hard-Gate em prompt** — MUST estruturado: "se você diz 'preciso decidir X', primeiro produza o canvas completo; depois PARE"
- **(F) Human-gate** — explicitamente para aprovação informada
- **(D) Adversarial leve** — modelo contra si mesmo (busca contra-argumentos às próprias alternativas)

### Composição com a casa

```markdown
# Hypothetical skill (~150L body)

## Description (FRONT MATTER)
Forced decision-canvas. Triggers whenever the agent says "we need to decide",
"I need your approval on", "before we proceed", or presents ≥1 option for
selection. Produces a 9-section canvas: what · why it arose · what's to
decide · alternatives · meaning · implications · resources · operational
plan · what-else-may-arise. Persists to decision_canvas.md. Surfaces only
when user explicitly says "stop canvassing" or proceeds.

## Hard rule (A)
When you say "preciso decidir X", you MUST produce the 9-section canvas
BEFORE asking approval. Skipping is a violation.

## Cross-link
- wait-what (Candidata A) — reactivo pós-misfire; decision-canvas é proativo pré-decisão
- analysis-loop PAINEL CEGO (lens-in-distinct como siblings da canvas)
- Briah's `criacao.md` (similar schema, diferente fase — Briah é criação, canvas é decisão)
```

### T-shirt M

- Subscripts: `decision_canvas_template.md` (template 9-section) + `decision_canvas_emit.py` (compact JSON for memory)
- Hard-rule text na skill (description 200-300 chars)
- 5-7 dias de triggering_audit para validar invocação correta

---

# PARTE 3 — ORDEM DE EXECUÇÃO JUSTIFICADA

```
PHASE A — Tour master (S+S+S = 3 itens, semana 1-2)
├─ A1 [S] Touring rank #1 — verification-before-completion-taco
├─ A2 [S] Touring rank #4 — MUST (E) Hard Rule #12   ← depende de A1
└─ A3 [S] TACO-wt rank #5 — DOCUMENTAR exit codes  ← independente

PHASE B — Mid-cost (M = 2-3 itens, semana 2-4)
├─ B1 [M] loop-engineering rank #2 — tdd-enforcer INNER 12
├─ B2 [M] TACO-cross-audit rank #3 — root-cause-before-fix + human pause
└─ B3 [M] TACO-subagent — HUMAN-PAUSE-GATE entre SCOUT/ARCHITECT

PHASE C — Novas skills (semana 4-6)
├─ C1 [S] Criar `wait-what` (mais barato)
├─ C2 [M] Criar `tdd-enforcer` (cross-cuts B1)   ← dependência: A2 já estabelece MUST
├─ C3 [M] Criar `decision-canvas` (E-NOVA — substitui human-pause-gate) ← dependência: nenhuma
├─ C4 [M] Criar `grilling` primitive standalone ← paralelo a C2/C3
└─ C5 [M×1] **grilling transversal**: MUST em 5 skills (loop-engineering + taco-planning + TACO-subagent + analysis-loop + briah) single commit — paralelo a Marcel Point #1 (E em 7 skills)

PHASE D — Validação cruzada (semana 6-8)
├─ D1 Run `triggering_audit.py` em cada skill modificada
├─ D2 Run `mine_transcripts.py --skill <name>` por 7 dias
├─ D3 Run quality_gate em todas (13 originais + 5 candidatas = 18 final)
└─ D4 Run `loop_converged.py` final composite check

### Decisão pendente do Gabriel (bloqueia C3)

> Antes de executar C3 (`critic-panel-as-skill` promoção), Gabriel precisa escolher entre:
> - **Opção A** (hard-rule-#1 strict): deletar fragment no ato da promoção
> - **Opção B** (pragmática dual): manter com flag `deprecated = true` por N=1 minor version
> - **Opção C** (third way): fragment vira thin-wrapper sobre a skill (forward), depois deletar
```

**Justificativa da ordem**: começar por itens S no Touring master (skill mais invocada, gates já prontos via hooks existentes) maximiza leverage cedo. Items M (loop-engineering, TACO-cross-audit) entram depois que S estão estabilizados, porque M dependem de P0 stable. Candidatas NOVAS entram PHASE C (depois de chão estabilizado), com ordem S → M → M.

---

# PARTE 4 — TACO-GATE: COMO MEDIR O SUCESSO

Cada item do roadmap acima tem **critério de pronto mensurável** (não auto-avaliação):

| Item | Métrica binária (exit-code) | Métrica contínua (7-day) |
|---|:---:|---|
| A1 (Touring rank #1) | `quality_gate.py --hygiene` exit 0 | `triggering_audit.py` antes/depois — redução em claims-sem-evidência |
| A2 (Touring rank #4) | `touring-quality score Touring-master --fail-below 0.80` exit 0 | `mine_transcripts.py` — frequência de teste-vermelho antes do código |
| A3 (TACO-wt rank #5) | tabela exit codes em SKILL.md | N/A (documentation-only) |
| B1 (loop-engineering rank #2) | `loop_phase_close.py --facts --seams-required` exit 0 | N/A (1-shot) |
| B2 (TACO-cross-audit rank #3) | `cross-audit --enable-human-pause` rejeita sem `root_cause_verdict` | redução de "fix without cause" |
| B3 (TACO-subagent) | (idem para TACO-subagent) | N/A |
| C1 (wait-what) | `triggering_audit.py` + 50 sessões reais | ocorrências de misfire recovery |
| C2 (tdd-enforcer) | guard D8: revert-fix → MUST FAIL | N/A |
| C3 (critic-panel-as-skill) | guard D8: skill ≡ fragment | consumers migrados |

**Composite gate de release**: `loop_converged.py --task skill-aprimoramento-2026-q3 --scope ~/.claude/skills/ --rust-full` deve retornar **exit 0**. Critério alinhado com **LEI L2** (loop-engineering): "sinal ausente ≠ zero".

---

# PARTE 5 — APRENDIZADOS PEDAGÓGICOS PARA O LEITOR

> Quem ler este documento e quiser escrever/refinar uma skill amanhã:

1. **Gap universal (E) RED-GREEN-REFACTOR** — Inserir MUST explícito ("código de produção sem teste vermelho não é entregue"); cross-link `obra:superpowers:test-driven-development` como fonte canônica.
2. **Promote fragments a skills quando o acesso é cross-skill**. `critic-panel.toml` em `adw-library/fragments/` (confirmado por `find`) deveria virar skill — só **deletar o fragment após a promoção** (RF6) para evitar divergência.
3. **Cuidado com (B) em SKILL.md**: scripts Python já emitem exit codes (cross_audit.py, dimension_scorer.py, loop_converged.py). Gap às vezes é só de documentação. **Verificar com `python3 <script> --help`** antes de classificar como △.
4. **Adote o padrão mattpocock "primitive + reuse"**: 4 primitives nomeados (`grilling`, `tdd`, `code-review`, `domain-modeling`) reusadas por 5+ skills cada. TACO-house tem primitives **espalhados** sem reuso formal — extrair e nomear é um exercício simples.
5. **A taxonomia A→G é estável após 3 sessões de uso** (obra + mattpocock + TACO-house diagnosis). Não proliferar para A→H. Se uma 8ª categoria surgir, ela será uma sub-classe de G (pre-conditional) ou uma composição que merece skill própria, não mecanismo.
6. **Verifique empíricamente antes de declarar gap** — Rank #5 é literalmente a lesson "veredito desalinhado do predicado" (memory) aplicada. A próxima vez que vir △ em uma SKILL.md, rode `python3 <script> --help` antes de classificar como gap.
7. **Composite gate, não checklist**: a regra hard do TACO-house é "**lei L2: o runner encerra o loop**". Critérios de pronto binários (exit-code de gate) > narratives (auto-avaliação). O `loop_converged.py` é o juiz de registro.

---

*Documento versão v1.0 (explicação pedagógica) — `docs/plans/2026-09-01-skill-aprimoramento/explanation.md`. Complementa o `plan.md` no mesmo bundle. Persistência paralela: `~/.claude/projects/-home-gabrielgadea-projects-touring/memory/skill-structured-reasoning-2026-09-01.md` v4 (atualizada por Edit subseqüente). Autoria Gabriel Gadea + Touring orquestrador. Data 01/09/2026.*
