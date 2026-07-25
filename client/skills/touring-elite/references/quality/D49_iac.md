# D49 — Infrastructure as Code (F4.9)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_9_iac`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/bridgecrewio/checkov` · Terraform · tflint · tfsec

## Definition

Avalia a infraestrutura como código: definição declarativa versionada (Terraform/Pulumi/k8s manifests), scanning de segurança da IaC (misconfig de cloud), detecção de drift (estado real vs declarado), e modularização. Infra clicada-à-mão é não-reproduzível e insegura.

## Why it matters

Infra como código dá reprodutibilidade, revisão (PR) e rollback. IaC sem scanning de segurança é onde misconfigurations de cloud (bucket público, SG aberto) vazam — a causa #1 de breaches de cloud. Drift (mudança manual fora do código) torna o IaC mentiroso.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | IaC scaneada, sem drift |
| 0.5–0.8 | ⚠ Warn | misconfig / drift |
| <0.5 | ❌ Fail | infra manual / insegura |

## MUST

```bash
touring-quality check --gate F4.9 --target <FILE>
touring-quality score <FILE> --dims F4.9 --format json
```

## SHOULD

```bash
# IaC files (.tf/.yaml) — Edit permitido. Scan + validate:
checkov -d .                                            # misconfig de segurança
terraform validate ; tflint                             # sintaxe + best practices
terraform plan                                          # detectar drift (real vs declarado)
```

## MAY

```bash
touring memory recall "quality:F4.9"
```

## Elite best practices (context7 — `/bridgecrewio/checkov`)

1. **Scan de segurança da IaC no CI (checkov/tfsec)** — pega bucket público, SG 0.0.0.0/0, encryption-off, IAM permissivo ANTES do apply. Fonte: `/bridgecrewio/checkov`.
2. **`terraform plan` em PR para detectar drift + revisar mudança** — o plan mostra exatamente o que mudará; revisar antes do apply; drift = mudança manual a reconciliar. Fonte: Terraform workflow.
3. **Módulos reutilizáveis + remote state com lock** — modularizar (não copy-paste de infra); state remoto com locking evita corrupção concorrente. Fonte: Terraform modules/backend.
4. **`tflint` para best practices do provider** — pega instance types inválidos, deprecated args, naming. Fonte: tflint.
5. **Least-privilege + tudo encriptado por default** — IAM mínimo, encryption-at-rest/in-transit obrigatório nos defaults (cruza com D19 config). Fonte: checkov policies.

## Common pitfalls

- IaC sem scan de segurança → misconfig de cloud em prod.
- Mudança manual no console (drift) → IaC mente sobre o estado real.
- State local/sem lock → corrupção em time.
- Copy-paste de infra em vez de módulos (divergência).

## Remediation

1. `checkov`/`tflint`/`terraform plan` → identificar misconfig/drift.
2. Corrigir nos arquivos IaC (.tf/.yaml = Edit permitido); modularizar; remote state.
3. `Write tool --path infra/main.tf --intent "terraform module" --kind TerraformModule` (checkov/tflint; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C12 SYSTEM-HEALTH**
- Dims relacionadas: D19 (config security), D48 (deploy), D52 (env)
- Keystone: `~/.claude/rules/elite-50-quality.md` (architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /bridgecrewio/checkov + Terraform) — maintained by touring-quality_
