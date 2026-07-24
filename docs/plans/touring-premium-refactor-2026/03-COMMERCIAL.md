---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "commercial"
created: "2026-05-11"
relates_to:
  - 01-ARCHITECTURE.md
  - 02-DEPLOYMENT.md
  - W14-product-tiers--distribution.md
---
# 03-COMMERCIAL — Touring Tiers & GTM Strategy

> **Status**: Proposed | **Date**: 2026-05-11
> **Approved by**: Gabriel Gadea (architect) — "tiers integrated into roadmap as W14"

## 1. Four-tier model

| Tier | Target | Telemetry | Support | License |
|---|---|---|---|---|
| **Free** | Solo, students, OSS contributors | ON (metrics, no PII) | Community (GitHub issues) | MIT OR Apache-2.0 |
| **Standard** | Active OSS maintainers (registered) | ON (metrics, opt-out) | GitHub + Discord | MIT OR Apache-2.0 |
| **Premium** | Senior solo devs, small teams | **OFF default** | 24h SLA email + Discord | Commercial |
| **Enterprise** | Regulated industries, 200+ devs | OFF + audit logs SIEM | 4h SLA + office hours + dedicated CS | Commercial + custom MSA |

## 2. Feature matrix by subsystem

| Subsystem | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Languages (touring-code) | rust+py | + ts+go | + java/cpp/swift | full polyglot |
| Storage backends | sqlite+tantivy | + candle emb | + qdrant + voyage | + on-prem registry |
| Analysis quality | blast basic | + TDG + Halstead | + cross-feature + temporal | + custom rules engine |
| Offensive security | ✗ | ✗ | concolic + z3 | + bug-bounty + private vuln-db |
| Intelligence | basic reasoning | + RL + bandit | + MCTS + GoT + Pensieve | + DSPy + custom |
| Generator kinds | 8 | 24 | 36 | + custom templates |
| Assists | 3 | 7 | 10 | + custom plugins |
| Orchestration | basic DAG | + decompose MCTS | + session persist | + multi-user sync |
| Hooks | CC basic | + RL hooks | + prediction (L7-B) | + custom handlers |
| Bindings | python | + wasm | + web + capnp | + desktop + postgis + custom |
| SSO/Audit/Registry/On-prem | — | — | — | ✓ |

## 3. Cargo features → tier mapping

```toml
# touring-server/Cargo.toml
[features]
default = ["tier-free"]

tier-free = [
  "touring-foundation/full",
  "touring-code/lang-rust", "touring-code/lang-python",
  "touring-analysis/blast-basic",
  "touring-hooks/claude-code",
  "touring-storage/storage-vec-sqlite", "touring-storage/storage-fts",
]

tier-standard = [
  "tier-free",
  "touring-code/lang-typescript", "touring-code/lang-go",
  "touring-code/parser-ast-grep",
  "touring-analysis/quality-tdg", "touring-analysis/quality-halstead",
  "touring-generator/generator-rust", "touring-generator/generator-python",
  "touring-generator/generator-typescript",
  "touring-assists/assist-rust", "touring-assists/assist-typescript",
  "touring-intelligence/intel-rl", "touring-intelligence/intel-bandit",
  "touring-storage/storage-emb-candle",
  "touring-hooks/hooks-rl",
  "touring-orchestration/decompose-mcts",
]

tier-premium = [
  "tier-standard",
  "touring-code/lang-java", "touring-code/lang-cpp", "touring-code/parser-syn",
  "touring-analysis/quality-mi", "touring-analysis/temporal-history",
  "touring-analysis/cross-feature",
  "touring-offensive/concolic-tracer", "touring-offensive/solver-z3",
  "touring-intelligence/intel-mcts", "touring-intelligence/intel-got",
  "touring-intelligence/intel-pensieve",
  "touring-storage/storage-vec-qdrant", "touring-storage/storage-emb-voyage",
  "touring-generator/generator-tsx", "touring-generator/vgp-strict",
  "touring-hooks/hooks-prediction", "touring-hooks/hooks-cortex",
  "touring-bindings/bind-wasm", "touring-bindings/bind-web",
  "telemetry-off-default",
]

tier-enterprise = [
  "tier-premium",
  "enterprise-sso", "enterprise-audit", "enterprise-registry",
  "enterprise-custom-rules", "enterprise-custom-templates",
  "enterprise-mcp-plugins", "enterprise-onprem",
  "touring-bindings/bind-desktop", "touring-bindings/bind-postgis",
  "touring-bindings/bind-capnp",
  "touring-intelligence/intel-dspy", "touring-intelligence/intel-clustering",
  "touring-offensive/solver-cvc5", "touring-offensive/vuln-pattern-db",
]
```

## 4. License key (JWT ed25519)

File: `~/.touring/license.key` (user) or `<project>/.touring/license.key`.

```json
{
  "sub": "user@company.com",
  "iss": "license.touring.dev",
  "iat": 1736000000,
  "exp": 1767536000,
  "tier": "premium",
  "features": ["intel-dspy", "bind-postgis"],
  "max_projects": 10,
  "trial": false
}
```

- Public key embedded in binary (offline verification)
- No key → tier-free graceful
- Expired → 30-day grace + warning
- Corrupted → clear error + support link

## 5. Pricing matrix

