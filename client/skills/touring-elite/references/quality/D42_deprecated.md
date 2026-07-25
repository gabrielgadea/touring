# D42 — Deprecated APIs (F4.3)

**Phase**: 4 (Best Practices & CI/CD) | **Priority**: P0 | **Tier target**: ≥0.95 (P0 — sempre PASS)
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f4_3_deprecated::F4_3`
**Enforcement**: ⛔ **BLOCK** on PreToolUse:Write/Edit (fail-closed)
**Elite reference (context7)**: `/rust-lang/rust` (`#[deprecated]`) · `/rust-lang/cargo` · cargo build deprecation warnings

## Definition

Detecta uso de **APIs deprecadas**: funções/métodos/itens marcados `#[deprecated]`, crates/edições obsoletas, e padrões que o compilador ou o ecossistema sinalizam como em vias de remoção. Novo código NÃO deve introduzir consumo de API deprecada.

## Why it matters

API deprecada = breaking change agendado. Construir sobre ela cria débito que quebra no próximo major. Detectar no momento da escrita (BLOCK) é ordens de magnitude mais barato que migrar depois sob pressão de upgrade.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 1.0   | ✅ Pass | sem uso de API deprecada |
| 0.5–0.9 | ⚠ Warn | uso deprecado com substituto disponível |
| <0.5  | ❌ Fail | ⛔ **BLOCK** — introduz consumo de API deprecada |

## MUST

```bash
touring-quality check --gate F4.3 --target <FILE>          # <0.5 = ⛔ BLOCK pré-write
touring-quality score <FILE> --dims F4.3 --format json
```

## SHOULD

```bash
cargo build 2>&1 | grep -i "deprecated"                     # warnings de deprecação do compilador
touring index find <deprecated_symbol>                      # mapear callers antes de migrar
# Remediação: migrar para a API substituta indicada na nota #[deprecated(note = "...")]:
Edit tool --path <FILE> --operation ssr --pattern '<old_api(...)>' --replacement '<new_api(...)>'
# `Edit tool --path <FILE> --operation ssr --pattern '<old_api>\(' --replacement '<new_api>('` (`#[deprecated(note = "...")]`)
```

## MAY

```bash
touring memory recall "quality:F4.3"
touring ast rust-semantic <FILE>                            # detectar derives/attrs deprecados
```

## Elite best practices (context7)

1. **`#[deprecated(since = "x.y.z", note = "use Foo::bar instead")]`** — ao deprecar API própria, SEMPRE incluir `since` + `note` com o substituto. Fonte: `/rust-lang/rust` (deprecated attribute). Dá caminho de migração automático ao consumidor.
2. **`#![deny(deprecated)]` em crate novo** — promover o warning a erro de compilação em código greenfield, garantindo zero uso deprecado. [training-data: rustc lints]
3. **Migração via edition + `cargo fix --edition`** — usar a ferramenta oficial para migrar idiomas/APIs entre editions de forma mecânica e auditável. Fonte: `/rust-lang/cargo` (edition migration).
4. **Verificar substituto na nota antes de migrar** — a `note` do `#[deprecated]` aponta a API canônica; migrar para ela, não inventar alternativa. [training-data: Rust API evolution]
5. **`cargo deny` para crates unmaintained** — `unmaintained = "all"` sinaliza dependências abandonadas (deprecação no nível de pacote — ver D44 F4.5). Fonte: cargo-deny advisories.

## Common pitfalls

- ⛔ Chamar método `#[deprecated]` em código novo "porque ainda compila".
- Ignorar warnings de deprecação no build (ruído acumula até virar erro no major).
- Migrar para alternativa não-canônica em vez da indicada na `note`.
- Edition antiga travando idiomas deprecados (migrar com `cargo fix --edition`).

## Remediation

1. `touring-quality check --gate F4.3 --target <FILE>` + `cargo build | grep deprecated` → localizar uso.
2. Ler a `note` do `#[deprecated]` → API substituta.
3. `touring index find` nos callers → migrar via `Edit tool --operation ssr`.
4. Re-score → PASS.

## Cross-references

- Decision matrix: **C05/C06 EDIT** + **C03 SYMBOL-LOOKUP** (callers)
- Dims relacionadas: D43 (F4.4 modernization), D44 (F4.5 pkg-mgmt), D40 (F4.1 idioms)
- Keystone: `~/.claude/rules/elite-50-quality.md` (6 BLOCK dims)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust-lang/rust + cargo) — maintained by touring-quality_
