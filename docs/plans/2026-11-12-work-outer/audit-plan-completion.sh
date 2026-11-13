#!/usr/bin/env bash
# Artifact check for the touring-code deepening + cross-audit of 12-13/11/2026
# (DAG task_1794458342745990025, plan docs/plans/2026-11-12-touring-code-aprofundamento/PLANO.md):
# exit 0 only when every artifact the candidates C1-C8 and the fixes F1-F14 promised exists,
# says what it should, and the gate evidence is newer than the last source change.
# Read-only. Run from anywhere: paths resolve from this file.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PLAN="$ROOT/docs/plans/2026-11-12-touring-code-aprofundamento"
AUDIT="$ROOT/docs/audits/cross-audit-2026-11-12.md"
EVID="${TOURING_EVIDENCE_DIR:-$HOME/claude-artefatos/touring}"
CODE="$ROOT/crates/touring-code"
fail=0
ok()    { printf '  ok    %s\n' "$1"; }
bad()   { printf '  FALTA %s\n' "$1"; fail=1; }
have()  { [ -e "$1" ] && ok "$2" || bad "$2 ($1)"; }
gone()  { [ -e "$1" ] && bad "$2 ($1 ainda existe)" || ok "$2"; }
says()  { grep -qF -- "$2" "$1" 2>/dev/null && ok "$3" || bad "$3 ($1 sem «$2»)"; }
lacks() { if grep -qF -- "$2" "$1" 2>/dev/null; then bad "$3 ($1 ainda tem «$2»)"; else ok "$3"; fi; }
# No Rust source under crates/ may still DEFINE the symbol (docs that tell the removal are fine).
undefined() {
  local hit
  hit=$(grep -rlF --include='*.rs' -- "$1" "$ROOT/crates" 2>/dev/null | head -1)
  [ -z "$hit" ] && ok "$2" || bad "$2 ($hit ainda tem «$1»)"
}

echo "plano e notas de desenho"
have "$PLAN/PLANO.md" "PLANO.md"
have "$PLAN/RETOMADA.md" "RETOMADA.md"
for c in 1 2 3 4 5 6 7 8; do have "$PLAN/C$c-desenho.md" "nota de desenho C$c"; done
says "$PLAN/C8-desenho.md" "## 12. Como foi construído" "C8 as built"
says "$PLAN/C8-desenho.md" "## 13. Regra de aresta" "C8 regra de aresta (F13)"

echo "C1 porta de parse"
have "$CODE/src/ast/grammar.rs" "módulo ast::grammar"
says "$CODE/src/ast/grammar.rs" "the_crate_builds_and_configures_a_parser_in_exactly_one_place" "guarda de um único Parser::new()"

echo "C2 referências do arquivo"
says "$CODE/src/ast/graph/references.rs" "pub struct FileReferences" "FileReferences"
have "$CODE/tests/file_references_characterization.rs" "caracterização de FileReferences"
lacks "$CODE/src/ast/graph/method_calls.rs" "pub fn extract_method_calls" "extrator de chamadas fora da superfície pública"

echo "C3 escrita única do índice"
says "$CODE/src/ast/graph/mod.rs" "fn insert_symbol" "porta de escrita de símbolo"
have "$CODE/tests/index_write_path.rs" "guarda do caminho de escrita"

echo "C4 vista do VGP"
have "$CODE/src/vgp/lang/vista.rs" "módulo vgp::lang::vista"
have "$CODE/tests/vgp_vista_overlays.rs" "teste de overlays"
have "$CODE/tests/lang_fallback_guard.rs" "guarda de fallback de língua"

echo "C5 espelho de sinais"
says "$CODE/src/sdk_signal_mirror.rs" "pub struct Mirror" "Mirror"
have "$ROOT/crates/touring-server/tests/sdk_hook_names_parity.rs" "paridade dos nomes de hook"
have "$CODE/tests/sdk_cross_emitter.rs" "paridade Rust × gen_sdk.py"
have "$ROOT/scripts/test_gen_sdk.py" "testes do gen_sdk.py"

echo "C6/C7 superfície lazy, heat e orçamento"
gone "$CODE/benches/lazy_loading.rs" "bench lazy removido"
gone "$CODE/benches/prioritized_pipeline.rs" "bench da fila quente removido"
gone "$CODE/examples/sdk_posttool_end_to_end.rs" "exemplo órfão removido"
undefined "struct PrioritizedPipeline" "PrioritizedPipeline removido"
undefined "struct AsyncSharedPipeline" "AsyncSharedPipeline removido"
undefined "fn order_hot_first" "ordenação por heat removida"
says "$ROOT/crates/touring-hook-runtime/src/hook_runtime.rs" "TOURING_PIPELINE_MEMORY_MB" "knob do orçamento de memória"
have "$ROOT/crates/touring-hook-handlers/src/hooks/heat_producers_tests.rs" "teste dos produtores de heat"

