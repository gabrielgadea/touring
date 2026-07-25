# Touring Decision Matrix — Task → Commands (constitutional, auto-load)

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-05-10
> **Authority**: Gabriel Gadea | **Origin**: post-mortem 2026-05-10 — perdi ~40min em iteração
> reativa porque escolhi comandos "que vinham à mente" em vez de mapear tarefa→ferramenta.
> Esta matriz é a defesa institucional contra esse anti-padrão.

## Princípio operacional

**Comando-greedy ≠ estratégia.** Touring tem 82 CLI commands + 99 MCP tools + 198 hooks
disponíveis. A diferença entre uma resolução de 10 segundos e 40 minutos de iteração é
saber QUAIS comandos a tarefa exige — não quais comandos eu lembrei.

Esta matriz é **prescritiva**: para cada categoria de tarefa, lista os comandos
**obrigatórios** (skip → composite 0.0), **recomendados** (skip → flag em audit), e
**opcionais** (proportional value). Antes de propor solução, TACO DEVE responder o
checklist específico da categoria.

---

## Task Taxonomy — 12 categorias canônicas

Cada tarefa que envolve código mapeia para uma das 12 categorias abaixo. Em caso de
overlap (ex: "ler arquivo grande para entender + refatorar"), aplicar a categoria
**mais profunda** (a que inclui a outra). Categorias ordenadas por profundidade
crescente.

| # | Categoria | Trigger words | Profundidade |
|---|---|---|---|
| **C01** | **READ-SCAN** (ler para conhecer) | "leia", "mostre", "abra", "veja", "o que tem em" | superficial |
| **C02** | **READ-COMPREHEND** (ler para entender semântica) | "explique", "como funciona", "que faz", "verifique" | médio |
| **C03** | **SYMBOL-LOOKUP** (achar definição ou consumers) | "onde está", "quem chama", "quem usa", "find" | médio |
| **C04** | **PRE-EDIT-TRIAGE** (decidir se vale editar) | "posso mexer aqui", "risco de", "blast radius" | médio |
| **C05** | **EDIT-MINOR** (1 file, sem API change) | "ajuste", "corrija typo", "renomeie var local" | médio |
| **C06** | **EDIT-MAJOR** (multi-file OR API change) | "refatore", "extraia", "mova", "introduza tipo" | profundo |
| **C07** | **NEW-SYMBOL** (criar fn/struct/module pub) | "crie", "adicione", "implemente" | profundo |
| **C08** | **CROSS-CALLER-COMPARE** (2+ callers paths) | "espelhar", "padrão simétrico", "por que A faz X mas B não" | profundo |
| **C09** | **DEBUG-ROOT-CAUSE** (algo não funciona) | "não funciona", "retorna vazio", "deveria", "stale" | profundo |
| **C10** | **ARCHITECTURAL** (multi-crate, novo subsystem) | "wiring", "integração", "expor para outros crates" | crítico |
| **C11** | **DEPENDENCY-FLOW** (entender cadeia de chamadas) | "quem chama quem", "cadeia", "fluxo", "como chega em" | crítico |
| **C12** | **SYSTEM-HEALTH** (diagnóstico ambiente) | "doctor", "saúde", "daemon", "índice stale" | crítico |

---

## Comando matrix — categoria → checklist

Cada categoria abaixo lista:
- **MUST** — sem isso, output composite 0.0
- **SHOULD** — skip exige justificativa explícita no output
- **MAY** — proportional value, usar conforme contexto

### C01 — READ-SCAN

```
MUST:    touring ast meta <file> --depth summary -j          # blast/quality/fan
MAY:     Read <file>                                          # raw content quando precisar
MAY:     touring ast highlight <file>                         # syntect rendering se >150L
```

**Exit criterion**: o user pediu para conhecer o file → reporto LOC, language, blast_radius,
quality_score, cognitive_score, top 5 pub symbols. NUNCA leio raw sem antes o meta.

### C02 — READ-COMPREHEND

```
MUST:    touring ast meta <file> --depth summary -j
MUST:    touring ast overview <file> -j                       # estrutura + symbols
MUST:    touring ast rust-semantic <file.rs>                  # Rust only: generics/traits
SHOULD:  touring ast blast <file>                             # cadeia dependências
SHOULD:  Read <file> (ranges cirúrgicos, não inteiro)
MAY:     touring ast tdg <file>                               # grade A-F
MAY:     touring file-knowledge extended <file>               # 23 campos enriquecidos
```

**Exit criterion**: entendo SEMÂNTICA (o que faz, por que existe, como se conecta),
não só sintaxe. Resposta inclui pelo menos 1 consumer real OR justifica "0 consumers
existem".

