#!/bin/bash
# Probe each hook with `{}` on stdin (what a degraded read substitutes), daemon skipped.
T=$(mktemp -d); cd "$T"
for h in pre-bash pre-write pre-edit-prevention pre-tool-use pre-edit pre-read pre-grep cli-suggest; do
  out=$(echo '{}' | env -i PATH="$PATH" HOME="$T" CLAUDE_PROJECT_DIR="$T" TOURING_NO_DAEMON=1 touring-hook $h 2>/dev/null); rc=$?
  printf '%-22s rc=%s out=%.230s\n' "$h" "$rc" "$(echo "$out" | tr '\n' ' ')"
done
echo "--- valid shape, nothing to inspect (empty command / path)"
for pair in 'pre-bash|{"tool_name":"Bash","tool_input":{"command":""}}' 'pre-write|{"tool_name":"Write","tool_input":{"file_path":"","content":""}}'; do
  h=${pair%%|*}; j=${pair#*|}
  out=$(echo "$j" | env -i PATH="$PATH" HOME="$T" CLAUDE_PROJECT_DIR="$T" TOURING_NO_DAEMON=1 touring-hook $h 2>/dev/null); rc=$?
  printf '%-22s rc=%s out=%.230s\n' "$h" "$rc" "$(echo "$out" | tr '\n' ' ')"
done
