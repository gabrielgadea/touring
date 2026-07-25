# Code Execution Gateway (CEG) — pointer

> **Auto-load stub** (migrado 19/07/2026, /doctor) | **Canonical body**: `~/.claude/skills/Touring/references/code-execution-gateway.md` (v1.0)

Pipeline typestate X0..X9 (`run_gateway`, crate `touring-ceg`) intercepta toda ação code-bearing (Bash/Write/ctx_execute/inferlets/jobs/MCP) antes da execução real; X3 VGP e X5 SANDBOX estruturalmente inskipáveis; invariante fail-open (exit 0 — nunca bloqueia sessão). Capabilities deny-by-default estilo Deno, 4 perfis built-in: ReadOnly / StagedWrite / Trusted / Sandboxed; `ENV_ALLOWLIST` nunca inclui vars de credencial. Observabilidade: `touring gate-metrics -j`. Stages X0-X9, perfis, staging area e key files: canonical body acima.
