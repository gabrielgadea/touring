# VP-Scout — Verified Protocol for Scouting (pointer)

> **Auto-load stub** (migrado 19/07/2026, /doctor) | **Canonical body**: `~/.claude/skills/Touring/references/VP-Scout-rule.md` (v1.2)

9 cadeias de verificação OBRIGATÓRIAS antes de reportar qualquer oportunidade/finding: 1 Feature Trace · 2 Dependency Cycle · 3 Already Implemented · 3b Test File Content (nunca afirmar gap de cobertura sem ler o corpo do teste) · 4/4b Homonimia (incl. cross-language) · 5 Compilation Evidence (**NUNCA afirmar erro de compilação sem `cargo check` executado** — plan docs são intenção, não estado) · 6 Staleness · 7 Wiring Cache Staleness (orphan claim exige grep de confirmação). Daemon degraded ≠ scout abortado (fallback cargo+grep+read). Cadeias completas, protocolo de execução e template JSON: canonical body acima; exemplos: `~/.claude/skills/Touring/references/vp-scout-examples.md`.
