"""risks — render_risks_md.

Extracted from renderers.py lines 1045-1210. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations

from .utilities import yaml_frontmatter, md_table, write_atomic, sha256_hex

def render_risks_md() -> str:
    """Render 05-RISKS.md — cross-wave risk register."""
    meta = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "type": "risk-register",
        "created": _TODAY,
        "veto_threshold": 0.80,
    }
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # 05-RISKS — Cross-Wave Risk Register

        > **Methodology**: Each risk tagged with probability (LOW/MEDIUM/HIGH), impact
        > (LOW/MEDIUM/HIGH/CATASTROPHIC), and mitigation strategy. Risks reviewed at
        > each wave kickoff. Score formula: risk = probability × impact.

        ## 1. Strategic risks (cross-wave)

        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R1 | Copilot adds code intelligence in 2027 | HIGH | MEDIUM | Polyglot + agentic + offensive — Copilot vendor-locked Microsoft |
        | R2 | Cursor pivots to platform | MED | HIGH | IDE-agnostic; we sell the infra they'd use |
        | R3 | Sourcegraph adds AI agent | HIGH | HIGH | Deeper rust + offensive; Cody is search-first, not agentic |
        | R4 | LLM commoditization → autocomplete-as-utility | HIGH | LOW | We don't compete with autocomplete; we sell intelligence infra |
        | R5 | Hostile OSS fork | MED | MED | License chooser: foundation MIT, premium Commercial |
        | R6 | Talent hiring difficulty | HIGH | HIGH | Remote-first, OSS contributor pipeline, equity-heavy comp |
        | R7 | Compliance failure (SOC2/HIPAA/GDPR) | LOW | CATASTROPHIC | SOC2 Year 1 priority; legal counsel from start |
        | R8 | BR-hostile capital market | MED | MED | Delaware-flip if raising US VC |
        | R9 | Anthropic/OpenAI deprecate APIs we depend on | MED | MED | BYOK abstraction layer; multiple provider support |
        | R10 | Single-developer bus factor | HIGH | CATASTROPHIC | Document everything (this plan), hire 2nd eng before Y1 end |

        ## 2. Per-wave technical risks

        ### W0 — Prep & Safety Net
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W0.1 | Snapshot tar > 200 MB | LOW | LOW | Tighter excludes (target/, .touring-cache/, .git/) |
        | R-W0.2 | cargo llvm-cov missing | MED | LOW | Fallback to cargo tarpaulin; skip if unavailable |
        | R-W0.3 | ADR drift before implementation | MED | MED | Re-generate plan from data; commit ADRs first |

        ### W1 — Dead Code Purge
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W1.1 | 0-LOC wasm crate used as test placeholder | LOW | LOW | Audit tests cuidadosamente antes de delete |
        | R-W1.2 | Cycle #1 fix introduces new cycle | LOW | MED | touring wiring cycles after every refactor step |

        ### W2 — Tooling Foundation
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W2.1 | Dep com features divergentes entre crates | MED | MED | Keep inline override; document |
        | R-W2.2 | cargo-deny blocks pre-existing dep license | MED | LOW | deny.toml [licenses] allowlist documented |

        ### W3 — Layer 1+2 Stabilization
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W3.1 | Rename touring-core breaks ~/.claude/settings.json | HIGH | MED | Shim crate por 2 versões |
        | R-W3.2 | resource-monitor sentinel-psi feature breaks macOS | MED | MED | CI Linux + macOS validation |

        ### W4 — touring-code Fusion (LARGE)
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W4.1 | 38 consumers break on import path change | HIGH | HIGH | Re-export shim `pub use touring_code::ast::*` por 2 versões |
        | R-W4.2 | Cargo feature unification quebra entre crate e consumer | MED | MED | cargo hack --feature-powerset em CI |
        | R-W4.3 | Parsing bench > 5% slower que baseline | MED | HIGH | Investigate + mitigate antes de gate; rollback se necessário |

        ### W5 — touring-storage Fusion
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W5.1 | Qdrant feature exige docker em CI | HIGH | LOW | Marcar como ignore por default; testar em local |
        | R-W5.2 | Candle BGE model download em test (network) | HIGH | LOW | Mockar EmbeddingProvider trait em testes |

        ### W6 — touring-intelligence Fusion (HIGHEST RISK)
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W6.1 | Cortex test debt repayment > 5 dias | HIGH | HIGH | 50% baseline aceitável; 80% W11 target |
        | R-W6.2 | 90k LOC build time explode dev iteration | HIGH | MED | profile.dev incremental=false + sccache (REGRA #12) |
        | R-W6.3 | Internal pub(crate) discipline quebra surface | MED | HIGH | cargo public-api snapshot antes/depois |
        | R-W6.4 | Macrociclo 618 persiste após fusão | LOW | HIGH | wiring impact pre-merge; rollback se persistir |

        ### W7 — touring-bindings Fusion
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W7.1 | pyo3 ABI breakage entre versões | MED | MED | Pin pyo3 = '0.24' em workspace.deps |
        | R-W7.2 | Tauri exige Webview2 Windows | HIGH | LOW | CI Linux apenas em W7; Windows em W14 |
        | R-W7.3 | wasm-bindgen exige rustup target add | HIGH | LOW | Documentar em CONTRIBUTING.md |

        ### W8 — touring-hooks Internal Split (CRITICAL)
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W8.1 | Façade reexport esconde API breakage | MED | HIGH | cargo public-api snapshot gate em CI |
        | R-W8.2 | Internal cycle entre hooks-cli e hooks-lifecycle | HIGH | MED | Bottom-up move order (W8.2 → W8.3 → W8.4); cargo-depgraph CI |
        | R-W8.3 | 224 files realocados → CI lento | HIGH | LOW | Test sharding + parallel runners |
        | R-W8.4 | SessionBus signal-based comm quebra com split | HIGH | HIGH | Validate via 24 hook events smoke (W8.12) |

        ### W9 — touring-server Internal Split
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W9.1 | Main binary target perde-se | MED | HIGH | cargo metadata explicit bin target |
        | R-W9.2 | Session sub-crate test parallelism quebra | MED | MED | serial_test crate para fixtures stateful |

        ### W10 — touring-orchestration Fusion
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W10.1 | Decompose extraction quebra `touring decompose` CLI | MED | HIGH | Smoke test decompose create/add/status |
        | R-W10.2 | Session manager extraction quebra TACO lifecycle | MED | HIGH | Validar com `touring session start <id>` |

        ### W11 — Test Debt Repayment
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W11.1 | Mutation kill rate 80% intangível | HIGH | MED | Aceitar 70% como mid-target; documentar |
        | R-W11.2 | Fuzz targets sem corpus inicial | MED | LOW | Coletar corpus de regression suite |

        ### W12 — Per-Project Deployment (LARGE)
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W12.1 | Hook dispatcher walk-up bug quebra CC em runtime | HIGH | CATASTROPHIC | Feature flag `--legacy-global` default ON em 0.x, OFF em 1.0 |
        | R-W12.2 | Multi-daemon esgota fds em workstation 50+ projetos | MED | LOW | Document limit + auto-shutdown opt-in |
        | R-W12.3 | Migration tool corrompe memory.db filtering | MED | HIGH | Backup automático antes de migrar |

        ### W13 — Publishing Pipeline
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W13.1 | docs.rs build falha com all-features (dep conflict) | HIGH | LOW | Limit features em `[package.metadata.docs.rs]` |
        | R-W13.2 | Sigstore Fulcio cert exige OIDC token em GHA | MED | LOW | Reusable workflow para sigstore-action |

        ### W14 — Product Tiers & Distribution
        | # | Risk | Prob | Impact | Mitigation |
        |---|---|---|---|---|
        | R-W14.1 | Stripe webhook retry on transient error | HIGH | MED | Idempotent license issuance via JTI uniqueness |
        | R-W14.2 | SSO provider downtime | LOW | HIGH | Graceful degrade to free tier + clear error |
        | R-W14.3 | Audit log volume em high-traffic enterprise | MED | MED | Log rotation + sampling rules |
        | R-W14.4 | Distro package signing keys mgmt | MED | HIGH | GitHub Actions OIDC + Vault for ephemeral keys |
        | R-W14.5 | Docker distroless lacks shell para debug | LOW | LOW | tini --once com /bin/sh embedded em side image |
        | R-W14.6 | License JWT comprometida | LOW | CATASTROPHIC | ed25519 key rotation + online revocation + 30d grace |

        ## 3. Risk score top 10 (probability × impact)

        | Rank | Risk ID | Description | Score |
        |---|---|---|---|
        | 1 | R-W12.1 | Hook dispatcher walk-up bug | HIGH × CATASTROPHIC = 12 |
        | 2 | R7 | Compliance failure (SOC2) | LOW × CATASTROPHIC = 4 |
        | 3 | R10 | Single-dev bus factor | HIGH × CATASTROPHIC = 12 |
        | 4 | R-W14.6 | License JWT comprometida | LOW × CATASTROPHIC = 4 |
        | 5 | R-W4.1 | 38 consumers break in W4 | HIGH × HIGH = 9 |
        | 6 | R-W4.3 | Parsing bench regression | MED × HIGH = 6 |
        | 7 | R-W6.1 | Cortex test debt overrun | HIGH × HIGH = 9 |
        | 8 | R-W6.3 | Internal pub(crate) breaks API | MED × HIGH = 6 |
        | 9 | R-W6.4 | Macrociclo 618 persists | LOW × HIGH = 3 |
        | 10 | R-W8.4 | SessionBus breaks with split | HIGH × HIGH = 9 |

        ## 4. Risk monitoring cadence

        - **Per-wave kickoff**: review wave-specific risks + adjust mitigation
        - **Monthly**: review strategic risks (R1-R10) + market intelligence
        - **After each PR**: cargo-deny + audit + vet (catches new supply-chain risks)
        - **Quarterly**: full risk register review with priority re-scoring

        ## 5. References

        - Per-wave risks: each `WX-*.md` has its own "Riscos Específicos" section
        - Cross-audit: `CROSS-AUDIT.md` dimension D7 covers supply-chain
        """)
    return fm + body


