# D52 — Environment Management (F4.12)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_12_env`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/hashicorp/vault` · `/getsops/sops` · 12-factor

## Definition

Avalia a gestão de ambiente e segredos: secrets fora do código (Vault/SOPS/KMS), paridade dev/staging/prod (12-factor), config via ambiente (não hardcoded), e ausência de segredos em `.env` commitados. Complementa D17 (detecção de secret no código) com a gestão correta do ciclo de vida.

## Why it matters

Segredo em env file commitado = vazamento (D17). Disparidade dev/prod = "funciona na minha máquina" e bugs só-em-prod. Config hardcoded impede o mesmo binário rodar em múltiplos ambientes. Gestão correta de env é a base de deploy seguro e reproduzível.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | secrets em vault, env-config, paridade |
| 0.5–0.8 | ⚠ Warn | config parcial / paridade fraca |
| <0.5 | ❌ Fail | secret em env file / hardcoded |

## MUST

```bash
touring-quality check --gate F4.12 --target <FILE>
touring-quality score <FILE> --dims F4.12 --format json
```

## SHOULD

```bash
touring-quality check --gate F2.4 --target <FILE>       # cruzar com D17 (secrets no código)
grep -rInE '(API_KEY|SECRET|TOKEN|PASSWORD)\s*=' .env* 2>/dev/null  # segredo em env file commitado
```

## MAY

```bash
touring memory recall "quality:F4.12"
```

## Elite best practices (context7)

1. **Secrets em Vault/KMS, injetados em runtime** — nunca em código/env file; o app pega o segredo de um secret manager no boot (least-privilege, rotação, audit). Fonte: `/hashicorp/vault`.
2. **SOPS para secrets versionados encriptados** — quando precisa versionar config sensível, SOPS encripta com KMS/age (o cleartext nunca entra no Git). Fonte: `/getsops/sops`.
3. **Config via ambiente (12-factor III)** — config que varia por ambiente vem de env vars/secret manager, não de arquivos por-ambiente no código; o mesmo artefato roda em qualquer lugar. Fonte: 12factor.net.
4. **Paridade dev/staging/prod** — minimizar divergência (mesmos serviços/versões via containers); reduz bugs "só-em-prod". Fonte: 12-factor X.
5. **`.env.example` versionado, `.env` no .gitignore** — documentar as variáveis necessárias sem vazar valores; nunca commitar `.env` real. [training-data: 12-factor].

## Common pitfalls

- ⚠ `.env` com segredos reais commitado (vazamento — D17).
- Config hardcoded por ambiente (impede reuso do binário).
- Disparidade dev/prod (versões/serviços diferentes → bug só-em-prod).
- Secret manager ausente → segredos circulam em texto plano.

## Remediation

1. Mover secrets para Vault/SOPS; config para env vars; `.env` → `.gitignore` + `.env.example`.
2. Rotacionar qualquer segredo já exposto; alinhar paridade via container.
3. `Write tool --path policy/<env>.hcl --intent "Vault policy" --kind VaultPolicy` (Vault/SOPS; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C12 SYSTEM-HEALTH** + **C05/C06 EDIT**
- Dims relacionadas: D17 (F2.4 secrets), D19 (F2.6 config), D49 (F4.9 IaC)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /hashicorp/vault + /getsops/sops + 12-factor) — maintained by touring-quality_
