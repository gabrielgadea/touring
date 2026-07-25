# D17 — Cryptographic Issues & Secrets (F2.4)

**Phase**: 2 (Security & Performance) | **Priority**: P0 | **Tier target**: ≥0.95 (P0 — sempre PASS)
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_4_secrets::F2_4`
**Enforcement**: ⛔ **BLOCK** on PreToolUse:Write/Edit (fail-closed)
**Elite reference (context7)**: `/gitleaks/gitleaks` (High reputation) · detect-secrets · GitGuardian · OWASP Cryptographic Storage

## Definition

Detecta **segredos hardcoded** (API keys, tokens, senhas, chaves privadas) e **uso de criptografia fraca** (algoritmos obsoletos MD5/SHA1/DES, IV estático, key management ruim). Cobre OWASP A02 Cryptographic Failures.

**Cobertura do verifier (`f2_4_secrets`, estendido 2026-06-20)**: prefixos de provedor — GitHub (`ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`/`github_pat_`), Slack (`xoxb-`/`xoxp-`/`xoxa-`/`xoxr-`/`xoxs-`), Stripe (`sk_live_`/`rk_live_`), AWS (`AKIA`/`ASIA`), Google (`AIza`), headers PEM; **assignments com nome de segredo** (`password = "…"`); **entropia genérica** (Shannon ≥ 4.5, len ≥ 24, sem espaços — pega tokens base62, ignora hashes hex cuja entropia ≤ 4.0). Bandas: strong → 0.0 (Fail/BLOCK) · keyword sem valor → 0.5 (Warn) · limpo → 1.0 (Pass).

## Why it matters

Um segredo commitado é **100% vazável** — uma vez no histórico, vale para sempre (rotação obrigatória). 78% dos incidentes de credential-leak vêm de segredos em código/config. Por ser P0 fail-closed, nenhum segredo chega a ser escrito.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 1.0   | ✅ Pass | sem segredo/crypto fraca |
| 0.5–0.9 | ⚠ Warn | match de baixa entropia — revisar |
| <0.5  | ❌ Fail | ⛔ **BLOCK** — segredo de alta entropia / crypto fraca |

## MUST

```bash
touring-quality check --gate F2.4 --target <FILE>          # 0 = PASS; <0.5 = ⛔ BLOCK pré-write
touring-quality score <FILE> --dims F2.4 --format json
```

## SHOULD

```bash
touring gotcha match <FILE>                                 # pitfalls de secret/crypto do arquivo
# Remediação: mover segredo p/ env/Vault e rotacionar; reescrever uso de crypto:
Edit tool --path <FILE> --operation rewrite --pattern '<segredo literal>' --replacement '<std::env::var("KEY")?>'
# `Edit tool --path <FILE> --operation ssr --pattern 'const [A-Z_]+: &str = "[a-zA-Z0-9]{32}";' --replacement 'fn <name>() -> String { std::env::var("<NAME>").expect("<NAME> set") }'` (gitleaks/detect-secrets)
```

## MAY

```bash
touring memory recall "quality:F2.4"                        # lições passadas de leak
```

## Elite best practices (context7 — `/gitleaks/gitleaks`)

1. **Regra = regex + keyword pre-filter + entropy** — toda detecção tem `keywords` (pré-filtro de string rápido) + `regex` Go + `entropy` (Shannon ≥ 3.5 no `secretGroup`). Fonte: `README.md` (Custom Gitleaks configuration). Reduz falso-positivo e custo.
2. **Validar regra com true/false positives** — toda rule nova é validada contra `tps` (deve casar) e `fps` (não deve). Fonte: `CONTRIBUTING.md` (`func Beamer()` + `validate(r, tps, fps)`). Aplica-se a qualquer detector custom.
3. **Allowlist estruturado, não desabilitar a regra** — usar `[[rules.allowlists]]` com `condition`, `paths`, `regexes`, `stopwords` em vez de remover a regra. Fonte: `extend_rule_allowlist_and.toml`. Mantém cobertura.
4. **Estender config default, não substituir** — `[extend] useDefault = true` + `disabledRules` cirúrgico. Fonte: `README.md`. Herda centenas de regras curadas.
5. **Crypto**: nunca MD5/SHA1 para segurança; usar `ring`/`RustCrypto` (Argon2/bcrypt para senhas, AES-GCM/ChaCha20-Poly1305 para AEAD), IV/nonce aleatório por mensagem, keys via KMS/Vault. [training-data: OWASP Cryptographic Storage Cheat Sheet]

## Common pitfalls

- ⛔ `const API_KEY = "sk-..."` / `let token = "ghp_..."` — segredo literal.
- ⛔ Segredo em `.env` commitado, em comentário, ou em teste fixture.
- MD5/SHA1 para hashing de senha; IV/nonce constante; `rand` não-CSPRNG para chaves.
- Desabilitar a regra inteira em vez de allowlist do FP específico.

## Remediation

1. `touring-quality check --gate F2.4 --target <FILE>` → localizar o segredo/uso fraco.
2. Mover segredo para env/Vault/SOPS (ver D52), **rotacionar** o segredo exposto.
3. Trocar crypto fraca por `ring`/`RustCrypto` via `Edit tool`.
4. Re-score `touring-quality check --gate F2.4` → PASS.

## Cross-references

- Decision matrix: **C05/C06 EDIT** (security gate pré-write) + **C09 DEBUG**
- Dims relacionadas: D13 (F2.1 OWASP), D19 (F2.6 config), D52 (F4.12 env/secrets mgmt)
- Keystone: `~/.claude/rules/elite-50-quality.md` (6 BLOCK dims)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: `/gitleaks/gitleaks`) — maintained by touring-quality_
