# D13 — OWASP Top 10 (F2.1)

**Phase**: 2 (Security & Performance) | **Priority**: P0 | **Tier target**: ≥0.95 (P0 — sempre PASS)
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_1_owasp::F2_1`
**Enforcement**: ⛔ **BLOCK** on PreToolUse:Write/Edit (fail-closed)
**Elite reference (context7)**: `/owasp/cheatsheetseries` (4219 snippets, High reputation) · Semgrep · CodeQL

## Definition

OWASP Top 10 cobre as classes de vulnerabilidade mais exploradas: **A03 Injection** (SQL/command/LDAP), **A01 Broken Access Control**, **A07 Identification/Auth Failures**, **A08 Software & Data Integrity** (deserialização insegura), **A02 Cryptographic Failures**, **A05 Security Misconfiguration**. Esta dimensão detecta padrões de código que introduzem essas classes.

## Why it matters

Injection é, historicamente, a **causa #1 de breaches**. Uma única query concatenada com input do usuário ou um `exec()` não-escapado é exploitável. Por ser P0, é a defesa de primeira linha do harness: nenhum código com injection-pattern chega a ser escrito.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 1.0   | ✅ Pass | sem padrão de injection |
| 0.5–0.9 | ⚠ Warn | padrão suspeito — revisar manualmente |
| <0.5  | ❌ Fail | ⛔ **BLOCK** — injection-pattern detectado |

## MUST

```bash
touring-quality check --gate F2.1 --target <FILE>          # 0 = PASS; <0.5 = BLOCK pré-write
touring-quality score <FILE> --dims F2.1 --format json
```

## SHOULD

```bash
touring-quality check --gate F2.1 --target <FILE> --format json | jq '.score, .blockers'
# Remediação (NÃO existe perfect-quality-*; usar Edit tool para reescrever o callsite):
Edit tool --path <FILE> --operation rewrite \
  --pattern '<query concatenada>' --replacement '<query parametrizada>'
touring index find <funcao_suspeita>                        # mapear callers do sink
```

## MAY

```bash
touring memory recall "quality:F2.1"                        # lições passadas de injection
touring gotcha match <FILE>                                 # pitfalls conhecidos do arquivo
```

## Elite best practices (context7 — `/owasp/cheatsheetseries`)

1. **Parameterized queries sempre** — nunca concatenar input em SQL. Fonte: `Injection_Prevention_Cheat_Sheet.md`. Ex.: `db.QueryRow("SELECT … WHERE email = ?", email)` em vez de `"… WHERE email = '" + email + "'"`.
2. **Allowlist validation no boundary** — validar com regex restritiva ANTES de usar. Fonte: `Injection_Prevention_Cheat_Sheet.md` → `^[a-z0-9]{3,10}$`. Java: `Pattern.matches("[a-zA-Z0-9\\s\\-]{1,50}", input)`.
3. **Escapar comandos shell** — nunca passar input cru a `exec`. Fonte: `Laravel_Cheat_Sheet.md` → `escapeshellarg`/`escapeshellcmd`. Em Rust: `std::process::Command` com args vetorizados (nunca `sh -c "… $input"`).
4. **Output encoding contextual** — encodar na saída por contexto (HTML/JS/URL). Fonte: `Java_Security_Cheat_Sheet.md` → OWASP Java Encoder `Encode.forHtml`. Defesa contra XSS armazenado.
5. **Defense-in-depth: validar entrada E encodar saída** — nunca confiar só num lado. Fonte: `Java_Security_Cheat_Sheet.md` (INPUT WAY + OUTPUT WAY).

## Common pitfalls

- ⛔ Concatenação de string em SQL/`format!` que vira query — exploitável.
- ⛔ `Command::new("sh").arg("-c").arg(format!("… {input}"))` — command injection.
- Blocklist em vez de allowlist (sempre incompleta).
- Confiar em validação só no client-side.

## Remediation

1. `touring-quality check --gate F2.1 --target <FILE>` → identificar o sink.
2. Reescrever via `Edit tool` (parametrizar query / vetorizar args / adicionar allowlist).
3. Re-score: `touring-quality check --gate F2.1 --target <FILE>` deve voltar a PASS.
4. `Edit tool --path <FILE> --operation ssr --pattern '<unsafe_sanitizer>' --replacement '<safe_alternative>'` (Semgrep/CodeQL; REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Patterns 3/4)

## Cross-references

- Decision matrix: **C09 DEBUG-ROOT-CAUSE** + **C05/C06 EDIT** (security gate pré-write)
- Dims relacionadas: D15 (F2.2 input validation), D17 (F2.4 secrets), D16 (F2.3 authz)
- Keystone: `~/.claude/rules/elite-50-quality.md` (6 BLOCK dims)

---
_D-rule v2.0 — enriched 2026-06-20 (context7 `/owasp/cheatsheetseries`) — gold-standard exemplar_
