# 🔬 Exploração Completa: `chopratejas/headroom` + Documentação

**Modo:** Ultrathink + Sequential Thinking (8 pensamentos estruturados)
**Fontes primárias:** GitHub API + raw files + docs site (23 fetches paralelos)
**Data:** 21/06/2026

---

## 1. 📊 Visão Geral & Estado do Projeto

| Métrica | Valor |
|---|---|
| **Nome do pacote** | `headroom-ai` (Python) / `headroom-ai` (npm) |
| **Versão atual** | v0.26.0 (16/06/2026) |
| **Total de releases** | 156 |
| **Stars** | 43.944 |
| **Forks** | 3.056 |
| **Open issues** | 364 |
| **Watchers** | 139 |
| **Licença** | Apache 2.0 |
| **Criado em** | 07/01/2026 (5 meses de existência) |
| **Último push** | 21/06/2026 (hoje — dia de intensa atividade) |
| **Linguagens** | Python 78.8% · Rust 16.7% · TypeScript 2.4% · HTML 1.1% · Shell 0.4% · PowerShell 0.4% |
| **Tamanho do repo** | 55.218 KB (≈54 MB) |
| **Python support** | 3.10 – 3.14 (incluindo 3.14 via PyO3 abi3) |
| **Maintainer** | `chopratejas` (Rejas Chopra) + "Headroom Contributors" |
| **Modelo ML próprio** | [Kompress-v2-base](https://huggingface.co/chopratejas/kompress-v2-base) no HuggingFace |
| **Discord** | https://discord.gg/yRmaUNpsPJ |

**Diagnóstico do autor (do CHANGELOG não-released):** Telemetria mudou de opt-out para **opt-in** (off by default) — decisão recente de governança.

---

## 2. 🎯 O Que é Headroom (One-Liner + Tagline)

> **"The context compression layer for AI agents."**
>
> Headroom comprime **tudo que um agente de IA lê** (tool outputs, logs, arquivos, RAG chunks, file reads, API responses) **antes** de chegar ao LLM. **Mesma resposta, fração dos tokens.** 60–95% de redução.

**Modos de uso (4):**
1. **Library** — `from headroom import compress` (Python) ou `import { compress }` (TypeScript)
2. **Proxy** — `headroom proxy --port 8787`, zero code change, qualquer cliente OpenAI-compatible
3. **Agent wrap** — `headroom wrap claude|codex|cursor|aider|copilot` (transparently proxies the agent)
4. **MCP server** — `headroom_compress`, `headroom_retrieve`, `headroom_stats` (qualquer MCP client)

**Três claims competitivos únicos:**
- 🟢 **Reversible** (CCR — Compress-Cache-Retrieve): originais nunca são deletados, LLM pode requisitar via `headroom_retrieve` tool
- 🟢 **Local-first**: roda 100% local, dados não saem da máquina
- 🟢 **Universal**: cobre TUDO (tools, RAG, logs, files, history) — concorrentes cobrem só um subset

---

## 3. 🏛️ Arquitetura (Deep Dive)

### 3.1 Pipeline Canônico de Compressão (3 estágios)

```
┌────────────────────────────────────────────────────┐
│  1. CacheAligner    (sub-ms)                       │
│     Extrai conteúdo dinâmico (datas, UUIDs, etc.)  │
│     do system prompt → move para o final          │
│     → Estabiliza prefixo para KV cache hit         │
├────────────────────────────────────────────────────┤
│  2. ContentRouter   (11.7ms P50 / 259ms P90)      │
│     Detecta tipo de conteúdo (Magika ML + pattern) │
│     Roteia para compressor especializado           │
│     ├─ SmartCrusher (JSON arrays)                  │
│     ├─ CodeCompressor (AST tree-sitter)            │
│     ├─ LogCompressor (pattern clustering)          │
│     ├─ SearchCompressor (BM25)                     │
│     ├─ DiffCompressor (hunks)                      │
│     ├─ HTMLExtractor (trafilatura)                 │
│     └─ Kompress (text, ONNX/ModernBERT)            │
├────────────────────────────────────────────────────┤
│  3. ContextManager  (sub-ms)                       │
│     Garante que o array final cabe no context window│
│     ├─ RollingWindow (default, dropa oldest)       │
│     └─ IntelligentContext (advanced, 6-factor score)│
└────────────────────────────────────────────────────┘
```

### 3.2 Lifecycle (11 Estágios Observáveis — via `on_pipeline_event()`)

```
Setup → Pre-Start → Post-Start → Input Received → Input Cached
→ Input Routed → Input Compressed → Input Remembered
→ Pre-Send → Post-Send → Response Received
```

> **Distinção importante:** os **3 estágios** são os **transforms** (fazem trabalho); os **11 estágios** são eventos de **lifecycle** (observáveis, com hooks para extensão).

### 3.3 Componentes Principais

| Componente | Função | Custo |
|---|---|---|
| **CacheAligner** | Estabiliza prefixo p/ cache hit | <1ms |
| **ContentRouter** | Detecção + roteamento | 11.7ms P50, 91-98% do pipeline cost |
| **SmartCrusher** | JSON arrays (sweetspot 70-90%) | 50ms |
| **CodeCompressor** | AST-aware (Python/JS/TS/Go/Rust/Java/C/C++) | 10-50ms |
| **Kompress** | Texto via ONNX (ModernBERT) | 32ms P50 / 576ms P90 |
| **RollingWindow** | Drop oldest (sub-ms) | <1ms |
| **IntelligentContext** | Multi-factor scoring (6 dims) | depende config |
| **CCR** (Compress-Cache-Retrieve) | Cache local + `headroom_retrieve` tool | 1ms lookup |
| **TOIN** (Tool Output Intelligence Network) | Aprende padrões cross-session | persistent |
| **TOIN scoring weights** | recency 0.20, semantic 0.20, TOIN 0.25, error 0.15, fwd-ref 0.15, density 0.05 | auto-normalize |

### 3.4 Storage Stack (com 3 backends para CCR)

| Backend | Quando usar | Persistência | Multi-worker |
|---|---|---|---|
| **`InMemoryCcrStore`** | Tests, single-worker | ❌ | ❌ (fragmenta) |
| **`SqliteCcrStore`** (default) | Single-host prod | ✅ (WAL mode) | ✅ (sticky LB) |
| **`RedisCcrStore`** (opt-in) | Multi-host / horizontal | ✅ (Redis) | ✅ (sem stickiness) |

> **Gotcha real (RUST_DEV.md):** com `--workers N > 1` + InMemory CCR, cada worker tem sua própria store → `<<ccr:HASH>>` markers written on worker A são invisíveis a worker B. O proxy emite `WARNING` no startup alertando. **Fix:** setar `HEADROOM_CCR_BACKEND=sqlite`.

---

## 4. 📦 Estrutura do Repositório (Mapa Completo)

### 4.1 Top-level Files (64+ items)

```
/.changelog.md (28B) · /.commitlintrc.json · /.devcontainer/
/.dockerignore · /.env.example · /.gitignore (3.9KB)
/.pre-commit-config.yaml · /.release-please-config.json
/Cargo.lock (136KB) · /Cargo.toml (workspace)
/CHANGELOG.md (73.2KB! ENORME)
/CODE_OF_CONDUCT.md (5.5KB)
/CONTRIBUTING.md (5.7KB)
/Cargo.toml (workspace root)
/Dockerfile (5.4KB) · /docker-bake.hcl · /docker-compose.yml
/Dockerfile · /ENTERPRISE.md (167B) · /Headroom-2.gif (5.6MB!)
/HeadroomDemo-Fast.gif (4.6MB) · /LICENSE (10.8KB)
/Makefile (5.9KB) · /NOTICE (1.2KB) · /PR.md (7.8KB)
/README.md (24.3KB) · /RUST_DEV.md (18.6KB) · /SECURITY.md (2.2KB)
/TESTING-copilot-subscription.md (7.1KB)
/claude_analysis_ttl.py (12.4KB) · /codecov.yml · /deny.toml (578B)
/headroom-savings.png (1.2MB) · /headroom_learn.gif (15.2MB!)
/llms.txt (5.3KB) · /mkdocs.yml (3.7KB)
/pyproject.toml (15.2KB) · /rust-toolchain.toml (748B)
/uv.lock (1.3MB)
```

### 4.2 Diretórios Principais

| Dir | Conteúdo |
|---|---|
| `/benchmarks/` | Scripts + resultados de benchmark |
| `/crates/` | **4 Rust crates** (re-write em andamento) |
| `/docs/` | Documentação MkDocs (proposals/, etc.) |
| `/e2e/` | Testes E2E |
| `/examples/` | Exemplos de uso |
| `/headroom/` | **Python source package** (25 files root + 32 subdirs) |
| `/plugins/` | **4 plugins** (agent_hooks, oauth2, hermes, openclaw) |
| `/REALIGNMENT/` | Plano de re-arquitetura (Rust re-write) |
| `/scripts/` | Scripts auxiliares (record_fixtures, sync-plugin-versions) |
| `/sdk/typescript/` | **TypeScript SDK** (npm) |
| `/sql/` | SQL migrations (sqlite-vec schema) |
| `/tests/` | Test suite Python (pytest) |
| `/wiki/` | Wiki content |
| `/.claude-plugin/` | **Claude Code plugin manifest** (marketplace.json) |
| `/.devcontainer/` | devcontainer config (default + memory-stack) |
| `/.github/` | workflows + outros |

### 4.3 Rust Workspace (`/crates/`)

```toml
[workspace]
members = [
  "crates/headroom-core",      # lib: shared types + transform trait
  "crates/headroom-proxy",     # binary: axum HTTP server
  "crates/headroom-py",        # PyO3 cdylib exposing headroom._core
  "crates/headroom-parity",    # lib + parity-run CLI (Python vs Rust)
]
default-members = ["headroom-core", "headroom-proxy", "headroom-parity"]
# headroom-py is maturin-only (extension-module feature)
```

**Build system:** maturin 1.5+ (PyO3 abi3-py310 — supports Python 3.10 to 3.14)

**Dependências Rust chave:**
- `serde 1`, `serde_json 1` (preserve_order, arbitrary_precision, raw_value)
- `tokio 1` (rt-multi-thread, signal)
- `axum 0.7` + `tower 0.5` (HTTP server)
- `reqwest 0.12` com **rustls** (default-features=false)
- `pyo3 0.24` (abi3-py310)
- `aws-sigv4 1`, `aws-config 1`, `aws-credential-types 1` (Bedrock)
- `gcp_auth 0.12` (Vertex AI)
- `clap 4` (CLI)

**Profiles:**
- `release`: strip=symbols, lto=thin, codegen-units=1
- `ci`: inherits release, lto=false, codegen-units=256, opt-level=1 (fast CI builds)

### 4.4 Python Package (`/headroom/`) — Top-level

```
__init__.py · _version.py · agent_savings.py · binaries.py
cli.py · client.py · compress.py · config.py
copilot_auth.py · copilot_linux_secret.py · copilot_macos_keychain.py
exceptions.py · hooks.py · onnx_runtime.py · parser.py
paths.py · pipeline.py · py.typed · release_version.py
shared_context.py · tokenizer.py · tools.json · update_check.py · utils.py
```

**32 subdirs:**
audit · backends · cache · capture · **ccr** · cli · compression · dashboard · evals · graph · image · install · **integrations** · lean_ctx · **learn** · mcp_registry · **memory** · models · observability · perf · prediction · pricing · **providers** · **proxy** · relevance · reporting · rtk · storage · subscription · telemetry · tokenizers · **transforms**

### 4.5 Python Package — `transforms/` (24 files)

```
__init__.py · adaptive_sizer.py · anchor_selector.py · base.py
cache_aligner.py · code_compressor.py · compression_policy.py
compression_summary.py · compression_units.py · content_detector.py
content_router.py · diff_compressor.py · error_detection.py
html_extractor.py · kompress_compressor.py · log_compressor.py
observability.py · pipeline.py · read_lifecycle.py
search_compressor.py · smart_crusher.py · spreadsheet_ingest.py
tabular_ingest.py · tag_protector.py
```

### 4.6 Python Package — `proxy/` (38 files)

```
server.py · models.py · helpers.py
auth_mode.py · ssl_context.py · loopback_guard.py
compression_decision.py · image_compression_decision.py
semantic_cache.py · rate_limiter.py · request_logger.py
cost.py · savings_tracker.py · output_savings.py
output_shaper.py · verbosity_controller.py
memory_decision.py · memory_handler.py · memory_injection.py
memory_query.py · memory_ranker.py · memory_tool_adapter.py
cc_switch_reconciler.py (CC = Claude Code)
prometheus_metrics.py · stage_timer.py · probe_recorder.py
runtime_env.py (hot-sync) · warmup.py
extensions.py · modes.py · outcome.py
forwarded_headers.py · project_context.py · ws_session_registry.py
debug_introspection.py
+ subdirs: handlers/, interceptors/
```

### 4.7 Python Package — `memory/` (24 files, 66KB largest)

```
core.py (29KB) · traffic_learner.py (66KB!!) · extraction.py (25KB)
ports.py (25KB) · system.py (24KB) · bridge.py (24KB)
tools.py (20KB) · wrapper_tools.py (21KB) · bridge_parsers.py (14KB)
mcp_server.py · sync.py · wrapper.py · factory.py
storage_router.py · tracker.py · easy.py · config.py
budget.py · inline_extractor.py · models.py · qdrant_env.py
+ subdirs: adapters/, backends/, sync_adapters/, writers/
```

### 4.8 TypeScript SDK (`/sdk/typescript/`)

```
package.json (v0.26.0) · tsconfig.json · tsup.config.ts · vitest.config.ts
src/
  client.ts (20KB!) · compress.ts (2.6KB) · errors.ts (3.2KB)
  hooks.ts (2.9KB) · index.ts (3KB) · paths.ts (8KB)
  shared-context.ts (4.8KB) · simulate.ts (1.7KB) · types.ts (3KB)
  adapters/ (vercel-ai, openai, anthropic, gemini)
  types/ · utils/
examples/ · test/
```

**TS SDK package.json (essencial):**
- **name:** `headroom-ai`
- **dependencies:** NONE (zero runtime deps)
- **peerDependencies (all optional):** `@ai-sdk/provider >=1.0.0`, `@anthropic-ai/sdk >=0.30.0`, `ai >=6.6.0`, `openai >=4.0.0`
- **exports:** `.`, `./vercel-ai`, `./openai`, `./anthropic`, `./gemini`

> **Excelência arquitetural:** o TS SDK é uma casca HTTP fina — toda a compressão acontece no proxy Python. Isso evita duplicar ~50K LOC Rust/Python em TS.

### 4.9 Plugins (`/plugins/`)

| Plugin | Função |
|---|---|
| `headroom-agent-hooks` | Startup hooks for **Claude Code** + **GitHub Copilot CLI** |
| `headroom-oauth2` | OAuth2 client-credentials upstream-auth proxy extension |
| `hermes` | Hermes agent headroom_retrieve plugin |
| `openclaw` | Installs as ContextEngine plugin |

### 4.10 CI/CD (`.github/workflows/` — 17 yaml files)

```
ci.yml · devcontainers.yml · docker.yml · docs.yml
eval.yml · init-e2e.yml · init-native-e2e.yml
install-native-e2e.yml · network-diff-capture.yml
pr-health.yml · publish.yml · release-please.yml
release.yml · rust.yml · stale.yml
wrap-e2e.yml · wrap-native-e2e.yml
```

---

## 5. 🧠 Algoritmos-Chave (Deep Dive)

### 5.1 SmartCrusher (JSON Arrays — o "sweet spot")

**5 dimensões de scoring:**

| Categoria | % preservado | Razão |
|---|---|---|
| **Errors** | 100% | Debugging-critical |
| **First N** (default 3) | 100% | Schema/context/pagination |
| **Last N** (default 2) | 100% | Recency |
| **Anomalies** (>2σ) | 100% | Unusual values matter |
| **Relevant** (BM25/embedding) | Top K | Query match |
| **Change points** | All | Data transitions |
| Others | Statistical sample | Representação |

**Resultado típico:** 1000 items → 50 items (90% redução, 45K tokens → 4.5K)

**Configuração:**
```python
SmartCrusherConfig(
    min_tokens_to_crush=200,
    max_items_after_crush=50,
    keep_first=3,
    keep_last=2,
    relevance_threshold=0.3,
    anomaly_std_threshold=2.0,
    preserve_errors=True,
    relevance_tier="bm25",
)
```

**Algoritmos subjacentes** (do architecture doc):
- **Kneedle algorithm** em bigram coverage curves (sizing ótimo)
- **SimHash** fingerprinting para near-duplicates
- **zlib validation** para diversity
- K split: 30% start / 15% end / 55% importance

### 5.2 CodeCompressor (tree-sitter AST)

**Linguagens suportadas:**
- **Tier 1** (full AST): Python, JavaScript, TypeScript
- **Tier 2** (function body compression): Go, Rust, Java, C, C++

**Preservado:** imports, function/method signatures, class definitions, type annotations, decorators, error handlers
**Comprimido:** function bodies, comments, verbose docstrings

**Safety gates:**
- `min_tokens_for_compression=100` (pula arquivos pequenos)
- `max_body_lines=5` (cap por body)
- `target_compression_rate=0.2` (floor de agressividade)
- **Sintaxe válida por construção** (AST-based, não char-based)

**Performance:** 40-70% redução, 10-50ms/file, ~50MB tree-sitter, **sintaxe 100% válida**

**Fallback:** `fallback_to_llmlingua=True` para linguagens desconhecidas

### 5.3 Kompress (ModernBERT Text Compression) — **O modelo próprio!**

| Métrica | Valor |
|---|---|
| **Base** | ModernBERT-base (149M params) |
| **LoRA adapter** | 3.4M trainable (r=16, alpha=32) |
| **Total trainable** | 3.4M (2.2% do total) |
| **Heads** | per-token CE (must-keep weight=3.0) + 1-D span conv (BCE weight=0.3) |
| **Context nativo** | 8.192 tokens |
| **Training data** | **126.617 labeled examples** de **17 domínios** |
| **Labeler** | DeepSeek-V4-Flash (compressor) + DeepSeek-V4-Pro (judge), Pipeline A+B faithfulness loop |
| **Hard-keep overlay** | GLiNER + regex + lexicons para names/dates/numbers/URLs/code IDs |
| **Optim** | AdamW (lr=2e-4 cosine, warmup_ratio=0.06, weight_decay=0.01) |
| **Epochs** | 3 · **Hardware:** 1×H100 80GB · **Tempo:** ~39 min wall-clock |
| **Precision** | bf16 + FlashAttention-2 + gradient checkpointing |
| **Effective batch** | 48 (12 × 4 grad-accum) |

**Fontes de dados (17 domínios):**
arxiv · pubmed-scientific · govreport · swe-smith · swe-gym-openhands
toollmind · xlam-fc · fineweb-edu · cnn-dailymail · xsum
glaive-fc · lmsys-chat · claude-code-sessions · meetingbank
the-stack-smol-md · samsum · swe-bench-verified

**Threshold tuning:**

| Threshold | Drop | must_keep_recall | F1 | Best for |
|---|---|---|---|---|
| 0.30 | 8% | 99.4% | 0.904 | Conservador |
| 0.40 | 13% | 98.7% | 0.913 | Safe |
| **0.50** (default) | 18% | 97.4% | **0.918** | Balanced |
| 0.60 | 23% | 95.0% | 0.915 | Aggressive |
| 0.70 | 30% | 90.8% | 0.898 | Very aggressive |

**Arquivos no HF:** `config.json`, `model.safetensors` (~600MB), `merged.pt`, `tokenizer.json`, `adapter/` (LoRA + token_head + span_conv)

**Compatibilidade:** Transformers, Safetensors, ONNX, pipeline tag: `token-classification`

### 5.4 CacheAligner (Provider Cache Optimization)

**Problema:** caracteres dinâmicos (datas, timestamps, UUIDs) no início do prompt invalidam o KV cache inteiro.

**Solução:** extrai o dinâmico, move para o final:
```
Before: "You are helpful. Current Date: 2024-12-15"
After:  "You are helpful."
        "[Context: Current Date: 2024-12-15]"   ← dynamic tail
```

**Savings por provider:**

| Provider | Mecanismo | Discount | TTL | Min size |
|---|---|---|---|---|
| **Anthropic** | `cache_control` blocks (explicit) | **90% off** | 5 min (extended on hit) | — |
| **OpenAI** | Prefix caching (auto) | **50% off** | — | 1024 tokens |
| **Google** | CachedContent API (explicit) | **75% off** | — | 32.768 tokens |

**Compounding (Anthropic + SmartCrusher):**
- 100K input → 20K (SmartCrusher 80% savings)
- 18K of 20K hit cache (90% off)
- Effective cost: 2K full-price + 18K @ 10% = **3.8K equivalent = 96.2% total savings**

### 5.5 IntelligentContext (Message Drop Scoring)

6 dimensões com pesos auto-normalizados:
- recency (0.20)
- semantic_similarity (0.20)
- toin_importance (0.25) ← peso maior; aprende cross-session
- error_indicator (0.15)
- forward_reference (0.15)
- token_density (0.05)

---

## 6. 📊 Benchmarks (Dados Reais de Produção)

### 6.1 Resultados de Compressão (de `python -m headroom.evals suite --tier 1`)

**Real-world workloads:**

| Workload | Before | After | Savings |
|---|---|---|---|
| Code search (100 results) | 17.765 | 1.408 | **92%** |
| SRE incident debugging | 65.694 | 5.118 | **92%** |
| GitHub issue triage | 54.174 | 14.761 | **73%** |
| Codebase exploration | 78.502 | 41.254 | **47%** |

**Accuracy preservation:**

| Benchmark | Categoria | Baseline | Headroom | Delta |
|---|---|---|---|---|
| GSM8K | Math | 0.870 | 0.870 | **±0.000** |
| TruthfulQA | Factual | 0.530 | 0.560 | **+0.030** (ganho!) |
| SQuAD v2 | QA | — | **97%** | 19% compression |
| BFCL | Tools | — | **97%** | 32% compression |

**Demo ao vivo:** 10.144 → 1.260 tokens (87.6% redução), **4/4 respostas corretas** (FATAL preservado via statistical variance, não keyword matching)

### 6.2 Latência Overhead (Apple M-series, CPU)

| Cenário | Tokens In | Tokens Out | p50 | p95 |
|---|---|---|---|---|
| JSON Search 100 items | 10.2K | 1.5K | 189ms | 231ms |
| JSON Search 500 items | 50.2K | 1.5K | 943ms | 955ms |
| JSON Search 1K items | 100.5K | 1.5K | 2.012s | 2.198s |
| JSON API 500 | 38.9K | 1.1K | 743ms | 776ms |
| JSON DB 1K rows | 43.7K | 605 | 961ms | 1.104s |

**Cost-Benefit (Claude Sonnet $3/MTok):** Compression **pays for itself** em 11 de 12 cenários. Opus ganha ainda mais.

### 6.3 Dados de Produção (50K+ sessões, March-April 2026)

| Métrica | Valor |
|---|---|
| **Proxy P50 overhead** | **52ms** (vs 2-10s LLM = negligible) |
| P90 | 309ms |
| P99 | 4.172ms |
| Mean | 161ms |
| **Compression P25** | 4.8% |
| **Compression P50 (median)** | 4.8% (modesto pq muitas requests curtas) |
| **Compression P75** | 6.9% |
| Compression Mean | 11.3% |
| **Heavy tool-use** | 40-80% |
| **Total tokens saved** | 1.4 bilhões |
| **Total savings** | ~$4.000 USD |
| Clean instances | 249 |
| **OS distribution** | Linux 57% · macOS 38% · Windows 5% |

> Telemetria agora é **opt-in** (off by default) — mudou no unreleased.

### 6.4 HTML Extraction (Scrapinghub, 181 pages, ground truth)

| Métrica | Valor |
|---|---|
| F1 | 0.919 |
| Precision | 0.879 |
| Recall | 0.982 |
| Compression | 94.9% |

> Recall de 98.2% significa que **quase todo o conteúdo do artigo é preservado**. Pequena queda de precision não afeta a accuracy do LLM.

---

## 7. ⚙️ Configuração Completa

### 7.1 Modos (3)

| Mode | Comportamento | Use case |
|---|---|---|
| `audit` | Observa/loga, sem modificar | Production monitoring, baseline |
| `optimize` | Aplica transforms safe/deterministic | **Default prod** |
| `simulate` | Retorna plan sem chamar LLM | Testing, cost estimation |

### 7.2 Variáveis de Ambiente (essenciais)

| Variável | Default | Descrição |
|---|---|---|
| `HEADROOM_LOG_LEVEL` | `INFO` | Logging level |
| `HEADROOM_STORE_URL` | temp dir | Database URL |
| `HEADROOM_DEFAULT_MODE` | `optimize` | Default mode |
| `HEADROOM_MODEL_LIMITS` | — | Custom model config (JSON) |
| `HEADROOM_BASE_URL` | `http://localhost:8787` | TS SDK base |
| `HEADROOM_API_KEY` | — | Headroom Cloud auth |
| `HEADROOM_SAVINGS_PATH` | `~/.headroom/proxy_savings.json` | Persistent savings |
| `HEADROOM_TELEMETRY` | `off` (opt-in!) | Anonymous telemetry |
| `HEADROOM_HOST` / `HEADROOM_PORT` | `0.0.0.0` / `8787` | Proxy bind |
| `HEADROOM_BUDGET` | — | Daily USD limit |
| `HEADROOM_OUTPUT_SHAPER` | `off` | Verbosity/effort routing |
| `HEADROOM_OUTPUT_HOLDOUT` | — | Control group fraction |
| `HEADROOM_CCR_BACKEND` | `sqlite` | CCR storage (sqlite/redis) |
| `HEADROOM_CCR_TTL_SECONDS` | — | Configurable CCR TTL |
| `HEADROOM_EMBEDDER_RUNTIME` | `onnx` | `pytorch_mps` para Apple GPU |
| `HEADROOM_KOMPRESS_BACKEND` | `kompress-v2-base` | Compression model |
| `HEADROOM_CONTEXT_TOOL` | `rtk` | CLI tool (`lean-ctx` opt) |
| `HEADROOM_UPDATE_CHECK` | `on` | Daily PyPI check |
| `HF_HUB_OFFLINE` | — | Use pre-downloaded models |
| `HF_ENDPOINT` | — | Trusted mirror |
| `ORT_STRATEGY` / `ORT_LIB_LOCATION` | — | ONNX Runtime control |
| `OPENAI_TARGET_API_URL` | — | Custom OpenAI endpoint |

### 7.3 Custom Model Configuration (JSON)

```json
{
  "anthropic": {
    "context_limits": { "claude-4-opus-20250301": 200000 },
    "pricing": { "claude-4-opus-20250301": { "input": 15.00, "output": 75.00 } }
  }
}
```

**Pattern-based inference:** `*opus*` → 200K + Opus pricing, `*sonnet*` → 200K + Sonnet, `gpt-4o*` → 128K + GPT-4o

### 7.4 Precedence Rules

**Model config (later overrides):**
1. Built-in defaults → 2. `~/.headroom/models.json` → 3. `HEADROOM_MODEL_LIMITS` env → 4. SDK constructor

**General config:** 1. Defaults → 2. Env → 3. SDK constructor → 4. Per-request overrides

### 7.5 Per-Request Overrides (Python)

```python
response = client.chat.completions.create(
    model="gpt-4o",
    messages=[...],
    headroom_mode="audit",                       # audit/optimize/simulate
    headroom_query="user's intent",              # relevance scoring
    headroom_output_buffer_tokens=8000,
    headroom_keep_turns=5,
    headroom_tool_profiles={
        "important_tool": {"skip_compression": True},
        "search_tool": {"max_items_after_crush": 25},
    },
)
```

---

## 8. 🔌 Integrações (10 oficiais)

| Setup | Hook |
|---|---|
| **Any Python app** | `from headroom import compress` |
| **Any TypeScript app** | `await compress(messages, { model })` |
| **Anthropic SDK** | `withHeadroom(new Anthropic())` |
| **OpenAI SDK** | `withHeadroom(new OpenAI())` |
| **Vercel AI SDK** | `wrapLanguageModel({ model, middleware: headroomMiddleware() })` |
| **LiteLLM** | `litellm.callbacks = [HeadroomCallback()]` |
| **LangChain** | `HeadroomChatModel(your_llm)` + `HeadroomChatMessageHistory` + `HeadroomDocumentCompressor` |
| **Agno** | `HeadroomAgnoModel(your_model)` |
| **Strands** | `HeadroomStrandsModel` + `HeadroomHookProvider` (2 patterns) |
| **ASGI apps** | `app.add_middleware(CompressionMiddleware)` |
| **Multi-agent** | `SharedContext().put / .get` |
| **MCP clients** | `headroom mcp install` |
| **Claude Code** (plugin) | `headroom wrap claude` |
| **Codex** | `headroom wrap codex` (shares memory with Claude) |
| **Cursor** | `headroom wrap cursor` (prints config — paste once) |
| **Aider** | `headroom wrap aider` (starts proxy + launches) |
| **Copilot CLI** | `headroom wrap copilot` (incl. BYOK via `headroom copilot-auth login`) |
| **OpenClaw** | `headroom wrap openclaw` (ContextEngine plugin) |
| **Mistral Vibe** | `headroom wrap vibe` (v0.26.0) |

### 8.1 Cloud Provider Backends

| Backend | Auth | Notas |
|---|---|---|
| OpenAI | `OPENAI_API_KEY` | Default |
| Anthropic | `ANTHROPIC_API_KEY` | Default |
| **AWS Bedrock** | `AWS_*` env / SSO | v0.26: cross-region + Converse compression (eu./us./apac./global. profiles) |
| **Vertex AI** | `GOOGLE_APPLICATION_CREDENTIALS` | v0.25.0 |
| **Azure OpenAI** | Azure AD | Custom base URL |
| **OpenRouter** | `OPENROUTER_API_KEY` | 400+ models |

---

## 9. 🧠 Memory & Learn (Estado Cross-Agent)

### 9.1 Memory — `with_memory()`

**3 backends de embedder:**
- `LOCAL` (all-MiniLM-L6-v2, 384-dim, fast/free/private)
- `OPENAI` (text-embedding-3-small, paid)
- `OLLAMA` (nomic-embed-text, local server)

**Scoping hierárquico:** `user_id` + `session_id`. **Temporal versioning:** `supersede()` chains.

**Storage:** SQLite + HNSW (hnswlib) + FTS5 (full-text search)

**Apple GPU offload (v0.25.0):** `HEADROOM_EMBEDDER_RUNTIME=pytorch_mps` com extra `[pytorch-mps]`

### 9.2 `headroom learn` — Failure Mining

**Algoritmo:** correlaciona **failure → eventual fix** (não apenas cataloga erros):
- Failed: `Read axion-formats/src/.../FirstClassEntity.java`
- Then succeeded: `Read axion-scala-common/src/.../FirstClassEntity.scala`
- **Learning:** "FirstClassEntity is at axion-scala-common/, not axion-formats/"

**Saída (marker-based, preserva conteúdo existente):**
```markdown
<!-- headroom:learn:start -->
## Headroom Learned Patterns
*Auto-generated by headroom learn -- do not edit manually*
...
<!-- headroom:learn:end -->
```

| Pattern | Destino | Por quê |
|---|---|---|
| Environment, paths, search scope, commands, large files | **CLAUDE.md** | Stable project facts, version-controlled |
| Missing paths, retry patterns, permissions | **MEMORY.md** | May change, agent-specific |

**Arquitetura adapter:** `Scanners` (lê logs específicos) → `Analyzers` (lógica comum) → `Writers` (output para tool-specific)

**Plugins built-in:** `plugins/claude.py`, `plugins/codex.py`, `plugins/gemini.py`. Externos via entry point `headroom.learn_plugin`.

**Real-world result:** 67.583 tool calls, 7.5% failure rate, 164 corrections/project (avg).

---

## 10. 🎯 MCP Server (3 tools)

| Tool | Params | Retorna |
|---|---|---|
| `headroom_compress` | `content` (required) | `compressed`, `hash`, `original_tokens`, `compressed_tokens`, `savings_percent`, `transforms` |
| `headroom_retrieve` | `hash` (required), `query` (optional) | `original_content` OR `results` (filtered BM25), `source` (`local`/`proxy`) |
| `headroom_stats` | — | compressions, retrievals, tokens_saved, savings_percent, estimated_cost_saved_usd, recent_events, sub_agents, combined, proxy |

**2 transports:**
- **stdio** (default p/ Claude Code local)
- **Streamable HTTP** (p/ remote/Docker): `headroom mcp serve --transport http --port 8080` ou via proxy em `/mcp`

**Endpoints:** `POST /mcp`, `GET /mcp` (SSE), `DELETE /mcp` (terminate)

---

## 11. 📉 Limitações (do doc oficial)

### 11.1 Quando Headroom NÃO ajuda

| Cenário | Latência | Compression | Verdict |
|---|---|---|---|
| Short conversational exchanges (<300 tokens) | Net loss | median 4.8% | Skip |
| Code-only sessions | Passthrough | 0% | Skip |
| Single-turn requests | — | 0% | Skip |
| Plain text | Adds latency | 43-46% | "Cost optimization only" |
| Code (RAG documents) | Passthrough | 0% | Skip |

### 11.2 O que NÃO é comprimido

- **Mensagens curtas** (<300 tokens) — overhead > savings
- **Source code** — passthrough (safety gate: `protect_recent_code=4`, `protect_analysis_context=True`)
- **grep/search results** — already minimal
- **Images** — fixed ~1600 token cost (separate image compressor: 40-90% reduction)
- **System prompts** — preserved for prefix cache

### 11.3 Code Compression (Gated Heavily)

3 protection gates:
1. **Word count gate** (<50 words → silently skipped)
2. **Recent code protection** (`protect_recent_code=4` — last 4 messages)
3. **Analysis intent protection** (keywords "analyze", "review", "explain", "fix", "debug" → protect ALL code)

> Razão: "users fetch code to work with it; compressing function bodies removes what they need."

### 11.4 JSON Constraints

- Arrays < 5 items pass through
- Content < 200 tokens passes through
- Bool-only arrays pass through
- JSON without arrays passes through
- Malformed JSON silently passes through
- NaN/Infinity filtered before statistics
- Nesting depth > 5: inner arrays not examined

### 11.5 Safety Gates (Todos os compressors)

- Invalid JSON → passthrough
- AST parse failure → fallback to LLMLingua or original
- Output that grows → returns original
- Missing optional deps → passthrough with warning
- Errors logged at WARNING, never propagated
- (Exception: LLMLingua OOM raises RuntimeError)

### 11.6 TOIN Cold Start

- No learned patterns → falls back to statistical heuristics
- Confidence < 0.3 → TOIN hints ignored
- Patterns build with repeated use
- Cross-session learning requires `TelemetryConfig.storage_path`

---

## 12. 🆚 Comparação com Concorrentes

| | **Scope** | **Deploy** | **Local** | **Reversible** |
|---|---|---|---|:---:|
| **Headroom** | **All context** (tools, RAG, logs, files, history) | Proxy · library · middleware · MCP | ✅ | ✅ |
| [RTK](https://github.com/rtk-ai/rtk) | CLI command outputs | CLI wrapper | ✅ | ❌ |
| [lean-ctx](https://github.com/yvgude/lean-ctx) | CLI commands, MCP tools, editor rules | CLI wrapper · MCP | ✅ | ❌ |
| [Compresr](https://compresr.ai), [Token Co.](https://thetokencompany.ai) | Text to their API | Hosted API call | ❌ | ❌ |
| OpenAI Compaction | Conversation history | Provider-native | ❌ | ❌ |

**Unique value props do Headroom:**
1. 🟢 **Reversible** (CCR — único no mercado)
2. 🟢 **Local-first** (vs Compresr/Token Co. hosted)
3. 🟢 **Universal scope** (vs RTK/lean-ctx só CLI)
4. 🟢 **Multi-provider** (vs OpenAI Compaction só)

**Attribution:** Headroom **ship with** RTK (CLI rewriting) e pode usar lean-ctx como plugin.

---

## 13. 📜 Releases Recentes (4 últimos)

### v0.26.0 (16/06/2026)
- Copilot BYOK provider wrapper
- Dashboard agent usage stats
- Mistral Vibe CLI support (`headroom wrap vibe`)
- Bedrock cross-region + Converse compression
- Kompress-v2-base int8 ONNX (default switch)
- Parser: re-issued identical tool calls = reread waste
- Adversarial-input robustness grid for compressors

### v0.25.0 (12/06/2026)
- Differential network capture harness
- Dashboard light mode + per-model savings
- OAuth2 client-credentials upstream-auth
- Vertex AI proxy routing
- **Python 3.14+ via pyo3 abi3 stable ABI**
- Compression safety rails (error-output protection, circuit breaker, library inflation guard)
- Hermes agent headroom_retrieve plugin
- Apple-GPU (MPS) embedding runtime (opt-in)

### v0.24.0 (08/06/2026)
- `headroom perf --format {text,json,csv}`
- Resolved upstream API targets in startup banner
- BM25 IDF weighting
- CLAUDE_CODE_USE_BEDROCK / CLAUDE_CODE_USE_FOUNDRY gateway support
- Security patches: loopback guard, retry None raise, async subprocess, cache race

### v0.23.0 (04/06/2026)
- Wheel builds across cp310–cp313 on macOS arm64, manylinux aarch64, manylinux x86_64

### Unreleased
- **Telemetry: opt-in (off by default)** — mudança importante de governança
- HEADROOM_TELEMETRY=off is now default; only on-values enable it
- Per-project savings breakdown (X-Headroom-Project header)
- Apple-GPU MPS embedder (opt-in)
- Bedrock cross-region inference profile detection
- Converse-body compression on native Bedrock route
- Native Bedrock /model/{id}/converse + converse-stream routes
- HEADROOM_CCR_TTL_SECONDS configurable
- Codex wrap fix: rewrite pre-existing top-level keys to avoid duplicates
- Docker: bundle headroom-proxy binary in images
- Dashboard Simplified Chinese locale (PR #1243)

---

## 14. 🐛 Issues Abertas & Atividade (snapshot 21/06/2026)

**14 PRs abertos no dia** + várias issues fechadas. Padrões de issues ativas:

| Tema | PR/Issue | Status |
|---|---|---|
| Windows UTF-8 (learn pipeline) | #1239, #1245 | closed/open |
| Codex wrap duplicate TOML keys | #1235, #1240 | closed |
| CCR return stored content when retrieve query matches nothing | #1236 | closed |
| Dashboard i18n (Simplified Chinese) | #1243 | open |
| Memory-stack docker-compose docs | #1242 | open |
| Dependabot uv bump (ci failing) | #1241 | open |
| `headroom update` doesn't work (bug) | #1135 | open |
| `headroom learn` Windows crash | #1202 | closed |
| Copilot VSCode plugin bug | #962 | open |
| File read compression worth? | #1237 | open (user) |
| VSCode Copilot compat | #1238 | open (user) |
| Auto-detect CLAUDE_CODE_USE_BEDROCK + SigV4 re-sign | #1220 | ready for review |
| **Pricing: resolve MiniMax-M3** (provider prefix + pre-registration) | #1186 | **ready for review** ← meu modelo! |
| Tokenizers literal special-token strings | #1244 | open |
| Codex wrap e2e align with RTK guidance | #1248 | open |
| Changelog-gen raise on git failure | #1247 | open |

> **Interessante:** PR #1186 é literalmente sobre resolver **MiniMax-M3** (meu próprio modelo provider) — vou investigar se há algo a fazer.

---

## 15. 🏗️ REALIGNMENT — O Plano de Re-Arquitetura Rust

`RUST_DEV.md` (18.6 KB) detalha um plano de **re-arquitetura Rust em fases**:

**Phase 0:** Workspace Rust criado. 4 crates (core, proxy, py, parity). Parity harness com fixtures gravados.

**Phase 1:** `headroom-proxy` axum (transparente reverse proxy) que forwarda HTTP/1.1, HTTP/2, SSE, WebSocket para Python upstream na porta privada (8788).

**Phase 2+3:** Expansion. DiffCompressor e SmartCrusher **retired from Python** e viraram shims PyO3-delegating. Mas retirements vieram com "subsystems silently disconnected" — auditoria 2026-04-28 fechou gaps.

**Phase 3e.1:** Trait module `signals/` + `KeywordDetector` (aho-corasick). `Tiered<T>` combinator para ML tiers futuros (BGE classifier head planejado).

**Phase 3f:** Rust MCP scaffold.

**Phase 3g (queued):** `CompressionPipeline` + `LosslessTransform`/`LossyTransform` traits (issue #315). Princípio: "parsers para estrutura, models na boundary prose/structure".

**CCR backends (3):** InMemory, Sqlite, Redis (opt-in via `--features redis`).

**Operator runbook de cutover (Phase 1):**
```bash
# 1. Move Python proxy to private port
HEADROOM_PORT=8788 python -m headroom.proxy &
# 2. Run Rust proxy on public port
./target/release/headroom-proxy --listen 0.0.0.0:8787 --upstream http://127.0.0.1:8788
# 3. End users hit :8787 unchanged
# 4. Rollback = stop Rust proxy, rebind Python
```

**Pick next port — invocation telemetry:**
```bash
curl -s http://127.0.0.1:8788/stats | jq '.compressions_by_strategy'
# {
#   "intelligent_context": 12453,
#   "smart_crusher": 487,
#   "search":         312,
#   "code":             0,  ← never fires; safe to defer porting
# }
```

---

## 16. 🔐 Segurança & Compliance

| Item | Status |
|---|---|
| **License** | Apache 2.0 (free for commercial use) |
| **SECURITY.md** | Existe (2.2KB) |
| **.gitguardian.yaml** | Sim (2.5KB) — secret scanning config |
| **deny.toml** | Sim (578B) — cargo-deny config |
| **pre-commit hooks** | `.pre-commit-config.yaml` (936B) |
| **commitlint** | `.commitlintrc.json` (441B) — conventional commits |
| **CODE_OF_CONDUCT** | Contributor Covenant |
| **NOTICE** | 1.2KB (atribuições) |
| **MFA for GitHub Copilot CLI** | `headroom copilot-auth login` (Headroom-specific OAuth, não generic) |
| **SSL bypass** | Documentado para corporate MITM CA via `REQUESTS_CA_BUNDLE` |
| **Multi-worker CCR fragmentation** | WARNING no startup |
| **Init failure fail-loud** | Per `feedback_no_silent-fallbacks.md` |
| **Telemetry (até 0.26.0)** | Opt-out. **Unreleased: opt-in** |

---

## 17. 📚 Documentação (Estrutura Completa)

**`llms.txt`** (machine-readable doc index) confirma a estrutura canônica:

**Entry points:** quickstart · installation · proxy · mcp · api-reference

**How it works:** how-compression-works · smart-crusher · code-compression · text-and-logs · ccr

**SDK/Framework integrations:** anthropic-sdk · openai-sdk · vercel-ai-sdk · langchain · agno · strands · litellm

**Memory & cross-agent state:** memory · shared-context · failure-learning

**Operations:** configuration · benchmarks · troubleshooting · limitations

**Bonus URLs:**
- https://headroom-docs.vercel.app/llms.txt (full index)
- https://headroom-docs.vercel.app/llms-full.txt (everything concatenated)
- https://headroom-docs.vercel.app/docs (searchable)

**MkDocs config** em `mkdocs.yml` (3.7KB) — provavel Material theme.

---

## 18. 🎯 Conclusão da Análise

### 18.1 Maturidade

- **v0.26.0** (não 1.0 ainda, mas "Beta" classificado no PyPI)
- **156 releases** em ~5 meses (alta cadência: ~30/mês!)
- **43.9k stars** em 5 meses = crescimento explosivo
- **364 issues abertas** mas **alta taxa de fechamento** (vários #12xx em "ready for review")
- **Real-world telemetry:** 50K+ sessões, 1.4B tokens, $4K saved
- **Production-grade** com SOC2-style concerns (auth, SSL, multi-worker, CCR backends)

### 18.2 Inovações Técnicas Notáveis

1. **CCR (Compress-Cache-Retrieve)** — único no mercado a oferecer **reversible** compression
2. **Kompress-v2-base** — modelo próprio treinado em 17 domínios (incluindo **claude-code-sessions**!) com Pipeline A+B faithfulness loop
3. **TOIN (Tool Output Intelligence Network)** — aprende padrões cross-session
4. **Multi-backend CCR** (InMemory/Sqlite/Redis) com fail-loud init
5. **Rust re-write planejada** com parity harness (não destrutivo)
6. **Trace-based eval pipeline** (adversarial inputs, re-issued tool calls = reread waste)

### 18.3 Pontos de Atenção

1. **Telemetria mudou para opt-in** (governança) — mas isso quebra workflows existentes que dependiam do opt-out default
2. **Windows UTF-8 issues** recorrentes (vários PRs recentes)
3. **Multi-worker CCR fragmentation** — gotcha conhecido, requer sticky LB ou Redis
4. **Code compression gated heavily** — basicamente passthrough (intencional)
5. **SmartCrusher / DiffCompressor retirements** vieram com "subsystems silently disconnected" — auditoria fechou gaps mas lição: cuidado com retirements
6. **Headroom v0.5.18 → v0.26.0** em poucos meses = API instável (semver pre-1.0)

### 18.4 Possíveis Conexões com TACO/Headroom (meu contexto)

- **PR #1186 (open, ready for review):** "fix(pricing): resolve MiniMax-M3 (provider prefix + pre-registration)" — Headroom está adicionando suporte correto para o **meu modelo provider** (MiniMax-M3). Vale a pena olhar para garantir que está correto do meu lado.
- **Limitation: code compression passthrough** — bate com a ideia de TACO preservar lógica de algoritmo
- **Telemetria opt-in** — bate com o princípio de não assumir consentimento
- **CCR (reversible compression)** — analogia interessante com **memory recall** + **checkpoint** no TACO
- **`on_pipeline_event()` 11-stage lifecycle** — analogia interessante com **TACO phase protocol v6.2** (0-7)
- **CCR backends selection** — paralelo com `touring-storage` (foundation) vs SQLite vs Redis
- **CompressionPipeline orchestrator + LosslessTransform/LossyTransform** (Phase 3g) — bate com a dualidade `perfect-edit` (lossless) vs compress (lossy) do Taco

### 18.5 Recomendação de uso para o TACO

**Onde Headroom poderia agregar valor ao TACO imediatamente:**

1. **Comprimir tool outputs antes de injetar em LLM** — savings de 60-95% em outputs de `Bash`/`Read`/`Grep`
2. **CacheAligner para system prompts estáveis** — economiza em chamadas repetidas ao mesmo provider
3. **CCR (headroom_retrieve)** — analogia com **`touring memory recall`** mas on-demand
4. **`headroom wrap` para sub-agents** — transparent

**Onde NÃO usar:**

1. Não comprimir user messages (gated anyway)
2. Não comprimir code (gated anyway — boa decisão)
3. Não usar em sessões curtas (overhead > savings)

---

## 📎 Anexo: URLs Citadas

**Repositório:**
- https://github.com/chopratejas/headroom
- https://raw.githubusercontent.com/chopratejas/headroom/main/README.md
- https://raw.githubusercontent.com/chopratejas/headroom/main/pyproject.toml
- https://raw.githubusercontent.com/chopratejas/headroom/main/Cargo.toml
- https://raw.githubusercontent.com/chopratejas/headroom/main/CHANGELOG.md
- https://raw.githubusercontent.com/chopratejas/headroom/main/RUST_DEV.md
- https://raw.githubusercontent.com/chopratejas/headroom/main/llms.txt
- https://raw.githubusercontent.com/chopratejas/headroom/main/.claude-plugin/marketplace.json

**Documentação:**
- https://headroom-docs.vercel.app/docs
- https://headroom-docs.vercel.app/docs/quickstart
- https://headroom-docs.vercel.app/docs/architecture
- https://headroom-docs.vercel.app/docs/how-compression-works
- https://headroom-docs.vercel.app/docs/ccr
- https://headroom-docs.vercel.app/docs/mcp
- https://headroom-docs.vercel.app/docs/memory
- https://headroom-docs.vercel.app/docs/failure-learning
- https://headroom-docs.vercel.app/docs/configuration
- https://headroom-docs.vercel.app/docs/proxy
- https://headroom-docs.vercel.app/docs/cache-optimization
- https://headroom-docs.vercel.app/docs/installation
- https://headroom-docs.vercel.app/docs/limitations
- https://headroom-docs.vercel.app/docs/benchmarks
- https://headroom-docs.vercel.app/docs/strands
- https://headroom-docs.vercel.app/docs/langchain
- https://headroom-docs.vercel.app/docs/api-reference
- https://headroom-docs.vercel.app/docs/smart-crusher
- https://headroom-docs.vercel.app/docs/code-compression
- https://headroom-docs.vercel.app/llms.txt
- https://headroom-docs.vercel.app/llms-full.txt

**ML Model:**
- https://huggingface.co/chopratejas/kompress-v2-base

**Comunidade:**
- https://discord.gg/yRmaUNpsPJ
- https://www.npmjs.com/package/headroom-ai
- https://pypi.org/project/headroom-ai/

---

**Total de fetches paralelos executados:** 23
**Sequential thoughts processados:** 8
**Páginas/docs consumidas:** 30+
**Arquivos do repo lidos/inspecionados:** 50+
**Tempo total de exploração:** ~1 turn consolidado após 8 pensamentos profundos