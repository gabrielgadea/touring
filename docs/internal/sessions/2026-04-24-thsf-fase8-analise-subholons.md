# THSF Fase 8 — analise sub-holons (wave D)

**Data**: 2026-04-24
**Escopo**: Aplicação do padrão adapter standalone a 3 sub-projetos
do `analise` (kazuba-geo-engine), criando 3 sub-holons aninhados:
`kazuba-process-analysis`, `kazuba-external-apis`, `kazuba-rust-core`.
**Resultado**: ✅ 9/9 capabilities invocáveis E2E + **33/33 audit
gates PASS** (era 21/21).

---

## 1. Contexto

Após a wave C (analise raiz como holon `kazuba-geo-engine`, 3
capabilities), Gabriel autorizou: *"aplique também em
process_analysis, external_apis, kazuba-rust-core"*.

Os 3 alvos têm escopos distintos:

| Alvo | Tipo | Tamanho |
|---|---|---|
| `analise/scripts/process_analysis/` | Sub-package Python interno (sem pyproject) | 22 subdirs, ~614 arquivos .py |
| `analise/scripts/external_apis/` | Sub-package Python interno (clients HTTP) | 11 API dirs, 18 client classes |
| `analise/packages/kazuba-rust-core/` | Crate Rust real (PyO3, Cargo.toml v2.9.0) | 10 .rs files / 1.778 LOC |

**Decisão arquitetural**: Opção B (3 sub-holons standalone) sobre
Opção A (capabilities adicionais no manifest analise raiz). Razões:
1. Cada um tem domínio próprio bem definido.
2. `discover_holons` desce até depth=6 e só bloqueia descida DENTRO
   de `.holon/` — sub-holons aninhados em depth=4 são descobertos
   normalmente, sem hack.
3. Mantém autonomy_guarantee em cada nível de granularidade.
4. Permite reuso futuro independente (alguém clonando só
   external_apis leva o `.holon/` junto).

---

## 2. Entregáveis

### 9 adapters reais (3 por sub-holon)

| Sub-holon | Capability | Adapter | LOC | O que faz |
|---|---|---|---|---|
| process-analysis | `health-check` | `health_check.py` | 75 | Python ver + subdir count + py file count |
| process-analysis | `module-stats` | `module_stats.py` | 165 | Walk recursivo: LOC, classes, funcs, async funcs por subdir top-level |
| process-analysis | `phase-catalog` | `phase_catalog.py` | 95 | Enumera `phases/` + `phase_registry/` (file, module, size, line_count) |
| external-apis | `health-check` | `health_check.py` | 70 | Python ver + api dir count + presença de __init__/README |
| external-apis | `api-registry` | `api_registry.py` | 85 | Para cada API: __init/README/SKILL presença, py_file_count, sub_clients, total_loc |
| external-apis | `client-catalog` | `client_catalog.py` | 95 | Parser AST stdlib do `__init__.py` extrai classes terminadas em `Client` |
| rust-core | `health-check` | `health_check.py` | 95 | Python ver + Cargo.toml/lock presença + crate name/version + cargo binary disponível |
| rust-core | `crate-info` | `crate_info.py` | 145 | Parse Cargo.toml via tomllib stdlib: package, lib, features, deps counts (deps/dev_deps/build_deps) |
| rust-core | `algorithms-inventory` | `algorithms_inventory.py` | 115 | Walk `algorithms/`: subdirs, .rs file count + total LOC, ignora target/mutants.out |

**Invariantes**:
- Stdlib-only (`json`, `pathlib`, `tomllib`, `ast`, `platform`,
  `sys`, `time`, `re`, `os`, `shutil`).
- Zero importação de `kazuba_*` ou de qualquer dep external_apis.
- Stdin = JSON, stdout = JSON, exit codes 0/1/2.
- `PROJECT_ROOT = parents[2]` aponta ao próprio sub-holon (não ao
  analise raiz) — escopo isolado.

### 9 schemas JSON

Cada capability tem schema em `.holon/schemas/<name>.json` com
request + response shapes. Validador holon doctor passa para todos.

