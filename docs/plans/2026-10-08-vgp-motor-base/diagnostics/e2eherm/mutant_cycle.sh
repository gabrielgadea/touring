#!/bin/bash
# Mutant chain, ONE hw-slot acquisition. The handler lives in the DAEMON, so each phase rebuilds the daemon
# and reruns the (unchanged) wave25 test exe. Mutation A is already applied by hand when this starts.
#   mA  : extractor without "prompt"            -> expect enter_plan_mode FAILED, task_create ok
#   mB  : recall hint never pushed              -> expect BOTH enrichment tests FAILED (control ok, seed assertion red)
#   rest: both reverted BY CONTENT              -> expect 6 passed + both E2EHERM-RAN lines, sha256 == original
# usage: mutant_cycle.sh <tag-prefix> <exe-label>
S=/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/e2eherm
tag=${1:?tag}; label=${2:?exe label}
exe=$(cat "$S/$label.exe")
W=/home/gabrielgadea/projects/touring-wt-e2eherm
F=$W/crates/touring-cli/src/cli/pretask.rs
ORIG_SHA=$(cut -d' ' -f1 "$S/pretask.sha256.original")
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm
cd "$W" || exit 1

A_OLD='for key in ["subject", "description", "prompt"] {'
A_NEW='for key in ["subject", "description"] {'
B_OLD='parts.push(format!("memory: {}", keys.join(", ")));'
B_NEW='let _ = format!("memory: {}", keys.join(", "));'

phase() {  # phase <name>
  local name=$1
  echo "################ phase $name"
  echo "pretask.rs sha256: $(sha256sum "$F" | cut -d' ' -f1)  (original $ORIG_SHA)"
  echo "extractor : $(grep -n 'for key in \[' "$F" | head -1 | sed 's/^ *//')"
  echo "mem push  : $(grep -nE 'parts.push\(format!\("memory:|let _ = format!\("memory:' "$F" | head -1 | sed 's/^ *//')"
  cargo build -p touring-hooks --bin touring-daemon > "$S/mutant.$tag.$name.build.log" 2>&1; echo "daemon build rc=$?"
  ls -la --time-style=+%T "$CARGO_TARGET_DIR/debug/touring-daemon" | awk '{print "daemon binary mtime:", $6}'
  ( cd "$W/crates/touring-hooks" && env -u TOURING_PROJECT_ROOT -u CLAUDE_PROJECT_DIR -u TOURING_DAEMON_SOCKET \
      -u TOURING_DAEMON_SOCK -u TOURING_SESSION_ID "$exe" --nocapture > "$S/mutant.$tag.$name.run.log" 2>&1 ); echo "test rc=$?"
  grep -E "^test |test result|E2EHERM|SKIP" "$S/mutant.$tag.$name.run.log" | cut -c1-240
  grep -A2 "panicked" "$S/mutant.$tag.$name.run.log" | cut -c1-300 | head -10
}

phase mA
echo "### revert A by content"; python3 -I "$S/mutate.py" "$F" "$A_NEW" "$A_OLD" || exit 3
echo "### apply B by content";  python3 -I "$S/mutate.py" "$F" "$B_OLD" "$B_NEW" || exit 3
phase mB
echo "### revert B by content"; python3 -I "$S/mutate.py" "$F" "$B_NEW" "$B_OLD" || exit 3
phase restored
NOW_SHA=$(sha256sum "$F" | cut -d' ' -f1)
echo "FINAL sha256 now=$NOW_SHA original=$ORIG_SHA $([ "$NOW_SHA" = "$ORIG_SHA" ] && echo IDENTICAL || echo DIFFERENT)"
echo "git diff --stat touring-cli: [$(git -C "$W" diff --stat -- crates/touring-cli | tr '\n' ' ')]"
