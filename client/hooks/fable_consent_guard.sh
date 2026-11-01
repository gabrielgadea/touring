#!/usr/bin/env bash
# fable_consent_guard.sh — PreToolUse(Bash | Write|Edit|MultiEdit|NotebookEdit): o consentimento do Fable é do humano.
#
# Source of truth: client/hooks/fable_consent_guard.sh. ~/.claude/hooks/fable_consent_guard.sh is a symlink to it.
# Tests: client/hooks/tests/test_fable_consent_guard.py. Companion of agent_model_gate.py (docs/economia-de-creditos-e-contexto.md §1).
#
# WHY: agent_model_gate.py refuses a Fable subagent unless ~/.claude/touring/fable_consent.json is live, and only
# Gabriel grants it (`! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h` at the prompt: the user's `!`
# never goes through PreToolUse, but the model's own Bash does). Without this guard the model could grant itself the
# consent it was just refused. Granting is therefore denied to the model; revoking (`--revogar-fable`) is safe and passes.
#
# WHAT IT DENIES (the model's own tool calls, nothing else):
#   Bash                                  command (text of tool_input) contains `--consentir-fable` or `fable_consent.json`
#   Write|Edit|MultiEdit|NotebookEdit     file_path/notebook_path contains `fable_consent.json`
# Only the exact file name is matched, not the bare `fable_consent`: that stem is also in this script's name, its test
# and the hook directory, and a guard that locks its own maintenance out gets disabled. File CONTENT is never judged.
#
# KNOWN LIMIT (declared, not hidden): this is textual. A command that builds the path or the flag at run time
# (`"fable_""consent.json"`, `$(printf ...)`) or reaches a shell through an MCP execute tool is not seen. The substrate
# closure is the Claude Code sandbox `filesystem.denyWrite` on that path.
#
# PURE BASH: one `cat` to read stdin, then builtins only (no jq, no python, no sed/grep): it runs on EVERY Bash call.
# FAIL-OPEN: anything unexpected exits 0 in silence. Never blocks on its own failure.

payload=$(cat) || exit 0
[[ $payload =~ \"tool_name\"[[:space:]]*:[[:space:]]*\"([A-Za-z]+)\" ]] || exit 0
tool=${BASH_REMATCH[1]}
rest=${payload#*\"tool_input\"}
blocked=0
case $tool in
  Bash)
    case $rest in
      *--consentir-fable* | *fable_consent.json*) blocked=1 ;;
    esac
    ;;
  Write | Edit | MultiEdit | NotebookEdit)
    if [[ $rest =~ \"(file_path|notebook_path)\"[[:space:]]*:[[:space:]]*\"([^\"]*)\" ]]; then
      case ${BASH_REMATCH[2]} in
        *fable_consent.json*) blocked=1 ;;
      esac
    fi
    ;;
esac
((blocked)) || exit 0
printf '%s' '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"[fable-consent-guard] O consentimento do Fable é do Gabriel, não do modelo: conceder (ou escrever o arquivo fable_consent.json) por Bash/Write/Edit não passa. Peça o sim dele; ele concede no prompt com `!` (os hooks não veem o `!` do usuário): ! python3 ~/.claude/hooks/agent_model_gate.py --consentir-fable 2h\nRevogar (--revogar-fable) o modelo pode. Sem consentimento: continue na sessão principal ou delegue a model sonnet (implementar) ou haiku (buscar). Corpo: docs/economia-de-creditos-e-contexto.md §1."}}'
exit 0
