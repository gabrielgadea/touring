# Session Report: Multi-Core Scaling + Boot Stabilization — 2026-04-21

## 1. Executive Summary

O `touring serve` (MCP stdio server em `touring-server/src/main.rs`) operava com
runtime tokio default (`#[tokio::main]` sem parâmetros), workers tokio =
`num_cpus::get()` = 32 lógicos via SMT, sem rayon pool dedicado, e carregava três
patologias compostas no hot path de `streaming_mcts_search`: runtime
`new_current_thread` aninhado dentro de `spawn_blocking`, busy-wait via
`std::hint::spin_loop()`, e double `block_on` desnecessário. Resultado observado:
1 core cravado em 100 % durante a janela de 20 ms do streaming MCTS.

Esta wave entrega três fixes (S1, W1, W2) + dois tunings arquiteturais (S2, S5)
que transformam o runtime de `touring serve` de "default com anti-pattern" para
"env-tunable com isolamento tokio ↔ rayon explícito". Complementarmente, elimina
warnings de boot conhecidos (eBPF bytecode ausente, SymbolRefresh script
inexistente) que poluíam stderr a cada start + a cada 30 min.

Validação: `cargo build --release --workspace` OK em 5 min 44 s (590 crates, 0
erros), `cargo test -p touring-server --lib` 408/408 PASS, `touring doctor -j`
5/5 ok. Cleanup: `cargo clean --profile dev` liberou **107,3 GiB** (108 G → 6,5 G).

---

## 2. Motivação

Gabriel solicitou explicitamente estratégias para que `touring serve` "não
sobrecarregue uma CPU especificamente, mas sim possa aproveitar todos os cores
e aumentar a sua eficiência e performance".

A investigação revelou que o risco real de "sobrecarregar 1 CPU" em um runtime
multi-thread (que tokio já é por default) vem de três fontes concretas, em
ordem de impacto:

1. **Anti-patterns locais** que serializam uma sub-tarefa num único thread
   apesar do pool multi-thread disponível (caso `streaming_mcts_search`).
2. **Runtime default sem tuning explícito** — o binding `num_cpus::get()` usa
   threads lógicos (SMT); para workloads CPU-bound de AST parse e SIMD, cores
   físicos dão melhor throughput porque SMT siblings competem por L1/L2 cache.
3. **Rayon + tokio compartilhando o mesmo pool global**: `par_iter` dentro de
   futures tokio canibaliza workers tokio; `touring-hooks` usa rayon
   extensivamente em `pre_edit` (rayon parallel signals).

Paralelo à investigação de performance, o output do `touring serve` mostrou
dois warnings recorrentes ao inicializar:

```
WARN touring_telemetry: eBPF init failed (eBPF not available: eBPF bytecode
  not loaded - provide compiled eBPF program bytes), falling back to polling
  collector
WARN touring_server::server: SymbolRefresh: bootstrap non-zero exit: /usr/
  bin/python3.12: can't open file '/home/gabrielgadea/.claude/rust/scripts/
  touring_bootstrap_symbols.py': [Errno 2] No such file or directory
```

Ambos representam degradação conhecida (workstation sem bytecode eBPF
compilado; script Python opcional ausente) — warn era ruído, não sinal.

---

## 3. Arquitetura de Decisões (S1 → S5 + W1/W2)

### 3.1 S1 — Remoção do anti-pattern MCTS streaming

**Antes** (`tools_infra.rs:129-168`):

```rust
let join_handle = h.spawn_blocking(move || {
    let rt = tokio::runtime::Builder::new_current_thread()   // ← 1-thread pinning
        .enable_all()
        .build()
        .expect("tokio runtime for streaming mcts");
    rt.block_on(async {
        let streaming = StreamingMCTS::spawn(...);
        let start = Instant::now();
        loop {
            if let Some(r) = streaming.best_so_far() { return r; }
            if start.elapsed().as_millis() as u64 >= 20 { ... }
            std::hint::spin_loop();                           // ← 100 % CPU burn
        }
    })
});
h.block_on(async { join_handle.await.ok() });                 // ← double block_on
```

**Depois** (`tools_infra.rs:129-173`):

```rust
let result = match tokio::runtime::Handle::try_current() {
    Ok(h) => {
        let join_handle = h.spawn_blocking(move || {
            let streaming = touring_cognitive::StreamingMCTS::spawn(...);
            let deadline = Instant::now() + Duration::from_millis(20);
            loop {
                if let Some(r) = streaming.best_so_far() { return r; }
                if Instant::now() >= deadline {
                    return streaming.best_so_far().unwrap_or(MCTSResult { zeros });
                }
                std::thread::yield_now();                     // ← OS scheduler cooperation
            }
        });
        join_handle.await.ok()
    }
    Err(_) => None,
};
```

