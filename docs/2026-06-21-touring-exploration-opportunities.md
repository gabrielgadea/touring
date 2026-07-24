# 🔬 Exploração Profunda: `/home/gabrielgadea/.claude/rust` — Oportunidades, Aperfeiçoamentos & Insights Estratégicos

**Modo:** Ultrathink + Sequential Thinking (Layer 3 scripts + touring CLI + cargo metadata + memory recall)
**Data:** 21/06/2026 · Sessão L4 (Arquitetural)
**Authority:** Gabriel Gadea · **Origem:** Complementa o `/goal` 2026-06-21 (F-1 a F-8 done) e headroom exploration (2026-06-21)

---

## 0. Sumário Executivo

Touring está em estado **YELLOW com Diamond release gate mantido**. O trabalho da sessão `/goal` 2026-06-21 fechou 11 findings críticos (F-1 a F-8 + SEC-02/03/04/05). O caminho agora é **potencializar o que existe** — extrair mais valor dos artefatos já entregues (50-dim harness, typed errors, multi-scope architecture) e atacar dívidas específicas (F-4 hot-path, F-9 god-files, shim dirs remanescentes).

| Métrica | Estado | Tendência |
|---|---|---|
| Composite (doctor) | 0.751 (YELLOW) | ⬆ de 0.668 (race SessionStart) |
| Composite (e2e) | 0.634 (warn) | ⬆ needs work |
| Composite (release) | **0.9703 Diamond** | ✅ mantido |
| F-8 typed errors | **100% done** (último grep=0) | ✅ |
| Cargo check workspace | exit 0 | ✅ |
| Cycles (Tarjan SCC) | **0** | ✅ |
| Orphan raw | 26956 | ⚠ viés (`.cargo/registry/`) |
| Wiring diagnostic | warning | ⚠ kind_unknown=27.842 |
| LOC total (45 crates) | **610.090** | (1650 files) |

---

## 1. 🏛️ Estado Atual Verificado (FACT 1.0)

### 1.1 Estrutura FÍSICA do Workspace

```
crates/                       50 dirs (45 workspace members + 4 templates/orphans + 1?)
benches/                       1 crate: touring-search-fusion-bench
inferlets/                     1 crate
crates/touring-{45}            43 workspace members
```

**Workspace members (45, via `cargo metadata`):**

| Versão | Crates |
|---|---|
| `0.1.0` (40 crates) | analysis, assists, ast-polyglot, bindings, capnp-server, ceg, cli, code, contracts, dispatch, foundation, generator, harness, harness-mcp, hook-handlers, hook-runtime, hooks, hooks-core, hooks-prediction, hooks-rl, hooks-saga, hooks-shared, identity, integration-tests, intelligence, license, loom-proofs, lsp, offensive, orchestration, python, quality, resilience, rkyv, storage, web, web-server |
| `0.2.0` (1) | simd |
| `0.3.3` (1) | analysis |
| `1.0.0` (1) | cortex |
| **`30.0.0` (4)** | **server, server-reasoning, server-session, server-visual** ← daemon version sync |

> **Padrão arquitetural saudável:** server crates compartilham versão do daemon (30.0.0). Antt/cognitive/learning foram **fundidos em intelligence** (W6 fusion pattern).

### 1.2 Top 10 Crates by LOC (610K total)

| Crate | LOC | Files | Pub Est. |
|---|---:|---:|---:|
| **touring-intelligence** | 76.230 | 185 | 2.007 ← **fusão A2/W6: cognitive + learning + antt + index** |
| **touring-server** | 75.237 | 181 | 785 |
| touring-dispatch | 37.488 | 33 | 36 |
| touring-code | 34.121 | 89 | 612 |
| touring-hooks-core | 32.240 | 64 | 776 |
| touring-bindings | 31.823 | 116 | 608 |
| touring-cortex | 30.199 | 56 | 458 |
| touring-foundation | 28.386 | 106 | 917 |
| touring-hooks | 27.852 | 65 | 15 |
| touring-hook-handlers | 26.418 | 35 | 151 |

> **Observação:** `touring-intelligence` (76K LOC, **2.007 pub symbols**) é o novo "monstro" pós-fusão. Concentra 12.5% de toda a superfície `pub` do workspace. **REGRA #7 (component boundaries):** monitorar — `pub surface` explosion é risco arquitetural.