### 3 manifests v0.1.0 (`autonomy_guarantee = true`)

```toml
# scripts/process_analysis/.holon/manifest.toml
[holon.identity]
name = "kazuba-process-analysis"
version = "0.1.0"
description = "Process analysis pipeline (apex_engine, dspy_cluster, mef_impact, phases, quality_gates, RLM, taco_coordinator, touring_bridge)"

# scripts/external_apis/.holon/manifest.toml
[holon.identity]
name = "kazuba-external-apis"
version = "0.1.0"
description = "Brazilian government API client registry (TCU, BACEN, IBGE, BrasilAPI, ANA, BNDES, ANTT + 7 sub-clients, Jurisprudência, Diário Oficial, Dados Abertos)"

# packages/kazuba-rust-core/.holon/manifest.toml
[holon.identity]
name = "kazuba-rust-core"
version = "2.9.0"  # match Cargo.toml package.version
description = "High-performance Rust extensions for Kazuba RAG (PyO3 + maturin, BM25, regex, NFKC, rayon)"
```

---

## 3. Validação E2E — números reais

### Discovery

```bash
$ holon discover ~/projects/analise
kazuba-rust-core            2.9.0    /analise/packages/kazuba-rust-core
kazuba-geo-engine           2.6.0    /analise
kazuba-process-analysis     0.1.0    /analise/scripts/process_analysis
kazuba-external-apis        0.1.0    /analise/scripts/external_apis
```

**4 holons descobertos sob `analise/`** (1 raiz + 3 sub-holons).
Walker `os.walk` desce normalmente; só bloqueia DENTRO de `.holon/`.

### Invocações

```bash
# process_analysis health-check (~50ms)
$ holon invoke kazuba-process-analysis health-check '{}' --root ~/projects
→ {"status":"ok","python_version":"3.12.3","subdir_count":21,"py_file_count":48,...}

# process_analysis phase-catalog (~80ms)
$ holon invoke kazuba-process-analysis phase-catalog '{}' --root ~/projects
→ {"phase_count":206,"registry_count":5,...}

# process_analysis module-stats depth=3 (~5s)
$ holon invoke kazuba-process-analysis module-stats '{"max_depth":3}' \
    --root ~/projects --timeout 60
→ {"module_count":20,"totals":{"py_files":614,"loc":179117,
   "classes":1708,"functions":2220,"async_functions":3},...}

# external_apis api-registry (~200ms)
$ holon invoke kazuba-external-apis api-registry '{}' --root ~/projects
→ {"api_count":11,"apis":[
     {"name":"ana","py_file_count":1,"total_loc":156,...},
     {"name":"antt","py_file_count":8,"total_loc":993,...},
     ... (10 outras: bacen, base, bndes, brasilapi, dados_abertos,
          diario_oficial, ibge, jurisprudencia, tcu)
   ]}

# external_apis client-catalog (~30ms)
$ holon invoke kazuba-external-apis client-catalog '{}' --root ~/projects
→ {"client_count":18,"clients":[
     "ANAClient","ANTTClient","ANTTAccidentesClient","ANTTConcessoesClient",
     "ANTTFiscalClient","ANTTInfraClient","ANTTInvestimentosClient",
     "ANTTMonitoramentoClient","ANTTTransporteClient","BACENClient",
     "APIClient","BNDESClient","BrasilAPIClient","DadosAbertosClient",
     "DiarioOficialClient","IBGEClient","JurisprudenciaClient","TCUClient"
   ]}

# kazuba-rust-core crate-info (~10ms)
$ holon invoke kazuba-rust-core crate-info '{}' --root ~/projects
→ {"package":{"name":"kazuba-rust-core","version":"2.9.0",
   "edition":"2021","license":"MIT",
   "description":"High-performance Rust extensions for Kazuba RAG system",...},
   "feature_count":72, "dependency_counts":{"dependencies":54,
   "dev_dependencies":5,"build_dependencies":0}, ...}

# kazuba-rust-core algorithms-inventory (~50ms)
$ holon invoke kazuba-rust-core algorithms-inventory '{}' --root ~/projects
→ {"subdir_count":2,"rs_files_total":10,"rs_loc_total":1778,
   "subdirs":[{"name":"fuzz","rs_files":4,"total_loc":160},
              {"name":"src","rs_files":6,"total_loc":1618}], ...}
```

