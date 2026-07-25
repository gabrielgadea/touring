# D25 — Frontend Performance (F2.12)

**Phase**: 2 (Security & Performance) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f2_12_frontend`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/googlechrome/lighthouse` · WebPageTest · Bundlephobia

## Definition

Avalia performance de frontend (aplicável a crates `bindings` web/WASM e UIs): tamanho de bundle, performance de render, lazy-loading, e Core Web Vitals (LCP, INP, CLS). Para Rust→WASM: tamanho do `.wasm`, code-splitting, e custo de boundary JS↔WASM.

## Why it matters

Cada 100ms de TTI custa ~7% de conversão. Bundle inchado e render bloqueante degradam UX diretamente. Para WASM, um `.wasm` de MBs sem split mata o first-load. É a dim de perf voltada ao usuário final.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | bundle enxuto, CWV verdes |
| 0.5–0.8 | ⚠ Warn | bundle grande / render bloqueante |
| <0.5 | ❌ Fail | otimizar |

## MUST

```bash
touring-quality check --gate F2.12 --target <FILE>
touring-quality score <FILE> --dims F2.12 --format json
```

## SHOULD

```bash
# Para WASM (Rust→web): medir tamanho + otimizar
wasm-opt -Oz; twiggy top <module.wasm>                  # tamanho por símbolo
# Para web: Lighthouse CI (LCP/INP/CLS), bundle analyzer
Edit tool --path <FILE> --operation rewrite   # lazy-load / code-split
```

## MAY

```bash
touring memory recall "quality:F2.12"
```

## Elite best practices (context7 — `/googlechrome/lighthouse`)

1. **Otimizar os Core Web Vitals** — LCP < 2.5s, INP < 200ms, CLS < 0.1; Lighthouse mede e prioriza. Fonte: Lighthouse / web.dev CWV.
2. **Code-splitting + lazy-load** — carregar só o necessário no first-paint; rotas/componentes pesados sob demanda. Fonte: Lighthouse (reduce unused JS).
3. **WASM: `wasm-opt -Oz` + `twiggy`** — minimizar `.wasm`; medir contribuição por símbolo; `wee_alloc`/strip para tamanho. [training-data: rust→wasm].
4. **Minimizar boundary JS↔WASM** — cada crossing tem custo de serialização; agrupar chamadas, passar dados em lote. [training-data: wasm-bindgen perf].
5. **Imagens/assets responsivos + caching** — formatos modernos, lazy `loading`, cache headers (ver D22). Fonte: Lighthouse (efficient cache policy).

## Common pitfalls

- Bundle monolítico (tudo no first-load).
- `.wasm` de MBs sem `wasm-opt`/split.
- Layout shift (CLS) por imagens sem dimensão.
- Boundary JS↔WASM chamado em loop apertado.

## Remediation

1. Medir (Lighthouse / `twiggy`) → identificar o maior ofensor.
2. Code-split, lazy-load, `wasm-opt -Oz` via `Edit tool`.
3. `Write tool --path <FILE> --intent "lazy-loaded component" --kind ReactComponent` ou `Edit tool --operation free-form` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + **C09 DEBUG-ROOT-CAUSE**
- Dims relacionadas: D22 (caching), D46 (build config), D26 (scalability)
- Keystone: `~/.claude/rules/elite-50-quality.md`

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /googlechrome/lighthouse) — maintained by touring-quality_
