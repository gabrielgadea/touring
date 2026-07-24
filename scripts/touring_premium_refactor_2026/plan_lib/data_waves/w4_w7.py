"""data_waves.w4_w7 — W4-W7: shim elimination + schema + rl + compose.

Extracted from data_waves.py lines 495-1043. Each ``_register_*`` helper
appends Wave instances to the shared ``WAVES`` list (in ``data_waves_pkg``).
"""
from __future__ import annotations

from . import WAVES
from ..dataclasses import Subtask, Wave

def _register_w4_w7() -> None:
    """W4-W7: Domain fusions (code, storage, intelligence, bindings)."""

    # ─── W4 — touring-code FUSION ─────────────────────────────────────────────
    WAVES.append(Wave(
        id="W4",
        name="touring-code Fusion",
        phase="F2-FUSIONS",
        depends_on=["W3"],
        cila="L4",
        rust_changes="FUSION",
        days_min=12,
        days_max=15,
        description=(
            "Fundir 4 crates relacionados (touring-ast 23k, touring-ast-polyglot 769L, "
            "touring-language 558L, touring-semantics 1072L) num único touring-code "
            "(~26k LOC). Sub-modules: parsers/{tree_sitter,ast_grep,syn} + languages "
            "+ semantics + graph + format + complexity + incremental. Features "
            "lang-* (7 idiomas) + parser-* (3 engines). Re-export shims preservam "
            "consumidores por 2 versões."
        ),
        contribution=(
            "Elimina duplicação intencional documentada (ast-polyglot extends ast). "
            "Reduz 4 crate-boundaries para 1, simplificando o grafo. Features "
            "modulares permitem usuário escolher engine de parsing (tree-sitter "
            "p/ polyglot, syn p/ Rust deep, ast-grep p/ structural rewrite)."
        ),
        effects=[
            "touring-code crate criado (~26k LOC src, ~6k tests, ratio ≥ 23%)",
            "4 crates deletados (ast, ast-polyglot, language, semantics)",
            "38 consumidores atualizados: touring_ast::X → touring_code::ast::X",
            "Features lang-rust (default), lang-typescript, lang-python, lang-go, lang-ruby, lang-java, lang-cpp",
            "Features parser-tree-sitter (default), parser-ast-grep, parser-syn",
            "Bench parsing: regressão < 5% (gate)",
            "Re-export shim 'pub use touring_code::ast::* as touring_ast' por 2 versões",
        ],
        subtasks=[
            Subtask(
                id="W4.1", name="Create touring-code skeleton + Cargo.toml",
                description=("cargo new --lib crates/touring-code via taco-forge perfect-create-crate. "
                             "Cargo.toml com features lang-* + parser-*. Adicionar a workspace members."),
                days=0.5,
                discover=["taco-forge perfect-create-crate --name touring-code --intent '...'",
                          "touring memory recall 'create-crate workflow'"],
                validation="cargo check -p touring-code exit 0; crate registrado em workspace.",
            ),
            Subtask(
                id="W4.2", name="Move touring-ast/src/* → touring-code/src/parsers/tree_sitter/ + ast deep",
                description=("Mover 23k LOC. Manter touring-ast namespace via pub mod ast. "
                             "Refatorar imports internos (use crate::ast::X)."),
                days=2.0,
                discover=["touring ast workspace-info | jq '.packages[] | select(.name==\"touring-ast\")'",
                          "wc -l crates/touring-ast/src/**/*.rs"],
                validation="cargo check -p touring-code exit 0; pub mod ast exposto.",
                blocking=True,
            ),
            Subtask(
                id="W4.3", name="Move touring-ast-polyglot/* → touring-code/src/parsers/ast_grep/",
                description=("769 LOC. Feature 'parser-ast-grep' opt-in. Wire em "
                             "touring_code::polyglot module."),
                days=1.0,
                validation="cargo check -p touring-code --features parser-ast-grep exit 0.",
            ),
            Subtask(
                id="W4.4", name="Move touring-language/* → touring-code/src/languages/",
                description="558 LOC. Tier matrix + capability tables. Sem feature gate.",
                days=0.5,
                validation="touring_code::languages::Lang exposto.",
            ),
            Subtask(
                id="W4.5", name="Move touring-semantics/* → touring-code/src/semantics/",
                description=("1072 LOC. Definition enum + source_to_def + multi_lang. "
                             "Atualizar import use touring_ast::languages::Lang → "
                             "use crate::languages::Lang."),
                days=0.5,
                validation="touring_code::semantics::Definition exposto.",
            ),
            Subtask(
                id="W4.6", name="Define features lang-* + parser-*",
                description=("[features] em Cargo.toml: lang-rust (default), "
                             "lang-typescript, lang-python, lang-go, lang-ruby, "
                             "lang-java, lang-cpp; parser-tree-sitter (default), "
                             "parser-ast-grep, parser-syn; semantic-search, "
                             "incremental-salsa."),
                days=0.5,
                validation="cargo check --no-default-features -p touring-code exit 0; "
                           "cargo check --all-features -p touring-code exit 0.",
            ),
            Subtask(
                id="W4.7", name="Update 25 consumers: touring_ast::X → touring_code::ast::X",
                description=("Identificar 25 consumers via touring wiring impact. "
                             "Atualizar imports. Re-export shim 'pub use touring_code::ast::* "
                             "as touring_ast' em crate stub touring-ast por 2 versões."),
                days=3.0,
                discover=["touring wiring impact 'touring_ast' --depth 2",
                          "grep -rln 'use touring_ast' crates/*/src/"],
                tdd_red=("def test_consumers_use_touring_code():\n"
                         "    \"\"\"RED: grep 'use touring_ast' should drop to 0 (or shim-only).\"\"\""),
                validation="grep 'use touring_ast::' crates/ → apenas em touring-ast shim crate.",
                blocking=True,
            ),
            Subtask(
                id="W4.8", name="Update 8 polyglot consumers",
                description="touring_ast_polyglot::X → touring_code::polyglot::X em ~8 consumers.",
                days=1.0,
                validation="grep 'touring_ast_polyglot' crates/ → apenas em shim.",
            ),
            Subtask(
                id="W4.9", name="Update 3 language consumers",
                description="touring_language::X → touring_code::languages::X.",
                days=0.5,
                validation="grep 'touring_language' crates/ → apenas em shim.",
            ),
            Subtask(
                id="W4.10", name="Update 2 semantics consumers",
                description="touring_semantics::X → touring_code::semantics::X.",
                days=0.5,
                validation="grep 'touring_semantics' crates/ → apenas em shim.",
            ),
            Subtask(
                id="W4.11", name="Bench parsing — regression < 5%",
                description=("cargo bench --workspace --baseline pre-refactor-<DATE> "
                             "| grep 'change' | grep -v 'within noise'. "
                             "Comparar parsing benches: rust syn, ts/py tree-sitter, "
                             "polyglot ast-grep."),
                days=1.0,
                discover=["touring memory recall 'bench regression budget'"],
                tdd_red=("def test_parsing_bench_within_5pct_of_baseline():\n"
                         "    \"\"\"RED: any bench > 5% slower than baseline FAILS.\"\"\""),
                validation="Nenhum bench mais lento que -5%. Idealmente alguns "
                           "ficam mais rápidos (cache compartilhado dentro do crate).",
                blocking=True,
            ),
            Subtask(
                id="W4.12", name="Tests pass + cycle re-check",
                description=("cargo test --workspace exit 0. "
                             "touring wiring cycles --min-depth 2: cycle count "
                             "monotonicamente não-crescente."),
                days=1.0,
                validation="cargo test workspace exit 0; cycle count ≤ baseline W3.",
            ),
            Subtask(
                id="W4.13", name="Delete old crates (ast, ast-polyglot, language, semantics)",
                description=("Remover diretórios + workspace members. Manter shim "
                             "crates touring-ast/etc com pub use re-exports."),
                days=0.5,
                validation="ls crates/touring-{ast,ast-polyglot,language,semantics}/src/ → "
                           "apenas lib.rs com pub use re-exports.",
            ),
            Subtask(
                id="W4.14", name="Update workspace members",
                description="Cargo.toml [workspace] members lista touring-code + shims.",
                days=0.2,
                validation="grep '\"crates/touring-code\"' Cargo.toml; cargo check exit 0.",
            ),
        ],
        gate=("touring-code 26k LOC, 6+3 features funcionais, ≥ 23% test ratio, "
              "0 cycle regression, < 5% perf regression, 38 consumers atualizados, "
              "shim crates por 2 versões."),
        risks=[
            "Consumer pode usar pub item interno de touring-ast não exposto em "
            "touring-code::ast → identificar via cargo check antes de delete",
            "Cargo features feature unification entre touring-code e consumers → "
            "testar todas combinações via cargo hack --feature-powerset",
            "Bench regression > 5% em parsing rust syn (single-thread bottleneck) → "
            "investigar e mitigar antes de gate",
        ],
    ))

    # ─── W5 — touring-storage FUSION ──────────────────────────────────────────
    WAVES.append(Wave(
        id="W5",
        name="touring-storage Fusion",
        phase="F2-FUSIONS",
        depends_on=["W3"],
        parallel_with=["W7"],
        cila="L3",
        rust_changes="FUSION",
        days_min=10,
        days_max=12,
        description=(
            "Fundir 6 crates pequenos relacionados a storage: touring-index (2.7k), "
            "touring-vfs (1.6k), touring-incremental-salsa (387L), touring-vector-store "
            "(1.2k), touring-embeddings (1.4k), touring-search-fusion (1.5k) → "
            "touring-storage (~10k LOC). Features 100% opt-in: storage-fts, "
            "storage-vec-*, storage-emb-*, storage-vfs-*, storage-salsa. "
            "Adicionar +500 LOC tests para crates com 0% ratio."
        ),
        contribution=(
            "6 crate-boundaries → 1. Embedding/vector backends ficam como features "
            "opt-in, reduzindo binary size para tier-free em ~30%. Repaga test-debt "
            "de search-fusion (0%) e salsa (0%)."
        ),
        effects=[
            "touring-storage criado (~10k LOC, ≥ 25% test ratio)",
            "6 crates absorvidos como submódulos",
            "11 features storage-* opt-in",
            "+500 LOC tests para search-fusion e salsa",
            "Consumers atualizados (~15 crates)",
        ],
        subtasks=[
            Subtask(
                id="W5.1", name="Create touring-storage skeleton",
                description="taco-forge perfect-create-crate. Cargo.toml com features storage-*.",
                days=0.5,
                validation="cargo check -p touring-storage exit 0.",
            ),
            Subtask(
                id="W5.2", name="Move touring-index → storage/src/fts/",
                description="2.7k LOC. Tantivy wrapper. Feature 'storage-fts' (default).",
                days=0.7,
                validation="cargo check -p touring-storage --features storage-fts exit 0.",
            ),
            Subtask(
                id="W5.3", name="Move touring-vfs → storage/src/vfs/",
                description="1.6k LOC. Submodules mem + disk. Features storage-vfs-mem, storage-vfs-disk (default).",
                days=0.7,
                validation="cargo test -p touring-storage vfs exit 0.",
            ),
            Subtask(
                id="W5.4", name="Move touring-incremental-salsa → storage/src/salsa/",
                description="387 LOC + 0% tests. Feature 'storage-salsa'. Adicionar tests +200 LOC.",
                days=1.0,
                tdd_red=("def test_salsa_incremental_invalidation():\n"
                         "    \"\"\"RED: tests for Durability tiers + Revision invalidation missing.\"\"\""),
                validation="cargo test -p touring-storage salsa: ≥ 5 tests pass.",
            ),
            Subtask(
                id="W5.5", name="Move touring-vector-store → storage/src/vec/",
                description=("1.2k LOC. Submodules sqlite, qdrant, in_memory. "
                             "Features storage-vec-sqlite (default), storage-vec-qdrant, storage-vec-mem."),
                days=0.7,
                validation="cargo check --features storage-vec-qdrant exit 0.",
            ),
            Subtask(
                id="W5.6", name="Move touring-embeddings → storage/src/embeddings/",
                description=("1.4k LOC. Providers: candle, fastembed, voyage. "
                             "Features storage-emb-candle (default), storage-emb-fastembed, storage-emb-voyage."),
                days=0.7,
                validation="cargo check --features storage-emb-voyage exit 0.",
            ),
            Subtask(
                id="W5.7", name="Move touring-search-fusion → storage/src/hybrid_search/",
                description=("1.5k LOC + 0% tests. Hybrid BM25 + vec + reranker. "
                             "Adicionar tests +300 LOC."),
                days=1.5,
                tdd_red=("def test_hybrid_search_rrf_fusion():\n"
                         "    \"\"\"RED: hybrid_search reciprocal_rank_fusion untested.\"\"\""),
                validation="cargo test -p touring-storage hybrid_search: ≥ 8 tests pass.",
            ),
            Subtask(
                id="W5.8", name="Define features storage-* + update 15 consumers",
                description=("Atualizar 15 consumers (touring-server, hooks, generator, etc.) "
                             "para importar de touring_storage. Shim crates."),
                days=3.0,
                discover=["touring wiring impact 'touring_index' --depth 2",
                          "touring wiring impact 'touring_vfs' --depth 2"],
                validation="cargo check --workspace exit 0; shims em 6 crates antigos.",
            ),
            Subtask(
                id="W5.9", name="Bench query latency — regression < 5%",
                description="cargo bench --workspace baseline-comparison. FTS query, vec search, hybrid.",
                days=1.0,
                validation="Bench delta vs baseline ≥ -5%.",
            ),
            Subtask(
                id="W5.10", name="Delete old crates + update workspace",
                description="Remove 6 crates + shims onde possível.",
                days=1.0,
                validation="ls crates/touring-{index,vfs,...}/ → shims only.",
            ),
        ],
        gate=("touring-storage 10k LOC, 11 features, ≥ 25% test ratio "
              "(0% crates repagos), < 5% perf regression, 15 consumers updated."),
        risks=[
            "Qdrant feature exige docker em CI → marcar como ignore por default",
            "Candle BGE download de modelo em test → mockar embedding provider",
        ],
    ))

    # ─── W6 — touring-intelligence FUSION ─────────────────────────────────────
    WAVES.append(Wave(
        id="W6",
        name="touring-intelligence Fusion",
        phase="F2-FUSIONS",
        depends_on=["W3", "W4"],
        cila="L4",
        rust_changes="MEGA-FUSION",
        days_min=15,
        days_max=20,
        description=(
            "MAIOR risco do plano. Fundir 4 crates (touring-cognitive 15k, "
            "touring-cortex 32k, touring-learning 41k, touring-antt 5.2k) num "
            "touring-intelligence de ~90k LOC. ELIMINA o macrociclo de depth 618. "
            "PRE-TEST gate: cortex test ratio 0.56% → 15% ANTES de fundir. "
            "Internal pub(crate) discipline; façade externa única. 11 features "
            "intel-* opt-in (reasoning, rl, pipeline, mcts, bandit, aco, ann, "
            "clustering, pensieve, got, dspy)."
        ),
        contribution=(
            "MUDANÇA ESTRUTURAL DEFINITIVA. Macrociclo de 618 entre 9 crates desaparece "
            "porque cognitive, cortex e learning passam a viver no mesmo crate "
            "(ciclos virtuais entre módulos não contam como ciclos de grafo). "
            "RL + reasoning + pipeline ficam coesos."
        ),
        effects=[
            "touring-intelligence 90k LOC, ≥ 20% test ratio",
            "Macrociclo de depth 618 ELIMINADO",
            "11 features intel-* modulares",
            "4 crates absorvidos como submódulos pub(crate)",
            "12 consumidores atualizados",
            "Bench MCTS/ANN/bandit < 5% regression",
        ],
        subtasks=[
            Subtask(
                id="W6.0", name="🛑 PRE-TEST: cortex test ratio 0.56% → 15%",
                description=("BLOCKER absoluto para todas subtarefas seguintes. "
                             "touring-cortex tem 31.8k src / 178 tests = 0.56%. "
                             "Antes de fundir, repagar para ≥ 15% (4.7k tests). "
                             "Focar em modules cache_strategy, circuit_breaker, "
                             "cross_audit, pipeline, scoring, signal_fusion."),
                days=5.0,
                discover=["wc -l crates/touring-cortex/src/**/*.rs crates/touring-cortex/tests/",
                          "cargo llvm-cov -p touring-cortex --json | jq '.totals'"],
                tdd_red=("def test_cortex_coverage_15pct():\n"
                         "    \"\"\"RED: tests/src LOC ratio < 15% in touring-cortex.\"\"\""),
                validation="cortex tests LOC ≥ 4.7k; mutation kill rate ≥ 50%.",
                blocking=True,
            ),
            Subtask(
                id="W6.1", name="Create touring-intelligence skeleton",
                description="taco-forge perfect-create-crate. Cargo.toml com 11 features intel-*.",
                days=0.5,
                validation="cargo check -p touring-intelligence exit 0.",
            ),
            Subtask(
                id="W6.2", name="Move touring-cognitive → intelligence/src/reasoning/",
                description=("15k LOC. Submodules: aco, ann_index, bm25_tfidf, "
                             "cognitive_mcts, got, mcts, pensieve, reasoning_engine, "
                             "etc. Features intel-reasoning (default), intel-mcts, "
                             "intel-aco, intel-ann, intel-got, intel-pensieve."),
                days=2.0,
                validation="cargo check -p touring-intelligence --features intel-reasoning exit 0.",
            ),
            Subtask(
                id="W6.3", name="Move touring-learning → intelligence/src/rl/",
                description=("41k LOC. Submodules: bandit, aco, clustering, online_rl, "
                             "ranking, semantic. Features intel-rl (default), intel-bandit, "
                             "intel-clustering."),
                days=2.0,
                validation="cargo check -p touring-intelligence --features intel-rl exit 0.",
            ),
            Subtask(
                id="W6.4", name="Move touring-cortex → intelligence/src/pipeline/",
                description=("32k LOC. Sub-modules: handler, fusion, scoring, fascicles, "
                             "cross_audit, signal_fusion, dspy. Features intel-pipeline (default), "
                             "intel-dspy."),
                days=2.0,
                validation="cargo check -p touring-intelligence --features intel-pipeline exit 0.",
            ),
            Subtask(
                id="W6.5", name="Move touring-antt → intelligence/src/ann/",
                description=("5.2k LOC. ANN index + reranker. Substitui depend in cognitive."),
                days=1.0,
                validation="cargo check -p touring-intelligence --features intel-ann exit 0.",
            ),
            Subtask(
                id="W6.6", name="Define 11 features intel-* opt-in",
                description=("Features matriz: intel-reasoning, intel-rl, intel-pipeline "
                             "(default); intel-mcts, intel-bandit, intel-aco, intel-ann, "
                             "intel-clustering, intel-pensieve, intel-got, intel-dspy (opt-in)."),
                days=1.0,
                validation="cargo hack --feature-powerset check exit 0.",
            ),
            Subtask(
                id="W6.7", name="Update 12 consumers",
                description=("touring_cognitive::X → touring_intelligence::reasoning::X. "
                             "Similar para learning + cortex + antt. Shim crates."),
                days=3.0,
                discover=["touring wiring impact 'touring_cognitive' --depth 2",
                          "touring wiring impact 'touring_learning' --depth 2",
                          "touring wiring impact 'touring_cortex' --depth 2"],
                validation="cargo check --workspace exit 0.",
                blocking=True,
            ),
            Subtask(
                id="W6.8", name="Bench MCTS / ANN / bandit — regression < 5%",
                description=("Critical benches: cognitive_mcts rollout latency, "
                             "ANN query P99, bandit selection latency. "
                             "Cargo bench comparison baseline."),
                days=2.0,
                tdd_red=("def test_intel_benches_within_5pct():\n"
                         "    \"\"\"RED: MCTS/ANN/bandit > 5% slower FAILS gate.\"\"\""),
                validation="3 benches dentro de -5% vs baseline.",
                blocking=True,
            ),
            Subtask(
                id="W6.9", name="Tests pass + cycle re-check",
                description=("cargo test --workspace exit 0. CRÍTICO: "
                             "touring wiring cycles --min-depth 2 retorna 0 cycles "
                             "OR apenas o intra-server depth 2 (W1 já consertou esse). "
                             "Macrociclo de 618 DEVE estar ELIMINADO."),
                days=1.0,
                tdd_red=("def test_no_macrocycle_618():\n"
                         "    \"\"\"RED: cycle of depth > 100 found.\"\"\""),
                validation="cycle_count ≤ 0 OR max_depth < 10; macrociclo de 618 GONE.",
                blocking=True,
            ),
            Subtask(
                id="W6.10", name="Delete old crates + workspace update",
                description="Remove cognitive, learning, cortex, antt. Shims para 12 consumers.",
                days=0.5,
                validation="ls crates/touring-{cognitive,learning,cortex,antt}/ → shims only.",
            ),
        ],
        gate=("touring-intelligence 90k LOC, 11 features, ≥ 20% test ratio "
              "(cortex repago em W6.0), MACROCICLO 618 ELIMINADO, < 5% perf "
              "regression em MCTS/ANN/bandit."),
        risks=[
            "Cortex test debt repayment (W6.0) pode levar > 5 dias se mutation "
            "kill rate alvo for muito agressivo → 50% baseline aceitável; 80% W11",
            "90k LOC build time pode degradar dev iteration → profile.dev "
            "incremental=false + sccache (REGRA #12)",
            "Internal pub(crate) discipline pode quebrar se houver re-export "
            "errado → cargo public-api snapshot antes/depois",
            "Macrociclo 618 pode persistir se algum sub-module ainda referencia "
            "crate externo de forma cíclica → wiring impact pre-merge",
        ],
    ))

    # ─── W7 — touring-bindings FUSION ─────────────────────────────────────────
    WAVES.append(Wave(
        id="W7",
        name="touring-bindings Fusion",
        phase="F2-FUSIONS",
        depends_on=["W3"],
        parallel_with=["W5"],
        cila="L3",
        rust_changes="FUSION + DELETE",
        days_min=8,
        days_max=10,
        description=(
            "Fundir 7 crates de bindings + DELETAR 3 mortos (já feito em W1). "
            "Originais: touring-python (3.5k), touring-wasm (2.7k), touring-capnp-server "
            "(1.5k), touring-web (3.5k), touring-web-server (1.7k), touring-desktop-ui "
            "(1.2k), touring-geopostgis (435L). Resulta em touring-bindings ~15k LOC "
            "com features 100% opt-in (default = empty). Tests +1k LOC para crates "
            "0%-ratio (web, python, desktop, postgis)."
        ),
        contribution=(
            "Bindings ficam isolados num único crate. Usuário não paga compile-time "
            "de pyo3 + wasm-bindgen + tauri + axum a menos que ative explicitamente. "
            "Default features empty = tier-free build mais leve."
        ),
        effects=[
            "touring-bindings ~15k LOC, ≥ 23% test ratio",
            "6 features bind-* mutuamente compatíveis",
            "Default features VAZIO (opt-in)",
            "+1k LOC tests para 4 crates antes em 0%",
            "Cargo hack --feature-powerset verifica todas combinações compilam",
        ],
        subtasks=[
            Subtask(
                id="W7.1", name="Create touring-bindings skeleton + Cargo.toml",
                description=("[features] default = []. bind-python, bind-wasm, "
                             "bind-capnp, bind-web, bind-desktop, bind-postgis. "
                             "Cada feature ativa dep externa (pyo3, wasm-bindgen, etc.)."),
                days=0.5,
                validation="cargo check -p touring-bindings exit 0 (sem features).",
            ),
            Subtask(
                id="W7.2", name="Move touring-python → bindings/src/bindings-python/",
                description="3.5k LOC + 0% tests. PyO3 bindings. +400 LOC tests.",
                days=1.0,
                tdd_red=("def test_python_bindings_smoke():\n"
                         "    \"\"\"RED: import touring_bindings should work.\"\"\""),
                validation="cargo test -p touring-bindings --features bind-python exit 0.",
            ),
            Subtask(
                id="W7.3", name="Move touring-wasm → bindings/src/bindings-wasm/",
                description="2.7k LOC. wasm-bindgen + inferlets.",
                days=0.7,
                validation="cargo build -p touring-bindings --features bind-wasm --target wasm32-unknown-unknown exit 0.",
            ),
            Subtask(
                id="W7.4", name="Move touring-capnp-server → bindings/src/bindings-capnp/",
                description="1.5k LOC. Cap'n Proto RPC.",
                days=0.7,
                validation="cargo check --features bind-capnp exit 0.",
            ),
            Subtask(
                id="W7.5", name="Move touring-web + touring-web-server → bindings/src/bindings-web/",
                description=("3.5k + 1.7k LOC + 0% tests em web. Leptos + Axum. +400 LOC tests."),
                days=1.5,
                tdd_red=("def test_web_health_endpoint():\n"
                         "    \"\"\"RED: /healthz endpoint untested.\"\"\""),
                validation="cargo test --features bind-web exit 0.",
            ),
            Subtask(
                id="W7.6", name="Move touring-desktop-ui → bindings/src/bindings-desktop/",
                description="1.2k LOC + 0% tests. Tauri. +200 LOC tests (mocked window).",
                days=1.0,
                validation="cargo check --features bind-desktop exit 0.",
            ),
            Subtask(
                id="W7.7", name="Move touring-geopostgis → bindings/src/bindings-postgis/",
                description="435 LOC + 0% tests. Geozero EWKB. +200 LOC tests com postgres mock.",
                days=0.7,
                validation="cargo test --features bind-postgis exit 0.",
            ),
            Subtask(
                id="W7.8", name="Features bind-* mutuamente compatíveis",
                description=("cargo hack --feature-powerset check valida que todas "
                             "combinações compilam. Single crate, dual binding "
                             "(python + web) funciona."),
                days=1.0,
                validation="cargo hack --feature-powerset --workspace check exit 0.",
            ),
            Subtask(
                id="W7.9", name="+1k LOC tests para 4 crates 0%-ratio",
                description="Total +1k LOC distribuídos: web (+400), python (+400), desktop (+200), postgis (+200).",
                days=2.0,
                validation="cargo llvm-cov --json | jq '.totals' ≥ 23% para touring-bindings.",
            ),
            Subtask(
                id="W7.10", name="cargo check per feature combo",
                description="Validar tier-free, tier-standard, tier-premium, tier-enterprise feature sets.",
                days=1.0,
                validation="4 cargo check invocations exit 0.",
            ),
            Subtask(
                id="W7.11", name="Delete old crates + workspace update",
                description="Remove 7 crates antigos. Shims onde necessário.",
                days=0.5,
                validation="ls crates/touring-{python,wasm,capnp-server,web,web-server,desktop-ui,geopostgis}/ → shims.",
            ),
        ],
        gate=("touring-bindings 15k LOC, 6 features opt-in, default = empty, "
              "≥ 23% test ratio, cargo hack feature-powerset exit 0."),
        risks=[
            "Pyo3 ABI breakage entre versões → pin pyo3 = '0.24' em workspace.deps",
            "Tauri exige sistema-Webview2 (Windows) → CI Linux apenas em W7; "
            "Windows CI adicionado em W12",
            "wasm-bindgen target wasm32-unknown-unknown exige rustup target add → "
            "documentar em CONTRIBUTING.md",
        ],
    ))


