# D19 — Configuration Security (F2.6)

**Phase**: 2 (Security & Performance) | **Priority**: P0 | **Tier target**: ≥0.95 (P0 — sempre PASS)
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_6_config::F2_6`
**Enforcement**: ⛔ **BLOCK** on PreToolUse:Write/Edit (fail-closed)
**Elite reference (context7)**: `/aquasecurity/trivy` · `/websites/embarkstudios_github_io_cargo-deny` · OWASP Security Misconfiguration

## Definition

Detecta **misconfiguration de segurança**: debug/verbose habilitado em produção, CORS permissivo (`*`), headers de segurança ausentes (CSP, HSTS, X-Frame-Options), stack traces expostos, build-scripts/executáveis não-confiáveis na cadeia de dependências. Cobre OWASP A05.

## Why it matters

Configuração insegura é explorável sem nenhum bug de código: `debug = true` em prod vaza internals → full pwn; CORS `*` permite credential theft cross-origin. É barato de prevenir e caro de remediar pós-incidente.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 1.0   | ✅ Pass | config endurecida |
| 0.5–0.9 | ⚠ Warn | config permissiva — revisar |
| <0.5  | ❌ Fail | ⛔ **BLOCK** — debug/CORS-* /header ausente em prod |

## MUST

```bash
touring-quality check --gate F2.6 --target <FILE>          # <0.5 = ⛔ BLOCK pré-write
touring-quality score <FILE> --dims F2.6 --format json
```

## SHOULD

```bash
cargo deny check bans                                       # executáveis/build-scripts não-confiáveis na cadeia
# Remediação: endurecer config (debug=false, CORS restrito, headers):
Edit tool --path <FILE> --operation rewrite --pattern 'debug = true' --replacement 'debug = false'
# `Edit tool --path <FILE> --operation ssr --pattern 'const DEBUG: bool = true;' --replacement 'const DEBUG: bool = cfg!(debug_assertions);'` (CSP/HSTS/CORS)
```

## MAY

```bash
touring memory recall "quality:F2.6"
touring gotcha match <FILE>
```

## Elite best practices (context7)

1. **Negar executáveis/build-scripts não-confiáveis na cadeia** — `[bans] build.executables = "deny"`, `interpreted = "deny"` com bypass por checksum. Fonte: `/websites/embarkstudios_github_io_cargo-deny` (`[bans.build]`). Bloqueia supply-chain via build.rs.
2. **`multiple-versions = "deny"` + `wildcards = "deny"`** — superfície de config previsível, sem dep fantasma. Fonte: cargo-deny `[bans]`.
3. **CORS allowlist explícita, nunca `*` com credenciais** — origem por allowlist; `Access-Control-Allow-Credentials` exige origem específica. [training-data: OWASP Security Misconfiguration / HTML5 CORS Cheat Sheet]
4. **Security headers por default** — CSP restritiva, HSTS `max-age` longo + preload, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`. [training-data: OWASP Secure Headers]
5. **Config como dado scaneável (IaC scanning)** — tratar config de infra como código e escanear (Trivy/checkov) por defaults inseguros. Fonte: `/aquasecurity/trivy` (misconfig scanning) — ver também D49 (F4.9 IaC).

## Common pitfalls

- ⛔ `debug = true` / `RUST_BACKTRACE=full` / verbose errors expostos ao cliente em prod.
- ⛔ `Access-Control-Allow-Origin: *` junto com credenciais.
- Ausência de CSP/HSTS; cookies sem `Secure`/`HttpOnly`/`SameSite`.
- build.rs de dep não auditado executando no build.

## Remediation

1. `touring-quality check --gate F2.6 --target <FILE>` → localizar o setting inseguro.
2. Endurecer via `Edit tool` (debug off, CORS allowlist, headers).
3. `cargo deny check bans` para a cadeia de build.
4. Re-score → PASS.

## Cross-references

- Decision matrix: **C05/C06 EDIT** + **C12 SYSTEM-HEALTH**
- Dims relacionadas: D17 (F2.4 secrets), D49 (F4.9 IaC), D52 (F4.12 env)
- Keystone: `~/.claude/rules/elite-50-quality.md` (6 BLOCK dims)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: cargo-deny + Trivy) — maintained by touring-quality_