### C03 — SYMBOL-LOOKUP

```
MUST:    touring index find <symbol> -j                       # primary verification
MUST:    touring ast find <symbol> -j                         # signature + module path
SHOULD:  touring wiring impact <symbol> --depth 2             # transitive consumers BFS
SHOULD:  grep -rn "<symbol>" crates/ --include='*.rs'         # Cadeia 7 (anti-staleness)
MAY:     touring tantivy search "<symbol>"                    # BM25 fuzzy lookup
```

**Exit criterion**: NUNCA reportar "símbolo X tem 0 consumers" sem ter rodado
`wiring impact` AND grep. Wiring DB pode estar stale (Cadeia 7 VP-Scout).

### C04 — PRE-EDIT-TRIAGE

```
MUST:    touring ast meta <file> --depth summary -j
MUST:    touring ast blast <file>                             # full dependency tree
MUST:    touring ast tdg <file>                               # STOP se D/F
MUST:    touring pre-edit                                     # score >= 0.8 (CILA budget)
SHOULD:  touring gotcha match <file>                          # pitfall DB
SHOULD:  touring memory recall "edit:<file>"                  # past lessons
```

**Exit criterion**: tenho score quantitativo de risco (blast_radius < 10 OR plano
mitigação) E score qualitativo (gotcha=0, score≥0.8).

### C05 — EDIT-MINOR

```
MUST:    [todo C04] + Edit/Write
MUST:    touring post-edit (auto via hook)                    # quality re-verify
SHOULD:  cargo check -p <crate>                               # compile gate
```

### C06 — EDIT-MAJOR

```
MUST:    [todo C04]
MUST:    touring wiring impact <each_changed_symbol> --depth 2  # blast antes
MUST:    touring wiring chains [--rebuild]                    # cadeia source→sink
SHOULD:  touring ast blast-cross-feature <file>               # cross-feature impact
SHOULD:  Edit/Write + touring pre-edit (score ≥ 0.8)          # gate antes de editar código
SHOULD:  cargo test -p <crate>                                # full test suite após
```

**Exit criterion**: cada callsite afetado foi inspecionado E nenhum padrão de
"caller A faz X, caller B esqueceu" sobrou (ver C08).

### C07 — NEW-SYMBOL

```
MUST:    touring index find <NewSymbol>                       # garantir non-collision
MUST:    touring generate verify --symbol <NewSymbol>         # VGP gate
MUST:    Write tool + touring post-write (auto hook)          # registra + valida novo símbolo
SHOULD:  touring wiring orphans -j (após criar)               # REGRA #0 check
SHOULD:  touring wiring suggest <new_symbol>                  # auto-wire hints
MAY:     touring generate render <kind> --vars '{}'           # template preview
```

### C08 — CROSS-CALLER-COMPARE (categoria critical-anti-bug)

> **Ativador**: existem 2+ funções que **deveriam** ter padrão simétrico (ex:
> `cli_index_rebuild` e `reindex_file_with_old` ambos chamam `process_file`).
> Esse é o sinal canônico do bug que custou ~40min de iteração na sessão 2026-05-10.

```
MUST:    touring ast find <fn_a> -j  &&  touring ast find <fn_b> -j
MUST:    touring wiring impact <fn_a> --depth 2  &&  touring wiring impact <fn_b> --depth 2
MUST:    touring ast blast <file_a>  &&  touring ast blast <file_b>
MUST:    Read body of both functions side-by-side               # exigência humana
SHOULD:  touring synergy --with-metrics -j                      # WIRED_PAIRS catalog
SHOULD:  diff -u <(touring ast find <fn_a> -j) <(touring ast find <fn_b> -j)
```

**Exit criterion**: tabela explícita listando o que cada caller faz, célula-a-célula:
"caller A → X, Y, Z" vs "caller B → X, _, Z". Qualquer asterisco vazio = bug
em potencial.

### C09 — DEBUG-ROOT-CAUSE

```
MUST:    touring doctor -j                                    # FASE 0 health gate
MUST:    touring status -j                                    # composite_health_score
MUST:    touring memory recall "<symptom>"                    # past similar bugs
MUST:    touring gotcha match <file>                          # known pitfalls
SHOULD:  touring gate-metrics -j                              # which counters changed
SHOULD:  touring wiring impact <suspected_fn> --depth 2       # blast do sintoma
SHOULD:  touring synergy report -j                            # cross-subsystem state
SHOULD:  C08 (cross-caller compare) se há 2+ callers          # anti-asimetria
```

**Exit criterion**: cita evidência forense específica (linha + comando + output)
que sustenta o root cause. Hipóteses sem evidência CLI = INFERENCE [<0.7].

