# VP-Scout — Verified Protocol for Scouting

> **Version**: v1.2 (slim) | **Type**: Mandatory Verification Protocol | **Applies to**: All TACO scout subagents
> **Analogy**: VGP is for code generation; VP-Scout is for discovery/integration analysis
> **Worked examples + chain origins**: `~/.claude/skills/Touring/references/vp-scout-examples.md` (load on demand)
> **CLI Ranked Guide**: `~/.claude/skills/Touring/SKILL.md`

---

## O Problema que VP-Scout Resolve

| Scout Error | Root Cause | VP-Scout Fix |
|---|---|---|
| "simd-search desabilitado" | Feature opcional declarada no fornecedor, mas consumer já ativou | Feature Trace chain |
| "LearningLoop não wired ao RL" | Sistema RL independente em outro crate; bridge criaria ciclo | Dependency Cycle Check |
| "ACO loop incompleto" | Homonimia — "ACO" existe em 2 crates DIFERENTES, sistemas independentes | Homonimia Check |
| "código morto" (DriftDetector) | Import existe num método, não significa não-usado em outro | Already Implemented |
| "32 compilation errors" | Infere estado de compilação de plan docs, NÃO executa cargo check | Chain 5: Compilation Evidence |
| "tests não cobrem método X" | Lê nomes de teste sem verificar que o corpo chama o método | Chain 3b: Test File Content |

---

## As 9 Cadeias Obrigatórias

### Cadeia 1: Feature Trace (oportunidade envolve feature gate)

```
PROBLEM: "Feature X está desabilitada"

CHAIN:
1. touring index find "X" → listar TODOS os #[cfg(feature = "X")]
   Se NÃO aparece em nenhum crate → feature não existe
   Se aparece SOMENTE no fornecedor → opcional, não habilitado
2. touring wiring modules <consumer_crate> -j → para CADA consumer
   Verificar features = [...] no Cargo.toml
3. touring ast find <symbol> → verificar #[cfg(feature)] usage

VERDICT:
- Fornecedor + consumer ativou → JÁ ATIVO → NÃO é oportunidade
- Fornecedor + NENHUM consumer ativou → oportunidade VÁLIDA
- Feature NÃO existe em nenhum crate → bug de nomenclatura
```

### Cadeia 2: Dependency Cycle Check (cruza crate boundary)

```
PROBLEM: "A (crate fundo) → B (crate topo) = integração"

CHAIN:
1. touring graph dependencies → A→B? B→A?
2. touring wiring modules <crate_B> -j → integration score
3. Se A↔B → CICLO BLOQUEANTE; Se A é "fundacional" → não pode depender de consumers

VERDICT:
- Ciclo detectado → BLOQUEADO por arquitetura
- Sem ciclo + A não é fundacional → oportunidade VÁLIDA
```

### Cadeia 3: Already Implemented Check (SEMPRE antes de propor integração)

```
PROBLEM: "Podemos fazer X"

CHAIN:
1. touring wiring orphans -j → buscar pelo símbolo/operação
2. touring index find <opportunity_name> → buscar em TODOS os crates
3. touring memory recall "X foi implementado" → buscar em lessons (score ≥ 0.8: reutilizar)

VERDICT:
- Já existe wiring em outro crate → JÁ IMPLEMENTADO
- Pub symbol com consumer = 1 → apenas 1 consumer, não é "oportunidade"
- Pub symbol com consumer = 0 → ORPHAN, verificar se deveria ser usado
```

### Cadeia 4: Homonimia Check (nomes genéricos: ACO|Loop|Handler|Index|Manager)

```
PROBLEM: "X existe em crate A e crate B — são o mesmo sistema?"

CHAIN:
1. touring index find "X" → listar TODOS os símbolos com nome "X"
2. touring wiring modules <crate_A/B> -j → ambos exportam? module_path diferente → HOMÔNIMOS
3. touring ast find <symbol> → comparar implementações semânticas

VERDICT:
- Mesmo module_path → mesma coisa
- module_path diferente em crates diferentes → HOMÔNIMOS (2 oportunidades separadas)
```

