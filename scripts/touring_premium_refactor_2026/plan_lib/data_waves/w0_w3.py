"""data_waves.w0_w3 — W0-W3: prep, dead code, tooling, foundation stabilization.

Extracted from data_waves.py lines 27-494. Each ``_register_*`` helper
appends Wave instances to the shared ``WAVES`` list (in ``data_waves_pkg``).
"""
from __future__ import annotations

from . import WAVES
from ..dataclasses import Subtask, Wave

def _register_w0_w3() -> None:
    """W0-W3: Prep, dead code, tooling, foundation stabilization."""

    # ─── W0 — Prep & Safety Net ──────────────────────────────────────────────
    WAVES.append(Wave(
        id="W0",
        name="Prep & Safety Net",
        phase="F1-PREP",
        depends_on=[],
        cila="L2",
        rust_changes="ZERO",
        days_min=5,
        days_max=7,
        description=(
            "Capturar snapshots completos, baselines de bench/test/coverage, e "
            "produzir 4 ADRs + Master Plan como constituição do refactor. "
            "Zero alterações de código de produção; somente leituras, medições, "
            "e documentação. Define a linha de base contra a qual TODAS as waves "
            "posteriores serão comparadas."
        ),
        contribution=(
            "Sem baselines, qualquer regressão é invisível. Sem ADRs, decisões "
            "arquiteturais ficam vulneráveis a drift. Esta wave imuniza o "
            "refactor contra retrocesso silencioso."
        ),
        effects=[
            "Snapshot tar pre-refactor (97 MB) + SHA-256",
            "Bench baseline para regression budget de ±5%",
            "Coverage baseline para gate de ≥20% per crate",
            "Wiring/cycles snapshot (2 cycles, depth max 618)",
            "ADR-001 (Architecture), ADR-002 (Deployment), ADR-003 (Commercial)",
            "MASTER-PLAN-2026 com 15 waves + critical path + DAG",
            "Touring memory lessons persistidas (tier=semantic)",
        ],
        subtasks=[
            Subtask(
                id="W0.1", name="Snapshot tar pre-refactor + SHA-256",
                description=("tar -czf crates+Cargo.{toml,lock} → docs/baselines/. "
                             "Excluir target/, .touring-cache/, .git/, __pycache__/. "
                             "SHA-256 hex armazenado em .sha256 sibling file."),
                days=0.5,
                discover=["touring memory recall 'snapshot pre-refactor'"],
                validation=("touring-snapshot-pre-refactor-<DATE>.tar.gz existe + "
                            ".sha256 sidecar; tamanho 80-150 MB esperado."),
            ),
            Subtask(
                id="W0.2", name="Bench baseline",
                description=("cargo bench --workspace --save-baseline pre-refactor-<DATE>. "
                             "Output: target/criterion/* + docs/baselines/bench-pre-refactor-<DATE>.log."),
                days=1.0,
                discover=["touring tantivy search 'criterion benchmark'",
                          "ls crates/*/benches/ → existing bench targets"],
                validation="docs/baselines/bench-pre-refactor-<DATE>.log existe; exit code 0.",
            ),
            Subtask(
                id="W0.3", name="CI baseline (cargo check + test --no-run)",
                description=("cargo check --workspace --all-targets → log. "
                             "cargo test --workspace --no-run --all-targets → log. "
                             "Captura tempos e warnings."),
                days=0.5,
                validation="2 logs em docs/baselines/, ambos exit 0.",
            ),
            Subtask(
                id="W0.4", name="Coverage baseline (cargo llvm-cov)",
                description=("cargo llvm-cov --workspace --json --output-path "
                             "docs/baselines/coverage-pre-refactor-<DATE>.json. "
                             "Aceita falhas em crates anêmicos."),
                days=1.0,
                discover=["which cargo-llvm-cov || cargo install cargo-llvm-cov"],
                validation="coverage-pre-refactor-<DATE>.json existe + parseável.",
            ),
            Subtask(
                id="W0.5", name="Wiring/cycle snapshot",
                description=("touring wiring audit -j > wiring-pre.json (~29 MB). "
                             "touring wiring cycles --min-depth 2 --format json > cycles-pre.json. "
                             "touring status -j > status-pre.json. "
                             "touring ast workspace-info > workspace-info-pre.json."),
                days=0.5,
                validation="4 JSON files em docs/baselines/; cycle_count documentado.",
            ),
            Subtask(
                id="W0.6", name="ADR-001 Premium Architecture Vision",
                description=("Documento canônico da topologia alvo: 13 crates "
                             "produtivos em 6 layers (Foundation → Kernel → Domain "
                             "Core → Intelligence → Application → Product). "
                             "Inclui mapa de absorções (46 → 13)."),
                days=1.0,
                validation="docs/plans/.../01-ARCHITECTURE.md existe; ≥ 600 LOC; "
                           "menciona todos os 13 crates target + 46 atuais.",
            ),
            Subtask(
                id="W0.7", name="ADR-002 Per-Project Deployment Model",
                description=(".touring/touring.toml schema + ~/.touring/toolchains/ "
                             "layout + daemon discovery walk-up + CLI surface + "
                             "external installer (install.touring.dev) + migration "
                             "tool (touring migrate --from-global)."),
                days=1.0,
                validation="docs/plans/.../02-DEPLOYMENT.md existe; ≥ 400 LOC.",
            ),
            Subtask(
                id="W0.8", name="ADR-003 Commercial Tiers + GTM Strategy",
                description=("4 tiers (free/standard/premium/enterprise) + Cargo "
                             "features mapping + JWT ed25519 license + pricing "
                             "matrix + competitive landscape + sales motion + "
                             "5-year financial forecast + OKRs Y1."),
                days=0.5,
                validation="docs/plans/.../03-COMMERCIAL.md existe; ≥ 500 LOC.",
            ),
            Subtask(
                id="W0.9", name="MASTER-PLAN-2026 + 15 wave files + cross-cutting docs",
                description=("Rodar generate_plan.py --all. Emite 26 markdown "
                             "(00-INDEX + 9 cross-cutting + 15 waves + CROSS-AUDIT) "
                             "+ 15 validate_WX.py + cross_audit_e2e.py."),
                days=1.0,
                discover=["python3 generate_plan.py --check (verify scaffold exists)"],
                validation="26 .md em docs/plans/touring-premium-refactor-2026/; "
                           "16 .py em scripts/touring_premium_refactor_2026/.",
            ),
        ],
        gate=("ADRs aprovados por Gabriel; baselines committed; cycle_count "
              "registrado (esperado: 2, depth max 618); cargo check exit 0."),
        risks=[
            "Snapshot tar muito grande (>200 MB) → revisar exclusions",
            "cargo llvm-cov ausente → fallback para cargo tarpaulin ou skip W0.4",
        ],
    ))

    # ─── W1 — Dead Code Purge ────────────────────────────────────────────────
    WAVES.append(Wave(
        id="W1",
        name="Dead Code Purge",
        phase="F1-PREP",
        depends_on=["W0"],
        cila="L2",
        rust_changes="DELETION",
        days_min=3,
        days_max=4,
        description=(
            "Eliminar 4 crates mortos/órfãos identificados na auditoria: "
            "touring-semantic-spike (66L archived) e touring-wasm-{client,common,server} "
            "(0 LOC cada). Fix Cycle #1 (file_tools↔project_tools intra-server). "
            "Atualizar workspace members. Zero impacto em consumidores reais."
        ),
        contribution=(
            "Sinaliza compromisso com hygiene desde o início. Reduz baseline "
            "para 42 crates antes de fusões maiores. Elimina 1 dos 2 ciclos."
        ),
        effects=[
            "−4 crates do workspace (semantic-spike + 3 wasm 0-LOC)",
            "−1 ciclo de dependência (depth 2, intra-server)",
            "Workspace Cargo.toml members atualizado",
            "Pub use de crates removidos limpos em toda tree",
        ],
        subtasks=[
            Subtask(
                id="W1.1", name="DELETE touring-semantic-spike",
                description=("Remover crates/touring-semantic-spike/ inteiro. "
                             "66 LOC archived per ARCHITECTURE.md; 0 pub symbols. "
                             "Remover entrada de [workspace] members em Cargo.toml."),
                days=0.5,
                discover=["touring index find 'touring_semantic_spike' (esperado 0 hits)",
                          "grep -rn 'touring-semantic-spike' Cargo.toml crates/*/Cargo.toml"],
                validation="cargo check --workspace exit 0; nenhuma referência restante.",
            ),
            Subtask(
                id="W1.2", name="DELETE touring-wasm-{client,common,server}",
                description=("Remover 3 crates de 0 LOC: touring-wasm-client, "
                             "touring-wasm-common, touring-wasm-server. "
                             "Atualizar [workspace] members + remover dev-deps órfãos."),
                days=0.5,
                discover=["wc -l crates/touring-wasm-{client,common,server}/src/*.rs",
                          "grep -rn 'touring-wasm-client\\|touring-wasm-common\\|touring-wasm-server' Cargo.toml crates/*/Cargo.toml"],
                validation="cargo check --workspace exit 0; touring wiring orphans -j no new orphans.",
            ),
            Subtask(
                id="W1.3", name="Audit + clean dead reexports",
                description=("grep por pub use referenciando crates removidos. "
                             "Limpar em touring-server/src/lib.rs, touring-hooks "
                             "façade, etc. Atualizar tests que importavam."),
                days=1.0,
                discover=["touring tantivy search 'pub use touring_semantic_spike'",
                          "touring tantivy search 'pub use touring_wasm_client'"],
                validation="cargo check --workspace --all-targets exit 0; "
                           "0 unused warnings novos.",
            ),
            Subtask(
                id="W1.4", name="Fix Cycle #1 (file_tools ↔ project_tools intra-server)",
                description=("Cycle de depth 2 detectado em "
                             "crates/touring-server/src/tools/file_tools.rs → "
                             "project_tools.rs. Refatorar: extrair tipos comuns "
                             "para tools/shared.rs OU inverter direção via trait."),
                days=1.0,
                discover=["touring ast blast crates/touring-server/src/tools/file_tools.rs",
                          "touring ast blast crates/touring-server/src/tools/project_tools.rs",
                          "touring wiring impact file_tools::* --depth 2"],
                tdd_red=("def test_no_cycle_file_project_tools():\n"
                         "    \"\"\"RED: cycle detector should report 0 cycles in tools/.\"\"\""),
                validation="touring wiring cycles --min-depth 2: Cycle #1 GONE; "
                           "cycle_count = 1 (only macrociclo of depth 618 remains).",
                blocking=True,
            ),
            Subtask(
                id="W1.5", name="Update workspace + validate",
                description=("Atualizar [workspace] members em Cargo.toml. Remover "
                             "4 crates. Validar com cargo check --workspace + "
                             "touring wiring orphans -j (deve estar estável)."),
                days=0.5,
                validation="cargo check --workspace exit 0; "
                           "cargo test --workspace --no-run exit 0; "
                           "orphan delta ≤ 0.",
            ),
        ],
        gate=("4 crates removidos; Cycle #1 eliminado; cargo check + test --no-run "
              "exit 0; orphans não aumentaram."),
        risks=[
            "Algum crate consumer-só-em-test poderia estar usando 0-LOC wasm crates "
            "como placeholder → revisar tests cuidadosamente",
        ],
    ))

    # ─── W2 — Tooling Foundation ─────────────────────────────────────────────
    WAVES.append(Wave(
        id="W2",
        name="Tooling Foundation",
        phase="F1-PREP",
        depends_on=["W0", "W1"],
        cila="L2",
        rust_changes="REFACTOR",
        days_min=4,
        days_max=5,
        description=(
            "Centralizar dependências externas em [workspace.dependencies], "
            "metadados em [workspace.package], lints em [workspace.lints]. "
            "Configurar cargo-deny, cargo-machete, cargo-mutants. CI gates "
            "para todos. Preparar terreno para todas as waves seguintes."
        ),
        contribution=(
            "Sem isso, cada crate vira ilha de configuração. Atualizar versão "
            "de uma dep externa exige tocar 42 Cargo.toml. Esta wave estabelece "
            "single source of truth."
        ),
        effects=[
            "[workspace.dependencies] com ~60 deps centralizadas",
            "[workspace.package] com license/edition/MSRV 1.83 partilhados",
            "[workspace.lints] strict (deny warnings + pedantic + nursery)",
            "cargo-deny config (bans, advisories, sources, licenses)",
            "cargo-machete CI gate (0 unused deps)",
            "cargo-mutants per-crate threshold (initial 50%, target 80% em W11)",
            "GitHub Actions workflow para deny+machete+mutants+msrv",
        ],
        subtasks=[
            Subtask(
                id="W2.1", name="Centralize external deps in [workspace.dependencies]",
                description=("Listar todas deps externas únicas (~60 nomes). "
                             "Adicionar a [workspace.dependencies] no Cargo.toml raiz "
                             "com versão fixa. Cada crate passará a usar .workspace = true."),
                days=1.5,
                discover=["cargo metadata --format-version 1 | jq '.packages[].dependencies[].name' | sort -u",
                          "touring memory recall 'workspace.dependencies'"],
                validation="[workspace.dependencies] tem ≥ 60 entradas; nenhuma versão duplicada.",
            ),
            Subtask(
                id="W2.2", name="Centralize [workspace.package] metadata",
                description=("license = 'MIT OR Apache-2.0', edition = '2021', "
                             "rust-version = '1.83', authors, version. "
                             "Permite per-crate herdar via license.workspace = true."),
                days=0.5,
                validation="[workspace.package] presente com 5+ campos.",
            ),
            Subtask(
                id="W2.3", name="Update 42 Cargo.toml: <dep>.workspace = true",
                description=("Para cada crate, substituir 'serde = \"1\"' por "
                             "'serde.workspace = true'. Mesmo para todas as deps "
                             "comuns. Manter overrides locais quando necessário."),
                days=1.5,
                tdd_red=("def test_no_inline_dep_versions():\n"
                         "    \"\"\"RED: nenhum crate deve ter version literal em deps comuns.\"\"\""),
                validation="grep -rn 'serde = \"' crates/*/Cargo.toml retorna 0 hits "
                           "para deps já em workspace.dependencies.",
            ),
            Subtask(
                id="W2.4", name="[workspace.lints] strict",
                description=("Deny warnings + clippy::pedantic + clippy::nursery + "
                             "rustdoc::broken_intra_doc_links. Per-crate override "
                             "apenas com justificativa documentada."),
                days=0.5,
                validation="cargo clippy --workspace -- -D warnings exit 0.",
            ),
            Subtask(
                id="W2.5", name="cargo-deny config",
                description=("deny.toml com [bans], [advisories], [sources], "
                             "[licenses] strict. Only allow MIT, Apache-2.0, BSD, "
                             "MPL. Block GPL, AGPL contagious."),
                days=0.5,
                discover=["cargo install --locked cargo-deny",
                          "touring memory recall 'cargo-deny licenses'"],
                validation="cargo deny check exit 0.",
            ),
            Subtask(
                id="W2.6", name="cargo-machete (0 unused deps)",
                description=("Auditar e remover deps declaradas mas não usadas. "
                             "Adicionar machete.toml com ignore-list para deps "
                             "feature-gated não detectadas automaticamente."),
                days=0.5,
                discover=["cargo install --locked cargo-machete"],
                validation="cargo machete exit 0 OR justified ignore-list.",
            ),
            Subtask(
                id="W2.7", name="cargo-mutants per-crate config",
                description=("[workspace.metadata.mutants] threshold inicial 50%. "
                             "Por-crate override para crates com fixture-heavy tests. "
                             "Não bloqueia em W2 — apenas baseline."),
                days=0.5,
                discover=["cargo install --locked cargo-mutants"],
                validation="cargo mutants --baseline workspace exit 0 (não enforça threshold).",
            ),
            Subtask(
                id="W2.8", name="CI workflow: deny + machete + mutants + msrv",
                description=(".github/workflows/quality.yml: 4 jobs em matriz. "
                             "cargo-msrv para verificar 1.83 não regride. "
                             "cargo-deny + machete bloqueiam PR. cargo-mutants warn-only."),
                days=1.0,
                validation="Push para branch + observe workflow green.",
            ),
        ],
        gate=("[workspace.dependencies] populated; 42 Cargo.toml usam .workspace=true; "
              "cargo-deny + machete clean; CI workflow ativo."),
        risks=[
            "Dep com features divergentes entre crates → manter inline com override",
            "cargo-deny pode bloquear deps pre-existentes com license unusual → "
            "documentar exceções em deny.toml [licenses] allowlist",
        ],
    ))

    # ─── W3 — Layer 1+2 Stabilization ────────────────────────────────────────
    WAVES.append(Wave(
        id="W3",
        name="Layer 1-2 Stabilization",
        phase="F3-STABILIZATION",
        depends_on=["W2"],
        cila="L3",
        rust_changes="REFACTOR + ABSORVE",
        days_min=8,
        days_max=10,
        description=(
            "Renomear touring-core → touring-foundation (slim). Absorver 6 crates "
            "anêmicos: touring-rule-engine, touring-definitions, touring-telemetry, "
            "touring-resource-monitor, touring-activity (+ extrair embedding/, "
            "mvkl/ → preparação para W5 storage). Identity + simd + rkyv "
            "permanecem standalone (kernel layer 2). Tests +25%/+30% LOC ratio."
        ),
        contribution=(
            "Foundation é o anchor de TODOS os crates. Se ela está kitchen-sink, "
            "todos herdam complexidade. Slim foundation = todas waves seguintes "
            "começam mais limpas."
        ),
        effects=[
            "touring-core renomeado para touring-foundation (re-export shim)",
            "5 crates anêmicos absorvidos como submódulos de foundation",
            "embedding/ extraído (vai para touring-storage em W5)",
            "Foundation atinge ≥ 25% LOC test ratio",
            "Identity atinge ≥ 30% LOC test ratio",
            "Macrociclo de 618 reduzido (crates absorvidos saem do grafo)",
        ],
        subtasks=[
            Subtask(
                id="W3.1", name="Rename touring-core → touring-foundation (+ shim)",
                description=("Cargo.toml name field updated. crates/touring-core/ → "
                             "crates/touring-foundation/. Re-export shim "
                             "'pub use touring_foundation::* as touring_core;' "
                             "em um stub crate touring-core mantido por 2 versões."),
                days=1.0,
                discover=["touring tantivy search 'touring_core::'",
                          "grep -rn 'touring_core::' crates/ | wc -l"],
                validation="cargo check --workspace exit 0; consumers ainda compilam "
                           "via shim.",
            ),
            Subtask(
                id="W3.2", name="Slim foundation: extract embedding/ (→ W5 storage)",
                description=("Mover crates/touring-foundation/src/embedding/ para "
                             "diretório temporário scripts/touring_premium_refactor_2026/staging/embedding/. "
                             "Atualizar consumers para tipo trait abstrato. "
                             "Implementação concreta vai para touring-storage em W5."),
                days=1.0,
                discover=["touring ast blast crates/touring-foundation/src/embedding/mod.rs",
                          "touring wiring impact 'foundation::embedding' --depth 2"],
                validation="cargo check exit 0; embedding/ não mais em foundation/src/.",
            ),
            Subtask(
                id="W3.3", name="Extract mvkl/ (multi-version key list) — keep in foundation",
                description="mvkl/ é primitive (não embedding-related). Mantém em foundation/.",
                days=0.5,
                validation="mvkl/ presente em foundation/src/.",
            ),
            Subtask(
                id="W3.4", name="Absorve touring-rule-engine → foundation/rules/",
                description=("443 LOC anêmicos. Mover para foundation/src/rules/. "
                             "Atualizar 1-2 consumers que importam direto. "
                             "Delete crate antigo."),
                days=0.5,
                discover=["touring wiring impact 'touring_rule_engine' --depth 2"],
                validation="cargo check exit 0; foundation::rules pública.",
            ),
            Subtask(
                id="W3.5", name="Absorve touring-definitions → foundation/types/",
                description="1.1k LOC. Mover para foundation/src/types/.",
                days=0.5,
                validation="cargo check exit 0; foundation::types pública.",
            ),
            Subtask(
                id="W3.6", name="Absorve touring-telemetry → foundation/telemetry/",
                description="990 LOC. Mover para foundation/src/telemetry/.",
                days=0.5,
                validation="cargo check exit 0; foundation::telemetry pública.",
            ),
            Subtask(
                id="W3.7", name="Absorve touring-resource-monitor → foundation/sentinel/",
                description=("2.4k LOC. Mover para foundation/src/sentinel/. "
                             "Feature 'sentinel-psi' para gating Linux-only."),
                days=1.0,
                validation="cargo check exit 0; foundation::sentinel pública.",
            ),
            Subtask(
                id="W3.8", name="Absorve touring-activity → foundation/activity/",
                description="781 LOC. Mover para foundation/src/activity/.",
                days=0.5,
                validation="cargo check exit 0; foundation::activity pública.",
            ),
            Subtask(
                id="W3.9", name="Foundation tests ≥ 25% LOC ratio",
                description=("Atual foundation ratio ~9% (1.2k / 13.6k). Após "
                             "absorções, total ~18k src. Adicionar tests até "
                             "atingir 25% (~4.5k tests). Focar em modules de "
                             "alto blast_radius."),
                days=2.0,
                tdd_red=("def test_foundation_coverage_25pct():\n"
                         "    \"\"\"RED: tests/src LOC ratio < 25%.\"\"\""),
                validation="wc -l foundation/src/**/*.rs vs tests/ ≥ 0.25.",
            ),
            Subtask(
                id="W3.10", name="Identity tests ≥ 30% ratio",
                description=("Atual identity ratio ~45% (720/1599) — já bom. "
                             "Manter ou aumentar. Garantir RFC-004 invariants "
                             "cobertos por proptest."),
                days=0.5,
                validation="identity tests ≥ 30%; proptest para EntityId determinism.",
            ),
            Subtask(
                id="W3.11", name="Cycle re-check",
                description=("touring wiring cycles --min-depth 2 → comparar com W0 "
                             "baseline. Esperado: macrociclo de 618 menor por absorção "
                             "(menos crates no grafo)."),
                days=0.5,
                validation="cycle depth max < 618 (redução documentada).",
            ),
        ],
        gate=("touring-foundation ≤ 18k LOC; 5 crates absorvidos; identity standalone OK; "
              "test ratio ≥ 25% foundation, ≥ 30% identity; cycle reduction "
              "vs W0 baseline documentada."),
        risks=[
            "Renomear touring-core afeta hooks em ~/.claude/settings.json → "
            "manter shim crate por 2 versões",
            "Absorver resource-monitor pode quebrar feature gating sentinel-psi → "
            "validar em CI Linux + macOS",
        ],
    ))


