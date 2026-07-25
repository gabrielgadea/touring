# D03 — Code Duplication (F1.3)

**Phase**: 1 (Code Quality & Architecture) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f1_3_duplication`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: jscpd · SonarQube duplication · `/rust-unofficial/patterns`

## Definition

Detecta blocos copy-paste e oportunidades de abstração (funções, traits, generics, macros) que eliminariam repetição. Alvo de elite: < 3% de linhas duplicadas.

## Why it matters

Código duplicado multiplica o custo de cada correção (fix em 1 lugar, bug persiste em N) e é fonte primária de bugs por divergência. DRY reduz superfície de manutenção e risco de inconsistência.

## Thresholds

| Dup % | Score | Status | Action |
|-------|-------|--------|--------|
| < 3% | 0.9+ | ✅ Pass | saudável |
| 3–8% | 0.5–0.8 | ⚠ Warn | extrair abstração |
| > 8% | <0.5 | ❌ Fail | refatorar |

## MUST

```bash
touring-quality check --gate F1.3 --target <FILE>
touring-quality score <FILE> --dims F1.3 --format json
```

## SHOULD

```bash
touring ast grep <FILE> '<padrão estrutural repetido>'   # localizar clones estruturais (ast-grep)
touring tantivy search "<snippet duplicado>"             # achar cópias no workspace
touring assist apply extract_function --file <FILE> --range L1:L2 --name <shared_fn>
```

## MAY

```bash
touring memory recall "quality:F1.3"
```

## Elite best practices (context7)

1. **Extrair função/trait compartilhada, não macro por reflexo** — preferir abstração tipada; macro só quando a repetição é sintática e não há tipo comum. [training-data: `/rust-unofficial/patterns`]
2. **Generics + trait bounds para duplicação por tipo** — `fn f<T: Trait>(...)` em vez de N cópias por tipo concreto. Fonte: rust patterns (newtype/generics).
3. **Regra dos 3** — abstrair na terceira ocorrência, não na segunda (evita abstração prematura errada). [training-data: SonarQube/Sourcery]
4. **`ast grep` para detecção estrutural** — clones semânticos (mesma forma, nomes diferentes) escapam de diff textual; ast-grep com metavars (`$X`) os encontra.
5. **DRY de conhecimento, não de código acidental** — dois trechos iguais por coincidência (não compartilham razão de mudar) NÃO devem ser fundidos. [training-data: pragmatic DRY]

## Common pitfalls

- Copy-paste com pequena variação → bug quando só uma cópia é corrigida.
- Abstração prematura (fundir 2 coisas que divergirão) — acoplamento errado.
- Duplicação estrutural invisível ao grep textual.

## Remediation

1. `touring ast grep`/`tantivy search` → mapear todas as cópias.
2. `touring assist apply extract_function` para função/trait compartilhada; generics para dup por tipo.
3. `Edit tool --path <FILE> --operation free-form --content-from <refactored.rs>` (REGRA #2 canonical workflows — extrair trait compartilhada; ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 2)

## Cross-references

- Decision matrix: **C08 CROSS-CALLER-COMPARE** (callsites similares) + **C06 EDIT-MAJOR**
- Dims relacionadas: D01 (complexity), D04 (SOLID), D11 (patterns)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: rust patterns + jscpd) — maintained by touring-quality_