### Doctor

```
process_analysis: errors=0 warnings=0
external_apis:    errors=0 warnings=0
kazuba-rust-core: errors=0 warnings=0
```

---

## 4. Invariantes preservados

| Invariant | Evidência |
|---|---|
| Autonomy | Cada sub-projeto continua buildando/rodando sem `.holon/` |
| Reversibility | `rm -rf */.holon/` em qualquer um dos 3 restaura estado pré-pilot |
| No framework imports | `grep -r "kazuba_" .holon/` em cada → 0 |
| Idempotência | repeat calls de qualquer capability → mesmo resultado em árvore imutável |
| Sub-holon discovery | depth=4 ainda dentro de DEFAULT_DEPTH_LIMIT=6; walker não bloqueia |

---

## 5. Audit gates — 33/33 PASS

```
[gate] RFC-001 fixtures (14 cases)                       PASS
[gate] RFC-003 CRDT semantics (14 cases)                 PASS
[gate] holon core suite (37 cases)                       PASS
[gate] E2E cross-language integration (11 cases)         PASS
[gate] Rust template: clippy 0 warnings                  PASS
[gate] Rust template: cargo test (4 cases)               PASS
[gate] Python template: ruff clean                       PASS
[gate] Python template: pytest (8 cases)                 PASS
[gate] TS template: structural integrity                 PASS
[gate] Invariant 6: Rust len == Python len               PASS
[gate] retention.py (7 cases)                            PASS
[gate] mcp_server.py (12 cases)                          PASS
[gate] conformance suite (14 public gates)               PASS
[gate] holon doctor on templates (0 errors)              PASS
[gate] holon doctor on konverter (0 errors)              PASS
[gate] konverter pilot: file-info invokable              PASS
[gate] konverter pilot: health-check invokable           PASS
[gate] analise pilot: health-check invokable             PASS
[gate] analise pilot: package-registry invokable         PASS
[gate] analise pilot: workspace-stats (scoped scan)      PASS
[gate] holon doctor on analise (0 errors)                PASS
[gate] process_analysis pilot: health-check invokable    PASS  (NEW)
[gate] process_analysis pilot: phase-catalog invokable   PASS  (NEW)
[gate] process_analysis pilot: module-stats invokable    PASS  (NEW)
[gate] holon doctor on process_analysis (0 errors)       PASS  (NEW)
[gate] external_apis pilot: health-check invokable       PASS  (NEW)
[gate] external_apis pilot: api-registry invokable       PASS  (NEW)
[gate] external_apis pilot: client-catalog invokable     PASS  (NEW)
[gate] holon doctor on external_apis (0 errors)          PASS  (NEW)
[gate] kazuba-rust-core pilot: health-check invokable    PASS  (NEW)
[gate] kazuba-rust-core pilot: crate-info invokable      PASS  (NEW)
[gate] kazuba-rust-core pilot: algorithms-inventory invokable PASS  (NEW)
[gate] holon doctor on kazuba-rust-core (0 errors)       PASS  (NEW)

==== Audit summary: 33 pass / 0 fail ====
```

---

## 6. Holons invocáveis por `holon invoke` (total=8)

| Holon | Capabilities | Status |
|---|---|---|
| `holon-rust-template` | echo | ✅ E2E |
| `holon-python-template` | echo | ✅ E2E |
| `holon-ts-template` | echo | Estrutural |
| `konverter` | file-info, health-check | ✅ E2E |
| `kazuba-geo-engine` (analise raiz) | workspace-stats, package-registry, health-check | ✅ E2E |
| `kazuba-process-analysis` | module-stats, phase-catalog, health-check | ✅ E2E (NEW) |
| `kazuba-external-apis` | api-registry, client-catalog, health-check | ✅ E2E (NEW) |
| `kazuba-rust-core` | crate-info, algorithms-inventory, health-check | ✅ E2E (NEW) |

---

## 7. Arquivos entregues

