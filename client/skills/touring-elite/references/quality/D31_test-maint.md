# D31 — Test Maintainability (F3.5)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_5_test_maint`
**Enforcement**: ⚠ WARN on PreToolUse:Edit/Write
**Elite reference (context7)**: `/testcontainers/testcontainers-rs` · WireMock · Mountebank

## Definition

Avalia manutenibilidade dos testes: isolamento (sem estado compartilhado entre testes), estratégia de mocking limpa, ausência de flakiness (não-determinismo), e fixtures reutilizáveis/determinísticas. Testes frágeis acabam desabilitados — e teste desabilitado é cobertura zero.

## Why it matters

Teste flaky destrói a confiança na suíte (o time começa a ignorar falhas). Teste acoplado/sem isolamento quebra em cascata. A consequência final é `#[ignore]` — o pior resultado, pois esconde o gap. Manter testes saudáveis é o que mantém a suíte viva.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | isolados, determinísticos |
| 0.5–0.8 | ⚠ Warn | flakiness / acoplamento |
| <0.5 | ❌ Fail | testes frágeis / #[ignore] acumulando |

## MUST

```bash
touring-quality check --gate F3.5 --target <FILE>
touring-quality score <FILE> --dims F3.5 --format json
```

## SHOULD

```bash
grep -rn '#\[ignore\]' <FILE>                           # testes desabilitados = gap escondido (VP-Scout 3b)
cargo test -- --test-threads=1 vs default               # detectar dependência de ordem/estado compartilhado
```

## MAY

```bash
touring memory recall "quality:F3.5"
```

## Elite best practices (context7)

1. **Isolamento total entre testes** — sem estado global compartilhado; cada teste cria e destrói seu setup. Rodar com `--test-threads=N` não deve mudar resultado (sinal de dependência de ordem). [training-data: test isolation].
2. **Testcontainers para dependências reais efêmeras** — DB/serviço em container por teste (descartável, isolado) em vez de mock frágil ou serviço compartilhado. Fonte: `/testcontainers/testcontainers-rs`.
3. **Mock no boundary, com contrato verificado** — WireMock/Mountebank para HTTP; mockar a fronteira externa, não a lógica interna; verificar que o mock reflete o contrato real. Fonte: WireMock.
4. **Determinismo: zero tempo real/random/ordem** — injetar clock/seed; nunca `sleep` ou `now()` direto em teste (flakiness). [training-data].
5. **Eliminar `#[ignore]`, não acumular** — teste ignorado = débito invisível; consertar ou remover (VP-Scout Cadeia 3b verifica corpo, não só nome). Fonte: Touring VP-Scout.

## Common pitfalls

- Estado global compartilhado → testes passam isolados, falham em paralelo.
- `sleep(n)`/`now()`/`rand()` sem injeção → flaky.
- Mock acoplado a detalhe interno → quebra em refactor.
- `#[ignore]` acumulando (gap escondido).

## Remediation

1. `grep '#\[ignore\]'` + rodar em paralelo → achar flaky/acoplados.
2. Isolar setup (testcontainers), injetar clock/seed, remover `#[ignore]` via `Edit tool`.
3. `Write tool --path tests/<container>.rs --intent "<testcontainer test>" --kind TestContainerTest` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 6)

## Cross-references

- Decision matrix: **C06 EDIT-MAJOR** + VP-Scout Cadeia 3b (test content)
- Dims relacionadas: D27 (coverage), D28 (test quality), D24 (concurrency)
- Keystone: `~/.claude/rules/elite-50-quality.md` (auditor-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: testcontainers + WireMock) — maintained by touring-quality_
