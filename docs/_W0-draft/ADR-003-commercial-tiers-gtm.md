# ADR-003 — Touring Commercial Tiers + Go-To-Market Strategy

> **Status**: Proposed | **Date**: 2026-05-11 | **Authors**: Gabriel Gadea (architect) + TACO (orchestrator)
> **Relates to**: ADR-001 (Architecture), ADR-002 (Deployment), MASTER-PLAN-2026 (W14)
> **Approved by Gabriel**: Tiers integrated into roadmap as W14

## 1. Context

Touring is being transformed from internal tool into a **premium commercial product**.
The architecture itself (ADR-001) demonstrates the quality bar. Now we define:

- **What is sold** (tiers + features)
- **How it is priced** (per-developer, per-team, enterprise)
- **Who is targeted** (segments)
- **How they discover and buy** (channels, sales motion)
- **What success looks like** (KPIs, financial forecast)

## 2. Decision

### 2.1 Four-tier model

| Tier | Target | Telemetry | Support | License |
|---|---|---|---|---|
| **Free** | Solo, students, OSS contributors | ON (metrics only, no PII) | Community (GitHub issues) | MIT OR Apache-2.0 |
| **Standard** | Active OSS maintainers (registered) | ON (metrics, opt-out) | GitHub + Discord community | MIT OR Apache-2.0 |
| **Premium** | Senior solo devs, small teams | **OFF by default** | 24h SLA email + private Discord | Commercial |
| **Enterprise** | Regulated industries, 200+ devs | OFF + audit logs to SIEM | 4h SLA + office hours + dedicated CS | Commercial + custom MSA |

### 2.2 Feature matrix by subsystem

| Subsystem | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Languages (touring-code) | rust+py | + ts+go | + java/cpp/swift | full polyglot |
| Storage backends | sqlite + tantivy | + candle emb | + qdrant + voyage | + on-prem registry |
| Analysis quality | basic blast | + TDG + Halstead | + cross-feature + temporal | + custom rules engine |
| Offensive security | ✗ | ✗ | concolic + solver | + bug-bounty + private vuln-db |
| Intelligence | basic reasoning | + RL + bandit | + MCTS + GoT + Pensieve | + DSPy + custom strategies |
| Generator kinds | 8 | 24 | 36 | + custom templates |
| Assists | 3 | 7 | 10 | + custom assist plugins |
| Orchestration | basic DAG | + decompose MCTS | + session persistence | + multi-user sync |
| Hooks | CC integration | + RL hooks | + prediction (L7-B) | + custom hook handlers |
| Bindings | python | + wasm | + web + capnp | + desktop + postgis + custom |
| SSO/Audit/Registry/On-prem | — | — | — | ✓ |

### 2.3 Cargo features → tier mapping

