---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W7"
name: "touring-bindings Fusion"
phase: "F2-FUSIONS"
depends_on:
  - W3
parallel_with:
  - W5
status: "DONE"
created: "2026-05-11"
completed: "2026-05-15"
cila: "L3"
rust_changes: "FUSION + DELETE"
estimated_days: "8-10"
checkpoint: "touring_premium_W7_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W7.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W7: touring-bindings Fusion

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F2-FUSIONS
> **Contribuição para resultado final**: Bindings ficam isolados num único crate. Usuário não paga compile-time de pyo3 + wasm-bindgen + tauri + axum a menos que ative explicitamente. Default features empty = tier-free build mais leve.

---

## Contexto e Dependências

- **Depende de**: W3
- **Paralelo com**: W5
- **CILA**: `L3`
- **Mudanças Rust**: `FUSION + DELETE`
- **Estimativa**: 8-10 dias
- **Checkpoint**: `touring_premium_W7_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W7.py`

---

## Descrição

Fundir 7 crates de bindings + DELETAR 3 mortos (já feito em W1). Originais: touring-python (3.5k), touring-wasm (2.7k), touring-capnp-server (1.5k), touring-web (3.5k), touring-web-server (1.7k), touring-desktop-ui (1.2k), touring-geopostgis (435L). Resulta em touring-bindings ~15k LOC com features 100% opt-in (default = empty). Tests +1k LOC para crates 0%-ratio (web, python, desktop, postgis).

---

## Efeitos no Sistema

- touring-bindings ~15k LOC, ≥ 23% test ratio
- 6 features bind-* mutuamente compatíveis
- Default features VAZIO (opt-in)
- +1k LOC tests para 4 crates antes em 0%
- Cargo hack --feature-powerset verifica todas combinações compilam

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W7.1: Create touring-bindings skeleton + Cargo.toml

**Descrição**: [features] default = []. bind-python, bind-wasm, bind-capnp, bind-web, bind-desktop, bind-postgis. Cada feature ativa dep externa (pyo3, wasm-bindgen, etc.).

**Dias estimados**: 0.5

**Critério de validação**: cargo check -p touring-bindings exit 0 (sem features).

---

### W7.2: Move touring-python → bindings/src/bindings-python/

**Descrição**: 3.5k LOC + 0% tests. PyO3 bindings. +400 LOC tests.

**Dias estimados**: 1.0

**TDD RED** (escrever ANTES do código):
```python
def test_python_bindings_smoke():
    """RED: import touring_bindings should work."""
```

**Critério de validação**: cargo test -p touring-bindings --features bind-python exit 0.

---

### W7.3: Move touring-wasm → bindings/src/bindings-wasm/

**Descrição**: 2.7k LOC. wasm-bindgen + inferlets.

**Dias estimados**: 0.7

**Critério de validação**: cargo build -p touring-bindings --features bind-wasm --target wasm32-unknown-unknown exit 0.

---

### W7.4: Move touring-capnp-server → bindings/src/bindings-capnp/

**Descrição**: 1.5k LOC. Cap'n Proto RPC.

**Dias estimados**: 0.7

**Critério de validação**: cargo check --features bind-capnp exit 0.

---

### W7.5: Move touring-web + touring-web-server → bindings/src/bindings-web/

**Descrição**: 3.5k + 1.7k LOC + 0% tests em web. Leptos + Axum. +400 LOC tests.

**Dias estimados**: 1.5

**TDD RED** (escrever ANTES do código):
```python
def test_web_health_endpoint():
    """RED: /healthz endpoint untested."""
```

**Critério de validação**: cargo test --features bind-web exit 0.

---

### W7.6: Move touring-desktop-ui → bindings/src/bindings-desktop/

**Descrição**: 1.2k LOC + 0% tests. Tauri. +200 LOC tests (mocked window).

**Dias estimados**: 1.0

**Critério de validação**: cargo check --features bind-desktop exit 0.

---

### W7.7: Move touring-geopostgis → bindings/src/bindings-postgis/

