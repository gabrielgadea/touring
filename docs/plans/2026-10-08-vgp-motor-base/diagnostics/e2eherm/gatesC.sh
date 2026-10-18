#!/bin/bash
# Gate C: the shared helper is #[path]-included by test targets in 3 OTHER crates — prove they still compile.
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
cd /home/gabrielgadea/projects/touring-wt-e2eherm || exit 1
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm
rc=0; n=0
run() { n=$((n+1)); echo "### $*"; "$@" > "$S/gatesC.$n.log" 2>&1; r=$?; echo "rc=$r"; tail -2 "$S/gatesC.$n.log" | cut -c1-160; [ $r -ne 0 ] && rc=1; }
run cargo check -p touring-generator --test e2e_diagnostic_rfc100
run cargo check -p touring-integration-tests --test wave15_cli_exposure_e2e --test entity_mcp_e2e --test wave16_status_mcp_e2e --test wave17_query_cache_e2e --test wave18_cache_expansion_e2e
run cargo check -p touring-server --test binary_e2e --test cli_language_e2e --test e2e_hook_integration --test f3_update_component_e2e --test graph_service_e2e
exit $rc
