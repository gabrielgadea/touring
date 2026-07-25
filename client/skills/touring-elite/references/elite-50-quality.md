# Elite 50-Dimension Quality Harness — Canonical Reference (constitutional, auto-load)

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-06-20
> **Authority**: Gabriel Gadea | **Origin**: `~/projects/touring/docs/2026-06-14-touring-elite-harness-strategy.md`
> **Engine (real)**: `touring-quality` binary (50 verifiers) + `touring-elite` aggregator (13 gates → `elite_aggregate.py`)
> **Per-dimension rules**: `~/.claude/skills/touring-elite/references/quality/D01..D52.md` (índice: `~/.claude/skills/touring-elite/references/quality/README.md`)
> **Complementa**: `touring-elite.md` (17-dim release gate) · `touring-decision-matrix.md` (C01-C12 task→cmd)

---

## Princípio operacional

Todo trabalho de **TACO**, **Touring**, das **skills** e dos **agents** deve atingir o nível **Premium de Elite de Mercado** medido nas **50 dimensões** (F1.1–F4.12). A excelência não é aspiração — é **mensurável** (`touring-quality`), **enforçável** (6 BLOCK dims) e **acionável** (remediação por dimensão). Antes de declarar qualquer entrega "completa", o gate 50-dim deve passar no tier-alvo da tarefa.

**Dois motores complementares — NÃO confundir:**

| Motor | Escopo | Comando real | Quando |
|---|---|---|---|
| **`touring-quality`** | 50 dims **por arquivo/workspace** (granular) | `touring-quality score <T> [--dims F1.1,F2.5]` · `check --gate F1.1 --target <T>` · `list` | Pré/pós-edit, auditoria por dimensão |
| **`touring-elite`** | 13 gates **release-readiness** (composite agregado) | `python3 ~/projects/touring/docs/elite_aggregate.py --check\|--json` | Release, PR >500 LOC, nova API pública |

---

## ⚠ COMANDOS REAIS — corrige alucinação sistêmica (VGP 2026-06-20)

As D-rules historicamente citavam comandos **inexistentes**. A sintaxe canônica **verificada** é:

| ❌ NÃO existe (não usar) | ✅ Real (verificado `--help`) |
|---|---|
| `touring quality score …` (subcommand) | `touring-quality score …` (binário standalone, hífen) |
| `touring-quality score <T> --gate F1.1` | `touring-quality check --gate F1.1 --target <T>` (1 dim) **ou** `touring-quality score <T> --dims F1.1` |
| `touring-quality check … --enforce warn` | flag `--enforce` **não existe**; enforcement = hook (BLOCK/WARN) ou `score --fail-below <N>` |
| ~~generators de qualidade dedicados~~ | **não existem** → remediar com `Edit`/`Write` (código) + re-score `touring-quality check` |

**Sintaxe canônica completa:**
```bash
touring-quality score <TARGET> [--workspace] [--dims F1.1,F2.5] [--format json] [--fail-below 0.80] [-o out.json]
touring-quality check --gate F2.1 --target <TARGET> [--format json]
touring-quality list                                   # 50 dims + enforcement glyph (⛔ BLOCK, ⚠ WARN)
```

---

## Catálogo das 50 dimensões — dim → enforcement → D-rule → elite-lib → agent owner

**Fase 1 — Code Quality & Architecture (12)**

| Dim | Nome | Pri | Enforce | D-rule | Elite ref (context7) | Agent owner |
|-----|------|-----|---------|--------|----------------------|-------------|
| F1.1 | Complexity | P1 | WARN | D01 | SonarQube · clippy | engineer |
| F1.2 | Maintainability | P1 | WARN | D02 | CodeClimate · Sourcery | engineer |
| F1.3 | Duplication | P1 | WARN | D03 | jscpd · SonarQube | engineer |
| F1.4 | Clean Code/SOLID | P1 | WARN | D04 | Sourcery · DeepSource | engineer |
| F1.5 | Technical debt | P1 | WARN | D05 | SonarQube SQALE | engineer |
| F1.6 | Error handling | P1 | WARN | D06 | thiserror · anyhow · Sentry | engineer |
| F1.7 | Component boundaries | P1 | ADVISORY | D07 | dependency-cruiser | architect · scouter |
| F1.8 | Dependency mgmt | P1 | ADVISORY (cycles→BLOCK) | D08 | Madge · dependency-cruiser | architect · scouter |
| F1.9 | API design | P1 | ADVISORY | D09 | OpenAPI · Spectral | architect |
| F1.10 | Data model | P1 | ADVISORY | D10 | Prisma · sqlfluff | architect |
| F1.11 | Design patterns | P1 | ADVISORY | D11 | rust-unofficial/patterns | architect |
| F1.12 | Arch consistency | P1 | ADVISORY | D12 | ArchUnit · dependency-cruiser | architect |