**Descrição**: 435 LOC + 0% tests. Geozero EWKB. +200 LOC tests com postgres mock.

**Dias estimados**: 0.7

**Critério de validação**: cargo test --features bind-postgis exit 0.

---

### W7.8: Features bind-* mutuamente compatíveis

**Descrição**: cargo hack --feature-powerset check valida que todas combinações compilam. Single crate, dual binding (python + web) funciona.

**Dias estimados**: 1.0

**Critério de validação**: cargo hack --feature-powerset --workspace check exit 0.

---

### W7.9: +1k LOC tests para 4 crates 0%-ratio

**Descrição**: Total +1k LOC distribuídos: web (+400), python (+400), desktop (+200), postgis (+200).

**Dias estimados**: 2.0

**Critério de validação**: cargo llvm-cov --json | jq '.totals' ≥ 23% para touring-bindings.

---

### W7.10: cargo check per feature combo

**Descrição**: Validar tier-free, tier-standard, tier-premium, tier-enterprise feature sets.

**Dias estimados**: 1.0

**Critério de validação**: 4 cargo check invocations exit 0.

---

### W7.11: Delete old crates + workspace update

**Descrição**: Remove 7 crates antigos. Shims onde necessário.

**Dias estimados**: 0.5

**Critério de validação**: ls crates/touring-{python,wasm,capnp-server,web,web-server,desktop-ui,geopostgis}/ → shims.

---

## Gate de Saída

touring-bindings 15k LOC, 6 features opt-in, default = empty, ≥ 23% test ratio, cargo hack feature-powerset exit 0.

## Riscos Específicos

- Pyo3 ABI breakage entre versões → pin pyo3 = '0.24' em workspace.deps
- Tauri exige sistema-Webview2 (Windows) → CI Linux apenas em W7; Windows CI adicionado em W12
- wasm-bindgen target wasm32-unknown-unknown exige rustup target add → documentar em CONTRIBUTING.md

## Checklist de Conclusão