```toml
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

### 2.4 License key system (JWT ed25519)

License key location: `~/.touring/license.key` (user-scoped) or
`<project>/.touring/license.key` (project override).

Format: JWT signed with **ed25519**; public key embedded in binary for offline
verification.

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

Verification flow:
- Binary validates JWT on startup → enables tier
- No key → graceful tier-free
- Expired → fallback to tier-free + warning + 30-day grace
- Corrupted → clear error + support link

### 2.5 Pricing matrix

| Plan | Annual price | Monthly price | Billing |
|---|---|---|---|
| Free | $0 | $0 | — |
| Standard | $0 (registered) | $0 | — |
| **Premium Individual** | **$348/yr** ($29/mo) | $39/mo | Stripe self-service |
| **Premium Team (5-30 seats)** | $288/seat/yr ($24/seat/mo) | $32/seat/mo | Stripe |
| **Business (30-200 seats)** | $228/seat/yr ($19/seat/mo) | $26/seat/mo | Stripe + invoice |
| **Enterprise (200+)** | Custom ($60-120k base + $35-50/seat/mo) | Custom | MSA |
| **Enterprise On-Prem** | Custom ($150-300k/yr + setup) | Custom | MSA |
| **OEM/Embedded** | Custom rev-share or flat license | — | Partnership |

#### Discount policy

| Case | Discount |
|---|---|
| Annual vs monthly | -25% |
| 5+ seats team rate | -17% baseline |
| OSS projects (verified) | -50% |
| Education (.edu verified) | Free Premium (up to 5 seats) |
| Non-profit (501c3 verified) | -50% |
| Volume 50 seats | -20% |
| Volume 100 seats | -25% |
| Volume 500 seats | -30% |

#### Example enterprise quote (bank, 300 devs, on-prem)

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

## 3. Competitive landscape

| Product | Focus | Price | Overlap |
|---|---|---|---|
| GitHub Copilot | LLM autocomplete + chat | $10/$19/$39 | LIMITED |
| Cursor | IDE-fork VSCode w/ AI | $20/$40 | LIMITED |
| **Sourcegraph Cody** | Code search + AI ent | Free/$9 Pro/Custom | **HIGH** |
| **Continue.dev** | OSS AI assistant | Free/Ent custom | **HIGH** |
| Aider | CLI git-native AI pair | OSS | MEDIUM |
| Codeium/Windsurf | Autocomplete + agent | Free/$15/Custom | LIMITED |
| Tabnine | Autocomplete enterprise | $12/$39 | LIMITED |
| JetBrains AI | Plugin IDE | $10 add-on | LIMITED |
| rust-analyzer + clippy | LSP + linter Rust | OSS | NICHE |
| ast-grep | AST search/rewrite polyglot | OSS | NICHE |
| **Semgrep** | Static analysis polyglot | Free/$40/Ent | **HIGH** |
| Snyk Code | Security DAST/SAST | Enterprise | LIMITED |

### Touring positioning

> **"Premium AI-native code intelligence platform — deep code understanding + agentic execution + persistent learning."**

Differentiation (7 moats):

1. **Not "AI assistant"** primarily — code intelligence platform with AI agentic capabilities
2. **Don't sell inference** — BYOK (Anthropic, OpenAI, Voyage, local Ollama)
3. **CLI-native, daemon-based, IDE-agnostic** — works with CC, Cursor, Cline, Aider, any MCP client
4. **Deep Rust (syn) + polyglot (tree-sitter + ast-grep)** — uniquely both
5. **Offensive security included** (concolic, erickson, vuln-db) — first in category
6. **Memory + RL persistent** across sessions and projects
7. **OSS-first with premium tiers** — hybrid (Sourcegraph + Continue model)

## 4. Sales motion + distribution

### 4.1 Three-tier motion

| Motion | Target | Cycle | Team | Channel |
|---|---|---|---|---|
| **PLG self-service** | Free → Premium → Business | 30-90 days | None | GitHub, content, community |
| **SLG inside sales** | Business 30-200 devs | 60-90 days | 1 SDR/AE | PQL inbound, demos, ROI |
| **SLG enterprise** | 200+ devs | 6-12 months | AE + SE + CS | RFP, ABM, executive briefings |

### 4.2 Acquisition channels (ranked by LTV/CAC)

| Channel | Cost | LTV/CAC est. | Volume |
|---|---|---|---|
| GitHub organic (stars + README) | $0 | ∞ | Limited |
| Hacker News / Lobste.rs | $0 | 50× | Bursty |
| Reddit (r/rust, r/programming) | $0 | 40× | Risk anti-promo |
| Podcast appearances | $0-500/ep | 30× | High-quality slow |
| Twitter/X dev community | $200/mo content | 25× | Steady |
| YouTube tutorials | $1k/video | 15× | Slow compound |
| Dev.to / Medium | $500/mo writer | 12× | SEO compound |
| Sourcegraph integration | strategic | high | Cross-pollination |
| Conference sponsorships | $5-25k/event | 5× | Niche (RustConf, RustNation) |
| Paid Google ads | $3-8/click | 3-5× | Volume, lower quality |

### 4.3 Distribution channels

```
curl install.touring.dev | sh        70%  PLG primary
brew install touring                 15%  macOS power users
docker pull touring/touring           3%  CI/CD
apt install touring                   5%  Debian/Ubuntu PPA
rpm install touring                   2%  RHEL/Fedora
scoop install touring                 3%  Windows
nix flake                             1%  NixOS devs
enterprise on-prem installer        custom  Enterprise SLG
```

### 4.4 Partner ecosystem

| Partner type | Examples | Touring offering |
|---|---|---|
| IDE integrations | VSCode, Cursor, Cline, Aider, JetBrains | Free MCP integration |
| LLM providers | Anthropic, OpenAI, Voyage, Cohere | BYOK; no markup |
| Cloud marketplaces | AWS, Azure, GCP | Listed, 5% rev share |
| Consultancies | Dev shops, agencies | 15% referral first year |
| Training partners | Educative, Frontend Masters | Co-marketing, custom curricula |
| OSS projects | Tokio, Rust Foundation, Bevy, Linkerd | Free Enterprise tier + brand |
| Security firms | Trail of Bits, Sourcegraph | Co-marketing on offensive |

### 4.5 Telemetry tier matrix

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

## 5. Success metrics + financial forecast

### KPIs by horizon

| KPI | T0 | M3 | M6 | M12 | M24 |
|---|---|---|---|---|---|
| GitHub stars | 1k | 5k | 15k | 35k | 80k |
| DAU active installs | 100 | 1k | 5k | 20k | 60k |
| Free→Premium conversion | — | 0.5% | 1% | 1.5% | 2% |
| Premium MRR | $0 | $5k | $25k | $120k | $400k |
| Enterprise ARR | $0 | $0 | $300k | $1.5M | $5M |
| **Total ARR** | $0 | $60k | $600k | **$2.9M** | **$9.8M** |
| Premium subs | 0 | 150 | 700 | 3,000 | 9,000 |
| Enterprise accounts | 0 | 0 | 1 | 5 | 17 |
| Monthly churn | — | 8% | 5% | 3% | 2% |
| NPS | — | 30 | 45 | 55 | 60+ |
| Test coverage workspace | 20% | 25% | 30% | 40% | 50% |
| External contributors | 1 | 10 | 50 | 200 | 500 |
| Conference talks | 0 | 2 | 5 | 12 | 25 |

### Financial forecast (5 years)

| Year | Revenue | Costs | Net | Headcount | Notes |
|---|---|---|---|---|---|
| Y1 (2027) | $1.2M | $1.8M | -$600k | 6 | Bootstrap; founders + 4 |
| Y2 (2028) | $5.8M | $5.0M | +$800k | 18 | Profitable; Series A optional |
| Y3 (2029) | $15M | $12M | +$3M | 40 | Series A confirmed |
| Y4 (2030) | $35M | $26M | +$9M | 80 | Series B optional |
| Y5 (2031) | $70M | $50M | +$20M | 130 | IPO-ready or M&A |

### Unit economics

| Metric | Value |
|---|---|
| Cost per premium user/month | ~$11 (infra $0.30 + license $0.05 + support $1.50 + sales $4 + marketing CAC $5) |
| Revenue per premium | $29/mo |
| **Gross margin** | **~62%** |
| LTV premium individual | $740 (3yr × 85% retention) |
| LTV premium team | $5,300/seat (4yr × 92%) |
| LTV enterprise | $1.38M/account (5yr × 95%) |
| **LTV/CAC premium** | **9.3×** (bench 3×) |
| **LTV/CAC enterprise** | **34.5×** (bench 3-5×) |

### SLA matrix

| Severity | Free | Standard | Premium | Enterprise |
|---|---|---|---|---|
| Critical (prod down) | community | community | 24h | **4h** |
| High (regression) | community | community | 48h | 8h |
| Medium (bug) | community | community | 5d | 1d |
| Low (enhancement) | community | community | 2 weeks | 1 week |
| Office hours | — | — | — | Weekly 1h |
| Dedicated CS | — | — | — | ✓ |

## 6. OKRs Year 1

**O1**: Establish Touring as the premium code intelligence platform for serious Rust + polyglot developers.
- KR1: 15k GitHub stars by M12
- KR2: 700 premium subs by M12 ($25k MRR)
- KR3: 5 enterprise pilots by M9
- KR4: NPS ≥ 45 by M9

**O2**: Build a world-class engineering foundation that itself demonstrates Touring quality.
- KR1: Test coverage ≥ 30% workspace-wide by M12
- KR2: Zero cycles workspace by M6
- KR3: docs.rs 100% green
- KR4: cargo-deny / audit / vet clean continuously

**O3**: Create defensible moats via community + ecosystem.
- KR1: 50 external contributors by M9
- KR2: 10 partner integrations live
- KR3: 5 published conference talks
- KR4: 20 customer case studies

## 7. Strategic risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Copilot adds code intelligence | High | Medium | Polyglot + agentic; Copilot vendor-locked |
| Cursor pivots to platform | Medium | High | IDE-agnostic; we sell the infra they'd use |
| Sourcegraph adds AI agent | High | High | Deeper rust+offensive; Cody is search-first |
| LLM commoditization | High | Low | We don't compete with autocomplete; we sell infra |
| Hostile OSS fork | Medium | Medium | License chooser: foundation MIT, premium Commercial |
| Talent hiring | High | High | Remote-first, OSS pipeline, equity-heavy comp |
| Compliance failure | Low | Catastrophic | SOC2 Year 1 priority; legal counsel from start |
| BR-hostile capital market | Medium | Medium | Delaware-flip if raising US VC |

## 8. References

- ADR-001: Architecture topology
- ADR-002: Per-project deployment
- MASTER-PLAN-2026: W14 detailed subtasks
- Memory: `decision:touring-premium-roadmap-2026-05-11`