### Cadeia 4b: Homonimia Cross-Language (OBRIGATÓRIA quando polyglot)

> **Origem + exemplo konverter**: `references/vp-scout-examples.md#cadeia-4b--origem`.

```
PROBLEM: "X existe em crate Rust A — mesmo sistema que candidatos em outras linguagens?"

CHAIN (cross-language):
1. Identificar linguagens: Cargo.toml (Rust) + pyproject.toml (Python) + package.json (TS/JS)
2. Grep multi-include SIMULTÂNEO (não uma de cada vez):
   grep -rn "<SymbolName>" <project_root> \
       --include='*.rs' --include='*.py' --include='*.pyi' \
       --include='*.ts' --include='*.tsx' --include='*.js' --include='*.jsx' \
       --include='*.go' --include='*.java' --include='*.cpp' --include='*.h'
3. Para cada match, classificar: Definição vs Uso vs Comentário/string
4. Comparar semântica entre linguagens

VERDICT:
- Apenas 1 linguagem → aplicar Cadeia 4 normal
- Multi-lang + semântica idêntica → mesmo sistema (bridge/PyO3 — potenciar)
- Multi-lang + semântica diferente → BLOCKED_HOMONYMIA_CROSS_LANGUAGE
- NUNCA inferir "mesmo sistema" sem confirmar semântica em ambas as linguagens
```

### Cadeia 5: Compilation Evidence (afirmações sobre erros de compilação)

```
PROBLEM: "Existem N erros de compilação" ou "o código não compila"

CHAIN:
1. NUNCA inferir estado de compilação a partir de plan docs ou análise estática.
   Plan docs descrevem INTENÇÃO, não estado atual.
2. SEMPRE executar:
   cd <workspace_root> && cargo check --workspace 2>&1 | grep "^error" | wc -l
3. Coletar contexto:  cargo check --workspace 2>&1 | grep -A3 "^error"
4. Verificar arquivos modificados:  touring index files "*.rs" -j | jq '.[] | select(.modified_recently)'

VERDICT:
- N erros no cargo check → afirmação VALIDADA com evidência real
- 0 erros no cargo check → compilação OK, NÃO reportar erros
- cargo check indisponível → MARCAR "UNVERIFIED" e reportar incerteza
- Inferência sem cargo check → FALSO POSITIVO, DESCARTAR
```

**Detalhe sobre exit codes do cargo check**: `references/vp-scout-examples.md#cadeia-5--detail-cargo-check-return-codes`.

### Cadeia 3b: Test File Content Check (afirmações sobre cobertura)

```
PROBLEM: "O método X não tem cobertura de teste"

CHAIN:
1. Localizar test files que referenciam o módulo:
   touring index find "<method>" -j | jq '.[] | select(.file_path | contains("test"))'
   Grep "<method>" --include="*test*" -l
2. Para cada test file, Read e verificar o CORPO (não o nome) — buscar chamadas reais
3. Verificar #[ignore]:  Grep "#\[ignore\]" <test_file>
4. Verificar comentários de exclusão:  Grep "NOTE.*test\|FIXME.*test\|TODO.*test"

VERDICT:
- Corpo do teste chama o método → COBERTO, não reportar gap
- Teste existe mas não chama → FALSO POSITIVO de cobertura
- Nenhum teste encontrado → gap REAL, reportar
- Teste com #[ignore] → gap CONDICIONAL, reportar com nota
```

### Cadeia 7: Wiring Cache Staleness (wiring orphans reporta órfão)