**Fase 2 — Security & Performance (13)**

| Dim | Nome | Pri | Enforce | D-rule | Elite ref | Agent owner |
|-----|------|-----|---------|--------|-----------|-------------|
| F2.1 | OWASP Top 10 | **P0** | **⛔ BLOCK** | D13 | OWASP CheatSheetSeries · Semgrep | engineer · auditor |
| F2.2 | Input validation | P1 | WARN | D15 | OWASP · Semgrep | engineer · auditor |
| F2.3 | AuthN/AuthZ | P1 | WARN | D16 | OWASP · OPA | engineer · auditor |
| F2.4 | Crypto / secrets | **P0** | **⛔ BLOCK** | D17 | gitleaks · detect-secrets | engineer · auditor |
| F2.5 | Dependency CVEs | **P0** | **⛔ BLOCK** | D14 | OSV-Scanner · RustSec | auditor |
| F2.6 | Config security | **P0** | **⛔ BLOCK** | D19 | Trivy · njsscan | auditor |
| F2.7 | DB performance | P1 | ADVISORY | D20 | sqlfluff · pganalyze | engineer |
| F2.8 | Memory mgmt | P1 | ADVISORY | D21 | Valgrind · rust ownership | engineer |
| F2.9 | Caching | P1 | ADVISORY | D22 | Redis | engineer |
| F2.10 | I/O bottlenecks | P1 | ADVISORY | D23 | tokio · strace | engineer |
| F2.11 | Concurrency | P1 | ADVISORY | D24 | tokio · Loom | engineer |
| F2.12 | Frontend perf | P1 | ADVISORY | D25 | Lighthouse | engineer |
| F2.13 | Scalability | P1 | ADVISORY | D26 | k6 · Locust | architect |

**Fase 3 — Testing & Documentation (13)**

| Dim | Nome | Pri | Enforce | D-rule | Elite ref | Agent owner |
|-----|------|-----|---------|--------|-----------|-------------|
| F3.1 | Test coverage | P1 | WARN | D27 | cargo-llvm-cov · Codecov | auditor |
| F3.2 | Test quality (mutation) | P1 | WARN | D28 | cargo-mutants · Stryker | auditor |
| F3.3 | Test pyramid | P1 | WARN | D29 | Playwright · Cypress | auditor |
| F3.4 | Edge cases | P1 | WARN | D30 | proptest · Hypothesis | auditor |
| F3.5 | Test maintainability | P1 | WARN | D31 | Testcontainers · WireMock | auditor |
| F3.6 | Security test gaps | P1 | WARN | D32 | OWASP ZAP | auditor |
| F3.7 | Performance test gaps | P1 | WARN | D33 | k6 · Gatling | auditor |
| F3.8 | Inline documentation | P1 | ADVISORY | D34 | rustdoc · Doxygen | scriber |
| F3.9 | API documentation | P1 | ADVISORY | D35 | OpenAPI Generator · Redoc | scriber |
| F3.10 | Architecture docs | P1 | ADVISORY | D36 | Mermaid · ADR-tools · C4 | scriber · architect |
| F3.11 | README completeness | P1 | ADVISORY | D37 | common-readme | scriber |
| F3.12 | Doc accuracy | P1 | ADVISORY | D38 | vale · codespell | scriber |
| F3.13 | Changelog/migration | P1 | ADVISORY | D39 | Keep a Changelog · semantic-release | scriber |

**Fase 4 — Best Practices & CI/CD (12)**