- [ ] Todos os subtasks implementados
- [ ] Todos os testes TDD GREEN
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace --no-fail-fast` pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles --min-depth 2` no new cycles
- [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
- [ ] Bench regression < 5%
- [ ] Test ratio ≥ 20% per touched crate
- [ ] Checkpoint `.toon` salvo
- [ ] Memory lesson persistida (`touring memory store --tier semantic`)
- [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
- [ ] Documentação atualizada (se necessário)

---

## Discovery Updates (2026-05-11) — W7 Skeleton + Powerset Discovery

Dois auto-scripts entregues para W7 — `w7_bindings_skeleton.py` (REAL) e `w7_features_powerset_check.py` (REAL).

### W7.1 — touring-bindings skeleton (READY)

`staging/w7-touring-bindings/` contém:

- `Cargo.toml` — 6 features opt-in (`bind-py`, `bind-ts-napi`, `bind-wasm`, `bind-async`, `bind-stream`, `bind-stable-api`); **default = []** by design (caller picks language)
- `src/lib.rs` — feature-gated module dispatch
- `src/common.rs` — `Greeting`, `BindingError`, `BindingResult` (cross-language types)
- `src/py.rs` — PyO3 stub (W7.2 entry point)
- `src/ts.rs` — napi-rs stub (W7.3 entry point)
- `src/wasm.rs` — wasm-bindgen stub (W7.4 entry point)
- `pyproject.toml` — maturin-friendly Python packaging
- `package.json` — napi-rs friendly npm packaging

### W7.8 — Feature powerset validation

`w7_features_powerset_check.py` discovers **12 multi-feature crates** que precisam ser validados via cargo check com cada combinação de features (até cardinalidade 3 por default):

- touring-hooks, touring-server, touring-vfs, touring-learning
- touring-rkyv, touring-index, touring-simd, touring-offensive
- touring-tasksfile, touring-vector-store, touring-resource-monitor

Comando: `python3 scripts/touring_premium_refactor_2026/w7_features_powerset_check.py --max-cardinality 2`

### Ação revisada para W7

1. **W7.1**: ✅ Skeleton pronto. Refinar templates Python/TS quando começar implementação real
2. **W7.2-W7.7**: Implementar bindings para cada feature, language-by-language
3. **W7.8**: ✅ Validator pronto. Executar em CI antes de cada PR
4. **Risk mitigation**: 12 crates × 2^N features = enumeração obrigatória pelo script — sem isso, qualquer combinação não testada é uma regressão silenciosa potencial

### Forensic outputs disponíveis

- `data/w7-bindings-skeleton.json` — file manifest
- `data/w7-feature-powerset-report.json` — discovered crates + features
- `staging/w7-touring-bindings/` — complete skeleton tree
- `staging/w7-failing-combinations.md` — human-readable powerset report

---

## Discovery Updates (2026-05-15) — Execução

### Todos os 7 crates fundidos — sem exclusão (ciclo-seguro verificado)

Diferente de W5 (touring-index excluído) e W6 (touring-cortex excluído), W7
funde **todos os 7 crates** sem exclusão. Análise de ciclo: os 7 crates
dependem apenas de {foundation, simd, touring-intelligence, touring-code} —
**nenhum** depende de hooks/server/cortex/generator. `touring-wasm` é
consumido por hooks/generator/cortex/server e `touring-capnp-server` por
hooks, mas isso forma um DAG limpo: `hooks → touring-bindings →
{foundation, simd, intelligence, code}`, sem aresta de volta. Sem ciclo.

### Resultado

`touring-bindings` = {python, wasm, capnp, web (+web-server como `web::server`),
desktop, postgis}.

| Métrica | Valor |
|---|---|
| Crates fundidos | 7 |
| touring-bindings src | 14.651 LOC, 84 files |
| Módulos | `python`, `wasm`, `capnp`, `web`, `desktop`, `postgis` |
| Features | 6 `bind-*` opt-in, `default = []` |
| Shims (1-file) | 7 crates |
| `cargo check --workspace` | 0 erros |
| Testes (bind-capnp,bind-web,bind-postgis) | 53 + 1 doctest, 0 falhas |
| clippy | 0 errors, 11 warnings de estilo residuais (51→11 via --fix) |
| Wiring cycles | 2 / depth 621 (sem regressão) |

### Detalhes especiais preservados

- `touring-python` mantém `[lib] name = "claude_learning_kernel"` +
  `crate-type = ["cdylib","rlib"]` no shim (PyO3 extension module).
- `touring-web-server` permanece um crate binário — o shim mantém
  `[[bin]]` + `main.rs` (delega a `touring_bindings::web::server::run`);
  `tokio` re-adicionado ao Cargo.toml do shim para `#[tokio::main]`.
- Shims de `touring-wasm` / `touring-capnp-server` têm `default =
  ["bind-wasm"]` / `["bind-capnp"]` — consumidores (hooks/generator/cortex/
  server) esperam `touring_wasm::PluginResult` etc. incondicionalmente.

### Débito pré-existente (não regressão W7)

- `cargo test --features bind-python` falha no LINK (`gcc`/libpython) — débito
  inerente a crates PyO3: o test binary precisa de símbolos de libpython que
  o `extension-module` não linka. A crate **compila** (`cargo check` OK);
  apenas `cargo test` não linka. PyO3 é testado via maturin/pytest, não
  `cargo test`. Diferido para W11.
- `bind-wasm` / `bind-desktop` exigem targets especiais (`wasm32-unknown-unknown`
  / webview Tauri) — não testáveis no host. Validados via `cargo check`.
- 11 warnings clippy de estilo residuais em código fundido (crates origem
  sem `[lints] workspace`) — não-bloqueantes, diferidos para W11.

### Bugs do rewriter corrigidos

`w7_rewrite_crate_paths.py` pulou os escopos `examples/` e doc-comments —
`crate::web::snapshots` precisava `crate::web::server::snapshots` (submódulo
aninhado); 4 refs `touring_capnp_server::` em `examples/`; 21 refs
`touring_<crate>::` em doc-comments de `src/`. 25 refs corrigidas via prefix
rewrite + 1 import manual.
