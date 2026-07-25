# D06 — Error Handling (F1.6)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.9
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_6_error_handling`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/dtolnay/thiserror` · `/dtolnay/anyhow` · Sentry · Rust Book ch. 9

## Definition

Avalia tratamento de erro idiomático: `Result<T, E>` + `?` para propagação, erros tipados com contexto, **zero `unwrap()`/`expect()`/`panic!()` em caminho de produção**, sem erros engolidos silenciosamente. Cobre robustez (P1 do CRC).

## Why it matters

`unwrap()` em produção = panic = serviço derrubado por input inesperado. Erro engolido = corrupção silenciosa (o pior tipo de bug). Tratamento idiomático transforma falhas em valores manejáveis e observáveis.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.9+ | ✅ Pass | Result+? , 0 unwrap em prod |
| 0.5–0.9 | ⚠ Warn | unwrap/expect em caminho não-test |
| <0.5 | ❌ Fail | refatorar para Result |

## MUST

```bash
touring-quality check --gate F1.6 --target <FILE>
touring-quality score <FILE> --dims F1.6 --format json
```

## SHOULD

```bash
touring ast grep <FILE> 'unwrap()'                       # localizar unwrap/expect em prod
touring ast rust-semantic <FILE>                         # contexto de Result/? usage
# Remediação — trocar unwrap por ? / or-default:
Edit tool --path <FILE> --operation ssr --pattern '$X.unwrap()' --replacement '$X?'
```

## MAY

```bash
touring memory recall "quality:F1.6"
touring gotcha match <FILE>
```

## Elite best practices (context7)

1. **`thiserror` para erros de biblioteca, `anyhow` para aplicação** — `#[derive(thiserror::Error)]` dá erros tipados+`Display` para APIs públicas; `anyhow::Result` para o topo da aplicação. Fonte: `/dtolnay/thiserror` + `/dtolnay/anyhow`.
2. **`?` para propagar, nunca `unwrap()` em prod** — usar `?` + `From`/`#[from]` para conversão automática. `unwrap_or_default()`/`unwrap_or_else()` quando há fallback legítimo. [training-data: Rust Book ch.9]
3. **`.context("...")` / `.with_context(|| ...)`** — anexar contexto ao propagar (anyhow) → stack-trace de domínio legível. Fonte: `/dtolnay/anyhow`.
4. **`expect()` só com invariante documentada** — `expect("BUG: checked above")` aceitável quando logicamente impossível falhar; nunca para I/O/input externo. [training-data: rust idioms]
5. **Nunca engolir erro** — `let _ = result;` ou `match { Err(_) => {} }` silencioso é proibido; logar (`tracing::error!`) ou propagar. Sentry-style: erros são eventos observáveis.

## Common pitfalls

- ⚠ `unwrap()`/`expect()` em handler/I/O de produção → panic.
- `Err` ignorado (`let _ =`) → corrupção silenciosa.
- `panic!()` para erro recuperável (deveria ser `Result`).
- NaN-panic: `sort_by(|a,b| a.partial_cmp(b).unwrap())` em floats → use `.unwrap_or(Ordering::Equal)`.

## Remediation

1. `touring ast grep <FILE> 'unwrap()'` → localizar (excluir `#[cfg(test)]`).
2. Converter para `?` + erro tipado (`thiserror`) via `Edit tool --operation ssr`.
3. `Edit tool --path <FILE> --operation ssr --pattern '\.unwrap\(\)' --replacement '.expect("<context>")'` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 1)

## Cross-references

- Decision matrix: **C05/C06 EDIT** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D11 (patterns), D24 (concurrency), D05 (tech debt)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: thiserror + anyhow) — maintained by touring-quality_