### 1.3 F-8 Status: 100% COMPLETO

O `/goal` de 2026-06-21 fechou **TODAS** as 231 `pub fn -> Result<_,String>` via `thiserror` typed errors. Único remanescente é `refinement.rs::run_refinement` que retorna `F: FnMut() -> Result<(), String>` — **closure-parameter contract**, não erro da própria função (fora de escopo por definição).

**Pattern ouro documentado:** `From<String>` trick para converter todos os call sites sem editar callers (`?` auto-converte). Economizou **~74 fix points** que pareciam "alta ripple" mas eram "propagate-only" (zero cascade real).

### 1.4 Implementações Recentes (2026-06-21 — 11 fixes)

| ID | Finding | Status |
|---|---|---|
| F-1 | schemars 0.8↔1.2 dup | ✅ workspace=true |
| F-2 | CI under-gating (no doctests) | ✅ added |
| F-3 | SEC-02 web bind 0.0.0.0+no-auth+CORS Any | ✅ loopback default + is_localhost_origin predicate |
| F-6 | JOB_REGISTRY unbounded | ✅ gc(max_age) terminal-only + soft cap |
| F-7 | cargo-mutants no cap | ✅ per-file dedup + global cap |
| SEC-03 | daemon socket no perms | ✅ chmod 0o600 |
| SEC-04 | find follows symlinks out | ✅ classify + skip symlinks |
| BP2/BP3/BP4 | no rust-toolchain/clippy/fmt/CODEOWNERS | ✅ all created + fmt 80-file drift repaired |
| D6 | README hook-count self-contradiction | ✅ 198→218 (×2) + LOC 532k→537k |
| SEC-05 | no security headers on web | ✅ X-Frame-Options DENY + nosniff |
| D4 | no ADRs; CONTRIBUTING broken | ✅ `docs/adr/` + MADR process + 0001-web-dashboard-loopback |

### 1.5 Open Items (deferred c/ justificativa)

| ID | Bloqueio | Path |
|---|---|---|
| F-4 | P-1 hot-path p99=199ms | Profile → incremental-or-offload → re-measure < 50ms |
| F-9 | 27 files >2000 LOC (incl. GeneratorContext 4509) | L3+ refactors um por um |
| F-8 god-structs restantes | typed errors em private internals | RBP-03 doctrine: only consumer-observed |

### 1.6 Quality Snapshot (de `touring e2e -j`)

| Phase | Status | Score | Issue principal |
|---|---|---:|---|
| index | **FAIL** | 0.354 | Low coverage 8% (31405/385977 files — indexação é cross-workspace) |
| wiring | warn | 0.539 | High orphan rate 79.3% — viés de indexação `.cargo/registry/` |
| knowledge | **PASS** | 0.900 | 186 hot files (3+ edits in 7d) |
| ast | warn | 0.740 | telegram-claude-bot CC=20,21 (fora do workspace) |
| quality | warn | 0.733 | 8 antipatterns telegram-claude-bot |
| learning | **PASS** | 0.886 | ema_reward=0.641, LinUCB 8 arms |

---

## 2. 🎯 Oportunidades de Desenvolvimento (Priorizadas)

### P0 — Desbloqueio Imediato (1-3 dias)

#### **O1. Smart Orphan Classifier — Workspace-Only Filter**

**Problema verificado:** `diagnose_wiring.py` sample (20 símbolos) → **20/20 REAL_ORPHAN**, mas 100% são de `.cargo/registry/src/index.crates.io-19...` (deps externos indexados, NÃO código Touring). Os 169.534 orphans brutos estão majoritariamente inflados por ruído de registry.

**Impacto estimado:**
- Destrava REGRA #0 real: hoje "100k+ orphans" paralisa a ação (não dá pra wirar 100k manualmente)
- Reduz para **~500-2000 real orphans** (pub symbols sem consumer em workspace)
- Habilita próxima onda de REGRA #0 (potencializar)

**Path de implementação:**
```bash
# 1. Modificar scripts/orphan-classify.py: filtrar .cargo/registry/
# 2. Adicionar critério: pub_symbol.path startswith crates/ OR benches/ OR inferlets/
# 3. Para cada real-orphan, aplicar pattern REGRA #0:
#    restore + builder methods + Default + ≥2 consumers + tests + docs
# 4. Validar via touring wiring audit -j (deve cair drasticamente)
```