```
PROBLEM: "Símbolo X reportado como orphan por touring wiring orphans"

CHAIN:
1. NUNCA aceitar orphan claim do wiring daemon sem verificação direta — wiring DB pode ter
   staleness de minutos após edits.
2. SEMPRE verificar via grep:
   grep -rn "<symbol>" crates/ --include="*.rs" | grep -v "^.*:.*//" | head -10
   Se aparece como consumer → NÃO é orphan real.
3. Verificar se símbolo foi adicionado na sessão corrente:
   touring memory recall "added:<symbol>"
   touring index find "<symbol>" -j | jq '.[].file_path'
4. Se wiring diz orphan E grep encontra consumer → WIRING_STALE.
   Ação: aguardar próximo rebuild ou `touring index rebuild`.

VERDICT:
- grep encontra consumer → WIRING_STALE (falso positivo do daemon)
- grep 0 + touring index sem resultado → orphan REAL
- símbolo adicionado na sessão + grep 0 → orphan REAL (novo símbolo ainda não wired)
```

**Exemplo real (Wave Preditiva 2026-04-20 — ShadowRolloutResult.as_hint)**: `references/vp-scout-examples.md#cadeia-7--exemplo-real`.

### Cadeia 6: Staleness Detection (referenciando plan docs)

```
PROBLEM: "Plan doc diz que task T está pendente"

CHAIN:
1. Verificar idade:  ls -l <plan_doc> → se > 7 dias → POTENTIALLY_STALE
2. Para cada task no plan doc, verificar NO CÓDIGO:
   touring index find <task_symbol> -j | jq length     # count > 0 → IMPLEMENTADA
   grep -rn <pattern> crates/ | head -5               # matches > 0 → IMPLEMENTADA
3. Cross-ref com memory:  touring memory recall "implemented:<task_symbol>"

VERDICT:
- Plan doc < 7d + symbol not found + grep 0 → task PROVAVELMENTE pendente
- Plan doc ≥ 7d + (symbol found OR grep matches) → task IMPLEMENTADA, plan doc STALE
- Plan doc ≥ 7d + symbol not found + grep 0 → UNCERTAIN, verificar manualmente
- NUNCA classificar NOT_IMPLEMENTED baseado SOMENTE em plan doc content
```

---

## Protocolo de Execução do Scout

```
PARA CADA oportunidade identificada pelo scout:

  SE oportunidade.feature_gate:
    EXECUTAR Cadeia 1 (Feature Trace)
    SE feature já ativa: MARCAR "JÁ IMPLEMENTADO / NÃO OPORTUNIDADE"

  SE oportunidade.cruza_crate_boundary:
    EXECUTAR Cadeia 2 (Dependency Cycle Check)
    SE ciclo: MARCAR "BLOQUEADO / CICLO"
    SE fundo_grafico: MARCAR "BLOQUEADO / FIM DE GRAFO"

  SE oportunidade.nome_genérico (ACO|Loop|Handler|Index|Manager):
    EXECUTAR Cadeia 4 (Homonimia Check)
    SE homônimos: SEPARAR em 2 oportunidades distintas

  SE workspace é polyglot (Rust + Python + TS, etc.):
    EXECUTAR Cadeia 4b (Homonimia Cross-Language) — OBRIGATÓRIO
    SE homônimos cross-language: BLOCKED_HOMONYMIA_CROSS_LANGUAGE
    SE mesmo sistema (binding/PyO3): potenciar via wrapper

  ANTES DE FINALIZAR:
    EXECUTAR Cadeia 3 (Already Implemented)
    SE já existe: MARCAR "JÁ IMPLEMENTADO / REVISAR"

  SE afirmando erros de compilação:
    EXECUTAR Cadeia 5 (Compilation Evidence) — OBRIGATÓRIO
    SE sem cargo check: DESCARTAR afirmação como FALSO POSITIVO

  SE referenciando plan docs:
    EXECUTAR Cadeia 6 (Staleness Detection) — OBRIGATÓRIO
    SE symbol encontrado: MARCAR "IMPLEMENTADO / PLAN DOC STALE"

  SE afirmando falta de cobertura:
    EXECUTAR Cadeia 3b (Test File Content Check)
    SE corpo do teste chama método: MARCAR "COBERTO / NÃO É GAP"

  SE touring wiring orphans reportar símbolo como orphan:
    EXECUTAR Cadeia 7 (Wiring Cache Staleness) — OBRIGATÓRIO
    SE grep encontra consumer: MARCAR "WIRING_STALE / NÃO É ORPHAN"
```