| Dim | Nome | Pri | Enforce | D-rule | Elite ref | Agent owner |
|-----|------|-----|---------|--------|-----------|-------------|
| F4.1 | Language idioms | P1 | WARN | D40 | clippy · ruff · ESLint | engineer |
| F4.2 | Framework patterns | P1 | ADVISORY | D41 | framework linters | engineer |
| F4.3 | Deprecated APIs | **P0** | **⛔ BLOCK** | D42 | `#[deprecated]` · deprecation warnings | engineer |
| F4.4 | Modernization | P1 | ADVISORY | D43 | jscodeshift · codemod | engineer |
| F4.5 | Package mgmt | **P0** | **⛔ BLOCK** | D44 | cargo-outdated · ncu · cargo-audit | auditor |
| F4.6 | Build config | P1 | ADVISORY | D46 | cargo-bloat · bundle-analyzer | engineer |
| F4.7 | CI/CD pipeline | P1 | ADVISORY | D47 | GitHub Actions · actionlint | scriber · auditor |
| F4.8 | Deployment strategy | P1 | ADVISORY | D48 | ArgoCD · Flagger | architect |
| F4.9 | Infrastructure as Code | P1 | ADVISORY | D49 | Terraform · checkov · tflint | architect |
| F4.10 | Monitoring/observability | P1 | ADVISORY | D50 | Prometheus · OpenTelemetry | architect |
| F4.11 | Incident response | P1 | ADVISORY | D51 | incident.io · PagerDuty | scriber |
| F4.12 | Environment mgmt | P1 | ADVISORY | D52 | Vault · SOPS | auditor |

**Consolidação (2026-06-20)**: duplicatas removidas — `D18_dep-cves` (≡ D14, F2.5) e `D45_pkg-mgmt` (≡ D44, F4.5). **50 dims = 50 D-rules canônicas.**

---

## 6-tier composite mapping (idêntico ao touring-elite)

| Score | Tier | Ação |
|-------|------|------|
| 0.95+ | 💎 Diamond | release-ready; BLOCK abaixo se gate exige Diamond |
| 0.90+ | 🥇 Platinum | best-in-class; WARN abaixo |
| 0.80+ | 🥈 Gold | production-grade; ADVISORY abaixo (**floor mínimo de entrega TACO**) |
| 0.70+ | 🥉 Silver | revisão humana requerida |
| 0.60+ | ⚪ Bronze | refatorar antes de merge |
| <0.60 | ⚫ Unranked | 🚫 reescrever — BLOCK |

---

## 3 Reflexos de Qualidade (extensão dos 9 Reflexos TACO)

| # | Reflexo | Ação default | Skip apenas se |
|---|---------|--------------|----------------|
| **10** | **Dim-Score-Verify** | Antes de declarar entrega completa: `touring-quality score <target> --fail-below 0.80` (tier-alvo da tarefa) | Edit trivial < 50 LOC sem API change |
| **11** | **Dim-Enforce-Block** | Para os 6 P0 (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5): `touring-quality check --gate <dim> --target <T>` ANTES de Write/Edit; FAIL = não prosseguir | Arquivo `.md/.json/.txt` sem código |
| **12** | **Dim-Auto-Remediate** | Score < tier-alvo numa dim → consultar D-rule (`skills/touring-elite/references/quality/D{nn}.md`) → aplicar fix via `Edit`/`Write` + re-score | Score já ≥ tier-alvo |

---

## 6 BLOCK dims (P0 — fail-closed) — enforcement obrigatório pré-Write

```bash
for dim in F2.1 F2.4 F2.5 F2.6 F4.3 F4.5; do
  touring-quality check --gate "$dim" --target "$FILE" --format json
done
# Qualquer FAIL (score < 0.5) num P0 → BLOCK: não escrever; remediar primeiro.
```

| Dim | Risco se ignorado |
|-----|-------------------|
| F2.1 OWASP | injection / XSS / deserialização → breach |
| F2.4 Secrets | segredo hardcoded = 100% vazável |
| F2.5 Dep CVEs | 78% dos breaches usam CVE conhecida |
| F2.6 Config | debug=true em prod = full pwn |
| F4.3 Deprecated | API deprecada = breaking change futuro |
| F4.5 Pkg mgmt | dependência EOL = sem patches de segurança |

