# TACO — Touring Agentic Code Orchestrator (pointer)

> **Auto-load stub** (migrado 19/07/2026, /doctor) | **Canonical body**: `~/.claude/skills/Touring/references/TACO-subagent-rule.md` (v6.3)

Todo subagent TACO é BOUND ao protocolo: prompt inicia com `@/home/gabrielgadea/.claude/skills/Touring/references/TACO-subagent-rule.md` como primeira linha, usa Touring CLI para discovery (VGP), e retorna **APENAS JSON cru** com campo `symbol_verification` (evidência CLI obrigatória — output sem o campo = checkpoint REJECT, composite 0.0). Fases sequenciais 0→7 com FASE 0 HEALTH GATE e FASE 4.5 PRE-IMPL AUDIT; roteamento CILA L0-L4; subagents herdam a permission mode do orquestrador (nunca forçar override mais estreito). Protocolo completo, templates, gates e Symbol Verification Table: canonical body acima.
