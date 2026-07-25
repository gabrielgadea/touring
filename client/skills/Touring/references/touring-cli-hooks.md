# Touring CLI — Lifecycle Hooks

> **Module**: 2/7 | **Version**: v4.9 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 7 — Hooks)

24 hooks de ciclo de vida do Claude Code (pre/post-*, session-*, cortex) + 2 hooks neurais (classify-intent, scan-pii). Cada hook é invocado pelo Claude Code em momentos específicos do fluxo de trabalho.

---

## 1. Hooks de ciclo de vida (Claude Code hooks)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring serve` | — | Inicia MCP server sobre stdio |
| `touring pre-read` | `pre-read` | Pré-injeção de contexto para Read |
| `touring post-read` | `post-read` | Aprende do conteúdo lido |
| `touring pre-bash` | `pre-bash` | Consulta histórico de comandos Bash |
| `touring post-bash` | `post-bash` | Captura resultado de comando Bash |
| `touring pre-edit` | `pre-edit` | Análise de impacto com scored signals, CILA budget, rayon parallel (v29) |
| `touring pre-write` | `pre-write` | Validação especulativa + anti-patterns antes de Write (v29) |
| `touring post-edit` | `post-edit` | Rastreia mudanças + feedback de qualidade multi-language (v29) |
| `touring post-write` | `post-write` | Verificação de qualidade + wiring registration para Write (v29) |
| `touring session-start` | `session-start` | Carrega conhecimento da sessão |
| `touring session-stop` | `session-stop` | Persiste estado da sessão |
| `touring prompt-enhance` | `prompt-enhance` | Aprimoramento de prompt nativo Rust |
| `touring post-tool-failure` | `post-tool-failure` | Registra falha + auto-cria gotcha + circuit breaker (Halt após 5+ falhas) (v30) |
| `touring post-compact` | `post-compact` | Re-aquece cache para arquivos mais acessados após compactação (v30) |
| `touring instructions-loaded` | `instructions-loaded` | Injeta stats de conhecimento do projeto na inicialização (v30) |
| `touring hook-memory-store` | `hook-memory-store` | Armazena evento de hook na memória touring (intent/quality/completion) |
| `touring hook-memory-recall` | `hook-memory-recall` | Recupera padrões de hook da memória touring |
| `touring decompose-event` | `decompose-event` | TaskCreated/TaskCompleted → touring decompose CLI (session context) |
| `touring pre-task-scout` | `pre-task-scout` | PreToolUse enrichment com touring-scouter quick-mode + SQLite LRU cache |
| `touring task-created` | `task-created` | Hook Rust para task created event (conv from Python) |
| `touring task-completed` | `task-completed` | Hook Rust para task completed event (conv from Python) |
| `touring post-tool-rl` | `post-tool-rl` | RL reward computation em Rust (conv from Python) |
| `touring cortex <event>` | `cortex` | Engine unificado de hooks |

## 2. Hooks neurais (FileKnowledgeDB)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring classify-intent` | `cli-classify` | Classificação CILA de intent |
| `touring scan-pii` | `cli-pii` | Detecção de PII |

---

**Outros módulos**: [overview](touring-cli-overview.md) | [intelligence](touring-cli-intelligence.md) | [tasks](touring-cli-tasks.md) | [rl-quality](touring-cli-rl-quality.md) | [generate](touring-cli-generate.md) | [meta](touring-cli-meta.md)
