"""architecture — render_architecture_md.

Extracted from renderers.py lines 275-416. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations

from .utilities import yaml_frontmatter, md_table, write_atomic, sha256_hex
from ..dataclasses import Wave

def render_architecture_md() -> str:
    """Render 01-ARCHITECTURE.md — full topology breakdown."""
    meta = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "type": "architecture",
        "created": _TODAY,
        "supersedes": "nothing (greenfield architectural redesign)",
        "relates_to": ["02-DEPLOYMENT.md", "03-COMMERCIAL.md",
                       "MASTER-PLAN-2026"],
    }
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # 01-ARCHITECTURE — Touring Premium Topology

        > **Status**: Proposed | **Date**: {_TODAY}
        > **Approved by**: {_AUTHOR_GABRIEL} via decision `decision:touring-premium-roadmap-2026-05-11`

        ## 1. Diagnostic context (from {_TODAY} forensic audit)

        | Symptom | Evidence |
        |---|---|
        | **Macrociclo arquitetural HIGH severity** | `touring wiring cycles` depth=618, 9 crates |
        | **Fragmentação excessiva** | 46 crates, 6 anêmicos (<1k LOC), 3 mortos (0 LOC), 1 archived |
        | **Mega-crates concentram 69% código** | hooks 152k, server 61k, learning 41k, cortex 32k, ast 23k |
        | **Test-debt catastrófico** | cortex 0.56%, 8 crates com 0 tests |
        | **No semver/MSRV foundation** | 0 `[workspace.dependencies]`, 0 `version.workspace` |
        | **Duplicação intencional documentada** | touring-ast-polyglot DOC: "Extends touring-ast" |

        ## 2. Decision: 13 crates produtivos + 2 test-only

        ### Topologia em 6 layers

        ```
        ┌─────────────────────────────────────────────────────────────────────┐
        │ LAYER 6 — PRODUCT  (touring-server, touring-hooks, touring-bindings)│
        │   Binaries + CC interface + external API surface                    │
        ├─────────────────────────────────────────────────────────────────────┤
        │ LAYER 5 — APPLICATION  (generator, assists, orchestration)          │
        ├─────────────────────────────────────────────────────────────────────┤
        │ LAYER 4 — INTELLIGENCE  (touring-intelligence)                      │
        │   Reasoning + RL + pipeline (mega-fusion — eliminates cycle 618)    │
        ├─────────────────────────────────────────────────────────────────────┤
        │ LAYER 3 — DOMAIN CORE  (code, storage, analysis, offensive)         │
        ├─────────────────────────────────────────────────────────────────────┤
        │ LAYER 2 — KERNEL  (simd, rkyv, identity)                            │
        ├─────────────────────────────────────────────────────────────────────┤
        │ LAYER 1 — FOUNDATION  (touring-foundation)                          │
        │   Zero deps in touring-*; configures everything                     │
        └─────────────────────────────────────────────────────────────────────┘
        ```

        ## 3. Crate catalog (13 productive)

        """)
    for crate in CRATES_TARGET:
        body += f"### Layer {crate.layer} — {crate.name}\n\n"
        body += f"- **Modules**: {', '.join(crate.modules[:8])}"
        if len(crate.modules) > 8:
            body += f" + {len(crate.modules) - 8} more"
        body += "\n"
        body += f"- **Public API**: {', '.join(crate.public_api)}\n"
        body += f"- **Features**: {', '.join(crate.features) if crate.features else '(none)'}\n"
        body += f"- **Internal deps**: {', '.join(crate.internal_deps) if crate.internal_deps else 'NONE (foundation)'}\n"
        body += f"- **LOC target**: {crate.loc_src_target:,} src / {crate.loc_test_target:,} test "
        body += f"({100 * crate.loc_test_target // max(crate.loc_src_target, 1)}% ratio)\n"
        body += f"- **Pub target**: {crate.pub_target}\n"
        body += f"- **MSRV**: {crate.msrv}\n"
        if crate.absorves:
            body += f"- **Absorves**: {', '.join(crate.absorves)}\n"
        if crate.notes:
            body += f"- **Notes**: {crate.notes}\n"
        body += "\n"
    body += textwrap.dedent("""\

        ## 4. Crates eliminated/merged

        | Source crate | Disposition | Target |
        |---|---|---|
        | touring-semantic-spike (66L archived) | DELETE | — |
        | touring-wasm-client (0L) | DELETE | — |
        | touring-wasm-common (0L) | DELETE | — |
        | touring-wasm-server (0L) | DELETE | — |
        | touring-core (rename+slim) | RENAME | touring-foundation |
        | touring-rule-engine (443L) | ABSORVE | touring-foundation/rules/ |
        | touring-definitions (1.1k) | ABSORVE | touring-foundation/types/ |
        | touring-telemetry (990L) | ABSORVE | touring-foundation/telemetry/ |
        | touring-resource-monitor (2.4k) | ABSORVE | touring-foundation/sentinel/ |
        | touring-activity (781L) | ABSORVE | touring-foundation/activity/ |
        | touring-ast (23k) | FUSE | touring-code/parsers/tree_sitter/ |
        | touring-ast-polyglot (769L) | FUSE | touring-code/parsers/ast_grep/ |
        | touring-language (558L) | FUSE | touring-code/languages/ |
        | touring-semantics (1072L) | FUSE | touring-code/semantics/ |
        | touring-index (2.7k) | FUSE | touring-storage/fts/ |
        | touring-vfs (1.6k) | FUSE | touring-storage/vfs/ |
        | touring-incremental-salsa (387L) | FUSE | touring-storage/salsa/ |
        | touring-vector-store (1.2k) | FUSE | touring-storage/vec/ |
        | touring-embeddings (1.4k) | FUSE | touring-storage/embeddings/ |
        | touring-search-fusion (1.5k) | FUSE | touring-storage/hybrid_search/ |
        | touring-cognitive (15k) | FUSE | touring-intelligence/reasoning/ |
        | touring-cortex (32k) | FUSE | touring-intelligence/pipeline/ |
        | touring-learning (41k) | FUSE | touring-intelligence/rl/ |
        | touring-antt (5.2k) | FUSE | touring-intelligence/ann/ |
        | touring-flow (809L) | FUSE | touring-orchestration/flow/ |
        | touring-tasksfile (1.2k) | FUSE | touring-orchestration/tasks/ |
        | touring-devrc-adapter (591L) | FUSE | touring-orchestration/devrc/ |
        | touring-python (3.5k) | FUSE | touring-bindings/bindings-python/ |
        | touring-wasm (2.7k) | FUSE | touring-bindings/bindings-wasm/ |
        | touring-capnp-server (1.5k) | FUSE | touring-bindings/bindings-capnp/ |
        | touring-web (3.5k) | FUSE | touring-bindings/bindings-web/ |
        | touring-web-server (1.7k) | FUSE | touring-bindings/bindings-web/ (merged) |
        | touring-desktop-ui (1.2k) | FUSE | touring-bindings/bindings-desktop/ |
        | touring-geopostgis (435L) | FUSE | touring-bindings/bindings-postgis/ |

        **Net**: 46 → 13 productive + 2 test-only = **15 manifests (-67%)**.

        ## 5. Quality gates (non-negotiable per crate)

        | Gate | Threshold | Verification |
        |---|---|---|
        | **Test ratio** | tests/src LOC ≥ 20% | cargo llvm-cov per crate |
        | **Mutation kill rate** | ≥ 80% | cargo mutants per crate |
        | **Documentation** | `#![warn(missing_docs)]` strict | cargo doc --warnings-as-errors |
        | **API stability** | snapshot via cargo public-api | CI gate per PR |
        | **SemVer** | cargo-semver-checks | CI gate before merge |
        | **MSRV** | 1.83 LTS | cargo-msrv verify |
        | **Lints** | `[workspace.lints]` strict | cargo clippy -- -D warnings |
        | **Supply chain** | clean | cargo deny + audit + vet |
        | **Performance** | Criterion baseline -5% budget | cargo bench regression CI |
        | **No unsafe** | without `// SAFETY:` comment | grep gate |
        | **No `unwrap()` em src/** | tests OK | clippy lint enforced |

        ## 6. References

        - Forensic audit: memory `audit:touring-arch-premium-refactor-2026-05-11`
        - Approved decisions: memory `decision:touring-premium-roadmap-2026-05-11`
        - Baselines: `docs/baselines/`
        - Sister docs: 02-DEPLOYMENT, 03-COMMERCIAL, 05-RISKS, 06-METRICS, 07-ROLLBACK
        """)
    return fm + body


