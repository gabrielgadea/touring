---
type: Strategy
title: "Otimização & Distribuição de CPU no Notebook (Omarchy)"
description: "Análise com --ultrathink das 32 threads do i9-14900HX, gargalos térmicos e monopolizadores. 5 fases: quick wins → workers paralelos → cache estrutural → thermal management → NUMA/hardware."
plan_id: "2026-08-31-cpu-otimizacao-notebook"
scope: "/home/gabrielgadea (Omarchy Linux, 4 dias uptime)"
bundle: "docs/plans/2026-08-31-cpu-otimizacao-notebook"
okf_version: "0.1"
timestamp: "2026-08-31T22:55:00-03:00"
tags: [strategy, cpu, optimization, omarchy, thermal, numa, ray-cpu, distribution]
---

# Estratégia — Otimização & Distribuição de CPU no Notebook

**Date**: 2026-08-31 22:55 BRT
**Hardware**: Intel(R) Core(TM) i9-14900HX · 24 cores / 32 threads · 62 GiB RAM · 125 GiB swap
**System**: Omarchy Linux, 4 dias uptime, load avg 1min **91.87**
**Inspetor**: TACO / loop-engineering OUTER steps 1-5 (memory recall + diagnose + portfolio)

---

## 1. Diagnóstico (ground truth medido)

### 1.1 Hardware (FACT)
- **CPU**: Intel i9-14900HX (GenuineIntel family 6, model 183, stepping 1)
- **Cores**: 24 físicos, **32 threads** (HT = 2 threads por core, mas i9-14900HX é **híbrida**: 8 P-cores + 16 E-cores)
- **L1d**: 896 KiB / 24 instances (1 por P/E core físico)
- **L2**: 32 MiB / 12 instances (clusters de 2 cores)
- **L3**: 36 MiB / 1 instance (compartilhado)
- **Freq max**: 5800 MHz turbo · base 800 MHz · intel_pstate com min_perf=15%
- **Memory**: 62 GiB total · 26 GiB usado · **4.8 GiB livre** · 36 GiB buff/cache
- **Swap**: 125 GiB total · **36 GiB usado** (28% utilization, mas paginação pesada)

### 1.2 Estado (FACT — medido neste turno)
| Métrica | Valor | Veredito |
|---|---|---|
| Load avg (1/5/15) | **91.87 / 86.34 / 53.69** | 🔴 **3× oversubscribed** (load > nCPU=32) |
| CPU0 freq sustained | 4199 → 4300 MHz | turbo ativo |
| governor | powersave → **performance** (F0.1 aplicado) | ✅ |
| intel_pstate max_perf_pct | 15 → **100** (F0.2 aplicado) | ✅ |
| Thermal zone 0/1/4 | **95°C** | 🔴 **5°C de TJmax** — throttle iminente |
| Thermal tjmax | 100°C (critical) | critical) |
| Thermal iwlwifi | 65°C | ✅ normal |
| Swap usage | 36 GiB / 125 GiB | 🟡 thrash de páginas |
| 96.0% us CPU | system computational bound | sim |

### 1.3 Monopolizadores (FACT)
| PID | Command | %CPU | cores | elapsed | Parent |
|---|---|---|---|---|---|
| 2796421 | `pipeline_runner --phases 3-4` | **2284%** | 22 cores | 12 min | claude session (Gabriel) |
| 2855798 | `touring-quality score analise` | 309% | 3 cores | 1.5 min | bash wrapper (session 3a87ce4a) |
| 2848184 | `touring-quality score analise` | 241% | 2.4 cores | 3 min | **init (1644)** ← 🔴 **órfão / race** |
| 1968515 | `touring-mcp` (serve) | 32% | 0.3 cores | 13h14m | claude session |

**Insight crítico**: 2 instâncias de `touring-quality score /home/gabrielgadea/projects/analise` rodando simultaneamente** com parent `init/1` (orfanato). Confirma o diagnóstico de race condition (PID 2848184 parent=init = daemon órfão que ninguém controla).

---

## 2. Causas raiz (raiz-cadeia)

```
THROTTLE TÉRMICO (95°C → 100°C)
  ├─ ① pipeline_runner monopoliza 25 cores sustained (2284% CPU)
  │    ├─ Sobrecarga sustained → calor dissipado > capacidade cooler
  │    └─ Cooler saturado: ambiente + room temp + airflow
  ├─ ② touring-quality race (2 instâncias paralelas)
  │    ├─ PID 2848184 (órfão, parent=init) — daemon watchdog retry loop
  │    ├─ PID 2855798 (session atual) — minha sessão de diagnose
  │    └─ Concorrência desperdiçada: 2 readers competindo no mesmo graph.db
  └─ ③ governor=performance + max_perf=100% NÃO resolve
       └─ intel_pstate já permitia turbo; governor só importa em idle
       └─ Limitação é FÍSICA (TDP da CPU vs cooler)

OVERHEAD COGNITIVO
  ├─ Chrome: 12% sustained (~4 cores) — 3 renderer pools + GPU process + crashpad
  ├─ touring-mcp: 32% (mas é o daemon global, necessário)
  └─ Hyprland compositor: 5% (baixo)
```

