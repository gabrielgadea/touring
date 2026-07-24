# THSF Fase 4 Wave 4D — Integration + pilot + bench

> **Data**: 2026-04-24  |  **Status**: 4D ENTREGUE ✅  |  **Próximo**: 4E (consolidation)
>
> **Resumo**: `holon invoke` agora dispatcha por `transport` (cli/wasm/capnp);
> pilot provider `holon-wasm-components` expõe as 3 capabilities via WASM e
> é descoberto no symbiosis cycle sem erros; bench comparativo amplia D3.4
> com o 4º transport WASM (**P50 = 12 ms**, 4× mais rápido que fs.subprocess).

---

## 1. Deliverables

| ID | Deliverable | Status | Evidência |
|---|---|---|---|
| 4D.1 | Manifest v2 schema (`transport` + `wasm_component` em offers) | ✅ | Schema JSON já suportava `adapter=wasm` desde Fase 1 (preparado pelo autor inicial) — zero mudanças necessárias |
| 4D.2 | `holon invoke` WASM adapter (dispatch por transport em `holon.py`) | ✅ | `_invoke_wasm()` novo + dispatch em `invoke_capability()` |
| 4D.3 | Pilot real — holon provider `holon-wasm-components` descoberto | ✅ | `holon doctor` 0 errors + `holon invoke` funciona E2E para 3 capabilities |
| 4D.4 | Bench comparativo WASM vs CLI vs capnp (extensão D3.4) | ✅ | `bench_d34.py` novo cenário `wasm.subprocess`; P50 = 12.1 ms |

---

## 2. `holon invoke` dispatch por transport

### 2.1 Mudança em `holon.py`

A função `invoke_capability()` foi refatorada para dispatch (antes hard-coded para CLI):

```python
if offer.adapter == "cli":
    exit_code, stdout, stderr = _invoke_cli(offer, capability, args_obj, timeout_s)
elif offer.adapter == "wasm":
    exit_code, stdout, stderr = _invoke_wasm(target, offer, capability, args_json, timeout_s)
elif offer.adapter == "capnp":
    raise HolonError("use clients/py/holon_capnp_client.py")  # fora de escopo
else:
    raise HolonError(f"Unknown adapter: {offer.adapter!r}")
```

### 2.2 `_invoke_wasm` implementação

3 passos:
1. Resolver `offer.wasm_component` relativo ao `manifest.root/.holon/` (ou absoluto).
2. Localizar `holon-wasm-runner` via env `HOLON_WASM_RUNNER` ou fallback hardcoded.
3. Spawnar: `holon-wasm-runner <wasm_path> invoke <capability> <args_json>`.

Output do runner é parseado:
- `{"ok": {exit-code, stdout: [bytes], stderr: [bytes], ...}}` → decode bytes UTF-8 → tuple.
- `{"err": {case, payload}}` → converter para stderr + exit_code=1.

### 2.3 Resposta enriquecida

O envelope de resposta agora inclui `transport: "cli" | "wasm" | "capnp"` para observabilidade:

```json
{
  "holon": "holon-wasm-components",
  "capability": "spec-version",
  "exit_code": 0,
  "stdout": "{\"spec_version\":\"0.1.0\"}",
  "stderr": "",
  "requester": "wave4d-pilot",
  "logged": true,
  "transport": "wasm"
}
```

---

## 3. Pilot — `holon-wasm-components` holon provider

### 3.1 Manifest criado

`holon-wasm-components/.holon/manifest.toml` declara 3 offers via
`adapter = "wasm"`:

```toml
[holon.identity]
name = "holon-wasm-components"
version = "0.1.0"
autonomy_guarantee = true

[holon.offers.spec-version]
adapter = "wasm"
wasm_component = "../target/wasm32-wasip2/release/holon_spec_version.wasm"

[holon.offers.blast-radius]
adapter = "wasm"
wasm_component = "../target/wasm32-wasip2/release/holon_blast_radius.wasm"

[holon.offers.quality-gate]
adapter = "wasm"
wasm_component = "../target/wasm32-wasip2/release/holon_quality_gate.wasm"
```

### 3.2 Validation E2E (3 capabilities)

```bash
$ holon doctor /home/gabrielgadea/.claude/rust/holon-wasm-components
{ "total_issues": 0, "errors": 0, "warnings": 0 }

$ holon invoke --root .../holon-wasm-components holon-wasm-components spec-version '{}'
{"transport":"wasm","stdout":"{\"spec_version\":\"0.1.0\"}","exit_code":0,...}

$ holon invoke ... holon-wasm-components blast-radius '{"graph":{...},"target":"c.rs"}'
{"transport":"wasm","stdout":"{\"target\":\"c.rs\",\"blast_radius\":3,...}","exit_code":0}

$ holon invoke ... holon-wasm-components quality-gate '{"source":"fn clean() -> i32 { 42 }","lang":"rust"}'
{"transport":"wasm","stdout":"{\"score\":1.0,\"total_antipatterns\":0,...}","exit_code":0}
```

### 3.3 Symbiosis integration

O novo holon é descoberto automaticamente em:

```bash
holon symbiosis /home/gabrielgadea
```

sem erros. `konverter.requires.quality-gate` continua satisfeito (o
symbiosis existente já matcha via `touring-master` — agora há um **segundo
provider** disponível via WASM transport, selecionável por requester
especificando `holon-wasm-components` explicitamente). Invariante
**konverter permanece não-invadido**: zero alterações em arquivos do
projeto (só o `.holon/manifest.toml` pré-existente).

---

