#!/bin/bash
# =============================================================================
# Claude Code Auto-Compact Hook - Stop Event
# Forces compaction when context >= threshold by returning continue:true
# =============================================================================

FLAG_FILE="${HOME}/.claude/context_alert"
COMPACT_TOPIC="${COMPACT_FOCUS:-trabalho atual}"

# Consume full stdin (JSON may be multi-line)
cat > /dev/null

# If flag file exists, context is above threshold
if [ -f "$FLAG_FILE" ]; then
    CONTEXT_PCT=$(cat "$FLAG_FILE")

    # Remove flag to prevent infinite loop
    rm -f "$FLAG_FILE"

    # Return JSON to continue with compact instruction
    cat << EOF
{
    "continue": true,
    "reason": "AUTO-COMPACT ATIVADO (contexto: ${CONTEXT_PCT}%)",
    "systemMessage": "CRITICAL: O contexto atingiu ${CONTEXT_PCT}%. Voce DEVE executar /compact imediatamente, preservando: estado atual da tarefa, decisoes arquiteturais, erros resolvidos, e proximos passos. Apos compactar, retome a tarefa anterior."
}
EOF

    exit 0
fi

# No alert - do nothing
echo '{}'
exit 0