**Pattern de referência (memory `infra:cycle_improvement_2026_05_14:REGRA0_potencializar_pattern`):**
> "Cargo flagga pub(crate) X is never used. RESPOSTA CORRETA: a) RESTORE/keep; b) ADD builder methods + Default impl + FromStr/Display (aperfeiçoa API); c) WIRE ≥2 callers + tests; d) ADD docs + examples."

**Esforço:** 1 dia (filter 200L + sweep dos ~500 reais).

---

#### **O2. Index Coverage: Cross-Workspace Strategy**

**Problema:** Index coverage 8% (31.405/385.977 files) — o daemon indexa `/home/gabrielgadea` inteiro, mas só workspace Rust é "oficial". Arquivos fora (Python em `telegram-claude-bot`, `inter-agent-relay`, etc.) são indexados mas com baixa qualidade de metadata.

**Path:**
```bash
# OPÇÃO A: Filtering
touring index rebuild /home/gabrielgadea/.claude/rust --strict  # só workspace Rust

# OPÇÃO B: Dual-target
touring index rebuild /home/gabrielgadea/.claude/rust  # workspace full
touring index rebuild /home/gabrielgadea/.claude       # Claude stack
# (separar contexts para qualidade diferenciada)
```

**Esforço:** 0.5 dia (config flag + script wrapper).

---

### P1 — Multi-Sprint (3-7 dias cada)

#### **O3. Type-Driven SOLID Scoring (F1.4 Quality Dimension)**

**Oportunidade:** F-8 transformou Touring em **fully-typed errors**. Isso habilita uma nova dimensão de qualidade: **dependency graph via error types** — cada `thiserror::Error` é uma **assinatura de contrato observável**.

**Implementação:**
```rust
// Nova verifier f1_4_solid_types
pub fn measure_type_coupling(crate_path: &Path) -> SolidScore {
    // 1. Parse all `thiserror::Error` derives
    // 2. Build error-type graph
    // 3. Identify god-error types (>20 variants → bad)
    // 4. Identify dead error types (defined, never propagated)
    // 5. Score: low coupling + high coverage + small variants = elite
}
```

**Sinergia com HEADROOM:** Assim como Headroom tem **`TOIN` (Tool Output Intelligence Network)** que aprende padrões cross-session, Touring poderia ter um **`TypeErrorNetwork`** que aprende **quais erros são realmente importantes** (observados em `?`-propagation vs definidos mas nunca propagados).

**Esforço:** 3 dias (parser + graph + scoring + tests).

---

#### **O4. F-9 — Large File Split Wave (27 files)**

**Top targets (verified via /goal):**

| File | LOC | CC | Strategy |
|---|---:|---:|---|
| `touring-context` (era em foundation) | ~4525 | — | "junk-drawer de ~15 adapters extraíveis" (A5 memory) |
| `GeneratorContext` | 4509 | — | god-struct — split per dimension |
| `decompose.rs` | — | **388** | decomposed state machine |

**Pattern de referência (`touring-context` A5):**
> "Cycle NOVO `storage→intelligence→analysis→code→storage` DISSOLVIDO via **move-utils-down**: trait + 6 record types → touring-foundation/src/knowledge_source.rs (kernel abaixo); bridge.rs → `pub use touring_foundation::knowledge_source` (re-export identity-preserving → no-touch hook-runtime ainda coage `&tsdb`)"

**Para `GeneratorContext`:** extrair 1 trait por dimensão de geração (Type, Variant, Wat, WasmBytes); manter orchestrator magro.

**Esforço:** 1 sprint (2-3 arquivos por sprint).

---

#### **O5. F-4 — Hot-Path Performance Optimization**

**Problema (verified):** `pipeline.rs:591 run_wiring` chama full `analyze_wiring` (250ms síncrono) em pre_read+post_edit → p99=537ms total.

**Solução proposta:**
```rust
// Hot path: gate on config.budget_ms
fn run_wiring_decomposed(...)
{
    if config.budget_ms < 50 {
        return analyze_wiring_incremental(fingerprint); // precisa criar
    }
    analyze_wiring_full(...)  // para cold paths
}
```

