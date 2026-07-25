# D36 — Architecture Documentation (F3.10)

**Phase**: 3 (Testing & Documentation) | **Priority**: P1 | **Tier target**: ≥0.8
**Status**: ✅ wired | **Verifier**: `touring_quality::verifications::f3_10_arch_doc`
**Enforcement**: ADVISORY (silent unless drift)
**Elite reference (context7)**: `/mermaid-js/mermaid` · ADR-tools/MADR · Structurizr (C4)

## Definition

Avalia documentação de arquitetura: ADRs (Architecture Decision Records) capturando o porquê de decisões, diagramas (C4 levels, Mermaid), e mapas de componentes/fluxos. Doc de arquitetura é o que reduz o tempo de onboarding de semanas para dias.

## Why it matters

Decisões arquiteturais sem registro são re-litigadas ou violadas por desconhecimento. Sem diagrama, o modelo mental do sistema vive só na cabeça de quem o construiu (bus factor). ADR + C4 dão a um novo dev (ou LLM) o "porquê" que o código não conta.

## Thresholds

| Score | Status | Action |
|-------|--------|--------|
| 0.8+ | ✅ Pass | ADRs + diagramas atualizados |
| 0.5–0.8 | ⚠ Warn | doc parcial / sem decisões registradas |
| <0.5 | ❌ Fail | arquitetura não-documentada |

## MUST

```bash
touring-quality check --gate F3.10 --target <FILE>
touring-quality score <FILE> --dims F3.10 --format json
```

## SHOULD

```bash
touring ast workspace-info                              # gerar mapa de crates/dependências (base do C4)
touring wiring chains                                   # fluxos source→sink para diagramas
# Diagramas: Mermaid em .md; ADRs em docs/ (MADR template)
```

## MAY

```bash
touring memory recall "quality:F3.10"
```

## Elite best practices (context7 — `/mermaid-js/mermaid`)

1. **Mermaid embutido no Markdown** — diagramas como código versionável (` ```mermaid `), renderizados pelo GitHub/docs; evoluem com o repo, não apodrecem como imagem estática. Fonte: `/mermaid-js/mermaid`.
2. **ADR por decisão arquitetural (MADR template)** — contexto + decisão + consequências + alternativas; um arquivo por decisão em `docs/adr/`. Fonte: ADR-tools/MADR.
3. **C4 model (Context→Container→Component→Code)** — diagramas em níveis de zoom; começar pelo Context, descer conforme necessário. Fonte: Structurizr/C4.
4. **Gerar a base a partir do código** — `workspace-info`/`wiring chains` dão o grafo real (crates, deps, fluxos) → diagrama fiel, não desenhado à mão e desatualizado. Fonte: Touring USP.
5. **ADR imutável + status** — decisão registrada não se edita; supersede com novo ADR (status: superseded by ADR-NNN). Mantém o histórico do porquê. Fonte: MADR.

## Common pitfalls

- Diagrama como PNG estático que apodrece (vs Mermaid versionado).
- Decisões importantes sem ADR → re-litigadas/violadas.
- Doc de arquitetura divergindo do código real (ver D38 drift; `wiring` revela).
- C4 só no nível Code (sem o Context que dá o panorama).

## Remediation

1. `touring ast workspace-info`/`wiring chains` → gerar base do diagrama.
2. Escrever ADRs (MADR) + Mermaid no `.md` via Write (docs são .md, permitido).
3. `Write tool --path docs/adr/00NN-<decision>.md --intent "<ADR>" --kind ArchitectureDecisionRecord` (REGRA #2 canonical workflows — ver `~/projects/touring/docs/2026-06-21-quality-remediation-patterns.md` Pattern 7)

## Cross-references

- Decision matrix: **C10 ARCHITECTURAL** + **C11 DEPENDENCY-FLOW**
- Dims relacionadas: D12 (arch consistency), D08 (dep cycles), D39 (changelog)
- Keystone: `~/.claude/rules/elite-50-quality.md` (scriber/architect-owned)

---
_D-rule v2.0 — enriched 2026-06-20 (context7: /mermaid-js/mermaid + MADR) — maintained by touring-quality_
