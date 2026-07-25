# D15 — Input Validation (F2.2)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_2_input_validation`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/owasp/cheatsheetseries` (Input Validation) · Semgrep · Bearer

## Definition

Verifica validação/sanitização de entrada no boundary: allowlist (não blocklist), prevenção de path traversal, open redirect, e coerção segura de tipos. Complementa F2.1 (OWASP injection) e F2.4 (secrets) no eixo de input não-confiável.

## Why it matters

Toda entrada externa é hostil até provado o contrário. Validação fraca é a porta de entrada para injection (A03), traversal e SSRF. Validar no boundary (uma vez, cedo) é mais robusto e barato que checar em cada uso.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | allowlist no boundary |
| 0.5–0.8 | ⚠ Warn | validação parcial/blocklist |
| <0.5 | ❌ Fail | input não-validado em sink |

## MUST

```bash
touring-quality check --gate F2.2 --target <FILE>
touring-quality score <FILE> --dims F2.2 --format json
```

## SHOULD

```bash
touring ast grep <FILE> '<sink que consome input externo>'   # localizar uso de entrada não-validada
touring gotcha match <FILE>
Edit tool --path <FILE> --operation rewrite --pattern '<uso cru>' --replacement '<validado>'
```

## MAY

```bash
touring memory recall "quality:F2.2"
```

## Elite best practices (context7 — `/owasp/cheatsheetseries`)

1. **Allowlist, nunca blocklist** — definir o que É permitido (`^[a-z0-9]{3,10}$`), não tentar enumerar o malicioso. Fonte: `Injection_Prevention_Cheat_Sheet.md`.
2. **Validar no boundary + tipar** — converter input em tipo de domínio validado na entrada (newtype com construtor que valida — ver D10); o resto do código confia no tipo. [training-data: OWASP + rust type-driven]
3. **Path traversal: canonicalizar e confinar** — resolver o path e verificar que fica dentro do diretório permitido (`canonicalize()` + `starts_with(base)`). Fonte: OWASP File Path Cheat Sheet.
4. **Open redirect: allowlist de destinos** — nunca redirecionar para URL vinda do usuário sem checar contra allowlist de hosts. Fonte: OWASP Unvalidated Redirects.
5. **Validar entrada E encodar saída** — defense-in-depth; validação não substitui output encoding (ver D13). Fonte: `Java_Security_Cheat_Sheet.md` (INPUT WAY + OUTPUT WAY).

## Common pitfalls

- Blocklist de caracteres "perigosos" (sempre incompleta).
- Validação só no client-side (bypassável).
- Path do usuário usado direto em `File::open` → traversal (`../../etc/passwd`).
- Confiar em `Content-Type`/header controlado pelo cliente.

## Remediation

1. `touring ast grep` → localizar sinks que consomem input externo.
2. Adicionar allowlist + newtype validado no boundary via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern '<unsafe_input>' --replacement '<validated_input>'` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 3)

## Cross-references

- Decision matrix: **C05/C06 EDIT** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D13 (F2.1 OWASP), D17 (F2.4 secrets), D10 (data model)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /owasp/cheatsheetseries) — maintained by touring-quality_
