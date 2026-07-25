# touring-excellence — Slash Command para Potencializar o Touring

> **Versão**: 1.0.0 | **Touring**: v28.13.0 | **Paradigma**: TACO Orchestrator

## Identidade

Este slash command executa uma análise completa do Touring (v28.13.0) usando:
- **Touring CLI** (`touring -j <subcommand>`) —intelligence layer nativo
- **Context7 MCP** (`mcp__plugin_context7_context7__*`) — documentação actualizada de librarias

## Comandos Obrigatórios (8 Análises)

### 1. Erros, Bugs e Warnings

```bash
# Clippy completo (nega todos os warnings)
cargo clippy --workspace -- -D warnings 2>&1

# Check geral
cargo check --workspace 2>&1

# Tests (esperado: 3851 passed)
cargo test --workspace --exclude touring-python 2>&1 | grep "^test result:"

# Touring CLI audit
touring -j wiring orphans
touring -j wiring status
touring -j flywheel status
```

**Context7 obrigatório**: rust-clippy, rust-error-handling

### 2. Integração Intra-Crate

```bash
# Overview de cada crate
touring ast overview crates/touring-hooks/src/lib.rs
touring ast overview crates/touring-cortex/src/lib.rs
touring ast overview crates/touring-cognitive/src/lib.rs
touring ast overview crates/touring-index/src/lib.rs

# Símbolos públicos sem consumidores (orphans)
touring -j wiring orphans

# Blast radius de módulos críticos
touring ast blast crates/touring-hooks/src/hook_registry.rs
```

**Context7 obrigatório**: rust-modularity, rust-api-design

### 3. Integração Inter-Crates

```bash
# Dependências entre crates
touring graph dependencies

# Wiring status (integração)
touring -j wiring modules

# Score de integração por módulo
touring -j wiring score <module>

# Cross-crate symbols
touring index find HookRuntime
touring index find SymbolStore
touring index find AdaptiveEngine
```

**Context7 obrigatório**: rust-architecture, microservices-patterns

### 4. Performance, Qualidade e Excelência

```bash
# Cognitive metrics
touring -j cognitive metrics

# Incremental cache status
touring -j incremental status

# Memory stats
touring -j memory stats

# Evolution insights
touring -j evolution insights

# MCTS search para otimização
touring mcts search "performance_optimization"
```

**Context7 obrigatório**: rust-performance, rust-async

### 5. Infraestrutura, Módulos e Arquitetura

```bash
# AST overview de arquivos críticos
touring ast overview crates/touring-hooks/src/hook_registry.rs
touring ast overview crates/touring-server/src/cli_handlers.rs
touring ast overview crates/touring-cortex/src/lib.rs

# Daemon health
touring-hook --daemon-health

# Wiring intelligence
touring -j wiring status

# SCHEMA_VERSION validation
grep -r "SCHEMA_VERSION" crates/*/src/
```

**Context7 obrigatório**: clean-architecture, rust-design-patterns

### 6. Integração Claude Code

```bash
# Hooks configuration
cat ~/.claude/settings.json | jq '.hooks'

# Touring hooks audit
ls -la ~/.claude/hooks/
cat ~/.claude/hooks/touring-hook

# Claude hooks
grep -r "hook" ~/.claude/settings.json
```

**Context7 obrigatório**: claude-code-sdk, claude-api

### 7. Aproveitamento Claude Code Intelligence

```bash
# Memory patterns
touring -j memory recall "pattern:claude_code"

# Gotchas积累
touring -j gotcha list

# Session history
touring -j session list

# Evolution drift
touring -j evolution drift

# Knowledge graph stats
touring -j memory stats
```

**Context7 obrigatório**: claude-api, anthropic-sdk

### 8. Oportunidades de Evolução

```bash
# Evolution tools effectiveness
touring -j evolution tools

# Learning status
touring -j learning status

# Suggest next actions
touring suggest next "touring_excellence"

# Cortex handlers count
touring -j cognitive engines

# MCTS opportunities
touring mcts search "touring_evolution"
```

**Context7 obrigatório**: react-hooks (contexto), nextjs (contexto), langchain (contexto)

## Workflow de Execução

### Phase 1: Discovery (touring-cli)

```bash
# 1. Wired orphans check
touring -j wiring orphans

# 2. Flywheel health
touring -j flywheel status

# 3. Daemon status
touring-hook --daemon-health

# 4. Clippy check
cargo clippy --workspace -- -D warnings 2>&1

# 5. Test suite
cargo test --workspace --exclude touring-python 2>&1 | grep "^test result:"
```

### Phase 2: Context7 Queries

Para CADA análise, buscar documentação actualizada:

```bash
# Exemplo para análise de erros
mcp__plugin_context7_context7__resolve-library-id(
  query="rust error handling best practices",
  libraryName="rust"
)

# Exemplo para performance
mcp__plugin_context7_context7__resolve-library-id(
  query="rust async performance tokio",
  libraryName="tokio"
)
```

### Phase 3: Synthesis

Agregar resultados de:
- `touring -j <commands>` (dados reais)
- `context7` (documentação actualizada)
- Análise de código fonte

### Phase 4: Deliverables

1. **errors.md** — Bugs e warnings encontrados
2. **intra_crate.md** — Oportunidades de integração intra-crate
3. **inter_crate.md** — Oportunidades de integração inter-crates
4. **excellence.md** — Recomendações de performance/qualidade
5. **architecture.md** — Melhorias de infraestrutura/arquitetura
6. **claude_code_integration.md** — Oportunidades de integração CC
7. **intelligence_usage.md** — Aproveitamento da inteligência CC
8. **evolution.md** — Roadmap de evolução

## Output Format

```markdown
# Touring Excellence Report — $(date)

## 1. Erros, Bugs e Warnings
### Findings
- [finding]
### Context7 References
- [doc_links]
### Recommendations
- [action]

## 2. Integração Intra-Crate
...

## 3. Integração Inter-Crates
...

## 4. Performance e Excelência
...

## 5. Infraestrutura e Arquitetura
...

## 6. Integração Claude Code
...

## 7. Aproveitamento CC Intelligence
...

## 8. Evolução
...

## Action Items (Prioritized)
1. [P0] ...
2. [P1] ...
3. [P2] ...
```

## Validação

Após implementar correções:

```bash
# Verify exit 0
cargo check --workspace 2>&1 | grep -c "^error" | grep -q "^0$"

# Verify clippy
cargo clippy --workspace -- -D warnings 2>&1 | grep -c "^error" | grep -q "^0$"

# Verify tests
cargo test --workspace --exclude touring-python 2>&1 | grep "3851 passed"

# Verify wiring
touring -j wiring orphans | jq '.orphan_count'  # deve ser 0
```
