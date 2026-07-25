# D32 — Security Test Gaps (F3.6)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_6_sec_tests`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/zaproxy/zaproxy` (OWASP ZAP) · Burp · Gauntlt

## Definition

Avalia se há testes cobrindo os controles de segurança: testes de autenticação/autorização (que negam acesso indevido), de validação de input (que rejeitam malicioso), e DAST (Dynamic Application Security Testing) nos endpoints. Segurança não-testada degrada silenciosamente.

## Why it matters

Controles de segurança (F2.1-F2.6) sem testes que provem que NEGAM o ataque são esperança, não garantia. Um teste de authz que verifica "user comum não acessa /admin" é a prova viva do controle. Sem ele, um refactor pode remover a checagem sem ninguém notar até o breach.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | testes negam ataque (authz/input) |
| 0.5–0.8 | ⚠ Warn | controles parcialmente testados |
| <0.5 | ❌ Fail | segurança não-testada |

## MUST

```bash
touring-quality check --gate F3.6 --target <FILE>
touring-quality score <FILE> --dims F3.6 --format json
```

## SHOULD

```bash
touring-quality check --gate F2.3 --target <FILE>       # cruzar com authz (D16): cada controle tem teste?
# DAST: OWASP ZAP baseline scan nos endpoints; testes negativos de auth/input
Write tool + touring generate verify --target <auth_control> --crate <C> # teste que prova a NEGAÇÃO
```

## MAY

```bash
touring memory recall "quality:F3.6"
```

## Elite best practices (context7 — `/zaproxy/zaproxy`)

1. **Teste negativo: provar que o ataque é NEGADO** — `assert` que user sem permissão recebe 403, input malicioso é rejeitado; o teste positivo (acesso permitido) não prova o controle. [training-data: security testing].
2. **DAST no CI (ZAP baseline)** — OWASP ZAP automated scan contra a app rodando; pega misconfig/headers/injection em runtime. Fonte: `/zaproxy/zaproxy` (baseline scan / automation framework).
3. **Cobrir cada dim de segurança com teste** — F2.1 (injection), F2.2 (input), F2.3 (authz), F2.4 (secrets) — cada controle tem um teste correspondente. [training-data: defense-in-depth].
4. **Fuzzing de entrada de segurança** — combinar com D30 (cargo-fuzz) para parsers/auth tokens. [training-data].
5. **Regressão de segurança** — todo bug de segurança corrigido vira um teste que falha se reintroduzido. [training-data: secure SDLC].

## Common pitfalls

- Testar só o caminho autorizado (não prova que o não-autorizado é negado).
- Nenhum DAST → vulnerabilidades de runtime invisíveis.
- Controle de segurança sem teste → removível silenciosamente em refactor.
- Bug de segurança corrigido sem teste de regressão.

## Remediation

1. Para cada controle (D13-D19), adicionar teste negativo que prova a negação.
2. Integrar ZAP baseline no CI; `Write tool + touring generate verify` para regressão.
3. `Write tool --path tests/sec/<scenario>.yaml --intent "<ZAP test>" --kind SecurityTest` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + **C08 CROSS-CALLER-COMPARE** (endpoints)
- Dims relacionadas: D13 (OWASP), D16 (authz), D15 (input), D30 (edge/fuzz)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /zaproxy/zaproxy) — maintained by touring-quality_