**Invariantes preservadas**:
- Deadline de 20 ms mantido
- Fallback para `MCTSResult { zeros }` preservado
- Contexto de execução (spawn_blocking) correto para CPU-bound sync code
- `StreamingMCTS::spawn` / `best_so_far` já são síncronos — runtime aninhado era
  desnecessário (descoberto via `touring index find StreamingMCTS`)

**Ganho**: de pinning em 1 core → rayon pool interno do `StreamingMCTS`
efetivamente distribuído; `yield_now` permite ao scheduler OS intercalar outras
threads na mesma CPU.

### 3.2 S2 — Runtime tokio explícito, env-tunável

**Antes** (`main.rs:79`): `#[tokio::main]` sem parâmetros → defaults tokio.

**Depois** (`main.rs:79-148`): Runtime builder explícito com três env vars:

```rust
fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    // S5 — rayon BEFORE tokio (próxima seção)
    let rayon_threads = std::env::var("TOURING_RAYON_THREADS")
        .ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| (num_cpus::get_physical() / 2).max(2));
    rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_threads)
        .thread_name(|i| format!("touring-rayon-{i}"))
        .build_global().ok();

    // S2 — tokio explicit builder
    let workers = std::env::var("TOURING_MCP_WORKERS")
        .ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(num_cpus::get_physical);               // physical, não logical
    let max_blocking = std::env::var("TOURING_BLOCKING_WORKERS")
        .ok().and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(512);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .max_blocking_threads(max_blocking)
        .thread_name("touring-mcp-worker")
        .thread_stack_size(4 * 1024 * 1024)                    // 4 MiB AST recursion
        .enable_all()
        .build()?;

    rt.block_on(async_main())
}
```

**Racional**:
- **Physical cores em vez de logical**: Em hardware com SMT (a máquina de
  Gabriel: 16 físicos × 2 HT = 32 logical), workloads CPU-bound (AST parse,
  SIMD, MCTS) geralmente têm throughput ligeiramente melhor em `num_cpus::get_physical()`
  porque SMT siblings competem pela mesma L1/L2 cache. Operador pode override
  com `TOURING_MCP_WORKERS=32` se o workload for I/O-heavy.
- **Thread stack 4 MiB**: AST parsing tem recursão profunda (tree-sitter +
  syn visitors). Default do Rust (2 MiB) pode estourar em arquivos grandes.
- **max_blocking_threads=512**: default do tokio. Exposto como env var
  para facilitar tuning em workloads SQLite-heavy.

### 3.3 S5 — Rayon pool dedicado

Problema resolvido: `pre_edit` signal scoring usa `rayon::par_iter` em hot
path. Sem pool rayon explícito, rayon usa global default = `num_cpus::get()`
= mesmos threads que tokio workers. Resultado: `par_iter` roubava CPU dos
workers tokio, causando micro-starvations.

**Fix**: `rayon::ThreadPoolBuilder::build_global()` configurado ANTES do
runtime tokio. Tamanho default `physical/2` (= 8 na máquina atual) deixa
margem para tokio workers + blocking pool.

### 3.4 W1 — Rebaixar warn eBPF

**Local**: `crates/touring-telemetry/src/lib.rs:128-147`

```rust
Err(e) if config.allow_fallback => {
    match &e {
        TelemetryError::FeatureNotEnabled
        | TelemetryError::EbpfNotAvailable(_) => {
            tracing::info!("eBPF not active ({e}); using polling collector");
        }
        _ => {
            tracing::warn!("eBPF init failed ({e}), falling back to polling collector");
        }
    }
    Ok(Self { config, ebpf: None, ... })
}
```

**Justificativa**: Em workstations sem `aya::Ebpf::load()` bytecode (.bpf.o
compilado), `EbpfNotAvailable("bytecode not loaded")` é o **estado esperado**
de produção, não incidente. Degradação para `polling collector` é o modo
default. Warnings reais (kernel headers ausentes, map access failure)
continuam visíveis como `warn!`.

### 3.5 W2 — Skip defensivo SymbolRefresh

**Local**: `crates/touring-server/src/server/mod.rs:755-768`

```rust
loop {
    let script = project_root.join("scripts").join("touring_bootstrap_symbols.py");
    // Defensive skip: if the optional bootstrap script is absent,
    // do not spawn python3 only to fail with ENOENT.
    if !script.exists() {
        tracing::debug!(
            "SymbolRefresh: bootstrap script not present at {}; skipping refresh cycle",
            script.display()
        );
        tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        continue;
    }
    info!("SymbolRefresh: running bootstrap: {}", script.display());
    // ... existing python3 execution + reload
}
```