| Plan | Annual | Monthly | Billing |
|---|---|---|---|
| Free | $0 | $0 | — |
| Standard | $0 (registered) | $0 | — |
| **Premium Individual** | **$348/yr** ($29/mo) | $39/mo | Stripe self-service |
| **Premium Team (5-30)** | $288/seat/yr | $32/seat/mo | Stripe |
| **Business (30-200)** | $228/seat/yr | $26/seat/mo | Stripe + invoice |
| **Enterprise (200+)** | Custom ($60-120k base + $35-50/seat/mo) | Custom | MSA |
| **Enterprise On-Prem** | Custom ($150-300k/yr + setup) | Custom | MSA |
| **OEM/Embedded** | Custom rev-share or flat license | — | Partnership |

### Discount policy

| Case | Discount |
|---|---|
| Annual vs monthly | -25% |
| 5+ seats team rate | -17% baseline |
| OSS projects (verified) | -50% |
| Education (.edu) | Free Premium (5 seats) |
| Non-profit (501c3) | -50% |
| Volume 50 seats | -20% |
| Volume 100 seats | -25% |
| Volume 500 seats | -30% |

## 6. Example enterprise quote (bank, 300 devs, on-prem)

| Component | $ |
|---|---|
| Base platform license | $60,000 |
| 300 seats × $400 | $120,000 |
| Private registry hosting | $12,000 |
| SSO setup (one-time) | $5,000 |
| Audit log SIEM integration | $8,000 |
| On-prem add-on | $90,000 |
| Dedicated CS 40h | included |
| **Annual ARR** | **$290,000** + $5k one-time |

## 7. Competitive landscape

| Product | Focus | Price | Overlap |
|---|---|---|---|
| GitHub Copilot | LLM autocomplete + chat | $10/$19/$39 | LIMITED |
| Cursor | IDE-fork w/ AI | $20/$40 | LIMITED |
| **Sourcegraph Cody** | Code search + AI ent | Free/$9/Custom | **HIGH** |
| **Continue.dev** | OSS AI assistant | Free/Ent custom | **HIGH** |
| Aider | CLI git-native | Free OSS | MEDIUM |
| Codeium/Windsurf | Autocomplete + agent | Free/$15/Custom | LIMITED |
| Tabnine | Autocomplete enterprise | $12/$39 | LIMITED |
| JetBrains AI | Plugin IDE | $10 add-on | LIMITED |
| rust-analyzer + clippy | LSP + linter | OSS | NICHE |
| ast-grep | AST search/rewrite | OSS | NICHE |
| **Semgrep** | Static analysis polyglot | Free/$40/Ent | **HIGH** |
| Snyk Code | Security DAST/SAST | Enterprise | LIMITED |

### Touring positioning

> **"Premium AI-native code intelligence platform — deep code understanding + agentic execution + persistent learning."**

7 moats:
1. Not "AI assistant" — code intelligence platform with agentic capabilities
2. Don't sell inference — BYOK (Anthropic, OpenAI, Voyage, local Ollama)
3. CLI-native, daemon-based, IDE-agnostic
4. Deep Rust (syn) + polyglot (tree-sitter + ast-grep) — uniquely both
5. Offensive security included — first in category
6. Memory + RL persistent across sessions
7. OSS-first with premium tiers

## 8. Sales motion

| Motion | Target | Cycle | Team |
|---|---|---|---|
| **PLG self-service** | Free → Premium → Business | 30-90 days | None |
| **SLG inside** | Business 30-200 devs | 60-90 days | 1 SDR/AE |
| **SLG enterprise** | 200+ devs | 6-12 months | AE + SE + CS |

## 9. Telemetry tier matrix

| Metric | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Command frequency | ON | ON | OFF | OFF |
| Error rate aggregate | ON | ON | OFF | OFF |
| Latency P50/P99 | ON | ON | OFF | OFF |
| Symbol counts | OFF | ON opt-out | OFF | OFF |
| Daemon uptime | OFF | ON opt-out | OFF | OFF |
| User identifier | ❌ NEVER | ❌ NEVER | ❌ NEVER | hash(email) only |
| Code content | ❌ NEVER | ❌ NEVER | ❌ NEVER | ❌ NEVER |
| Audit log entries | — | — | — | ✓ self-hosted SIEM |

## 10. Financial forecast 5 years

| Year | Revenue | Costs | Net | Headcount |
|---|---|---|---|---|
| Y1 (2027) | $1.2M | $1.8M | -$600k | 6 |
| Y2 (2028) | $5.8M | $5.0M | +$800k | 18 |
| Y3 (2029) | $15M | $12M | +$3M | 40 |
| Y4 (2030) | $35M | $26M | +$9M | 80 |
| Y5 (2031) | $70M | $50M | +$20M | 130 |

## 11. OKRs Y1

**O1** — Establish Touring as premium code intelligence platform.
- KR1: 15k GitHub stars by M12
- KR2: 700 premium subs by M12 ($25k MRR)
- KR3: 5 enterprise pilots by M9
- KR4: NPS ≥ 45 by M9

**O2** — Build world-class engineering foundation.
- KR1: Test coverage ≥ 30% workspace-wide by M12
- KR2: Zero cycles by M6
- KR3: docs.rs 100% green
- KR4: cargo-deny / audit / vet clean continuously

**O3** — Create defensible moats via community + ecosystem.
- KR1: 50 external contributors by M9
- KR2: 10 partner integrations live
- KR3: 5 conference talks
- KR4: 20 customer case studies

## 12. References

- W14 wave file: `W14-product-tiers--distribution.md` (detailed subtasks)
- Memory: `decision:touring-premium-roadmap-2026-05-11`