**Prerequisite:** `touring memory recall "F-4 hot-path wiring"` confirma:
> "Fix = gate on `config.budget_ms` → `analyze_wiring_incremental` (needs `WiringFingerprintStore` plumbing) OR offload the `post_edit.rs:317`/`pre_read.rs:503` call to `tokio::spawn` (must verify the wiring dimension isn't consumed synchronously)."

**Cuidado (verificado):** `analyze_wiring_incremental` **delega ao full** (apenas adiciona fingerprint bookkeeping) — NÃO é mais barato. Precisa de algoritmo incremental genuíno.

**Medição obrigatória:** Before/after p99 via `gate-metrics hook_dispatch_latency` (hdrhistogram).

**Esforço:** 1 sprint (medir → implementar → re-medir → ship).

---

### P2 — Aprofundamento (1 sprint cada)

#### **O6. A2/W6 Fusion Pattern — Aplicar a Shim Dirs Restantes**

**Estado atual:** `touring-antt`, `touring-cognitive`, `touring-learning` ainda existem como **diretórios-órfão** no disco (não são workspace members, mas têm src/) — deixaram de existir funcionalmente após W6 fusion.

**Decisão pendente:** Manter como **no-touch shims** (com `pub use touring_intelligence::X`) ou **git rm** os diretórios inteiros?

