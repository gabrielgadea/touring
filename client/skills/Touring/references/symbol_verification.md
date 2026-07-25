# Symbol Verification Table — Wave TRM 2026-05-02 (Constitutional)

> **Origem**: Wave TRM 2026-05-02 — architect inventou 5 nomes de métodos
> (`MemoryGuard::tick`, `::status`, `gate_metrics::record_pressure_tick`,
> `post_edit::complete`, `handle_status`). 1 wave de retrabalho. Defesa
> institucional cross-cutting aplicada aos 5 agentes + TACO-subagent.md +
> /TACO-task + Touring-native tooling/SKILL.md + CLAUDE.md (REGRA #15).

## Princípio operacional

**TODA fase TACO** que produz JSON output mencionando símbolos (function /
struct / method / type) DEVE incluir um campo dedicado classificando cada
símbolo citado em categoria canônica COM evidência CLI ou justificativa
explícita.

A invenção de símbolos é um bug semântico que escala rápido — uma vez no
blueprint do architect, propaga para o engineer (que tenta importar), para o
doc (que documenta), para a próxima session (que cita do doc). Esta tabela
corta a propagação na origem.

---

## Schema canônico por role

### Scouter — `cited_symbols` (per finding)

Implementado via VP-Scout v1.2 Chain 8 (`touring-scouter.md`).

**Categorias permitidas**:
- `found` — símbolo confirmado via `touring index find`
- `found_via_grep` — não está no índice mas grep encontrou (advisory: rebuild)
- `not_found` — símbolo não existe (finding INVALIDATED)

**Schema JSON (cada entry)**:

```json
{
  "symbol": "MemoryGuard::start_ticker",
  "status": "found|found_via_grep|not_found",
  "evidence_cmd": "touring index find MemoryGuard::start_ticker -j",
  "evidence_excerpt": "{\"file_path\": \"crates/touring-resource-monitor/src/guard/mod.rs\", \"line\": 67}",
  "verdict": "VERIFIED|INDEX_STALE|BLOCKED_INVENTED_SYMBOL"
}
```

### Architect — `symbol_verification`

Implementado via Phase 5.0 SYMBOL VERIFICATION GATE (`touring-architect.md`).

**Categorias permitidas**:
- `verified_existing` — símbolo já existe no codebase (CLI evidence required)
- `to_be_created` — símbolo será criado nesta task (creates_in_subtask required)
- `unverified_planned` — símbolo hipotético (confidence < 0.7 + requires_followup: true)

**Schema JSON (campo completo)**:

```json
"symbol_verification": {
  "verified_existing": [
    {
      "symbol": "compute_composite_health_score",
      "file": "crates/touring-server/src/cli/status.rs",
      "line": 97,
      "evidence_cmd": "touring index find compute_composite_health_score",
      "evidence_excerpt": "{\"file_path\": \"...\", \"kind\": \"fn\"}"
    }
  ],
  "to_be_created": [
    {
      "symbol": "MemoryGuard::start_ticker",
      "expected_file": "crates/touring-resource-monitor/src/guard/mod.rs",
      "expected_signature": "pub async fn start_ticker(&self, interval: Duration) -> Result<(), TrmError>",
      "creates_in_subtask": "S-10",
      "rationale": "Singleton ticker spawning tokio interval"
    }
  ],
  "unverified_planned": [
    {
      "symbol": "AdaptiveEngine::pin_rayon_pool",
      "rationale": "future integration with touring-cognitive",
      "confidence": 0.5,
      "requires_followup": true
    }
  ]
}
```

### Engineer — `symbol_verification`

Implementado via Phase 4.5 SYMBOL VERIFICATION TABLE (`touring-engineer.md`).

**Categorias permitidas** (**NO `unverified_planned`** — engineer cria, não especula):
- `imported_existing` — símbolo já existia, importado
- `created_this_subtask` — símbolo criado nesta subtask
- `modified_existing` — símbolo já existia, modificado

**Schema JSON**:

```json
"symbol_verification": {
  "wave_anchor": "TRM 2026-05-02",
  "verification_protocol_version": "1.0",
  "imported_existing": [
    {
      "symbol": "tokio::time::interval",
      "evidence_cmd": "touring index find interval -j",
      "evidence_excerpt": "{\"crate\": \"tokio\", \"module\": \"time\"}"
    }
  ],
  "created_this_subtask": [
    {
      "symbol": "MemoryGuard::start_ticker",
      "created_in_file": "crates/touring-resource-monitor/src/guard/mod.rs",
      "created_at_line": 67,
      "signature": "pub async fn start_ticker(&self, interval: Duration) -> Result<(), TrmError>",
      "post_edit_evidence": "touring ast overview crates/.../guard/mod.rs returns symbol at line 67"
    }
  ],
  "modified_existing": [
    {
      "symbol": "compose_quality_evolution",
      "file": "crates/touring-analysis/src/quality.rs",
      "line": 142,
      "original_signature": "pub fn compose(...) -> f64",
      "new_signature": "pub fn compose(..., tdg: &TdgReport) -> f64",
      "evidence_cmd": "touring index find compose_quality_evolution"
    }
  ]
}
```

### Auditor — `vgp_cross_verification`

Implementado via Phase 0.6 VGP CROSS-VERIFICATION (`touring-auditor.md`).

**Operação**: re-executa CLI sobre ≥ 50% sample dos symbol_verification claims dos
upstream agents (architect, engineers). Detecta fraud semântica (evidence_excerpt
diverge da re-execução).

**Schema JSON**:

```json
"vgp_cross_verification": {
  "wave_anchor": "TRM 2026-05-02",
  "upstream_agents_audited": ["architect", "engineer-S-10"],
  "samples_checked": 12,
  "samples_passed": 11,
  "samples_failed": 1,
  "fraud_detections": [],
  "invented_symbols_detected": [],
  "uncreated_symbols_detected": ["MemoryGuard::missing_ticker"],
  "verdict": "PASS|FAIL"
}
```

### Scriber — `documented_symbols`

Implementado via Phase 0.5 VGP FOR DOCUMENTATION (`touring-scriber.md`).

**Categorias permitidas**:
- `verified_existing` — símbolo confirmado via CLI antes do Write
- `planned_future` — citação legítima mas item ainda não existe (marcar PLANNED|PROPOSED no texto)
- `deprecated_removed` — item removido (explicar contexto histórico)

**Schema JSON (cada entry)**:

```json
{
  "symbol": "MemoryGuard::start_ticker",
  "status": "verified_existing|planned_future|deprecated_removed",
  "evidence_cmd": "touring index find MemoryGuard::start_ticker -j",
  "evidence_excerpt": "{\"file_path\": \"...\", \"line\": 67}",
  "documented_in_file": "docs/2026-05-02-wave-trm.md",
  "documented_at_line": 87
}
```

---

## Anti-padrões canônicos (BLOCKED automático)

| Anti-padrão | Detecção | Veredicto |
|---|---|---|
| `BLOCKED_INVENTED_SYMBOL` | `touring index find` retorna 0 + sem categoria `to_be_created` | composite=0.0, status=failed |
| `BLOCKED_UNVERIFIED_LOCATION` | symbol existe mas file:line citado não bate | composite=0.0 |
| `BLOCKED_PHANTOM_LOCATION` | line_number > `wc -l file` | composite=0.0 |
| `BLOCKED_FRAUD_DETECTED` | upstream evidence_excerpt diverge da re-execução do auditor | composite=0.0, status=failed |
| `BLOCKED_NO_SYMBOL_VERIFICATION` | upstream JSON sem o field obrigatório | composite=0.0 |
| `BLOCKED_FALSE_CONFIDENCE` | architect cita `unverified_planned` com `confidence ≥ 0.7` ou `requires_followup ≠ true` | partial |
| `BLOCKED_INFERENCE` | engineer alega "deve existir" sem `touring index find` output citado | composite=0.0 |

---

## Cross-role consequence chain (defesa em camadas)

```
Scouter Chain 8 fails    → finding excluded → architect doesn't see it
                                              ↓
Architect Phase 5.0 fails → blueprint blocked → engineer doesn't get DAG
                                              ↓
Engineer Phase 4.5 fails  → subtask blocked  → wiring audit blocked
                                              ↓
Auditor Phase 0.6 detects → upstream composite=0.0 (fraud detected)
                                              ↓
Scriber Phase 0.5 fails   → doc rewritten as PLANNED|PROPOSED or removed
```

Cada camada bloqueia propagação para a próxima. Ainda que uma falhe, a próxima
detecta e corta. Esta é a defesa em profundidade contra alucinação.

---

## Comandos VGP por role

### Scouter (Chain 8)

```bash
SYMBOL="<cited>"
INDEX_RESULT=$(touring index find "$SYMBOL" -j)
COUNT=$(echo "$INDEX_RESULT" | jq 'length // 0')

if [ "$COUNT" -gt 0 ]; then
  STATUS="found"
elif grep -rn "fn $SYMBOL\|struct $SYMBOL\|enum $SYMBOL\|trait $SYMBOL" crates/ --include="*.rs" -l 2>/dev/null | head -1; then
  STATUS="found_via_grep"
else
  STATUS="not_found"  # → BLOCKED_INVENTED_SYMBOL
fi
```

### Architect (Phase 5.0)

```bash
# Para cada símbolo no blueprint draft:
# Categoria A — verified_existing
touring index find <symbol> -j | jq '.[] | {file_path, line, kind, module_path}'
# Se 0 results → mover para B (to_be_created) ou C (unverified_planned)

# Categoria B — to_be_created
# Confirmar que existe subtask que cria: jq '.dag[] | select(.id == "S-10")'

# Categoria C — unverified_planned
# Confidence DEVE ser < 0.7 + requires_followup: true
```

### Engineer (Phase 4.5)

```bash
# Para cada Edit/Write:
# Categoria A — imported_existing
touring index find <symbol> -j

# Categoria B — created_this_subtask
touring ast overview <created_file> -j | jq '.symbols[] | select(.name == "<symbol>")'

# Categoria C — modified_existing
touring ast find <symbol> -j > /tmp/before.json
# (Edit) → touring ast find <symbol> -j > /tmp/after.json
diff /tmp/before.json /tmp/after.json
```

### Auditor (Phase 0.6)

```bash
# Sample 50%+ random + risk-weighted
SYMBOLS=$(jq -r '.symbol_verification.verified_existing[].symbol' /tmp/upstream.json | shuf | head -N)
for symbol in $SYMBOLS; do
  CLI=$(touring index find "$symbol" -j)
  if [ -z "$CLI" ] || [ "$CLI" = "[]" ]; then
    echo "BLOCKED_INVENTED_SYMBOL: $symbol"
  fi
done

# Para to_be_created — confirm subtask exists + file created
touring decompose status -j | jq --arg id "<subtask_id>" '.tasks[] | select(.id == $id)'
ls -la <created_in_file>
touring ast overview <file> -j | jq --arg s "<symbol>" '.symbols[] | select(.name == $s)'
```

### Scriber (Phase 0.5)

```bash
# Antes de Write em qualquer .md:
touring index find "<symbol>" -j      # confirma existência
ls -la "<file_path>"                  # confirma path real
touring memory recall "wave:<id>"     # confirma wave registrada
grep -n "<wired_pair>" /home/gabrielgadea/projects/touring/crates/touring-server/src/cli/synergy.rs
```

---

## Implementação por arquivo

| Arquivo | Phase / seção | Linha aproximada |
|---|---|---|
| `~/.claude/agents/touring-scouter.md` | Chain 8 | 544 |
| `~/.claude/agents/touring-architect.md` | Phase 5.0 | 422 |
| `~/.claude/agents/touring-engineer.md` | Phase 4.5 | 343 |
| `~/.claude/agents/touring-auditor.md` | Phase 0.6 | 268 |
| `~/.claude/agents/touring-scriber.md` | Phase 0.5 | 192 |
| `~/.claude/rules/TACO-subagent.md` | seção CONSTITUTIONAL | 218 |
| `~/.claude/commands/TACO-task.md` | seção CONSTITUTIONAL | 174 |
| `~/.claude/skills/Touring-native tooling/SKILL.md` | Three Mandatory Properties (cross-ref) | 37 |
| `~/.claude/CLAUDE.md` | REGRA #15 | 344 |
| `~/.claude/skills/Touring/SKILL.md` | Princípio 2 + Reference Map + Golden Rule 11 | (este file) |

---

## Memória persistente

```bash
touring memory recall "lesson:wave_trm_2026_05_02:vgp_symbol_verification" -j
```

Lesson registra: trigger, escopo (8 arquivos), patches por arquivo, anti-padrões adicionados, cross-role consequence chain, e operational principle.

---

## Por que existe

1. **REGRA #0 — Potencializar**: invenção de símbolos é redução de qualidade
2. **edição-com-gate — Agentic paradigm**: provenance via daemon, validação por CLI
3. **Operational reality**: documentação errada propaga durante meses; código quebrado falha rápido. Esta tabela é mais perigosa que parece — sem ela, agentes downstream consomem `.md` falsos como verdade
4. **Wave TRM 2026-05-02**: 5 inventões custaram 1 wave inteira de retrabalho — este é o anchor empírico