### Novos (22)

```
projects/analise/scripts/process_analysis/.holon/
  ├── manifest.toml
  ├── adapters/{health_check,module_stats,phase_catalog}.py
  └── schemas/{health-check,module-stats,phase-catalog}.json

projects/analise/scripts/external_apis/.holon/
  ├── manifest.toml
  ├── adapters/{health_check,api_registry,client_catalog}.py
  └── schemas/{health-check,api-registry,client-catalog}.json

projects/analise/packages/kazuba-rust-core/.holon/
  ├── manifest.toml
  ├── adapters/{health_check,crate_info,algorithms_inventory}.py
  └── schemas/{health-check,crate-info,algorithms-inventory}.json

docs/2026-04-24-thsf-fase8-analise-subholons.md (este relatório)
```

### Editados (1)

```
tools/holon/tests/run_full_audit.sh   (+12 gates: 21 → 33)
```

---

## 8. Descobertas concretas sobre os 3 sub-projetos

### process_analysis (`179.117 LOC, 1.708 classes, 2.220 funcs`)
- 21 subdirs top-level (apex_engine, dspy_cluster, mef_impact,
  phase_registry, phases, pipeline, plans, process_type_config,
  quality_gates, resilience, rlm, scripts, taco_coordinator,
  tests, tools, touring_bridge, utils, validators, observability,
  bridge, learning).
- `phases/` tem **206** arquivos .py (cada fase é um módulo).
- `phase_registry/` tem 5 arquivos.
- Apenas 3 funções `async def` em todo o subtree (síncrono).

### external_apis (`11 API dirs, 18 client classes`)
- 11 dirs de API: ana, antt, bacen, base, bndes, brasilapi,
  dados_abertos, diario_oficial, ibge, jurisprudencia, tcu.
- 18 classes `*Client` exportadas (10 main + 7 ANTT subs +
  `APIClient` base abstract).
- ANTT é o maior (8 arquivos, 993 LOC) — main + 7 sub-clients.
- TCU tem subdir `enriched/` (único API com sub-clients além do ANTT).

### kazuba-rust-core (`v2.9.0, 72 features, 54 deps`)
- PyO3 + maturin (Python 3.10+ ABI3 stable).
- crate-type: cdylib (Python ext) + rlib (Rust testing).
- 72 feature flags definidas (rico em config).
- 54 dependencies + 5 dev-deps + 0 build-deps.
- algorithms/ tem 2 subdirs: `src/` (6 .rs / 1.618 LOC) +
  `fuzz/` (4 .rs / 160 LOC).

---

## 9. Zero débitos

- ✅ 9 adapters todos testados E2E via `holon invoke`
- ✅ 9 schemas conformes RFC-001
- ✅ 3 manifests passam `holon doctor` com 0 errors / 0 warnings
- ✅ Stdlib-only — zero deps externas
- ✅ Cada sub-projeto continua autônomo (autonomy_guarantee=true)
- ✅ 33/33 audit gates verdes
- ✅ Discovery aninhado validado: 4 holons sob `analise/`

---

---

## 10. Cross-audit pós-implementação (mesma data)

Após entrega inicial de 33/33 gates, executou-se auditoria cruzada
em 5 fases sob o critério "código cumpre o propósito declarado, não
apenas não crasha".

### Fase A — evidence collection (63 invocações hostis)

Matriz de exit codes para cada (capability × input) — 9 × 7 = 63
combinações. Resultado inicial: **drift detectado** — `module-stats`
era o único adapter que rejeitava JSON inválido com exit=2; os
outros 8 silenciavam (exit=0 + payload vazio).

### Fase B — schema vs response conformance

Validador stdlib-only (sem dep `jsonschema`) percorre cada campo
`required` + tipo + enum + minimum dos response schemas. **Drift = 0**:
9/9 capabilities retornam responses com todos os campos obrigatórios
declarados, tipos corretos, enums respeitados.

### Fase C — invariants in practice

