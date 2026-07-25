# D51 — Incident Response (F4.11)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_11_incident`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: incident.io · FireHydrant · PagerDuty · Google SRE

## Definition

Avalia a prontidão para incidentes: runbooks (procedimentos passo-a-passo para falhas conhecidas), processo de on-call, MTTR (mean time to recovery), postmortems blameless, e procedimentos de rollback. Quando algo quebra às 3h, runbook é a diferença entre 5 min e 5 horas de downtime.

## Why it matters

Incidentes são inevitáveis; o que diferencia é a velocidade de recuperação (MTTR). Runbook transforma conhecimento tribal em ação executável por qualquer on-call. Postmortem blameless converte cada incidente em melhoria sistêmica (não em culpa). Touring tem circuit-breaker nativo (recuperação automática parcial).

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | runbooks + rollback + postmortem |
| 0.5–0.8 | ⚠ Warn | processo parcial |
| <0.5 | ❌ Fail | sem prontidão para incidente |

## MUST

```bash
touring-quality check --gate F4.11 --target <FILE>
touring-quality score <FILE> --dims F4.11 --format json
```

## SHOULD

```bash
# Runbooks/postmortems são .md → Write permitido. Verificar presença:
ls docs/runbooks/ docs/postmortems/ 2>/dev/null
touring memory recall "incident:<symptom>"             # incidentes passados como base de runbook
```

## MAY

```bash
touring memory recall "quality:F4.11"
```

## Elite best practices (context7)

1. **Runbook por modo de falha conhecido** — passo-a-passo executável (detectar→mitigar→verificar→escalar), não prosa; o on-call segue sem precisar do autor. Fonte: Google SRE / incident.io.
2. **MTTR como métrica-chave** — medir e reduzir o tempo de recuperação; rollback rápido (D48) > root-cause sob pressão. Fonte: PagerDuty/DORA (MTTR).
3. **Postmortem blameless** — focar no sistema (que defesa faltou), não na pessoa; cada incidente gera ações de melhoria rastreadas. Fonte: Google SRE postmortem culture.
4. **Severidade + escalation claros** — SEV1/2/3 com critérios objetivos e quem acionar; reduz hesitação no momento crítico. Fonte: incident.io/FireHydrant.
5. **Degradação graciosa automatizada** — circuit breaker + fallback (Touring tem nativo) recupera sem humano; runbook para o que a automação não cobre. [training-data: resilience + Touring circuit_breaker].

## Common pitfalls

- Conhecimento de recuperação só na cabeça de uma pessoa (bus factor).
- Sem runbook → debug ad-hoc sob pressão (MTTR alto).
- Postmortem com culpa → pessoas escondem incidentes.
- Sem rollback testado → "rollback" vira novo incidente.

## Remediation

1. `touring memory recall "incident:..."` → base de runbook a partir de incidentes passados.
2. Escrever runbooks/postmortems (.md = Write permitido); definir severidade/escalation.
3. `Write tool --path RUNBOOKS/<incident>.md --intent "incident runbook" --kind IncidentRunbook` (incident.io/PagerDuty; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C12 SYSTEM-HEALTH** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D50 (monitoring), D48 (deploy/rollback), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: incident.io + Google SRE) — maintained by touring-quality_