---

## Mapeamento dimensão → agent (qual agent é dono de quais dims)

| Agent | Dims primárias | Foco |
|-------|----------------|------|
| **touring-scouter** | F1.7, F1.8 (discovery) | mapear boundaries/deps antes de tocar |
| **touring-architect** | F1.9–F1.12, F2.13, F3.10, F4.8–F4.10 | API/data/patterns/consistency/scale/arch-docs/deploy/IaC/observability |
| **touring-engineer** | F1.1–F1.6, F2.1–F2.4, F2.7–F2.12, F4.1–F4.4, F4.6 | code quality, security-in-code, perf, idioms |
| **touring-auditor** | F2.5, F2.6, F3.1–F3.7, F4.5, F4.12 | dep CVEs, config, testing completo, pkg/env |
| **touring-scriber** | F3.8–F3.13, F4.7, F4.11 | documentação completa, CI/CD docs, runbooks |

Cada agent DEVE rodar `touring-quality check`/`score` nas suas dims primárias antes de retornar JSON, e incluir o resultado no campo `quality_dimensions` do output.

---

## Template canônico de uma D-rule (toda rule em `quality/` segue)

```
# D{nn} — {Nome} ({F-id})
**Phase** · **Priority** (P0/P1) · **Tier target** (≥0.8) · **Enforcement** (⛔BLOCK/⚠WARN/ADVISORY) · **Verifier** (módulo real touring-quality)
## Definition          — o que mede, preciso (não "Phase X scoring for Y")
## Why it matters       — racional de elite + referência de mercado
## Thresholds           — bandas de score reais + ação
## MUST                 — touring-quality REAL (check/score), sintaxe verificada
## SHOULD               — touring ast tdg/rust-semantic/wiring + remediação via Edit/Write
## MAY                  — touring memory recall / context7
## Elite best practices — 3-5 práticas REAIS de context7 COM atribuição de fonte
## Common pitfalls      — específicos, reais
## Remediation          — caminho real (perfect-quality-* = PLANNED W7)
## Cross-references      — decision matrix C-cat + dims relacionadas
```

---

## Status do roadmap (real vs PLANNED)

| Camada | Status | Evidência |
|--------|--------|-----------|
| 50 verifiers (`touring-quality/src/verifications/`) | ✅ REAL | `f1_1..f4_12` wired |
| `touring-quality {score,check,list}` CLI | ✅ REAL | `touring-quality --version` = 0.1.0 |
| `touring-elite` 13-gate aggregator | ✅ REAL | `elite_aggregate.py` Diamond 0.9703 |
| 50 D-rules de referência | ✅ REAL (enriquecidas 2026-06-20) | `skills/touring-elite/references/quality/` |
| 6 BLOCK PreToolUse hooks | ⚠ PARCIAL | W5 wired em settings.json (verificar) |
| `touring quality` subcommand (integração no touring CLI) | 🔻 PLANNED | hoje é binário standalone `touring-quality` |

---

## Cross-references

| Tópico | Local |
|--------|-------|
| Índice das 50 D-rules | `~/.claude/skills/touring-elite/references/quality/README.md` |
| Release gate 17-dim/13-gate | `~/.claude/rules/touring-elite.md` + skill `touring-elite` |
| Task → comando (C01-C12) | `~/.claude/rules/touring-decision-matrix.md` |
| Padrões de combinação de tools | `~/.claude/rules/tool-combination-patterns.md` |
| Estratégia-fonte (50 dims, gap analysis, roadmap) | `~/projects/touring/docs/2026-06-14-touring-elite-harness-strategy.md` |
| Análise profunda 50-dim (2026-06-20) | `~/projects/touring/docs/2026-06-20-elite-50-deep-analysis.md` |
| Skill master Touring | `~/.claude/skills/Touring/SKILL.md` |
| TACO constitution | `~/.claude/CLAUDE.md` |

---

_v1.0 — 2026-06-20 | Keystone do harness 50-dim. Corrige comandos alucinados na fonte._
_Todo trabalho TACO/Touring mira ≥ Gold (0.80); release mira ≥ Diamond (0.95)._
