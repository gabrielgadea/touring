# PLN2 S1-S3 — PlanRegistry Wiring + SchemaRegistry MCP + Bundle Plans

**Data**: 2026-04-11
**Session**: TACO Iteration 3
**Status**: CONCLUIDO — composite_score: 1.0, 0 erros de compilacao

## Objetivo

Completar a integracao PLN2 que tinha ficado incompleta:
1. `PlanRegistry` (touring-generator) era orphan sem consumidor em touring-server
2. `SchemaRegistry` (touring-generator) sem exposicao MCP
3. `bundle_plans` e `schema_registry_info` funcoes ausentes em generator_tools.rs

## Mudancas

### Bug Fix: cortex_dispatcher.rs
- **Arquivo**: `crates/touring-hooks/src/cortex_dispatcher.rs`
- **Problema**: `}` duplicado em linha 277 causava compilation error
- **Fix**: Removida a chave extra

### S-1: params.rs (`crates/touring-server/src/params.rs`)
- `GeneratorBundleParams`: para `touring_generator_bundle` MCP tool
  - `plans_json: Vec<String>` — planos em JSON para execucao sequencial
  - `dry_run: bool` — pular commit se true
- `SchemaVersionParams`: para `touring_generator_schema_check` e `touring_generator_schema_registry_info`
  - `version: String` — versao para verificar compatibilidade

### S-2: generator_tools.rs (`crates/touring-generator/src/generator/generator_tools.rs`)
- `submit_plan_with_registry(plan_json, dry_run, registry: &PlanRegistry)`: registra plano antes da execucao, atualiza status (Rendered -> Committed/Failed)
- `bundle_plans(plans_json: Vec<String>, dry_run: bool)`: wrapper ergonomico sobre `bundle()` com Vec<String> owned
- `schema_registry_info()`: retorna engine_version + migration paths do SchemaRegistry
- `schema_registry_check(version: &str)`: verifica se versao e compativel com o engine atual
- 6 unit tests adicionados:
  - `schema_registry_info_returns_ok`
  - `has_engine_version`
  - `migration_count_is_number`
  - `check_current_version_compatible`
  - `check_unknown_version_incompatible`
  - `check_returns_engine_version`

### S-3: server/mod.rs (`crates/touring-server/src/server/mod.rs`)
- Campo `plan_registry: touring_generator::SharedPlanRegistry` adicionado ao TouringServer struct
- Inicializacao: `Arc::new(touring_generator::PlanRegistry::new())` em `new()`
- `generator_submit_plan` agora usa `submit_plan_with_registry()` passando `&self.plan_registry`
- Novo `#[tool]`: `touring_generator_schema_registry_info` — total: 69 tools

### Test Fix: tools/mod.rs
- Contagem atualizada: 66 -> 69 tools

## MCP Tools — Generator Suite

| Tool | Funcao | Status |
|------|--------|--------|
| `touring_generator_submit_plan` | Pipeline completa + registry tracking | atualizado (S-3) |
| `touring_generator_bundle` | Bundle sequencial de planos | existente |
| `touring_generator_schema_check` | Verificar compatibilidade de versao | existente |
| `touring_generator_schema_registry_info` | Info do SchemaRegistry | NOVO (S-3) |

## Metricas

- **Workspace tests**: 4826 passando, 0 falhando
- **Tool count**: 69 (`#[tool]` annotations em server/mod.rs)
- **Orphans delta**: 0 (nenhum novo orphan introduzido)
- **PlanRegistry**: de orphan para wired (touring-server e consumer)

## Arquitetura de Integracao

```
TouringServer
+-- plan_registry: SharedPlanRegistry (Arc<PlanRegistry>)
|   +-- PlanRegistry::register(handle)       -- antes da execucao
|   +-- PlanRegistry::update_status(id, status) -- apos execucao
+-- #[tool] generator_submit_plan
    +-- submit_plan_with_registry(&self.plan_registry)
        +-- run_pipeline() -> commit
        +-- ExecutionStatus: Rendered | Committed | Failed
```

## Decisoes de Design

| Decisao | Racional |
|---------|---------|
| `SharedPlanRegistry` como campo do server | Lifecycle gerenciado pelo server; sem global state |
| `ExecutionStatus`: Rendered/Committed/Failed | Apenas 3 estados necessarios; Running/Completed nao existem |
| `submit_plan_with_registry` recebe `&PlanRegistry` | Injecao de dependencia; testavel sem server |
| tools/mod.rs count test atualizado | Previne regressao silenciosa em contagem de tools |

## Alternativas Consideradas

- `OnceLock<PlanRegistry>` global: descartado por ser estado global nao testavel
- `submit_plan_with_registry` como metodo do server: descartado para manter generator_tools testavel sem server
- Expor `bundle_plans` como MCP tool novo: descartado pois `touring_generator_bundle` ja existe
