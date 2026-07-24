# THSF Fase 3 — Cap'n Proto Typed Federation: Kickoff Report

**Data**: 2026-04-23
**Sessão**: continuação direta da entrega de Fases 1+2
**Status Fase 3**: D3.1 entregue; D3.2/D3.3/D3.4 especificadas para próxima sessão

---

## 1. Contexto

Fases 1+2 do THSF entregaram o baseline operacional (30 holons, 30/30
handshakes aceitos, CRDT logging, dashboard). Esta fase introduz o COMBO E
(Typed Federation via Cap'n Proto) como transport alternativo ao CLI
adapter, mirando três problemas mensuráveis da baseline:

| Limitação CLI (Fase 1+2) | Solução Fase 3 |
|---|---|
| `_dict_to_argv` leaky: dicts JSON → `--key value` não lida com positional args | RPC typed com structs fortes; args viram `Data` (JSON hoje, AnyPointer no futuro) |
| Latência subprocess ~12ms/call | Cap'n Proto RPC ~0.4ms/call (30× speedup projetado) |
| Overhead de parsing JSON em ambas pontas | Zero-copy archival + promise pipelining |

---

## 2. D3.1 — Schema `holon-core.capnp` (ENTREGUE)

**Path**: `~/.claude/tools/holon/schemas/capnp/holon-core.capnp`

**Artefatos**:
- File ID: `@0xa1b2c3d4e5f67890` (MSB set conforme Cap'n Proto conv.)
- 2 enums: `Adapter` (cli/capnp/wasm), `HandshakeStatus` (accepted/rejected)
- 7 structs: `Version`, `Capability`, `HolonInfo`, `InvokeRequest`,
  `InvokeResponse`, `HandshakeEdge`, `SymbiosisReport`
- 2 interfaces:
  - `Holon` — 3 métodos (listCapabilities, invoke, info)
  - `HolonRegistry` — 5 métodos (listHolons, getHolon, findByCapability, symbiosis, specVersion)

**Validação**: script Python caseiro confirmou:
- Balanço de chaves (11 pares)
- Numeração sequencial em todos os blocos (@0..@N)
- Decorators sintáticos corretos (`enum`, `struct`, `interface`)

**Validação completa (2026-04-23 post-install)**:
- `capnp compile -o-` → binário 14.9 KB gerado sem erros
- `capnp compile -ocapnp` → round-trip canonical format OK
- Type IDs gerados deterministicamente (ex: `Adapter @0xcae1cf5b566ed07a`)
- `capnp 1.0.1` instalado via `apt install capnproto libcapnp-dev`

**Decisões arquiteturais**:

1. **Localização do schema**: `~/.claude/tools/holon/schemas/capnp/` (fora
   do workspace Rust) preserva o princípio THSF de não-invasão. O crate
   `touring-capnp-server` (D3.2) carregará via `include_str!` com path
   absoluto ou via symlink em `crates/touring-capnp-server/schemas/`.

2. **Promise pipelining habilitado**: `HolonRegistry.getHolon` retorna
   `Holon` (capability-as-value). Cliente pode pipelinar:
   ```
   r.getHolon("touring-index").invoke(req)  # 1 round-trip only
   ```

3. **Args opaque em MVP**: `InvokeRequest.args` e `InvokeResponse.stdout`
   são `Data` (bytes). JSON encoding interno para MVP; migração para
   `AnyPointer` (structured) quando as capability schemas individuais
   forem portadas.

4. **Symbiosis server-side**: `HolonRegistry.symbiosis` expõe o mesmo
   cycle que `holon symbiosis` na CLI — permite daemons remotos rodarem
   symbiosis sob demanda.

5. **specVersion**: método de negociação forward-compat.

---

## 3. D3.2 — `touring-capnp-server` crate (PENDENTE, L, ~3 dias)

### Escopo

Criar novo crate Rust em `~/.claude/rust/crates/touring-capnp-server/`
implementando o servidor Cap'n Proto.

### Dependências (Cargo.toml)

```toml
[dependencies]
capnp = "0.19"
capnp-rpc = "0.19"
capnpc = "0.19"
tokio = { workspace = true, features = ["full"] }
# Re-exports do próprio workspace
touring-hooks = { path = "../touring-hooks" }
touring-index = { path = "../touring-index" }

[build-dependencies]
capnpc = "0.19"
```

### Arquivos a criar

| Path | Responsabilidade |
|---|---|
| `build.rs` | `capnpc::CompilerCommand::new().file(...).run()` |
| `src/lib.rs` | Re-exports públicos |
| `src/server.rs` | `TouringCapnpServer` struct impl de `HolonRegistry` |
| `src/holon_impl.rs` | `TouringHolonImpl` impl de `Holon` (delegate para `HookRuntime`) |
| `src/bin/touring-capnp.rs` | Binário daemon (UnixListener em `/run/user/$UID/holon/registry.sock`) |
| `tests/roundtrip_e2e.rs` | E2E: servidor in-process + cliente + 3+ métodos chamados |
| `Cargo.toml` | Deps + features `tokio_unstable` se necessário |

### Integração com existing

- **HookRuntime**: `invoke_capability` → `HookRuntime::cli_query_via_hook(...)`
  idempotente, sem tocar actor pattern
- **Symbol store**: listar Touring crates via `touring_index::…` ou via
  `discover_holons` chamado diretamente (preferível — mantém simetria com
  o Python path)

### Sockets

Default: `/run/user/$UID/holon/registry.sock`. Criado em startup;
`systemd socket activation` como bonus (opcional).

### Testes target

- `test_server_starts_and_stops`
- `test_list_holons_returns_30`
- `test_get_holon_touring_master_returns_valid_cap`
- `test_invoke_symbol_index_pipelined` (1 RTT via promise pipelining)
- `test_symbiosis_reports_accepted_count`
- `test_spec_version_matches_schema_constant`

---

## 4. D3.3 — Clientes Python (PENDENTE, M, ~2 dias)

### Escopo

Dois clientes Python demonstrando consumo real:

#### 4.1 `~/.claude/tools/holon/clients/py/holon_capnp_client.py`

Cliente genérico (pycapnp) que faz:
- Conecta a `/run/user/$UID/holon/registry.sock`
- Wrapping de alto nível: `HolonCapnpClient.invoke(holon, cap, args)`
- Promise pipelining expose

#### 4.2 Usuário em `/home/gabrielgadea/projects/analise/`

Novo helper em `analise/src/kazuba_geo_engine/integrations/holon_capnp.py`:
- Consome `symbol-index` via capnp client
- Compara latency com `subprocess.check_output(["holon", "invoke", ...])`

#### 4.3 Usuário em `/home/gabrielgadea/projects/claude-trading/`

Novo helper em `claude_trading/integrations/holon_capnp.py`:
- Consome `mcts-planner` via capnp client
- Demonstra promise pipelining:
  ```python
  holon = registry.getHolon("touring-master")  # returns promise
  result = holon.invoke(...)  # pipelines on previous
  # Single RTT
  ```

### Dependências Python

```
pycapnp>=2.0
```

Instalar via venv local do projeto — NÃO globalmente (princípio THSF).

---

## 5. D3.4 — Benchmark comparativo (PENDENTE, S, ~1 dia)

### Script

`~/.claude/tools/holon/scripts/bench_symbiosis.py` que:

1. Invoca `symbol-index` 1000× via:
   - **fs-baseline** (CLI subprocess — Fase 1 atual)
   - **capnp** (Fase 3 — pycapnp client)
   - **mcp (ref)** (opcional — se MCP server estiver disponível como controle)

2. Mede P50, P95, P99 latency por transport
3. Produz relatório markdown em `docs/2026-04-XX-thsf-bench-results.md`

### Target (per plano mestre)

```
fs-baseline:  12.3 ms/call
capnp:         0.4 ms/call (30× speedup)
mcp (ref):     8.1 ms/call (baseline de mercado)
```

### Critérios de aceitação

- capnp P50 < 1ms
- capnp P99 < 5ms
- 0 erros em 1000 calls
- Cap'n Proto cliente reutiliza conexão (não reconecta per-call)

---

## 6. Dependências e riscos

### R3.1 — `capnp`/`capnpc` binário ~~não presente~~ MITIGADO [RESOLVIDO 2026-04-23]

`capnp 1.0.1` instalado via `sudo apt-get install -y capnproto libcapnp-dev`.
Schema compila sem erros. Próxima sessão precisa apenas instalar
`capnpc-rust` via cargo (`cargo install capnpc`) para gerar stubs Rust
para touring-capnp-server crate.

### R3.2 — Schemas divergem entre server Rust e clientes Python [HIGH]

**Mitigação**: CI check que valida hash SHA-256 do `.capnp` em todas as
locations (server crate + clients dir + schemas canônico).

```bash
# pre-commit hook
sha256sum schemas/capnp/holon-core.capnp
sha256sum crates/touring-capnp-server/schemas/holon-core.capnp  # symlink idealmente
```

### R3.3 — Roundtrip Rust ↔ Python via pycapnp pode ter incompat [MED]

**Mitigação**: teste E2E obrigatório (`tests/python_interop_e2e.rs`)
invocando pycapnp como subprocess e validando I/O.

### R3.4 — tokio runtime conflict se touring-capnp-server usa runtime próprio [MED]

**Mitigação**: seguir o padrão de `touring-server/src/main.rs` — reutilizar
`#[tokio::main]` com features `full`. Não mexer no actor do HookRuntime
(usar apenas `cli_query_via_hook` que já é thread-safe).

---

## 7. Cronograma estimado

| Dia | Work |
|---|---|
| 1 | Install capnp CLI + `cargo new --lib touring-capnp-server` + build.rs |
| 2 | Implement `TouringHolonImpl::list_capabilities` + `::info` |
| 3 | Implement `TouringHolonImpl::invoke` (delegate to HookRuntime) + 6 tests |
| 4 | `HolonRegistry` impl + binário daemon + integration test |
| 5 | Python client (pycapnp) + smoke test vs Rust server |
| 6 | analise integration + claude-trading integration |
| 7 | Benchmark harness + 1000-call run + report markdown |
| 8-10 | Buffer: debugging, schema evolution, polish, docs |

**Total estimado**: 10 dias-pessoa. Consistente com plano mestre (XL).

---

## 8. Exit criteria Fase 3 completa

```bash
# Server daemon vivo
systemctl --user is-active touring-capnp.service  # active

# Python client invoca e retorna < 1ms
python3 holon_capnp_client.py touring-master symbol-index --bench
# → capnp p50=0.4ms, p95=0.8ms, p99=2.1ms

# Schemas sincronizados
./scripts/validate-schema-sync.sh
# → PASS (all locations hash-match)

# Benchmark report existe
ls docs/2026-*-thsf-bench-results.md
```

---

## 9. Comandos para próxima sessão (retomar Fase 3)

```bash
# 1. Instalar capnp
which capnp || sudo apt-get install -y capnproto libcapnp-dev

# 2. Validar compilação do schema (deve passar)
capnp compile -o /dev/null /home/gabrielgadea/.claude/tools/holon/schemas/capnp/holon-core.capnp

# 3. Gerar stubs Rust (preview)
capnp compile -orust /home/gabrielgadea/.claude/tools/holon/schemas/capnp/holon-core.capnp

# 4. Iniciar D3.2: bootstrap do crate
cd ~/.claude/rust
cargo new --lib crates/touring-capnp-server
# Editar Cargo.toml do workspace root → adicionar member

# 5. Copiar este kickoff + atualizar conforme implementação avança
```

---

## 10. Estado entregue nesta sessão

| Deliverable | Status |
|---|---|
| CLAUDE.md menção THSF (REFERENCES + FRAMEWORKS) | ✓ entregue |
| D3.1 schema `holon-core.capnp` | ✓ entregue |
| D3.1 validação estrutural + capnp compile | ✓ entregue (14.9 KB binário OK) |
| capnp CLI (1.0.1) instalado no ambiente | ✓ entregue |
| D3.2 crate `touring-capnp-server` | PENDING próxima sessão |
| D3.3 clientes Python (pycapnp) | PENDING próxima sessão |
| D3.4 benchmark vs fs-baseline | PENDING próxima sessão |

**Confidence score da especificação**: 0.90. O schema em si é estável;
o tempo estimado de D3.2-D3.4 tem margem porque a primeira vez integrando
Cap'n Proto no workspace pode revelar fricções (build.rs, tokio runtime).

**Próximo check-in**: após D3.2 completado, revisar se prazo permanece
dentro do envelope 10 dias.

---

*Autor: Claude Opus 4.7 em sessão colaborativa com Gabriel Gadea.*
*Autoridade final: Gabriel.*