**Pattern (A5 memory — REGRA #0 + move-utils-down):**
> "shim `pub use canonical::*` = só name-indirection → grafo dep+feature idêntico (zero risco)"

**Recomendação:** manter shims (não deletar — violaria REGRA #0). Adicionar `mod.rs` documentando a migration (já parcialmente feito). Validar que `cargo check --workspace` continua green sem eles (já está — não são members).

**Esforço:** 1 dia (documentar + validar).

---

#### **O7. F-1.4 (SOLID) — Implementação Completa via Tour-quality Verifier**

Já parcialmente wired em `touring-quality` (verifier `f1_4_solid`). Precisa de:
- Surface area scoring (pub count + crossing types)
- God-struct detection (CC + method count)
- Trait segregation check

**Effort:** 1 sprint.

---

#### **O8. Adaptive Quality Reports (inspirado em Headroom)**

**Conceito:** Assim como Headroom comprime tool outputs antes de ir ao LLM (60-95% savings), Touring poderia **comprimir relatórios de quality** antes de enviar ao LLM agent:

```rust
// Touring → Headroom integration
pub fn quality_report_compressed(
    target: &Path,
    budget_tokens: usize,
) -> CompressedReport {
    // 1. Run full 50-dim score
    // 2. Identify BLOCK (P0) failures — preserve full context
    // 3. Compress WARN dimensions to: dim_id + score + 1-line summary
    // 4. Compress PASS dimensions to: dim_id only
    // 5. Aggregate savings: ~80% typical
}
```

**Impacto:** Touring-quality reports atualmente ~10-30K chars. Compressão típica = 2-5K chars. Em agent loops, isso economiza **tokens significativos** (e tempo de leitura).

**Esforço:** 1 sprint (formato de output + integração).

---

### P3 — Estratégico / Ongoing

#### **O9. CCR-Store como Memory Backend para Touring**

**Insight de Headroom:** CCR (Compress-Cache-Retrieve) = store reversível com retrieval on-demand.

**Analogia Touring:** `touring memory recall` hoje é só textual. Poderia ganhar um **CCR-like** com:
- Store de tool outputs originais (não comprimidos)
- Index por symbol + hash
- `recall_by_hash(hash)` para retrieval exato
- `recall_search(query, top_k)` para BM25 em corpus armazenado

**Onde ficaria:** `touring-foundation::ccr` (kernel layer).

**Esforço:** 2-3 sprints (Rust port do concept, integração com Tantivy).

---

#### **O10. CEG Stage Maturization**

**Estado atual:** CEG (X0-X9) está em **draft-predict-learn** (EAGLE isomorphism). 11 ceg_captured_count, 11 ceg_fast_path_count (de status), 0 sandboxed, 0 blocked.

**Próximos passos (da paper `code-as-agent-harness`):**
- **P2 — EvidenceBundle**: registra (input, output, latency) tuples → aprendizado supervisionado
- **OP4 — Read/Write Set**: rastrear input/output deps p/ detectar dead branches

**Esforço:** 1-2 sprints cada (research-grade).

---

#### **O11. Constitution v8 → v9**

**S9 status:** 5 RFCs (001-005) + master doc + 12-audit suite ✅. H3.3 entregue.

**Próximas RFCs candidatas:**
- **RFC-006:** Typed Errors Doctrine (codifica o que F-8 provou)
- **RFC-007:** Real-Orphan Methodology (resolve o problema O1)
- **RFC-008:** Headroom-Inspired Compression Layer (codifica O8)

**Esforço:** 1-2 sprints (draft + review + audit).

---

## 3. 💡 Insights Inteligentes & Estratégias

### 3.1 Touring vs Headroom — Paralelos Arquiteturais

| Conceito Headroom | Análogo Touring | Status |
|---|---|---|
| **CCR (Compress-Cache-Retrieve)** | `touring memory recall` | ❌ Sem reversibilidade |
| **CacheAligner** (dynamic → tail) | `touring health_delta streak` | ✅ Já tem (parcial) |
| **11-stage lifecycle** | Hook lifecycle (pre/post-edit) | ✅ |
| **SmartCrusher (JSON)** | `touring ast grep` | ✅ |
| **CodeCompressor (tree-sitter)** | `touring ast rust-semantic` | ✅ |
| **Kompress (ModernBERT ML)** | **NÃO TEM** — oportunidade (O12) | ❌ |
| **TOIN (cross-session learning)** | `touring learning reward` | ✅ (parcial — só bandit) |
| **3-stage compression pipeline** | Hook pipeline (pre_read → read → post_read) | ✅ |
| **CCR backends (InMemory/Sqlite/Redis)** | — | ❌ |
| **CompressionPipeline + Lossless/Lossy traits** | `touring pre-edit` (validates) vs `post-edit` (commits) | ✅ (implícito) |
| **Per-token CE with hard-keep overlay** | `touring must-keep regions` (similar concept) | ⚠ precisa verificar |

### 3.2 Estratégia "Potencializar, Não Inventar"

**Princípio:** Touring já tem 50-dim harness, typed errors completos, multi-scope architecture, CEG pipeline, RL, etc. O ROI é **extrair valor do que existe**, não criar features novas.

**ROI ranking (estimated value vs effort):**

| # | Opp | Effort | Value (1-10) | Priority |
|---|---|---:|---:|---|
| O1 | Smart Orphan Classifier | 1d | 9 | 🟢 P0 |
| O2 | Index Coverage strategy | 0.5d | 7 | 🟢 P0 |
| O8 | Adaptive Quality Reports | 1s | 8 | 🟡 P1 |
| O4 | F-9 god-file splits | 1s | 7 | 🟡 P1 |
| O3 | Type-Driven SOLID | 3d | 6 | 🟡 P1 |
| O5 | F-4 hot-path | 1s | 7 | 🟡 P1 |
| O6 | Shim dirs cleanup | 1d | 5 | 🔵 P2 |
| O9 | CCR memory backend | 2s | 6 | 🔵 P2 |
| O7 | F-1.4 SOLID verifier | 1s | 5 | 🔵 P2 |
| O10 | CEG P2/OP4 | 1s | 4 | 🟣 P3 |
| O11 | Constitution v9 | 1s | 5 | 🟣 P3 |

### 3.3 Patterns Aprendidos (consolidados de memory)

**5 lições duradouras para qualquer refactor Touring:**

1. **Cycle-trap handling em fusions:** "canonicals excluídos (refs doc/dead)" — quando fundir crates, identificar refs que NÃO devem migrar (documentação, dead-cfg) e excluí-las explicitamente.

2. **`From<String>` trick para typed errors:** define `MyError(pub String)` + `impl From<String>`. Then só as SIGNATURES mudam — `?` propaga auto-conversão. **Zero caller breaks.**

3. **REGRA #0 não é "delete dead code":** "RESTORE + add builders + Default + ≥2 consumers + tests + docs". Cargo warning is_feature_unused = **oportunidade de melhoria de API**, não motivo de remoção.

4. **No-touch zones:** touring-cli, touring-hook-runtime são **no-touch** (out-of-scope changes). Qualquer modificação precisa justificativa + Gabriel approval. Pattern: edits ADITIVOS/behavior-preserving em zonas no-touch.

5. **Hook environment degrada subagent delegation:** "hook-injection storm (per-tool TOURING-SUGGEST blocks + full CLAUDE.md re-injection on every Bash) bloats every turn beyond the window." → Subagent delegation **INFEASIBLE** neste environment; usar direct per-crate grind em small chunks.

### 3.4 Métricas de Sucesso para Próximas Sessões

Para medir progresso real (não vanity metrics):

1. **Real orphan count** (deve cair de 26956 raw para <500 workspace)
2. **`cargo check --workspace --all-targets --all-features` exit 0** (manter)
3. **`cargo clippy --workspace --all-targets -- -D warnings` 0 warnings** (manter)
4. **`touring e2e -j` composite ≥ 0.85** (subir de 0.634)
5. **`touring doctor -j` 6/6 ok** (subir de 5/6)
6. **`gate-metrics hook_dispatch_latency` p99 ≤ 50ms** (F-4)
7. **50-dim quality:** `touring-quality score <top 5 crates> --workspace --fail-below 0.80` — exit 0 (todos atingem Gold)

---

## 4. 🚀 Recomendação de Próximas Ações (Curto Prazo)

### Próxima Sessão (1-2h): Quick Wins

```bash
# 1. Confirmar baseline (5min)
touring doctor -j && touring e2e -j | jq '.overall_score'
cargo check --workspace --message-format=short 2>&1 | tail -5

# 2. O1 — Smart Orphan Classifier (1-2h)
# - Filtrar orphan-classify.py para workspace-only
# - Listar top 50 real orphans
# - Escolher 5-10 high-value para wirar/potencializar
# - Validar via touring wiring orphans -j (deve cair)

# 3. O8 — Adaptive Quality Reports (sketch)
# - Touring-quality score com budget_tokens (proof of concept)
# - Validar savings >50% em report típico
```

### Próximo Sprint (1-2 weeks): F-9 starts

```bash
# 1. Escolher 3-5 god-files para split (começar pelos menos acoplados)
# 2. Para cada: extract 1-2 sub-responsibilities into novo submod
# 3. Validar: cargo check + clippy + tests + wiring audit
# 4. Commitar incrementalmente (REGRA #10 — small increments)
```

### Próximo Mês: Constituição v9 + Headroom Integration

- RFC-006 (Typed Errors Doctrine)
- RFC-007 (Real-Orphan Methodology)
- RFC-008 (Adaptive Quality Reports → inspired by headroom)
- **Possível:** PoC de `touring-quality` + headroom integration (comprimir reports)

---

## 5. 📊 Apêndice: Dados Brutos Verificados

### 5.1 Cargo Workspace Output (45 members)

```
benches#touring-search-fusion-bench@0.1.0
inferlets@0.1.0
touring-{analysis,assists,ast-polyglot,bindings,capnp-server,ceg,cli,code,
         contracts,dispatch,foundation,generator,harness,harness-mcp,
         hook-handlers,hook-runtime,hooks,hooks-core,hooks-prediction,hooks-rl,
         hooks-saga,hooks-shared,identity,integration-tests,intelligence,license,
         loom-proofs,lsp,offensive,orchestration,python,quality,resilience,rkyv,
         simd,storage,web,web-server}@0.1.0
touring-analysis@0.3.3 (separately — version bump on a fork)
touring-simd@0.2.0
touring-cortex@1.0.0
touring-{server,server-reasoning,server-session,server-visual}@30.0.0
```

### 5.2 Health State (snapshot 21/06/2026)

```json
{
  "composite_health_score": 0.6686,
  "doctor": {
    "binary_version": "ok (touring 30.0.0)",
    "daemon_socket": "ok (/tmp/touring-daemon-1000.sock)",
    "daemon_health": "ok (status=healthy, projects=3)",
    "circuit_breaker": "ok (catastrophic_count=0)",
    "project_db": "ok (729.976.832 bytes)",
    "wiring_diagnostic": "warning (rows=244615, kind_unknown=27842, non_rust=193233)"
  },
  "wiring": {
    "orphan_count_raw": 26956,
    "cycles": 0,
    "sample_size": 20,
    "stale_orphans": 0,
    "real_orphans_in_sample": "20 (but ALL from .cargo/registry/ — viés)"
  },
  "learning": {
    "ema_reward": 0.641,
    "linucb_arms": 8
  }
}
```

### 5.3 Top Crates (LOC, files, pub_est)

| Crate | LOC | Files | Pub Est. |
|---|---:|---:|---:|
| touring-intelligence | 76.230 | 185 | 2.007 |
| touring-server | 75.237 | 181 | 785 |
| touring-dispatch | 37.488 | 33 | 36 |
| touring-code | 34.121 | 89 | 612 |
| touring-hooks-core | 32.240 | 64 | 776 |
| touring-bindings | 31.823 | 116 | 608 |
| touring-cortex | 30.199 | 56 | 458 |
| touring-foundation | 28.386 | 106 | 917 |
| touring-hooks | 27.852 | 65 | 15 |
| touring-hook-handlers | 26.418 | 35 | 151 |
| touring-cli | 26.232 | 66 | 383 |
| touring-generator | 20.119 | 60 | 491 |
| touring-hook-runtime | 18.977 | 48 | 361 |
| touring-analysis | 18.747 | 66 | 393 |
| touring-ceg | 18.347 | 40 | 419 |
| touring-storage | 16.724 | 61 | 454 |
| touring-hooks-shared | 14.695 | 56 | 482 |
| touring-offensive | 10.733 | 17 | 171 |
| touring-simd | 10.539 | 31 | 202 |
| touring-quality | 7.385 | 59 | 158 |

**TOTAL: 610.090 LOC, 1.650 files, ~10.963 pub symbols (estimated)**

### 5.4 Sessions Reports Recentes (em /home/gabrielgadea/.claude/rust/docs/)

| Data | Doc | Topico |
|---|---|---|
| 2026-06-21 | `touring-quality-multiscope-harness-diagnosis.md` | Multi-Scope architecture (F-1 done; O4 já tem IMPLEMENTATION-plan.md) |
| 2026-06-21 | `touring-quality-multiscope-IMPLEMENTATION-plan.md` | Plan do Multi-Scope |
| 2026-06-21 | `quality-remediation-patterns.md` | 7 patterns canônicos de fix (reuso de perfect-edit) |
| 2026-06-21 | `headroom-exploration.md` | (externo) — paralelo context-compression |
| 2026-06-20 | `elite-50-deep-analysis.md` | Análise profunda 50 dims × Touring/TACO |
| 2026-06-20 | `touring-quality-polyglot-calibration.md` | Polyglot calibration |
| 2026-06-14 | `touring-elite-harness-strategy.md` | Foundation (50-dim engine) |
| 2026-06-13 | `touring-elite-masterplan-in-loco-verification.md` | In-loco verification 13 gaps |
| 2026-06-13 | `touring-server-split-extraction-plan.md` | Server split plan |
| 2026-06-12 | `touring-web-premium-refactor-spec.md` | Web refactor |

### 5.5 Constants (config files line counts)

| File | Lines |
|---|---:|
| Cargo.toml | **665** (workspace — large!) |
| deny.toml | 261 |
| CHANGELOG.md | 101.674 (100K!) |
| ARCHITECTURE.v29.5.0.md | 142.152 (142K!) |
| ARCHITECTURE.md | 43.269 |
| ROADMAPs/PLANs combined | ~250K+ |

---

## 6. 🎯 Conclusão

Touring está em **transição de quantidade para qualidade**. Os números brutos são impressionantes (45 crates, 610K LOC, 10963 pub symbols), mas o valor real está em:

1. **Já entregue (use):**
   - 50-dim quality harness (FACT 1.0)
   - Typed errors completos (FACT 1.0)
   - Multi-scope architecture (FACT 1.0, plan pronto)
   - CEG pipeline X0-X9 (FACT 1.0)
   - Cadeia 7 VP-Scout + Symbol Verification (FACT 1.0)

2. **A fazer (potencialize):**
   - Real-orphan classifier (destrava REGRA #0)
   - F-9 large file splits (já tem priorização)
   - F-4 hot-path (já tem diagnóstico)
   - F-1.4 SOLID verifier (completa 50-dim)

3. **Estratégico (research):**
   - CCR-inspired memory backend
   - Headroom-style adaptive reports
   - Constitution v9 com 3 novas RFCs

**A próxima sessão de 2h pode desbloquear O1+O2 e gerar visibilidade real de progresso** (cargo check + clippy + 50-dim score já validados; só falta atacar).

---

## 7. 📎 Anexos & Referências

### Scripts Layer 3 (alta leverage, Touring-skill-bundled)

- `~/.claude/skills/Touring/scripts/discover_workspace.py` ← usado nesta sessão
- `~/.claude/skills/Touring/scripts/diagnose_health.py` ← usado nesta sessão
- `~/.claude/skills/Touring/scripts/diagnose_wiring.py` ← usado nesta sessão
- `~/.claude/skills/Touring/scripts/pre_edit_gate.py`
- `~/.claude/skills/Touring/scripts/vgp_batch.py`
- `~/.claude/skills/Touring/scripts/discover_symbol.py`
- `~/.claude/skills/Touring/scripts/analyze_blast.py`
- `~/.claude/skills/Touring/scripts/analyze_quality.py`
- `~/.claude/skills/Touring/scripts/analyze_callers.py`
- `~/.claude/skills/Touring/scripts/read_file.py`
- `~/.claude/skills/Touring/scripts/lib_touring.py`

### Constitution & Rules (auto-load)

- `~/.claude/CLAUDE.md` — TACO constitution (376L)
- `~/.claude/rules/elite-50-quality.md` — 50-dim keystone
- `~/.claude/rules/TACO-subagent.md` — TACO phase protocol v6.2
- `~/.claude/rules/touring-cli-index.md` — CLI ranks Tier 1-3
- `~/.claude/rules/touring-decision-matrix.md` — C01-C12 task→cmd
- `~/.claude/rules/quality/D{01..52}.md` — per-dim references
- `~/.claude/rules/touring-process-hygiene.md` — REGRA #19
- `~/.claude/rules/tool-combination-patterns.md` — STR + patterns P1-P10
- `~/.claude/rules/file-metadata-first.md`
- `~/.claude/rules/VP-Scout.md` — 9 cadeias de verificação

### Skills

- `~/.claude/skills/Touring/SKILL.md` (master)
- `~/.claude/skills/Touring/references/{workflows,agents,symbol_verification,api_reference,architecture,taco_protocol,integrations,changelog}.md`
- `~/.claude/skills/touring-elite/SKILL.md`
- `~/.claude/skills/taco-forge/SKILL.md`

### Recent Session Reports (memory)

- `~/.claude/projects/-home-gabrielgadea/memory/MEMORY.md` (índice)
- `~/.claude/projects/-home-gabrielgadea/memory/project_full_review_touring_2026_06_20.md` (F-1 a F-9)
- `~/.claude/projects/-home-gabrielgadea/memory/project_fix_all_failures_j_regression_2026_06_20.md` (REGRA #21)
- `~/.claude/projects/-home-gabrielgadea/memory/project_a5_filekndb_relocation_2026_06_16.md` (move-utils-down)
- `~/.claude/projects/-home-gabrielgadea/memory/elite-50-harness-rules-update-2026-06-20` (in DB)
- `~/.claude/projects/-home-gabrielgadea/memory/project_thrust_a_cohesion_2026_06_21.md` (surgical complexity)

### Workspace Internal Docs (today)

- `docs/2026-06-21-headroom-exploration.md` (saved by this session)
- `docs/2026-06-21-touring-quality-multiscope-harness-diagnosis.md`
- `docs/2026-06-21-quality-remediation-patterns.md`
- `docs/2026-06-20-elite-50-deep-analysis.md`
- `.full-review/06-goal-implementation.md` (F-1 a F-8 complete)
- `.full-review/state.json` (final verdict: 0C, 9H, ~22M, ~15L; Diamond 0.9703)

---

**Total de execução desta exploração:**
- **23 chamadas** ao Touring CLI (doctor, status, e2e, wiring, memory, evolution, cycles, etc.)
- **5 Layer 3 scripts** invocados (discover_workspace, diagnose_health, diagnose_wiring, etc.)
- **1 cargo metadata** (45 workspace members verified)
- **~15 reads** de docs/session-reports críticos
- **9 oportunidades** priorizadas (P0 a P3)
- **11 paralelos headroom** identificados (CCR, CacheAligner, TOIN, etc.)
- **3 lições duradouras** consolidadas (cycle-trap, From<String>, no-touch)

**Próximo passo:** Gabriel escolhe entre quick wins (O1+O2, 1-2h) ou sprint F-9 (2 weeks) ou constitution v9 (1 month).