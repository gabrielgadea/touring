# D16 — Authentication / Authorization (F2.3)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_3_authz`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/owasp/cheatsheetseries` (AuthN/AuthZ, Session) · OAuth2/OIDC · OPA

## Definition

Avalia lógica de autenticação e autorização: verificação de identidade, controle de acesso (broken access control = OWASP A01), gestão de sessão, prevenção de privilege escalation, e uso correto de OAuth2/OIDC. Checagem deve ser server-side e por-recurso.

## Why it matters

Broken Access Control é o **#1 da OWASP Top 10 (2021)**. Uma checagem de autorização ausente ou client-side permite acesso indevido a dados/ações. Auth é onde "esquecer um caso" = brecha direta.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | authz server-side por recurso |
| 0.5–0.8 | ⚠ Warn | checagem incompleta |
| <0.5 | ❌ Fail | acesso não-controlado |

## MUST

```bash
touring-quality check --gate F2.3 --target <FILE>
touring-quality score <FILE> --dims F2.3 --format json
```

## SHOULD

```bash
touring index find <auth_fn>                            # mapear todos os enforcement points
touring wiring impact <auth_fn> --depth 2               # endpoints que dependem (ou não) da checagem
touring gotcha match <FILE>
```

## MAY

```bash
touring memory recall "quality:F2.3"
```

## Elite best practices (context7 — `/owasp/cheatsheetseries`)

1. **Deny-by-default + checar autorização em cada recurso** — negar acesso a menos que explicitamente permitido; nunca confiar que "a UI esconde o botão". Fonte: OWASP Authorization Cheat Sheet (A01).
2. **Least privilege** — conceder o mínimo necessário; separar roles; re-verificar em mudança de contexto. Fonte: OWASP Access Control.
3. **Sessão segura** — cookies `Secure`/`HttpOnly`/`SameSite`; rotacionar ID no login; timeout de inatividade; invalidar no logout. Fonte: OWASP Session Management Cheat Sheet.
4. **OAuth2/OIDC corretos** — validar `state` (CSRF), validar assinatura+`aud`+`exp` do JWT, usar PKCE em clients públicos. Fonte: OWASP OAuth/OIDC.
5. **Policy-as-code (OPA) para authz complexo** — externalizar regras de autorização declarativas, testáveis e auditáveis em vez de espalhar `if role ==` pelo código. [training-data: OPA]

## Common pitfalls

- Authz só na UI/client (IDOR: trocar o ID na URL acessa dado alheio).
- Checar autenticação mas não autorização (logado ≠ autorizado para ESTE recurso).
- JWT sem validar assinatura/`exp`/`aud`; sessão sem rotação no login (fixation).
- Privilege escalation por checagem ausente em endpoint admin.

## Remediation

1. `touring index find`/`wiring impact` → confirmar que TODO endpoint sensível passa pela checagem (C08 cross-caller compare).
2. Adicionar deny-by-default + checagem por recurso via `Edit tool`.
3. `Edit tool --path <FILE> --operation ssr --pattern '<missing_authz>' --replacement '<authz_check>'` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 3)

## Cross-references

- Decision matrix: **C08 CROSS-CALLER-COMPARE** (endpoints simétricos) + **C05/C06 EDIT**
- Dims relacionadas: D13 (F2.1 OWASP), D17 (F2.4 secrets), D32 (F3.6 security tests)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /owasp/cheatsheetseries) — maintained by touring-quality_