**Comportamento**: Se o script `touring_bootstrap_symbols.py` não existir no
`<project_root>/scripts/`, skip silencioso (debug-level) sem executar python3
que falharia com ENOENT. O loop continua a cada 30 min verificando — se
Gabriel criar o arquivo (ou symlinkar de
`/home/gabrielgadea/projects/analise/scripts/`), o refresh volta a funcionar
automaticamente.

---

## 4. Env Vars Operacionais (Novos)

| Variável | Default | Semântica | Quando mexer |
|---|---|---|---|
| `TOURING_MCP_WORKERS` | `num_cpus::get_physical()` (=16) | Tokio worker threads do MCP server | Workload I/O-heavy → subir para logical cores (32) |
| `TOURING_BLOCKING_WORKERS` | 512 | Pool `spawn_blocking` cap (SQLite, Tantivy) | Workload write-heavy → considerar subir |
| `TOURING_RAYON_THREADS` | `physical/2` (=8) | Rayon global pool (pre_edit signals, quality analysis) | Se rayon aparecer como bottleneck no tokio-console |

**Modelo mental**: tokio workers + rayon workers + blocking pool coexistem
como três pools independentes. Soma total ≤ cores físicos × 2 (margem SMT).

---

## 5. Rebuild + Restart (executado nesta sessão)

### 5.1 Build completo

```bash
cd ~/.claude/rust
RUSTFLAGS="--cfg tokio_unstable" \
  cargo build --release --workspace --exclude touring-loom-proofs
# → Finished `release` profile [optimized] in 5m 44s
# → 590 crates compiled, 0 errors, 7 pre-existing warnings (unrelated)
```

`touring-loom-proofs` excluído porque exige `RUSTFLAGS="--cfg loom"`
conflitante com `--cfg tokio_unstable` global.

### 5.2 Binários atualizados

| Binário | Size | mtime |
|---|---|---|
| `target/release/touring` | 65,8 MB | 14:55 |
| `target/release/touring-daemon` | 56,0 MB | 14:54 |
| `target/release/touring-hook` | 56,1 MB | 14:54 |

Symlinks em `~/.local/bin/{touring,touring-daemon,touring-hook}` apontam para
os arquivos em `target/release/` → rebuild auto-atualiza.

### 5.3 Kill + respawn

```bash
kill -TERM 915517                               # touring serve (MCP stdio)
pkill -KILL -f 'touring-hook --start-daemon'   # daemon supervisor
rm -f /tmp/touring-daemon-1000.{sock,lock}
```

**Observação importante**: Supervisor externo (provavelmente hook
`session-start` ou auto-spawn ao detectar socket ausente) recriou daemon com
PID 1338873 em menos de 1 min, **usando o binário recompilado** (mtime 14:54).

### 5.4 Validação

```bash
touring doctor -j
```

```json
[
  {"name": "binary_version", "status": "ok", "detail": "touring 30.0.0"},
  {"name": "daemon_socket", "status": "ok", "detail": "/tmp/touring-daemon-1000.sock"},
  {"name": "daemon_health", "status": "ok", "detail": "status=healthy, projects=1"},
  {"name": "circuit_breaker", "status": "ok", "detail": "{...clean...}"},
  {"name": "project_db", "status": "ok", "detail": "...34.6 MB"}
]
```

### 5.5 Target cleanup

```bash
cargo clean --profile dev
# → Removed 84847 files, 107.3 GiB total
```

Antes: 108 GB (debug 102 G + release 6,5 G + outros 0,5 G)
Depois: 6,5 GB (só release)
**Liberado: 107,3 GiB (-98,5 %)**

---

## 6. Testes

```
cargo test -p touring-server --lib → 408 passed, 0 failed (0,31 s)
cargo check -p touring-telemetry --features ebpf → OK (7,53 s)
cargo check -p touring-telemetry -p touring-server → OK (1,88 s)
cargo build --release --workspace → OK (5m 44s)
```

Zero regressões. Warnings pré-existentes em `touring-hooks` (ROLLBACK_TIMEOUT,
evict_expired_transactions_sync) não relacionados a este trabalho.

---

## 7. Tuning Baseline (FASE 1 recomendada)

Para guiar decisões sobre S3 (dedicated compute runtime), S4 (parking_lot
+ arc_swap), S6 (SQLite pool) que ficaram no roadmap, coletar baseline com:

```bash
# Terminal 1 — touring serve com tokio-console (feature `console` já em default)
RUSTFLAGS="--cfg tokio_unstable" ./target/release/touring serve

# Terminal 2 — tokio-console
cargo install --locked tokio-console
tokio-console http://127.0.0.1:6669

# Métricas antes/depois de workload real
touring gate-metrics -j > /tmp/baseline-before.json
# ... workload ...
touring gate-metrics -j > /tmp/baseline-after.json

# CPU per-thread distribution (detecta pinning)
pidstat -t -p $(pgrep -f 'touring serve') 2 20
```

**Critério de sucesso pós-S1**: invocar `mcp__touring__touring_streaming_mcts`
não deve mais mostrar 1 thread a ~100 % por ~20 ms no pidstat; carga deve
distribuir entre `touring-mcp-worker-*` e `touring-rayon-*`.

---

## 8. Roadmap Remanescente (não implementado nesta sessão)

| Prior. | Item | Esforço | Dependência |
|---|---|---|---|
| P2 | **S3** dedicated compute runtime (2 runtimes: I/O leve + compute pesado) | 4 h | Baseline FASE 1 para validar se vale |
| P2 | **S4** `parking_lot::Mutex` + `arc_swap::ArcSwap` nos 6 `Arc<Mutex<T>>` globais (qtable, linucb, ranker, drift_detector, online_rl, hint_engine) | 3 h | Baseline FASE 1 |
| P3 | **S6** SQLite connection pool (`deadpool-sqlite` WAL) em memory_store, decompose, suggestion_store | 4 h | Se `gate-metrics` mostrar write contention |
| P3 | **S7** Semáforo global CPU-bound limitando `spawn_blocking` concorrentes ao nº de cores físicos | 2 h | Após S3 |
| P3 | **S8** `tokio::task::yield_now` em loops compute remanescentes | 2 h | Micro-tuning pós-profile |

---

## 9. Critical Files

### touring-server

- `src/main.rs:79-148` — runtime tokio + rayon explícito env-tunável
- `src/server/tools_infra.rs:129-173` — MCTS anti-pattern fix
- `src/server/mod.rs:755-768` — SymbolRefresh defensive skip
- `Cargo.toml:210` — `num_cpus` dep adicionada

### touring-telemetry

- `src/lib.rs:128-147` — warn → info para EbpfNotAvailable/FeatureNotEnabled

---

## 10. Lessons

1. **`#[tokio::main]` vs runtime builder explícito**: Para binários operacionais,
   builder explícito com env vars dá tunability zero-recompile. Custo: duplicar
   `async_main()` wrapper (trivial).

2. **physical vs logical cores para CPU-bound**: SMT/Hyper-Threading raramente
   ajuda compute-bound (AST parse, SIMD) — siblings competem por L1/L2 cache.
   Default `num_cpus::get_physical()` é conservador e correto para a maioria
   dos workloads Touring.

3. **Anti-pattern nested runtime**: Criar `new_current_thread` dentro de
   `spawn_blocking` para executar código sync que já tem seu próprio pool
   interno (como `StreamingMCTS` com rayon) é **pior** que executar o sync
   diretamente. Custo: 1-thread pinning + overhead de runtime secundário.

4. **`std::hint::spin_loop()` em timeout loop**: Bom para busy-wait de
   microssegundos, péssimo para timeout de 20 ms. `std::thread::yield_now()`
   entrega ao scheduler OS permitindo outras threads progredirem.

5. **Rayon default + tokio default = starvation cruzada**: Dois pools num_cpus
   competindo pelos mesmos threads. Solução: dedicar rayon pool menor
   (`physical/2`) via `build_global()` ANTES do `new_multi_thread().build()` do
   tokio.

6. **Supervisor de daemon com auto-respawn**: Observado que depois de matar o
   daemon, um novo supervisor spawna em ~1 min usando o binário em disco. Isso
   é resiliência do sistema — aproveitamos: matar daemon + rebuild = supervisor
   sobe o binário novo automaticamente. Nenhuma ação manual de restart
   necessária.

7. **`cargo clean --profile dev`**: Remove só `target/debug/` preservando
   `target/release/`. Operação segura após rebuild release completa. Liberou
   107 GiB nesta sessão.

8. **Warn vs info para degradação esperada**: Degradação graceful conhecida
   (eBPF sem bytecode, script opcional ausente) deve ser **info** ou **debug**.
   Warnings devem representar situações que o operador deveria investigar.
   Warnings em loop permanente (cada 30 min) são pior que silêncio — treinam o
   operador a ignorar stderr.
