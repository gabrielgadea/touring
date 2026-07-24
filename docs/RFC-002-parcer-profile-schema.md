# RFC-002: PARCER Profile Schema

**Status**: Active
**Type**: Specification
**Layer**: ESAA / S3
**Author**: TACO (Constitution v8.0 Draft)
**Date**: 2026-05-09
**Version**: 1.0.0

---

## 1. Context and Motivation

PARCER (Persona·Audience·Rules·Context·Execution·Response) is a 6-dimensional
contract that binds subagent behavior. Each dimension constrains what a subagent
may infer, what it must verify, and how it must respond. ESAA prescribes explicit
behavioral contracts rather than implicit instruction following.

This RFC formalizes the PARCER schema used by all 5 TACO subagents and establishes
the canonical YAML structure, validation rules, and drift detection protocol.

**Relation to S3**: S3 delivered 5 PARCER YAML files in `~/.claude/agents/`. These
close the ESAA "gap" (line 98 of master plan: "PARCER profiles — subagents are
.md, not 6-dim PARCER YAML").

---

## 2. PARCER Dimensions

| Dim | Name | Description | Format |
|-----|------|-------------|--------|
| P | **Persona** | Role identity and behavioral constraints | YAML block (2-5 sentences) |
| A | **Audience** | Primary consumer of agent output and calibration expectations | YAML block (1-3 sentences) |
| R | **Rules** | Hard constraints (MUST) and soft guidance (SHOULD) | YAML list of strings |
| C | **Context** | Inject and exclude rules for context window management | YAML map with `inject`/`never_inject` |
| E | **Execution** | Phase-by-phase protocol (ordered list of execution steps) | YAML list of strings |
| R | **Response** | Output format schema and valid/invalid examples | YAML map with `format`/`valid_examples` |

---

## 3. Canonical Schema (YAML)

```yaml
schema_version: "1.0"
agent_id: <string>       # unique identifier, kebab-case
agent_role: <string>     # scouter|architect|engineer|auditor|scriber

persona:
  role: |
    <multi-line string describing agent identity>
  identity_constraints:
    - <string>
    - <string>

audience:
  primary: <string>      # orchestrator|user|another_agent
  calibration: <string>   # what qualifies as valid output

rules:
  hard:
    - <string>           # MUST obey — violation = failed output
    - <string>
  soft:
    - <string>           # SHOULD follow — violation = degraded quality

context:
  inject:
    - <string>           # context fields always provided
    - <string>
  never_inject:
    - <string>           # context fields explicitly excluded

execution:
  - <string>             # Phase 0: ...
  - <string>             # Phase 1: ...
  - <string>            # ... ordered list of phases

response:
  format:
    schema_ref: <string> # e.g. "scouter-output.schema.json"
    content_type: <string>  # e.g. "application/json"
  valid_examples:
    - '<json-string>'   # example valid output
  invalid_examples:
    - '<json-string>'  # example invalid output
```

### 3.1 Field Rules

| Field | Required? | Validation |
|-------|-----------|------------|
| `schema_version` | YES | Must be `"1.0"` |
| `agent_id` | YES | kebab-case, max 64 chars, alphanumeric + hyphens |
| `agent_role` | YES | One of: `scouter`, `architect`, `engineer`, `auditor`, `scriber` |
| `persona.role` | YES | Non-empty string, max 2000 chars |
| `persona.identity_constraints` | YES | Non-empty list of strings |
| `rules.hard` | YES | Non-empty list of strings; each starts with "ALWAYS" or "NEVER" |
| `rules.soft` | YES | Non-empty list of strings |
| `execution` | YES | Non-empty ordered list; each entry starts with "Phase N:" |
| `response.format.schema_ref` | YES | String, valid filename pattern |
| `response.valid_examples` | YES | Non-empty list; each is a valid JSON string |
| `response.invalid_examples` | YES | Non-empty list; each is a JSON string that fails schema |

---

## 4. Existing PARCER Profiles (v1.0)

All profiles confirmed in `~/.claude/agents/`:

| Agent | File | Lines | Status |
|-------|------|-------|--------|
| touring-scouter | `touring-scouter.parcer.yaml` | 78L | ✅ Active |
| touring-architect | `touring-architect.parcer.yaml` | 78L | ✅ Active |
| touring-engineer | `touring-engineer.parcer.yaml` | 79L | ✅ Active |
| touring-auditor | `touring-auditor.parcer.yaml` | 76L | ✅ Active |
| touring-scriber | `touring-scriber.parcer.yaml` | 70L | ✅ Active |

### 4.1 touring-scouter Profile Summary

- **Role**: Deep codebase intelligence scout using VP-Scout v1.1 with 7 mandatory
  verification chains. JSON-only output. Every finding cites CLI evidence.
- **Hard rules**: 12 constraints including Chain 7 (wiring staleness), Chain 8
  (cited_symbols), no inference without CLI, no compilation claims without cargo check.
- **Execution**: 10 phases (Phase 0 daemon → Phase 8 JSON output)
- **Output schema**: `scouter-output.schema.json` → `application/json`

### 4.2 touring-architect Profile Summary

- **Role**: Highest-level architectural agent. Grounded empirically-verified blueprints.
  MCTS multi-path planning (min 3 paths). Context7 library best practices.
- **Hard rules**: 10 constraints including `touring index find` for all cited symbols,
  Context7 docs before finalizing design, MCTS minimum 3 paths.
- **Execution**: 11 phases (Phase 0 daemon → Phase 10 JSON blueprint)
- **Output schema**: `architect-output.schema.json` → `application/json`

### 4.3 touring-engineer Profile Summary

- **Role**: Elite implementation agent for TACO Phase 5. VGP + speculative shadow
  validation + RL reward loops. MUST use `mode=acceptEdits`.
- **Hard rules**: 10 constraints including `touring pre-edit` (composite ≥ 0.8),
  `touring ast meta`, `taco-forge perfect-create/edit`, zero orphan pub symbols (REGRA #0).
- **Execution**: 11 phases (Phase 0 daemon → Phase 10 JSON output)
- **Output schema**: `engineer-output.schema.json` → `application/json`

### 4.4 touring-auditor Profile Summary

- **Role**: Elite cross-audit agent. Phase 4.5 PRE-IMPL AUDIT mandatory gate.
  vgp_cross_verification (≥ 50% of upstream claims re-executed). E2E proof required.
- **Hard rules**: 10 constraints including Phase 4.5 mandatory gate, vgp
  cross-verification ≥ 50%, `touring e2e -j`, engineer composite_score ≥ 1.0 verification.
- **Execution**: 12 phases (Phase 0 daemon → Phase 11 JSON audit report)
- **Output schema**: `auditor-output.schema.json` → `application/json`

### 4.5 touring-scriber Profile Summary

- **Role**: Elite documentation agent. Code + docs + skill change together.
  TOON v1.0 checkpoint artifacts. REGRA #13 SKILL HYGIENE enforcer.
- **Hard rules**: 10 constraints including `taco-forge checkpoint` for retrospective
  artifacts, SKILL.md update after skill-relevant changes, `touring memory store`
  after wave milestone, `touring diary write --aaak`.
- **Execution**: 9 phases (Phase 0 daemon → Phase 8 JSON doc report)
- **Output schema**: `scriber-output.schema.json` → `application/json`

---

## 5. Drift Detection Protocol

PARCER drift occurs when the YAML file content diverges from the implemented behavior.

### 5.1 Detection Method

For each agent, compare `rules.hard` entries against actual agent behavior:

```bash
# Example: detect scouter drift
python3 -c "
import yaml
with open('~/.claude/agents/touring-scouter.parcer.yaml') as f:
    content = yaml.safe_load(f)
hard_rules = content['rules']['hard']
print(f'Scouter hard rules: {len(hard_rules)}')
for rule in hard_rules:
    print(f'  - {rule}')
"
```

### 5.2 Audit Script #3 (D9.7)

Audit script #3 must verify:
1. All 5 PARCER YAMLs parse as valid YAML
2. `schema_version` equals `"1.0"`
3. `agent_role` is one of the 5 valid roles
4. `rules.hard` list is non-empty
5. Each `rules.hard` entry starts with "ALWAYS" or "NEVER"
6. `execution` list contains Phase 0 and at least one more phase

### 5.3 Audit Script #4 (D9.7)

Audit script #4 must verify drift:
1. Mutate one `rules.hard` constraint in each YAML
2. Run drift detection
3. Verify the mutation is detected (diff reported)
4. Revert mutation

---

## 6. Adding a New Agent

To add a new PARCER profile:

1. Create `~/.claude/agents/touring-<name>.parcer.yaml` with all 6 dimensions
2. Validate: `python3 -c "import yaml; yaml.safe_load(open('touring-<name>.parcer.yaml'))"`
3. Run audit scripts #3 and #4 against the new file
4. Register in TACO subagent pool (update `agents.md` + CLAUDE.md reference)
5. Persist: `touring memory store "parcer:new-agent:<name>" "...` --tier semantic

---

## 7. Reference Files

| File | Purpose |
|------|---------|
| `~/.claude/agents/touring-scouter.parcer.yaml` | Scouter PARCER profile (78L) |
| `~/.claude/agents/touring-architect.parcer.yaml` | Architect PARCER profile (78L) |
| `~/.claude/agents/touring-engineer.parcer.yaml` | Engineer PARCER profile (79L) |
| `~/.claude/agents/touring-auditor.parcer.yaml` | Auditor PARCER profile (76L) |
| `~/.claude/agents/touring-scriber.parcer.yaml` | Scriber PARCER profile (70L) |

---

## 8. Relationship to ESAA

ESAA specifies PARCER profiles as behavioral contracts that:
- Prevent unconstrained inference ("ALWAYS cite CLI evidence")
- Enforce verification before reporting ("NEVER cite symbol without touring index find")
- Structure context injection to prevent overflow
- Define phase-ordered execution protocols
- Mandate JSON-only output for parseability

All 5 TACO subagents implement ESAA's PARCER contract. No deviation from the
6-dimension structure is permitted without RFC amendment.

---

## 9. Schema Validation Example

```python
import yaml, json, sys

def validate_parcer(path):
    with open(path) as f:
        doc = yaml.safe_load(f)

    errors = []

    # schema_version
    if doc.get('schema_version') != '1.0':
        errors.append('schema_version must be "1.0"')

    # agent_id format
    import re
    if not re.match(r'^[a-z][a-z0-9-]{0,63}$', doc.get('agent_id', '')):
        errors.append('agent_id must be kebab-case, max 64 chars')

    # agent_role
    valid_roles = {'scouter','architect','engineer','auditor','scriber'}
    if doc.get('agent_role') not in valid_roles:
        errors.append(f'agent_role must be one of {valid_roles}')

    # rules.hard non-empty
    hard = doc.get('rules', {}).get('hard', [])
    if not hard:
        errors.append('rules.hard must be non-empty')
    for rule in hard:
        if not (rule.startswith('ALWAYS') or rule.startswith('NEVER')):
            errors.append(f'rules.hard must start with ALWAYS/NEVER: {rule}')

    # execution phases
    exec_phases = doc.get('execution', [])
    phase_nums = []
    for step in exec_phases:
        import re
        m = re.match(r'Phase (\d+)', step)
        if m:
            phase_nums.append(int(m.group(1)))
    if not phase_nums or min(phase_nums) != 0:
        errors.append('execution must start with Phase 0')

    return errors

if __name__ == '__main__':
    for f in sys.argv[1:]:
        errors = validate_parcer(f)
        if errors:
            print(f'FAIL {f}: {errors}')
        else:
            print(f'PASS {f}')
```

---

## 10. Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) |

---

**RFC-002 v1.0.0 — PARCER Profile Schema — ESAA S3 spec formalized**