---

## Template de Resposta do Scout (com VP-Scout metadata)

```json
{
  "role": "scout",
  "status": "completed",
  "result": {
    "opportunities": [
      {
        "id": 1,
        "name": "...",
        "verification_chain": ["feature_trace", "dependency_cycle"],
        "chain_results": {
          "feature_trace": { "status": "PASS|FAIL", "evidence": "..." },
          "dependency_cycle": { "status": "PASS|FAIL", "evidence": "..." },
          "already_implemented": { "status": "PASS|FAIL", "evidence": "..." },
          "homonimia": { "status": "PASS|FAIL", "evidence": "..." }
        },
        "classification": "REAL_OPPORTUNITY|JA_IMPLEMENTED|BLOCKED_CYCLE|BLOCKED_GRAPHIC|BLOCKED_HOMONYMIA",
        "evidence": "citing actual command outputs"
      }
    ],
    "false_positives_avoided": 3,
    "chains_executed": ["feature_trace", "dependency_cycle", "already_implemented", "homonimia"]
  },
  "quality_gates": { "functional": 1.0, "robust": 1.0, "readable": 1.0, "documented": 1.0, "secure": 1.0, "no_regression": 1.0 },
  "issues": [],
  "next_recommendations": []
}
```

---

## Hard Rules

1. **Todas as cadeias aplicáveis DEVEM ser executadas** para cada oportunidade antes de reportar
2. **Chain Results DEVEM aparecer no JSON** — sem evidência de chain = incomplete
3. **False positives DEVEM ser marcados como BLOCKED_* com razão específica** — não apenas "não existe"
4. **Homonimia detecta SEMPRE** — mesmo nome em crates diferentes com module_paths diferentes = sistemas independentes
5. **NUNCA afirmar erros de compilação sem cargo check executado** — plan docs são INTENÇÃO, não estado. Inferência sem cargo check = FALSO POSITIVO automático
6. **NUNCA afirmar falta de cobertura sem ler o corpo do teste** — nome do teste não prova o que ele cobre
7. **Daemon indisponível não bloqueia scouting** — usar fallback (cargo check, Grep, Read) e marcar campos afetados como "daemon_degraded"
8. **VERIFY_BEFORE_REPORT** (Code-First Gate — FIX-S4): VERIFICAR diretamente no código com `cargo check`, `grep`, e `touring index find` antes de qualquer afirmação. Cadeias 5 e 6 são OBRIGATÓRIAS. Falha em verificar = FALSO POSITIVO automático
9. **NUNCA aceitar orphan claim sem grep de verificação** (Cadeia 7 — FIX-W20): `touring wiring orphans` pode ter staleness de minutos. Sempre confirmar via `grep -rn "<symbol>" crates/`. Homonimia intra-crate via type alias + struct com nomes divergidos por rename — verificar via grep, não apenas pelo índice.

## VP-Scout vs VGP

| Aspect | VGP | VP-Scout |
|---|---|---|
| Quando | Antes de escrever código | Antes de reportar oportunidade |
| Foco | Assinaturas e campos reais | Dependência circular, homonimia, já implementado |
| Comandos | touring index find, touring ast find | touring wiring, touring graph, touring memory |
| Saída | Código especulado validado | Oportunidades classificadas com evidência |
| Gate | Speculate score >= 0.8 | classification != BLOCKED_* |

---

## Worked Examples (load on demand)

Para ver as cadeias aplicadas a casos reais (FPs evitados com sucesso), consulte `references/vp-scout-examples.md`:

- **Opp3 (simd-search)** — Cadeia 1 + 3 evitam FP "feature desabilitada"
- **Opp5 (ACO pheromone)** — Cadeia 4 detecta homônimos `AcoPheromone` em touring-simd ≠ touring-hooks
- **Homonimia Intra-Crate** — `CognitiveMCTS` (type alias) ≠ `PheromoneMCTS` (struct renomeada)
- **Orphan Falso por Wiring Staleness** — `ShadowRolloutResult.as_hint` aparenta orphan mas tem consumer real