---

## 3. Estratégia por fases (medida → ajustada → medida)

#### FASE 0 — Quick Wins (✅ JÁ APLICADA, 5 min)
- [x] governor `powersave` → `performance`
- [x] intel_pstate max_perf_pct 15 → 100
- [ ] **Matar touring-quality órfão** (PID 2848184, parent=init) — kill confirmado via `pgrep -af` antes
- [ ] Limpar swap: `sudo swapoff -a && sudo swapon -a` para forçar page-in

**Ganho esperado**: marginal sob carga sustained (governor não era o limitador), mas previne throttle durante bursts curtos.

#### FASE 1 — Distribuição (10-30 min) — O MAIOR IMPACTO
- [ ] **pipeline_runner: workers paralelos** — Sequential F3-F4 → `--workers 8` (parallel per-document)
- [ ] **touring-quality singleton mutex** — 1 score por vez (lock `/tmp/touring-quality.lock`) para evitar race
- [ ] **Process priority**: pipeline_runner → nice +19 (background), touring-quality → nice -5 (foreground), chrome → nice +5
- [ ] **cgroup pin**: P-cores (0-15) para cargo/compile, E-cores (16-31) para I/O wait / chrome

**Ganho esperado**:
- Pipeline: 2284% (monopólio) → 8× ~280% workers (eficiência por thread ~3-5× quando paraleliza bem)
- touring-quality race: 2× 250% → 1× 250% (libera 1 instância completa)
- thermal headroom: 95°C → ~85°C (8°C de folga)

#### FASE 2 — Estrutural (1-3 horas) — REDUZIR CPU NECESSÁRIA
- [ ] **touring-quality cache por hash do projeto** (TTL 24h para projetos inalterados)
- [ ] **cargo build distributed cache** (`.cargo-cache` compartilhado entre touring + konverter + analise)
- [ ] **Touring daemon write-coalescing**: batch writes ao graph.db para reduzir checkpoint frequency
- [ ] **pipeline_runner: incremental F3-F4** (já tem `--resume`, mas verificar que não reprocessa docs finalizados)

**Ganho esperado**:
- touring-quality: skip-able em 80%+ runs (cache hit) → economiza ~250% sustained
- Cargo incremental: build release 17min → 5-8min (cache compartilhado)
- thermal: 85°C → 70°C sustentado

#### FASE 3 — Hardware-aware (3-7 dias)
- [ ] **NUMA binding explícito**: P-cores (0-15) para código CPU-bound, E-cores (16-31) para I/O-bound
- [ ] **Migrar Chrome renderer pool para E-cores**: chrome já usa `--num-raster-threads=4`; forçar affinity via `taskset -c 16-31`
- [ ] **GPU offload p/ similarity search** (touring-quality tem `candle` + `wgpu` — buscar via GPU libera P-cores)

**Ganho esperado**:
- P-cores liberados para tarefas críticas
- GPU roda similarity search 10-50× mais rápido que CPU SIMD
- thermal: solução permanente (reduz TDP médio)

#### FASE 4 — Térmico sustentável (semanas)
- [ ] **Cooler upgrade / undervolt** (PL1/PL2 tuning via `intel_pstate` no_turbo ou ThrottleStop)
- [ ] **Power scheduler**: performance em horas ativas (08h-22h), powersave em idle noturno
- [ ] **Workload scheduling**: tours pesados (cargo release) em background noturno quando cooler tem headroom

---

## 4. Decisão imediata (recomendo)

**Para esta sessão**:
1. ✅ FASE 0 completa (governor + intel_pstate)
2. **AGORA**: Matar `touring-quality score analise` órfão (PID 2848184) — **race detected**
3. **AGORA**: Adicionar lock mutex para touring-quality score (`flock /tmp/touring-quality.lock` no script)
4. **AGORA**: pipeline_runner com `--workers 8` se re-invocado

**Próximo turno** (FASE 1 + 2):
- Implementar `touring-quality-score` wrapper script com singleton lock + pri scheduling
- Cache hash-based em `~/.claude/touring/quality_cache/`
- Distributed cargo cache entre workspaces

---

## 5. Limites da análise

- Não medi **per-core temperature** (só zone0/1/4 agregado) — não dá para saber qual core está hot
- Não verifiquei **thermal design power (TDP) atual vs PL1/PL2** — i9-14900HX tem PL1=55W, PL2=157W
- Não rodei **benchmarks antes/depois** para medir ganho numérico (próxima iteração)
- Não investiguei **CPU throttling events** no journal (pode estar throttling intermitente abaixo da visibilidade)

---

## 6. Memory store (registro de insight)

`touring-cpu-optimization-2026-08-31`:
- Hybrid CPU awareness (P/E cores)
- Thermal é limitador real (não governor)
- 2 touring-quality órfãos → race condition conhecida
- Pipeline runner é o monopolizador, não daemon
- intel_pstate com min_perf=15% cria floor artificial em idle

---

_TACO-loop-engineering OUTER step 7 (strategy persisted) | OUTER steps 1-5 (memory recall + diagnose + portfolio) | Composite Score unknown until FASE 1 measured_