## 4. Bench comparativo (4D.4)

### 4.1 Extensão de `bench_d34.py`

Novo cenário `wasm.subprocess` via função `_scenario_wasm_subprocess()`.
CLI args novos:

```
--wasm-runner <path>       (default: holon-wasm-components/target/release/holon-wasm-runner)
--wasm-component <path>    (default: .../holon_spec_version.wasm)
--wasm-capability <name>   (default: "spec-version")
--wasm-args <json-string>  (default: "{}")
```

Cenário adicionado ao default `--scenarios` list. Roda com
`min(runs, invoke_runs)` para respeitar bounds de fork-dominated workload.

### 4.2 Resultados completos (N=300, warmup=50, invoke_runs=50)

| Transport | Runner | P50 | Notas |
|---|---|---:|---|
| capnp.spec_version | Rust (persistent UDS)   |   **11 μs** | protocol floor |
| capnp.spec_version | Python (pycapnp)        |   **28 μs** | +17 μs binding |
| capnp.list_holons  | Rust                    |   51 μs | RPC + walkdir |
| capnp.list_holons  | Python                  |  128 μs | +77 μs binding |
| **wasm.subprocess**    | **Python (fork+runner)** | **12 112 μs** (≈ 12 ms) | **fork + wasmtime init + invoke** |
| fs.subprocess      | Python (`holon discover`) | 48 460 μs (≈ 48 ms) | fork + Python startup + walk |
| capnp.invoke (e2e) | Rust                    | 48 639 μs (≈ 49 ms) | capnp + server subprocess |
| capnp.invoke (e2e) | Python                  | 51 018 μs (≈ 51 ms) | capnp + pycapnp + server |

### 4.3 Insights [confidence tags]

**[FACT 1.0]** WASM subprocess (12.1 ms) é **4× mais rápido** que
fs.subprocess (48.5 ms). Razões:
- WASM runner é binário Rust compilado, inicialização instantânea.
- fs.subprocess carrega o Python interpreter + parseia TOML manifests.

**[FACT 1.0]** WASM subprocess é **~1000× mais lento** que capnp.
Razões:
- capnp: conexão UDS persistente, ~10-50 μs por call.
- WASM cold: fork + exec + wasmtime Engine::new + Component::from_file + invoke.

**[INFERENCE 0.9]** Em embedding warm-path (consumer embeds wasmtime
e pré-instancia componentes), WASM invoke cairia para sub-ms. O
subprocess path é pessimista — representa "invoke isolado vindo de
processo externo". Consumidores long-running verão latência muito
menor.

**[INFERENCE 0.85]** WASM **não compete com capnp em latência**; compete
com CLI subprocess. O trade-off é: WASM troca ~3× latência (capnp.invoke
~50 ms vs WASM 12 ms) por **isolamento sandbox + portabilidade
cross-language** (um .wasm binário roda em Go, Zig, JS, etc via
`wasmtime` engine embutido).

### 4.4 Decision matrix para escolha de transport

| Use case | Transport recomendado |
|---|---|
| Discovery-heavy (list + find + info em loop) | **capnp** (~10-50 μs) |
| Long-running consumer embarcando runtime | capnp se Rust/Python; **WASM** se outras linguagens |
| Subprocess tooling (one-shot CLI invocations) | **WASM** (12 ms) > CLI (48 ms) |
| Sandbox isolation requerido (multi-tenant, untrusted code) | **WASM** (wasmtime sandbox + fuel limits) |
| Cross-language library | **WASM** (WIT bindings auto-gerados) |
| Legacy integration (adapter=cli obrigatório) | CLI subprocess |

---

## 5. Arquivos tocados (Wave 4D)

| Path | Status | Propósito |
|---|---|---|
| `tools/holon/holon.py` | EDIT | Novas funções `_invoke_cli` + `_invoke_wasm`; `invoke_capability` passa a dispatchar |
| `rust/holon-wasm-components/.holon/manifest.toml` | CREATE | Pilot provider manifest |
| `tools/holon/benchmarks/bench_d34.py` | EDIT | 5º cenário `wasm.subprocess` + 4 args CLI novos |
| `tools/holon/benchmarks/bench_d34.sh` | EDIT | Summary row inclui `wasm.p50` |
| `rust/docs/2026-04-24-thsf-fase4-wave4d.md` | CREATE | Este relatório |
| `CLAUDE.md` | EDIT | Fase 4 → 80% |

**Alterações em `crates/` Touring workspace**: **zero**.
**Alterações em `/home/gabrielgadea/projects/konverter/`**: **zero**.

---

## 6. Invariantes THSF preservadas

- `autonomy_guarantee=true`: ✅ (holon-wasm-components + konverter)
- Reversibilidade: `rm .holon/manifest.toml` desativa o pilot.
- Zero-invasão em konverter: manifest pré-existente em `/projects/konverter/.holon/` intocado.
- Touring workspace: zero rebuilds necessários para Fase 4.

---

## 7. Próximos passos (Wave 4E)

| Task | Tipo | Size |
|---|---|---|
| Consolidar CHANGELOG Fase 4 | docs | S |
| Exit criteria checklist | docs | S |
| Memory index final | docs | S |

Wave 4E é essencialmente documentação. Fase 4 pode ser declarada **COMPLETA**
após 4E. Ou Gabriel pode preferir adicionar stretch goals (pilot real em
konverter invocando via `holon invoke konverter → quality-gate → usando
código real do projeto; WASI 0.3 wave 4F; symbol-index port, etc.).

---

**Fase 4 em 80% (4/5 waves entregues).**