| Invariante | Método | Resultado |
|---|---|---|
| stdlib-only | grep static + whitelist Python stdlib | ✅ 9/9 |
| autonomy_guarantee | `mv .holon /tmp; import; mv back` | ✅ 3/3 sub-projetos |
| discovery aninhado | `holon discover` em 3 raízes (parent/sibling/self) | ✅ 14/4/1 holons |

### Fase D — bugs + potencialização

- **Zero TODOs/FIXMEs/XXX/HACK** em 21 arquivos adicionados.
- **Standardização de JSON handling**: 8 adapters tinham
  `except json.JSONDecodeError: pass` — promovidos a comportamento
  estrito (mesmo de `module-stats`):

```python
except json.JSONDecodeError:
    sys.stderr.write("invalid JSON on stdin\n")
    return 2
```

Justificativa: surface erros de invocação cedo ao invés de mascarar.
Consistência cross-adapter é mais importante que tolerância silenciosa.
**Resultado pós-fix**: matriz de exit codes uniforme — todos os 9
adapters retornam exatamente os mesmos exit codes para os mesmos
inputs hostis (ver tabela §10.1 abaixo).

### Fase E — pytest E2E formal

Nova suite `tools/holon/tests/test_analise_subholons.py` com
**112 testes** cobrindo:

| Dimensão | Testes | O que prova |
|---|---|---|
| Discovery (3 roots) | 3 | Parent/sibling/self scoping |
| Doctor per sub-holon | 3 | 0 errors / 0 warnings |
| Direct subprocess invoke | 9 | Adapters retornam JSON válido |
| Required field presence | 9 | Schema response.required ⊆ response |
| `holon invoke` transport | 9 | Equivalência CLI ↔ direto |
| Edge cases (9 × 7) | 63 | Exit codes uniformes em inputs hostis |
| Stdlib-only static | 9 | Grep AST sem deps externas |
| Semantic correctness | 6 | crate name, 18 clients, módulos conhecidos, etc |
| Canary count | 1 | 9 capabilities total |

**112/112 PASS em 5.35s**.

### §10.1 — Matriz pós-fix de exit codes (consistência uniforme)

```
capability                                    empty_d empty_s inv_jsn jsn_arr extra_k jsn_nul malform
kazuba-process-analysis::health-check               0       0       2       0       0       0       2
kazuba-process-analysis::module-stats               0       0       2       0       0       0       2
kazuba-process-analysis::phase-catalog              0       0       2       0       0       0       2
kazuba-external-apis::health-check                  0       0       2       0       0       0       2
kazuba-external-apis::api-registry                  0       0       2       0       0       0       2
kazuba-external-apis::client-catalog                0       0       2       0       0       0       2
kazuba-rust-core::health-check                      0       0       2       0       0       0       2
kazuba-rust-core::crate-info                        0       0       2       0       0       0       2
kazuba-rust-core::algorithms-inventory              0       0       2       0       0       0       2
```

Contrato consistente:
- bytes vazios OU parse OK → exit 0 (mesmo se shape inesperada)
- bytes não-vazios + parse falha → exit 2 (invocation error)

### Audit consolidado: 33 gates → 22 (pytest substitui 12 ad-hoc)

Os 12 gates ad-hoc (3 por sub-holon × 3 sub-holons + 3 doctors)
foram consolidados em 1 gate pytest único: `analise sub-holons E2E
(112 cases)`. Net cobertura efetiva: **112 assertions vs 12** —
~9× mais robusto, com matrix de edge cases que os gates ad-hoc
não cobriam.

```
==== Audit summary: 22 pass / 0 fail ====
```

---

**🏁 SUB-HOLONS ANALISE DECLARADOS COMPLETOS — 2026-04-24**

*Padrão adapter standalone agora aplicado em 4 níveis de granularidade
dentro do projeto analise: raiz (kazuba-geo-engine) + 3 sub-projetos
(process_analysis, external_apis, kazuba-rust-core). 8 holons total
invocáveis via `holon invoke`. Zero regressões, zero débitos.*

*THSF prova que sub-holons aninhados são descobertos pelo walker
default (depth=6) sem hack: `analise/scripts/X/.holon/` em depth=4
e `analise/packages/Y/.holon/` em depth=4 ambos visíveis sob
`--root ~/projects`.*