echo "C8 arestas resolvidas no índice"
says "$CODE/src/ast/graph/mod.rs" "pub enum Importers" "Importers (Known/Isolated/Uncovered)"
says "$CODE/src/ast/graph/mod.rs" "fn load_file_edges" "carga das arestas"
says "$CODE/src/ast/graph/mod.rs" "fn coverage_expected" "regra de densidade"
have "$ROOT/crates/touring-hook-runtime/src/index_slot.rs" "IndexSlot"
says "$ROOT/crates/touring-hook-runtime/src/hook_runtime_ext.rs" "fn sync_index_edges_of" "refresh das arestas por hook"
says "$ROOT/crates/touring-cli/src/cli/health.rs" "TOURING_FILE_EDGES_DISABLED" "kill switch no status"
says "$ROOT/crates/touring-hook-handlers/src/shared/signal_pipeline.rs" "TOURING_GRAPH_BLAST_MAX_CHARS" "teto do texto de blast"
have "$CODE/tests/index_loader_guard.rs" "guarda do carregador único"
have "$CODE/tests/file_edges.rs" "testes das arestas"
have "$ROOT/crates/touring-storage/tests/file_graph.rs" "testes do grafo no storage"

echo "F13-F16 evidência da aresta e detector de órfãos"
WIRING="$ROOT/crates/touring-storage/src/knowledge_wiring.rs"
says "$WIRING" "contract_source IS NOT 'ast_inferred'" "F13 regra única exclui o palpite por nome"
says "$ROOT/crates/touring-storage/tests/file_graph.rs" "fn a_file_edge_requires_import_evidence_not_a_name_guess" "F13 teste da regra"
says "$WIRING" "fn find_type_qualified_producers" "F14 passe R1 (Tipo::assoc)"
says "$WIRING" "fn find_receiver_method_producers" "F14 passe R2 (método por receptor)"
says "$WIRING" "fn a_type_qualified_call_reaches_the_method_of_the_type_module_through_a_crowd" "F14 teste do R1"
says "$ROOT/crates/touring-hooks-core/src/symbol_extractors.rs" "fn a_raw_identifier_segment_resolves_to_the_module_it_names" "F15 teste do segmento r#"
says "$ROOT/crates/touring-cli/src/cli/handlers/index.rs" "fn a_python_alias_use_is_import_evidence_for_the_file_edge" "F16 teste do alias Python"
says "$AUDIT" "## Rodada final" "relatório cobre a rodada final"

echo "relatório do cross-audit"
have "$AUDIT" "docs/audits/cross-audit-2026-11-12.md"
for s in "## Veredito" "## Fase 6 — E2E PROOF" "## Juiz de convergência" "## Decisões abertas com Gabriel"; do
  says "$AUDIT" "$s" "seção «$s»"
done
lacks "$AUDIT" "Preenchido ao final" "seção do juiz preenchida"

echo "gates do workspace (G1)"
SUM="$EVID/g1/summary.txt"
if [ ! -f "$SUM" ]; then
  bad "resumo dos gates ($SUM)"
else
  n=$(grep -c ' exit=' "$SUM")
  red=$(grep ' exit=' "$SUM" | grep -vc ' exit=0 ')
  [ "$n" -ge 8 ] && [ "$red" -eq 0 ] && ok "$n gates, todos exit=0" || bad "gates: $n linhas, $red com exit≠0 ($SUM)"
  if grep -qE 'failed=[1-9]|^  (FAILED|ERR) ' "$SUM"; then bad "resumo acusa teste falho ou erro de build"; else ok "nenhum teste falho no resumo"; fi
  grep -q '^end ' "$SUM" && ok "a rodada dos gates terminou" || bad "resumo sem a linha «end»: rodada em curso ou interrompida"
  # The run must have STARTED after the last source change it certifies: a file edited while the
  # suite runs is not what the suite measured. The first step's log (`fmt.log`) is closed seconds
  # after the start, so its mtime stands for it. Docs do not compile: *.md is out.
  newest=$( { git -C "$ROOT" diff --name-only HEAD -- crates scripts; git -C "$ROOT" ls-files --others --exclude-standard -- crates scripts; } \
            | grep -v '\.md$' \
            | while IFS= read -r f; do [ -f "$ROOT/$f" ] && stat -c '%Y %n' "$ROOT/$f"; done | sort -rn | head -1)
  src_ts=${newest%% *}; src_file=${newest#* }
  start_ts=$(stat -c %Y "$EVID/g1/fmt.log" 2>/dev/null || echo 0)
  if [ -z "$newest" ] || [ "$src_ts" -le "$start_ts" ]; then ok "gates iniciados depois da última mudança de fonte"
  else bad "fonte mais nova que o início dos gates: ${src_file#"$ROOT"/}"; fi
fi

[ "$fail" -eq 0 ] && echo "RESULTADO: todos os artefatos presentes" || echo "RESULTADO: há artefato faltando"
exit "$fail"
