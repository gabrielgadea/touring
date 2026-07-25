# D48 — Deployment Strategy (F4.8)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_8_deploy`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/argoproj/argo-cd` · Flagger · Spinnaker

## Definition

Avalia a estratégia de deploy: progressive delivery (blue-green, canary), rollback automático, zero-downtime, e GitOps (estado declarativo versionado). Deploy big-bang sem rollback é all-or-nothing — qualquer regressão derruba 100% dos usuários.

## Why it matters

Como o código chega à produção é tão crítico quanto o código. Canary expõe a mudança a 1% antes de 100%, com rollback automático em regressão de métrica — limita o blast radius de um bug. Big-bang sem rollback transforma um bug em incidente total.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | progressive + rollback + zero-downtime |
| 0.5–0.8 | ⚠ Warn | deploy manual / sem canary |
| <0.5 | ❌ Fail | big-bang sem rollback |

## MUST

```bash
touring-quality check --gate F4.8 --target <FILE>
touring-quality score <FILE> --dims F4.8 --format json
```

## SHOULD

```bash
# Manifests de deploy (.yaml = Edit permitido). Verificar estratégia + rollback definidos:
grep -iE 'strategy|canary|rollback|maxSurge|maxUnavailable' deploy/*.yaml
```

## MAY

```bash
touring memory recall "quality:F4.8"
```

## Elite best practices (context7 — `/argoproj/argo-cd`)

1. **GitOps: estado declarativo como fonte da verdade** — o cluster converge para o que está no Git; deploy = merge, rollback = revert. Auditável e reproduzível. Fonte: `/argoproj/argo-cd`.
2. **Canary com rollback automático por métrica** — Flagger/Argo Rollouts promove gradualmente (1%→100%) e reverte se SLI (erro/latência) regredir. Fonte: Flagger/Argo Rollouts.
3. **Zero-downtime: rolling com health checks** — `maxUnavailable=0`, readiness probes; novo só recebe tráfego quando saudável. Fonte: k8s rolling update / Argo.
4. **Blue-green para cutover instantâneo + rollback instantâneo** — duas versões lado a lado, switch de tráfego atômico, rollback = switch de volta. Fonte: Argo Rollouts blue-green.
5. **Deploy desacoplado de release (feature flags)** — deployar código dark + ativar via flag; rollback = desligar flag sem redeploy. [training-data: progressive delivery].

## Common pitfalls

- Big-bang deploy sem rollback (regressão → 100% afetados).
- Deploy manual (não-reproduzível, propenso a erro).
- Sem health/readiness probe → tráfego para instância não-pronta (downtime).
- Deploy e release acoplados (não dá para desligar feature sem redeploy).

## Remediation

1. Definir estratégia (canary/blue-green) + rollback automático nos manifests.
2. Adotar GitOps (Argo) + feature flags via Edit (.yaml).
3. `Write tool --path k8s/deployment.yaml --intent "canary deployment" --kind KubernetesManifest` (ArgoCD/Flagger; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C12 SYSTEM-HEALTH**
- Dims relacionadas: D47 (CI/CD), D49 (IaC), D51 (incident), D50 (monitoring)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /argoproj/argo-cd + Flagger) — maintained by touring-quality_