### C10 — ARCHITECTURAL

```
MUST:    touring ast workspace-info                           # cargo metadata
MUST:    touring wiring cycles --min-depth 2                  # Tarjan SCC
MUST:    touring wiring audit -j                              # full orphan + score
MUST:    Context7 (mcp__plugin_context7) para best practices  # external knowledge
SHOULD:  touring decompose create plan "<intent>"             # DAG nativo
SHOULD:  touring memory store <plan_key> --tier semantic      # persist decision
SHOULD:  /plan ou skill taco-planning                        # plano Pln2-grade
```

### C11 — DEPENDENCY-FLOW

```
MUST:    touring wiring chains [--rebuild]                    # source→sink module graph
MUST:    touring wiring impact <entry_point> --depth 4        # BFS profundo
MUST:    touring ast blast <entry_file>                       # local tree
SHOULD:  touring synergy wired                                # 50 WIRED_PAIRS catalog
MAY:     touring ast blast-cross-feature <file>               # se há feature gates
```

**Exit criterion**: produzo um GRAFO (texto ou tabela) com nodes = funções,
edges = chamadas, indicando o caminho do entry_point até o ponto de interesse.

### C12 — SYSTEM-HEALTH

```
MUST:    touring doctor -j                                    # 5 components ok
MUST:    touring status -j                                    # composite_health_score
MUST:    touring e2e -j                                       # composite 0-1
SHOULD:  touring gate-metrics -j                              # all counters snapshot
SHOULD:  touring learning status                              # RL convergência
SHOULD:  touring health-delta status                          # per-path streak
MAY:     touring evolution drift -j                           # alert level
```

---

## Pre-Action Checklist — perguntas que TACO DEVE responder

Antes de propor qualquer solução em código, responder mentalmente (e citar a evidência):

```text
[ ] Q1 — Em qual das 12 categorias esta tarefa cai? (se múltiplas, qual é a mais profunda?)
[ ] Q2 — Rodei TODOS os comandos MUST da categoria?
[ ] Q3 — Algum SHOULD foi pulado? Por quê?
[ ] Q4 — Se há 2+ callers/sites similares, rodei C08 (cross-caller compare)?
[ ] Q5 — Meu diagnóstico cita pelo menos 1 output CLI literal (não inferência)?
[ ] Q6 — Verifiquei wiring/blast ANTES da implementação, não DURANTE/DEPOIS?
[ ] Q7 — A solução proposta toca arquivos com blast_radius > 10? Tenho plano?
[ ] Q8 — Se há gotcha histórico para arquivo OU sintoma, recall foi feito?
```

**Falha-fechado**: se Q2 retorna "não", parar; rodar comandos; retomar.
**Composite**: se Q5 retorna "não", marcar resposta como INFERENCE [<0.7], NUNCA FACT [1.0].

---

## Reflex Triggers — situações que ATIVAM comandos automaticamente

Quando o input do user contém qualquer trigger abaixo, executar o comando
correspondente ANTES de qualquer outra ação:

| Trigger no input | Reflex automático | Categoria |
|---|---|---|
| Cita um path de arquivo | `touring ast meta <path> --depth summary -j` | C01-C12 (sempre) |
| "verifique" / "analise" | `touring ast meta` + `touring ast overview` | C02 |
| "onde está" / "quem chama" | `touring index find` + `touring wiring impact` | C03 |
| "editar" / "modificar" / "ajustar" | `touring ast blast` + `touring pre-edit` | C04-C05 |
| "refatore" / "extraia" / "mova" | `touring wiring chains` + `Edit` (após blast) | C06 |
| "crie" / "adicione" / "implemente" | `touring index find` (collision) + `Write` (após VGP) | C07 |
| "espelhe" / "como X faz" / 2+ funções similares | **C08 OBRIGATÓRIO** (cross-caller compare) | C08 |
| "não funciona" / "retorna vazio" / "stale" | `touring doctor` + `touring status` + `touring memory recall` | C09 |
| "wiring" / "integração" / "expor" | `touring ast workspace-info` + `touring wiring audit` | C10 |
| "cadeia" / "fluxo" / "quem chama quem" | `touring wiring chains` + `touring wiring impact --depth 4` | C11 |
| "saúde" / "doctor" / "índice" | `touring doctor -j` + `touring status -j` | C12 |
| Mencionou um símbolo PascalCase ou snake_case | `touring index find <symbol>` | sempre |
| Edit em arquivo .rs com >100 LOC | `touring ast rust-semantic` + `touring ast tdg` | C04+ |
| 2+ Edits no mesmo file em uma sessão | `touring health-delta status <file>` | C12 |
| Antes de declarar "task completa" | `touring wiring orphans -j` (REGRA #0) | sempre |

---

## Anti-padrões a partir do post-mortem 2026-05-10

| Anti-padrão observado | Como detectar | Como prevenir |
|---|---|---|
| Editar arquivo sem rodar `blast` antes | Hook post-edit avisa "API surface changed" sem ter avisado em pre-edit | Reflex Trigger "Edit em .rs" |
| Diagnosticar "0 consumers" via grep só | Wiring DB stale; conclusão falsa | Cadeia 7 VP-Scout + `wiring impact` |
| Comparar 2 funções similares lendo manualmente | C08 não foi disparado | Reflex Trigger "espelhe / como X faz" |
| Iterar reativamente após teste falhar | Faltou wiring chains ANTES da impl | Q6 do checklist (verificar wiring ANTES, não DEPOIS) |
| Build apenas 1 crate quando 2 binários afetados | `cargo build -p crate_A` não pega `crate_B`'s bin | `touring ast workspace-info` → ver TODOS `[[bin]]` antes |
| `tracing::debug!` para erro real | Errors silenciados; só visíveis com RUST_LOG=debug | Hook gate-metrics counter + warn/error |

---

## Acoplamento com regras existentes

| Regra existente | Como esta matriz a estende |
|---|---|
| **file-metadata-first** | Vira C01-C04 MUST primeira linha |
| **VP-Scout (7 cadeias)** | Cadeia 7 (wiring staleness) vira MUST do C03 |
| **REGRA #0 (potencializar)** | Reflex Trigger "antes de declarar complete" → wiring orphans |
| **REGRA #11 (git proibido)** | Mantém — esta matriz não envolve git |
| **TACO-subagent Phase 0** | Reusa `touring doctor` + `touring status` como gates |
| **Constitutional Symbol Verification Table** | C03/C07 MUST inclui `touring index find` cited_symbols |

---

## Self-audit semanal

```bash
# A cada N sessões com >1 hour, executar:
touring memory recall "iteration:reactive"        # casos onde fiz iteração reativa
touring evolution insights -j | jq '.patterns'    # padrões aprendidos via RL
touring memory recall "anti-pattern"              # gotchas acumulados

# Se aparecer >2 instâncias do mesmo anti-padrão → atualizar esta matriz.
```

---

## Hard mandate (não-negociável)

Toda task que envolva código DEVE começar com:
1. **Classificação** explícita (qual categoria C01-C12)
2. **Checklist** MUST executado (citando outputs)
3. **Cross-caller check** (C08) se há 2+ paths similares no escopo
4. **Pre-Action Q1-Q8** respondidas antes da primeira Edit/Write

Skip → resposta marcada como INFERENCE [<0.7], NUNCA FACT [1.0]. Re-trabalho subsequente
custa exponencialmente mais do que esses 10-30 segundos de discovery upfront.

---

## Referências cruzadas

| Tópico | Local |
|---|---|
| Touring CLI ranks Tier 1-9 | `~/.claude/rules/touring-cli-index.md` |
| Touring Skill master | `~/.claude/skills/Touring/SKILL.md` |
| File metadata first | `~/.claude/rules/file-metadata-first.md` |
| VP-Scout 7 cadeias | `~/.claude/skills/Touring/references/VP-Scout-rule.md` |
| TACO subagent protocol | `~/.claude/skills/Touring/references/TACO-subagent-rule.md` |
| Symbol Verification Table | `~/.claude/skills/Touring/references/symbol_verification.md` |

---

_v1.1 — 2026-05-26 (slim) | Origin: post-mortem da sessão "domain_circuit incremental indexing fix" (2026-05-10)._
_~40min de iteração reativa poderiam ter sido 5 minutos com C08 + C11 aplicados upfront._

---

## Lessons Learned — Sessão 2026-05-11 (Touring Premium Refactor)

5 lições da sessão massive de 8+ horas (55 Python scripts, 164 pytest tests, 26 plan docs, 8 iterações W8 v1→v5) — padrões aplicáveis a sessões futuras similares:

- **L1** — Iterar versões com forensic measurement (single hypothesis per iteration; stop quando v(N+1) move <10%)
- **L2** — Leaf invariant para bucket classification (shared bucket = no outgoing `crate::` deps)
- **L3** — `textwrap.dedent` gotcha (todas as linhas devem ter prefix matching common leading)
- **L4** — Cross-audit `--baseline` mode distingue PENDING vs FAIL
- **L5** — Forensic discovery first, refactor second (script de medição ANTES do script de execução)

**Detalhe + exemplos + código de cada lição**: `~/.claude/skills/Touring/references/touring-decision-matrix-lessons.md`
