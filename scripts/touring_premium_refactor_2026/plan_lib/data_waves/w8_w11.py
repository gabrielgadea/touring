"""data_waves.w8_w11 — W8-W11: pragmatic split + integration + retrospectives.

Extracted from data_waves.py lines 1044-1481. Each ``_register_*`` helper
appends Wave instances to the shared ``WAVES`` list (in ``data_waves_pkg``).
"""
from __future__ import annotations

from . import WAVES
from ..dataclasses import Subtask, Wave

def _register_w8_w11() -> None:
    """W8-W11: Internal splits + orchestration fusion + test debt repayment."""

    # ─── W8 — touring-hooks INTERNAL SPLIT ────────────────────────────────────
    WAVES.append(Wave(
        id="W8",
        name="touring-hooks Internal Split",
        phase="F3-STABILIZATION",
        depends_on=["W4", "W5", "W6", "W7"],
        cila="L4",
        rust_changes="SPLIT",
        days_min=15,
        days_max=20,
        description=(
            "CRITICAL — Claude Code interface. touring-hooks (152k LOC, 224 files, "
            "1483 pub) é o monolito que conversa com CC. NÃO pode ser deletado, "
            "mas DEVE ser internamente split em 6 sub-crates workspace-internal: "
            "hooks-core (handler trait, runtime, context), hooks-lifecycle (session, "
            "task, plan_mode, cortex), hooks-cli (70+ cli_handlers_* files), "
            "hooks-tools (MCP wiring), hooks-prediction (layer7), hooks-rl. "
            "Façade externa touring-hooks reexporta tudo — API pública intacta."
        ),
        contribution=(
            "Reduz fragmentação interna SEM quebrar surface externa. Permite "
            "iterar em sub-crate isoladamente (ex: hooks-prediction sem rebuildar "
            "tudo). Elimina possíveis ciclos internos. Cycle re-check espera "
            "ZERO ciclos workspace-wide."
        ),
        effects=[
            "6 sub-crates workspace-internal criados",
            "Façade touring-hooks (pub use _) mantém API externa idêntica",
            "224 files distribuídos em 6 buckets temáticos",
            "Hook hot-path < 5ms P99 (pre-edit, post-edit)",
            "Cycle re-check: ZERO ciclos workspace-wide (incluindo macrociclo)",
            "TACO 24 hook events smoke-test pass",
        ],
        subtasks=[
            Subtask(
                id="W8.1", name="Create 6 internal sub-crates",
                description=("taco-forge perfect-create-crate para cada: hooks-core, "
                             "hooks-lifecycle, hooks-cli, hooks-tools, hooks-prediction, "
                             "hooks-rl. Workspace members atualizado."),
                days=1.0,
                discover=["touring memory recall 'perfect-create-crate'"],
                validation="cargo check -p touring-hooks-core ... exit 0 (6 crates).",
            ),
            Subtask(
                id="W8.2", name="Move hooks/core/* → touring-hooks-core",
                description=("HookHandler trait, HookRuntime, HookContext, error types. "
                             "Bottom of internal stack. Zero deps em outros hooks sub-crates."),
                days=2.0,
                discover=["touring ast blast crates/touring-hooks/src/handler.rs"],
                validation="cargo check -p touring-hooks-core exit 0; "
                           "touring-hooks-core sem deps de outros touring-hooks-*.",
                blocking=True,
            ),
            Subtask(
                id="W8.3", name="Move lifecycle/* → touring-hooks-lifecycle",
                description=("session_start/stop, task_create/completed, plan_mode, "
                             "cortex, fascicles. Depende de hooks-core."),
                days=2.0,
                validation="cargo check -p touring-hooks-lifecycle exit 0.",
            ),
            Subtask(
                id="W8.4", name="Move cli_handlers/* → touring-hooks-cli (70+ files)",
                description=("Split por subdomínio: cli_handlers (core), cli_handlers_index, "
                             "cli_handlers_decompose, cli_handlers_e2e, etc. "
                             "Manter logical grouping. Maior bloco de trabalho."),
                days=4.0,
                discover=["ls crates/touring-hooks/src/cli_handlers*.rs | wc -l"],
                validation="cargo check -p touring-hooks-cli exit 0; "
                           "70+ files reorganizados em subdiretórios temáticos.",
                blocking=True,
            ),
            Subtask(
                id="W8.5", name="Move tools/* → touring-hooks-tools (MCP wiring)",
                description="Mcp tool handlers + registry + dispatchers.",
                days=2.0,
                validation="cargo check -p touring-hooks-tools exit 0; 99 MCP tools registered.",
            ),
            Subtask(
                id="W8.6", name="Move layer7_prediction → touring-hooks-prediction",
                description="Predictive focus cache + co_edit_predictor + L7-B.",
                days=1.0,
                validation="cargo check -p touring-hooks-prediction exit 0.",
            ),
            Subtask(
                id="W8.7", name="Move rl-related → touring-hooks-rl",
                description="pre_tool_rl, post_tool_rl, learning_loop, reward injection.",
                days=1.0,
                validation="cargo check -p touring-hooks-rl exit 0.",
            ),
            Subtask(
                id="W8.8", name="Façade touring-hooks reexports",
                description=("crates/touring-hooks/src/lib.rs = pub use touring_hooks_core::*; "
                             "pub use touring_hooks_lifecycle::*; etc. Mantém public API "
                             "idêntica para consumers externos."),
                days=0.5,
                validation="cargo public-api -p touring-hooks → diff vs pre-W8 baseline = 0 changes.",
            ),
            Subtask(
                id="W8.9", name="Tests reorganize per sub-crate",
                description=("32k LOC tests redistribuídos. Cada sub-crate testa sua "
                             "responsabilidade. Integration tests ficam em "
                             "touring-integration-tests."),
                days=1.5,
                validation="cargo test --workspace exit 0; cada sub-crate ratio ≥ 20%.",
            ),
            Subtask(
                id="W8.10", name="Bench hook hot-path < 5ms P99",
                description=("Criterion bench pre-edit, post-edit, pre-bash. P99 < 5ms. "
                             "Internal crate-boundary overhead deve ser zero (compiled "
                             "out via #[inline] em re-exports)."),
                days=1.0,
                tdd_red=("def test_pre_edit_p99_under_5ms():\n"
                         "    \"\"\"RED: pre-edit P99 > 5ms FAILS.\"\"\""),
                validation="hdrhistogram P99 < 5ms para pre-edit; < 8ms para post-edit.",
                blocking=True,
            ),
            Subtask(
                id="W8.11", name="Cycle re-check — ZERO cycles",
                description=("touring wiring cycles --min-depth 2 → cycle_count = 0. "
                             "Esta é a wave que ELIMINA o último ciclo significativo."),
                days=0.5,
                validation="cycle_count = 0; objective de zero cycles workspace-wide atingido.",
                blocking=True,
            ),
            Subtask(
                id="W8.12", name="Validation: 24 hook events TACO smoke test",
                description=("Rodar todos os 24 hook events através de uma session "
                             "TACO E2E simulada. Pre-read, pre-edit, post-edit, "
                             "session_start, etc. Cada hook event deve completar < 50ms."),
                days=1.5,
                validation="24 hook events: 24 PASS, 0 FAIL.",
            ),
        ],
        gate=("touring-hooks split em 6 sub-crates internos, façade externa intacta, "
              "0 cycles workspace, hook hot-path < 5ms P99, 24 hook events smoke pass."),
        risks=[
            "Façade reexport pode esconder API breakage → cargo public-api snapshot "
            "antes/depois (gate em CI)",
            "Internal cycle entre hooks-cli e hooks-lifecycle se cli depende "
            "indiretamente de lifecycle → bottom-up move ordering (W8.2 → W8.3 → W8.4)",
            "224 files = 32k tests realocados → tests CI rodam por longer time; "
            "considerar test sharding",
            "Hook handlers usam SessionBus signal-based comm — split pode quebrar "
            "se signals não forem re-exportados corretamente",
        ],
    ))

    # ─── W9 — touring-server INTERNAL SPLIT ───────────────────────────────────
    WAVES.append(Wave(
        id="W9",
        name="touring-server Internal Split",
        phase="F3-STABILIZATION",
        depends_on=["W8"],
        parallel_with=["W10"],
        cila="L3",
        rust_changes="SPLIT",
        days_min=10,
        days_max=12,
        description=(
            "touring-server (61k LOC, 161 files, 628 pub) é god-binary. Split "
            "interno em 6 sub-crates: server-cli (CLI dispatch), server-tools "
            "(tools/*), server-reasoning (reasoning/*), server-session (session, "
            "snapshot), server-telemetry (telemetry init), server-visual (visual/, "
            "flow viz). Façade touring-server mantém o binary `touring` no "
            "main.rs. API externa intacta."
        ),
        contribution=(
            "Reduz mega-binary para façade slim 25k LOC. Cada sub-crate testável "
            "isoladamente. CLI dispatch latency baixa porque imports são re-exports."
        ),
        effects=[
            "6 sub-crates server-* internos",
            "Façade touring-server slim ~25k LOC (binary + main + dispatch)",
            "82 CLI commands smoke-test exit 0",
            "CLI dispatch P99 < 10ms",
        ],
        subtasks=[
            Subtask(
                id="W9.1", name="Create 6 internal sub-crates",
                description="taco-forge perfect-create-crate × 6.",
                days=1.0,
                validation="6 crates cargo check exit 0.",
            ),
            Subtask(
                id="W9.2", name="Move cli/* → server-cli",
                description="CLI handlers + arg parsing + dispatch table.",
                days=1.5,
                validation="cargo check -p touring-server-cli exit 0.",
            ),
            Subtask(
                id="W9.3", name="Move tools/* → server-tools",
                description="Tool registry + handlers + MCP integration.",
                days=1.0,
                validation="cargo check -p touring-server-tools exit 0.",
            ),
            Subtask(
                id="W9.4", name="Move reasoning/* → server-reasoning",
                description="Reasoning engine wiring, verification, persistence.",
                days=1.0,
                validation="cargo check -p touring-server-reasoning exit 0.",
            ),
            Subtask(
                id="W9.5", name="Move session/* + snapshot/* → server-session",
                description="Session manager + .toon snapshot + diary.",
                days=1.0,
                validation="cargo check -p touring-server-session exit 0.",
            ),
            Subtask(
                id="W9.6", name="Move telemetry/* + telemetry_init.rs → server-telemetry",
                description="OTel init, fmt subscriber, console subscriber probe.",
                days=0.5,
                validation="cargo check -p touring-server-telemetry exit 0.",
            ),
            Subtask(
                id="W9.7", name="Move visual/* → server-visual + façade",
                description=("Visual emitters (flow.rs, mod.rs). Server crate fica "
                             "como façade + main binary."),
                days=0.5,
                validation="cargo build --bin touring exit 0.",
            ),
            Subtask(
                id="W9.8", name="Tests reorganize",
                description="6k LOC tests por sub-crate. Integration tests fora.",
                days=1.5,
                validation="cargo test --workspace exit 0.",
            ),
            Subtask(
                id="W9.9", name="Bench CLI dispatch < 10ms P99",
                description="touring status, touring doctor, touring ast meta benches.",
                days=1.0,
                tdd_red=("def test_cli_dispatch_p99_under_10ms():\n"
                         "    \"\"\"RED: P99 > 10ms FAILS.\"\"\""),
                validation="P99 < 10ms para 3 commands hot-path.",
            ),
            Subtask(
                id="W9.10", name="Validation: 82 CLI commands smoke test",
                description="Rodar `touring <cmd> --help` para 82 subcomandos. Exit 0 todos.",
                days=1.0,
                validation="82/82 smoke tests exit 0.",
            ),
        ],
        gate=("touring-server façade 25k LOC, 6 internal sub-crates, "
              "CLI dispatch P99 < 10ms, 82 commands smoke pass."),
        risks=[
            "Main binary still in touring-server façade; ensure cargo metadata "
            "shows it as the [[bin]] target",
            "Session sub-crate has heavy state (snapshot persist) — careful with "
            "test parallelism",
        ],
    ))

    # ─── W10 — touring-orchestration FUSION ───────────────────────────────────
    WAVES.append(Wave(
        id="W10",
        name="touring-orchestration Fusion",
        phase="F3-STABILIZATION",
        depends_on=["W9"],
        parallel_with=["W9"],
        cila="L3",
        rust_changes="FUSION",
        days_min=5,
        days_max=7,
        description=(
            "Fundir touring-flow (809L), touring-tasksfile (1.2k), touring-devrc-adapter "
            "(591L), + extrair decompose/ + session/ + diary/ de touring-server "
            "para o novo touring-orchestration (~3.5k LOC). Features flow-dag, "
            "tasks-sqlite, decompose-mcts, session-persist."
        ),
        contribution=(
            "Concentra orquestração (DAG, tasks, decompose, session) num único "
            "crate. Permite touring-server depender só dele para essas operações."
        ),
        effects=[
            "touring-orchestration ~3.5k LOC, ≥ 25% test ratio",
            "3 crates absorvidos (flow, tasksfile, devrc-adapter)",
            "Decompose + session + diary extraídos de touring-server",
            "4 features modulares",
        ],
        subtasks=[
            Subtask(
                id="W10.1", name="Create touring-orchestration skeleton",
                description="taco-forge perfect-create-crate.",
                days=0.5,
                validation="cargo check -p touring-orchestration exit 0.",
            ),
            Subtask(
                id="W10.2", name="Move touring-flow → orchestration/flow/",
                description="809 LOC. DAG primitives.",
                days=0.5,
                validation="cargo check --features flow-dag exit 0.",
            ),
            Subtask(
                id="W10.3", name="Move touring-tasksfile → orchestration/tasks/",
                description="1.2k LOC. Tasksfile DSL + SQLite persistence.",
                days=0.7,
                validation="cargo check --features tasks-sqlite exit 0.",
            ),
            Subtask(
                id="W10.4", name="Move touring-devrc-adapter → orchestration/devrc/",
                description="591 LOC + 0% tests. Devrc adapter. +200 LOC tests.",
                days=0.7,
                tdd_red=("def test_devrc_parses_real_devrcfile():\n"
                         "    \"\"\"RED: devrc parser untested.\"\"\""),
                validation="cargo test --features tasks-sqlite exit 0.",
            ),
            Subtask(
                id="W10.5", name="Extract decompose from touring-server → orchestration/decompose/",
                description=("Decompose MCTS lives in touring-server hoje. "
                             "Move para orchestration/decompose/. Touring-server "
                             "agora depende de touring-orchestration."),
                days=1.0,
                validation="cargo check --features decompose-mcts exit 0.",
            ),
            Subtask(
                id="W10.6", name="Extract session + diary → orchestration",
                description="Session manager, diary writer. Touring-server delega para orchestration.",
                days=1.0,
                validation="touring session start <id> ainda funciona via orchestration.",
            ),
            Subtask(
                id="W10.7", name="Features + tests",
                description="4 features + +500 LOC tests total. Ratio ≥ 25%.",
                days=1.5,
                validation="cargo llvm-cov -p touring-orchestration ratio ≥ 25%.",
            ),
            Subtask(
                id="W10.8", name="Update consumers + delete old",
                description="Touring-server, touring-hooks atualizados. Shims onde necessário.",
                days=0.5,
                validation="cargo check --workspace exit 0.",
            ),
        ],
        gate=("touring-orchestration 3.5k LOC, 4 features, ≥ 25% test ratio, "
              "3 crates absorvidos."),
        risks=[
            "Decompose extraction quebra touring decompose CLI → smoke test "
            "decompose create/add/status",
            "Session manager extraction quebra TACO session lifecycle → "
            "validar com touring session start <id>",
        ],
    ))

    # ─── W11 — TEST DEBT REPAYMENT ────────────────────────────────────────────
    WAVES.append(Wave(
        id="W11",
        name="Test Debt Repayment",
        phase="F4-QUALITY",
        depends_on=["W6", "W7", "W8", "W9", "W10"],
        parallel_with=["W12"],
        cila="L3",
        rust_changes="TESTS-ONLY",
        days_min=10,
        days_max=15,
        description=(
            "Repagar test debt remanescente para garantir 20%+ ratio em TODOS os crates "
            "e mutation kill rate ≥ 80% workspace-wide. Inclui proptest para tipos chave "
            "(Identity, Plan, Definition) + fuzz targets para parsers e serializers. "
            "Wave 'invisible' mas crítica para premium quality gate."
        ),
        contribution=(
            "Plano premium não tem espaço para test-debt. Esta wave fecha a brecha. "
            "Mutation kill rate ≥ 80% prova que tests não são meramente cosmeticos."
        ),
        effects=[
            "touring-intelligence (cortex herdado) 15% → 20%",
            "touring-bindings (web/python/desktop) 8% → 18%",
            "touring-foundation (sentinel/telemetry) 15% → 22%",
            "Mutation kill rate workspace ≥ 80%",
            "50 proptest properties (Identity, Plan, Definition)",
            "8 fuzz targets (parsers, serializers)",
            "NENHUM crate < 20% test ratio",
        ],
        subtasks=[
            Subtask(
                id="W11.1", name="touring-intelligence test ratio 15% → 20%",
                description=("Cortex pipeline + fusion + scoring + cross_audit precisam "
                             "de mais cobertura. Foco em paths de fusão (handler dispatch + "
                             "signal_fusion)."),
                days=3.0,
                tdd_red=("def test_signal_fusion_combines_3_layers():\n"
                         "    \"\"\"RED: signal_fusion 3-layer combine untested.\"\"\""),
                validation="cargo llvm-cov -p touring-intelligence ratio ≥ 20%.",
            ),
            Subtask(
                id="W11.2", name="touring-bindings test ratio 8% → 18%",
                description="Web + python + desktop + postgis cobrem APIs externas.",
                days=3.0,
                validation="cargo llvm-cov -p touring-bindings ratio ≥ 18%.",
            ),
            Subtask(
                id="W11.3", name="touring-foundation test ratio 15% → 22%",
                description="Sentinel (PSI) + telemetry (OTel) + plugin registry.",
                days=2.0,
                validation="cargo llvm-cov -p touring-foundation ratio ≥ 22%.",
            ),
            Subtask(
                id="W11.4", name="Mutation kill rate workspace ≥ 80%",
                description=("cargo mutants --workspace --threshold 0.80. "
                             "Identificar mutations que sobrevivem; add tests focados."),
                days=3.0,
                discover=["cargo install --locked cargo-mutants",
                          "touring memory recall 'cargo mutants kill rate'"],
                tdd_red=("def test_mutation_kill_rate_80pct():\n"
                         "    \"\"\"RED: mutants kill rate < 80%.\"\"\""),
                validation="cargo mutants exit 0; kill_rate ≥ 80%.",
            ),
            Subtask(
                id="W11.5", name="Proptest properties (Identity, Plan, Definition)",
                description=("50 properties total: EntityId determinism (~10), "
                             "Plan typestate transitions (~15), Definition resolution "
                             "(~10), wire format roundtrip (~10), wiring graph "
                             "invariants (~5)."),
                days=1.5,
                validation="cargo test proptest:: exit 0; ≥ 50 properties.",
            ),
            Subtask(
                id="W11.6", name="Fuzz targets (parsers, serializers)",
                description=("8 cargo-fuzz targets: rust syn parser, tree-sitter rust/py/ts/go, "
                             "ast-grep pattern matcher, rkyv wire deserializer, "
                             "tantivy query parser, JWT license verifier."),
                days=2.5,
                discover=["cargo install --locked cargo-fuzz"],
                validation="cargo fuzz list ≥ 8 targets; 100 iterations smoke pass.",
            ),
        ],
        gate=("NENHUM crate < 20% test ratio; mutation kill rate workspace ≥ 80%; "
              "≥ 50 proptest properties; ≥ 8 fuzz targets em CI."),
        risks=[
            "Mutation kill rate 80% pode levar muito tempo se tests são "
            "majoritariamente integration → aceitar 70% como mid-target",
            "Fuzz targets precisam corpus inicial → coletar de regression suite",
        ],
    ))


