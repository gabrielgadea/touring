# SPEC — Touring Web · Premium Refactor "Elite"

> **Versão**: 1.0 | **Data**: 12/06/2026 | **Autoridade**: Gabriel Gadea | **Autor**: TACO
> **Fontes da verdade**: `~/.claude/downloads/touring.zip` (41 artboards + 3 CSS, explorados exaustivamente in loco em `/tmp/touring-ux/`), estado atual do código (`crates/touring-bindings/src/web/` + `crates/touring-web/`), context7 (`/leptos-rs/book`).
> **Objetivo**: elevar o Touring Web ao padrão **Premium de Elite de Mercado** — um único design system, diagramação harmônica em todas as páginas, todas as telas do zip materializadas e 100% conectadas a dados reais.

---

## 0. Sumário executivo

O Touring Web hoje tem 18 rotas funcionais e 24 endpoints reais, mas **20 dialetos visuais** (um prefixo CSS por página: `ws- qd- qr- el- pl- ql- hud- st- wir- hk- chn- db- nd- srch- mem- stage- cg- hlt- fed- workflow-`) acumulados em 6.327 linhas de CSS. O zip de UX contém a resposta: três gerações de design (wireframes → hi-fi "Obsidian Observatory" → **Elite**), onde a série `hifi-elite-*` (14 artboards, a mais recente e numerosa) define uma linguagem **world-class**: Inter Tight única, warm ink, hairlines, espaço generoso, um só prefixo `.el-*`.

Esta SPEC canoniza a série **Elite** como design system único, especifica a migração página-a-página das 17 telas existentes, adiciona **5 superfícies novas** presentes no zip e ausentes no produto (`/mcp`, `/speculate`, `/wiring/impact`, command palette ⌘K, inspector tri-pane), define os **endpoints novos** necessários (todos mapeados a comandos `touring` CLI reais), e redesenha o **Workspace Graph** com fallback 2D para ambientes sem WebGL.

**Resultado esperado**: 22 rotas + 1 overlay global, 1 design system, CSS ≤ 2.500 linhas, zero dado fake, prova E2E por página.

---

## 1. Fontes da verdade — exploração do zip

### 1.1 Inventário (41 artboards, 3 folhas de estilo, 1 canvas)

| Série | Arquivos | Design system | Papel nesta SPEC |
|---|---|---|---|
| **Wireframes** `wf1-5` | 5 | `.wf-*` (paper) | **Metáforas estruturais** — 5 organizações globais avaliadas |
| **Hi-Fi** `hifi-*` | 22 | `.hf-*` "Obsidian Observatory" (Fraunces, denso, glow, titlebar 32 + statusbar 22) | **Densidade de dados e visualizações** — inventário máximo do que cada tela mostra |
| **Elite** `hifi-elite-*` | 14 | `.el-*` "world-class" (Inter Tight only, warm ink, hairlines, generous space) | **LINGUAGEM CANÔNICA** — tipografia, tokens, componentes, layout |

As 5 metáforas dos wireframes: **WF1 Cockpit** (NASA mission control, grid 5×4 denso), **WF2 Atlas** (grafo é a interface, painéis flutuantes), **WF3 Tri-pane** (DAW/Final Cut: library | canvas | inspector + transport), **WF4 Command** (Raycast keyboard-first), **WF5 Tab-panel** (Notion/Linear). A série Elite **sintetiza**: shell fixo com sidebar (WF5), páginas de visualização com right-rail inspector (WF3), palette ⌘K global (WF4), atlas gráfico (WF2). O cockpit denso (WF1) sobrevive como **dashboard**.

### 1.2 Decisão de linguagem: Elite vence

Evidências: (a) o `state.json` do canvas rotula a série anterior como "Elite · legacy overview" — a série `hifi-elite-*` é a iteração final; (b) 14 artboards Elite cobrem todas as superfícies; (c) o `elite.css` é o sistema mais disciplinado (1 prefixo, hairlines 0.06/0.10, 5 escalas de texto). A série Hi-Fi permanece como **referência de conteúdo** (que dados mostrar), não de forma.

**Consequência tipográfica**: **Fraunces sai**. O display canônico é **Inter Tight 500** com letter-spacing -0.04em e itálico para ênfase editorial (padrão do `hifi-elite-overview`: "Good *afternoon*, Gabriel"). O CSS atual (Wave 4) usa Fraunces — será substituído.

---

## 2. Diagnóstico do estado atual

### 2.1 O que existe (verificado in loco 12/06/2026)

- **18 rotas** em `app.rs`: `/` `/dashboard` `/quality` `/quality/rules` `/quality/diff` `/federation` `/health` `/orphans` `/wiring` `/workspace` `/search` `/memory` `/chains` `/hooks` `/plans` `/sessions` `/cognitive` `/settings`
- **24 endpoints** em `server/mod.rs`: `/api/{health,status,orphans,search,memory,memory/stats,gate-metrics,sessions,decompose,decompose/templates,decompose/ready,cognitive,cognitive/engines,wiring/modules,wiring/chains,viz/wiring/svg,viz/workspace,quality/signal,quality/rules/evaluate,quality/diff,quality/federation,quality/history,events}` + `/ws/quality` — todos shellam `touring` CLI real
- **10 componentes**: `page_chrome.rs` (PageHero/KpiStrip/KpiCell/Panel), `sidebar.rs` (17 itens, 6 seções), `error_boundary`, `heatmap`, `radar_chart`, `signal_card`, `sparkline`, `tables`, `theme_toggle`
- **CSS**: `touring-web/public/assets/styles/main.css` — 6.327 linhas, 20 prefixos de classe
- **Build**: Trunk (WASM ~11 MB dist) + axum (`touring-web-server`, porta 3000)
- **Workspace graph**: 3d-force-graph via JS interop (9 externs, 8 Effects), Pauling shells, sem fallback quando WebGL ausente

### 2.2 A desarmonia, quantificada

| Sintoma | Evidência |
|---|---|
| 20 mini design systems | `grep -oE '^\.[a-z]+-' main.css \| sort \| uniq -c`: ws-138, qd-67, qr-52, el-52, pl-43, ql-38, hud-36, st-34, wir-33, hk-31, chn-31, db-29, nd-25, srch-24, mem-22, stage-21, cg-19, hlt-18, fed-17, workflow-14 |
| Tipografia mista | Fraunces (Wave 4) convive com Inter Tight; tamanhos de display divergem por página |
| Tokens duplicados | accent/surfaces redeclarados por seção de página; hairlines variando 0.08–0.12 |
| Diagramação heterogênea | Sparkline/radar/barras implementados ad-hoc por página, sem primitivos compartilhados |
| `page_chrome` parcial | Hero/KPI harmonizados na Wave 4, mas o **corpo** das páginas mantém o dialeto antigo |

### 2.3 Gap de superfícies (zip → produto)

| Artboard | Rota atual | Status |
|---|---|---|
| elite/hifi `overview`, `cockpit`, `cockpit-dense` | `/dashboard` | existe, requer rediagramação |
| elite/hifi `atlas`, `atelier` | `/workspace` | existe, requer redesign + fallback 2D |
| elite/hifi `chain` | `/chains` | existe, requer view integrada ao atlas |
| elite/hifi `quality`, `quality-rules`, `quality-diff` | `/quality*` | existem, requerem migração elite |
| elite-pack `federation/health/hooks/orphans/search/cognitive` | rotas próprias | existem, requerem migração elite |
| elite/hifi `memory` | `/memory` | existe, **palácio isométrico ausente** |
| elite/hifi `sessions` | `/sessions` | existe, **detail view + ribbon ausentes** |
| elite/hifi `plans` | `/plans` | existe, requer board + tasks table |
| elite/hifi `settings` | `/settings` | existe, requer swatches + flags + macros |
| **elite/hifi `mcp`** | — | **NÃO EXISTE** |
| **elite/hifi `speculate`** | — | **NÃO EXISTE** |
| **elite `wiring` / hifi `wiring-impact`** | — | **NÃO EXISTE** (atual `/wiring` é lista de módulos) |
| **elite/hifi `command`** (⌘K) | — | **NÃO EXISTE** |
| **elite `tripane`** | — | **NÃO EXISTE** |

---

## 3. Design System canônico — "Elite"

### 3.1 Tokens (fonte: `styles/elite.css` do zip, verbatim, estendido)

```css
:root, [data-theme="dark"] {
  /* Surfaces */
  --el-ink:        #0a0a0d;   /* page background */
  --el-surface:    #111114;   /* card */
  --el-surface-2:  #16161b;   /* input / inset */
  --el-surface-3:  #1c1c22;   /* elevated */

  /* Hairlines */
  --el-line:        rgba(255,255,255,0.06);
  --el-line-strong: rgba(255,255,255,0.10);

  /* Text scale (5 degraus) */
  --el-fg:   #fafafa;  --el-fg-2: #d4d4d8;  --el-fg-3: #a1a1aa;
  --el-fg-4: #71717a;  --el-fg-5: #52525b;

  /* Semantic */
  --el-accent:      #5eead4;
  --el-accent-soft: rgba(94,234,212,0.10);
  --el-accent-hair: rgba(94,234,212,0.18);
  --el-pos:  #84cc16;  --el-neg: #f43f5e;  --el-warn: #f59e0b;
  --el-violet: #a78bfa;  /* secundária: wavefront, estados "prev", GoT/MCTS */
  --el-cyan:   #67e8f9;  /* terciária: profundidade 3, fontes de dados */

  /* Type */
  --el-display: 'Inter Tight', system-ui, sans-serif;
  --el-body:    'Inter Tight', system-ui, sans-serif;
  --el-mono:    'JetBrains Mono', monospace;
}
[data-theme="light"] {
  --el-ink: #fafaf8; --el-surface: #ffffff; --el-surface-2: #f4f4f2; --el-surface-3: #ececea;
  --el-line: rgba(10,10,13,0.08); --el-line-strong: rgba(10,10,13,0.14);
  --el-fg: #16161a; --el-fg-2: #2e2e33; --el-fg-3: #5b5b63; --el-fg-4: #8a8a92; --el-fg-5: #b3b3ba;
  --el-accent: #0d9488; --el-accent-soft: rgba(13,148,136,0.10); --el-accent-hair: rgba(13,148,136,0.22);
  /* pos/neg/warn/violet/cyan: mesmos hues, saturação -10% */
}
```

Regras globais: `font-feature-settings: 'ss01','cv11','cv02'` · `font-variant-numeric: tabular-nums` · `letter-spacing: -0.005em` body · `::selection` accent-soft · scrollbar invisível.

### 3.2 Escala tipográfica (obrigatória — fim dos tamanhos ad-hoc)

| Papel | Font | Size | Weight | Tracking | Uso |
|---|---|---|---|---|---|
| `display-hero` | Inter Tight | 56–96px (clamp por viewport) | 500 | -0.04em, lh 0.95 | h1 de página; itálico em 1 palavra para ênfase |
| `stat-hero` | Inter Tight | 88–140px | 500 | -0.04em, lh 1 | número composto do hero (tabular) |
| `display-2` | Inter Tight | 22–34px | 500 | -0.026em | h2 de seção |
| `stat` | Inter Tight | 26–34px | 500 | -0.04em | KPI value |
| `body` | Inter Tight | 13–14.5px | 400/500 | -0.005em | texto corrido |
| `eyebrow` | Inter Tight | 10.5px | 500 | +0.18em, uppercase, fg-4 | rótulo de seção/painel |
| `mono` | JetBrains Mono | 10.5–13px | 400–600 | — | código, IDs, métricas, paths |
| `table-th` | Inter Tight | 10.5px | 500 | +0.16em uppercase, fg-5 | cabeçalhos |

### 3.3 Componentes base `.el-*` (catálogo fechado — nada fora dele)

Do `elite.css` verbatim: `el-card` (surface, line, radius 12), `el-card-quiet` (transparente), `el-hr`, `el-btn` (30px) + `-primary` (fg sobre ink) + `-ghost` + `-sm` (24px), `el-kbd`, `el-tag` (22px pill) + `-on/-pos/-neg/-warn`, `el-pulse` (dot 7px animado 2.2s), `el-row` (grid, border-bottom hairline, padding 14 0), `el-icon` (16px stroke 1.5), `el-side` (30px nav item) + `el-side-section`, `el-prog-track/-fill` (2px), `el-titlebar` (44px), `el-tl` (traffic lights), `el-spark` (stroke 1.25), `el-table` (th uppercase 10.5, td 13px, hover 0.015).

**Extensões novas** (nomeadas, especificadas nesta SPEC): `el-shell` (grid global), `el-rail` (right rail 360px, fundo ink), `el-kpi-strip/-cell`, `el-hero`, `el-breadcrumb`, `el-result-row` (palette/search), `el-stage-dot` (pipeline), `el-toggle` (switch 28×16), `el-swatch` (theme preview), `el-ribbon` (event lanes), `el-transport` (scrubber bar).

### 3.4 Motion

- Transições de UI: 120–140ms (`background, color, border-color`); nada acima de 200ms em interação.
- `el-pulse`: 2.2s cubic-bezier(0.4,0,0.6,1) — único keyframe permanente.
- Partículas/wavefront (atlas, impact): `requestAnimationFrame` no canvas/SVG, pausável, **desligado sob `prefers-reduced-motion`**.
- Skeletons de Suspense: fade 150ms, sem shimmer agressivo.

### 3.5 Iconografia

SVG inline stroke 1.5 (estilo `el-icon`), 16px. Substituir os glifos unicode do sidebar atual (◇ ∿ ▤ ↻ ✧ ◎ ⚙) por ícones SVG consistentes — os artboards elite usam ícones lineares discretos. Manter unicode somente em `el-kbd` (⌘K, ⌥1) e marcadores AAAK (P/R/L/W/E).

---

## 4. Arquitetura de componentes (Leptos 0.8)

### 4.1 Shell global — `EliteShell`

Todos os artboards elite compartilham o wrapper. Componente único substitui a composição ad-hoc atual:

```
EliteShell
├── ElTitlebar (44px): traffic-lights · "Touring" · divider · Breadcrumb(workspace/página)
│   · SearchBox(320px, abre ⌘K) · bell ghost · Settings btn
├── grid: 232px sidebar | 1fr main | [360px right-rail opcional]
│   ├── ElSidebar (refactor do atual: mesmas 17+5 entradas, classes .el-side)
│   │   └── footer: daemon status (el-pulse) + user card (avatar gradient "G")
│   ├── main (overflow-y auto; padding 52px 64px; max-width 1280px center nas páginas editoriais)
│   └── ElRail (children; fundo --el-ink; seções com el-hr)
└── CommandPalette (overlay global, fechado por padrão)
```

Props: `rail: Option<Children>`, `breadcrumb: &'static str`. As páginas **não** instanciam mais sidebar/hero próprios.

### 4.2 Chrome v2 (evolução do `page_chrome.rs`)

- `PageHero` ganha: `title_em` já existe; adicionar `stat_hero` (número 88–140px com `/ 10 000` e delta), `actions: Children`.
- `KpiStrip/KpiCell`: manter API; visual = 4 colunas com hairline vertical entre células, padding 22 24 (artboard elite-overview).
- `Panel`: manter; header com eyebrow + `el-eyebrow-num` (numeração de painel opcional `01·`, `02·` — microdetalhe premium dos artboards).

### 4.3 Primitivos de diagramação (novos, SVG puro em Rust — fim do ad-hoc)

| Componente | Assinatura (props essenciais) | Usado em | Referência de artboard |
|---|---|---|---|
| `Sparkline` | `points: Signal<Vec<f64>>, w, h, area: bool` | dashboard, sessions, federation, health | elite-overview (480×80 hero; 90×26 metric) |
| `AreaChart` | `series, grid_y: Vec<f64>, annotations` | dashboard (signal evolution 900×200), hooks (latency) | hifi-elite §1 |
| `RadarChart` | `axes: Vec<RadarAxis>, overlay: Option<Vec<f64>>, bottleneck: Option<usize>` | quality (5 eixos), quality/diff (overlay prev violet dashed → curr accent), speculate (6 eixos + pesos) | elite-quality, elite-speculate |
| `EventRibbon` | `lanes: Vec<&str>, events: Vec<RibbonEvent{x,lane,w,color}>, playhead: Option<f64>` | dashboard (activity), sessions (hook ribbon 880×68), hooks (60s ribbon), tripane | elite-sessions, hifi-hooks-live |
| `DepthRings` | `depths: Vec<DepthRing{r,count,state}>, nucleus: &str` — elipses ry=rx·0.78, past/wavefront/future | wiring/impact, mini no dashboard | elite-wiring, hifi-wiring-impact |
| `PipelineStages` | `stages: Vec<Stage{label,count,state}>` — circles 10/14px, connector lines, glow no ativo | plans (VGP), speculate, dashboard | elite-plans |
| `IsoPalace` | `wings: Vec<Wing>` — polígonos isométricos skewX(-25), drawer expandido | memory | elite-memory, hifi-memory |
| `MiniBars` | `values, highlight_from` | dashboard activity (30 barras), pensieve | elite-overview |
| `ProgressTrack` | já existe como classe; componentizar com valor + delta | tabelas por eixo | vários |

Todos determinísticos (sem `Math.random`), dados via props reativas, cores apenas por token.

### 4.4 Padrões Leptos (context7 `/leptos-rs/book` — aplicar uniformemente)

1. **`LocalResource` + `<Suspense>` + `Suspend`**: cada página declara seus resources e usa UM `<Suspense>` por região com fallback skeleton `.el-card` — substitui os `match resource.get()` manuais com "…" espalhados. Para regiões que aguardam 2+ resources, `Suspend::new(async move { let a = a.await; let b = b.await; ... })`.
2. **`Memo`** para reshape caro de JSON (ex.: 142 counters do gate-metrics → top-30; 2.169 nodes do atlas → clusters). Hoje o reshape roda em closures de view repetidamente.
3. **`<For each=… key=…>`** em toda lista dinâmica (orphans, sessions, counters, results) — key estável (id/path), nunca índice.
4. **Context global**: `provide_context` no `App` para `RwSignal<Theme>` (já existe) + **novo** `WorkspaceCtx { project_path, daemon_status }` — elimina prop-drilling do path e habilita o breadcrumb/titlebar.
5. **Tick de refresh**: padronizar um `RefreshBus` (signal de tick + intervalo configurável em /settings) em vez dos timers per-página.

---

## 5. Inventário página-a-página (existentes → alvo Elite)

Formato: **Estado** → **Alvo** (layout do artboard de referência) · **Dados** · **Interações novas**.

### 5.1 `/dashboard` — Overview editorial
- **Alvo** (`hifi-elite-overview` + KPIs do `hifi-elite.jsx`): hero editorial "Good *afternoon*, Gabriel" (display 96, data em pt-BR) com `stat_hero` composite 140px + sparkline 480×80; "Next moves" (3 cards de sugestão com % e CTA); "The state of things" (4 metric cards com sparkline 90×26); "Activity" (EventRibbon 720×120, hooks por lane); "Pipeline" (PipelineStages VGP); right-rail: activity live (MiniBars 30) + system metrics + docs.
- **Dados**: `/api/quality/signal` (composite + delta), `/api/quality/history` (sparklines), `/api/status` (KPIs), `/api/gate-metrics` (activity), `/api/events` (live), **novo** `/api/suggest` (next moves — `touring wiring suggest --top 3 -j` + cycles).
- **Interações**: períodos 24h/7d/30d/90d; CTAs navegam (cycles→/quality, orphans→/orphans, drift→/health).

### 5.2 `/quality` — Radar 5 eixos
- **Alvo** (`hifi-elite-quality`): KPI strip (Composite/Bottleneck/Federation rank/Trend); grid `480px 1fr`: RadarChart 5 eixos (bottleneck com halo dashed warn) | tabela por eixo (progress 120px + delta) + card "Suggested action" accent-soft com botões (Wiring cycles · Speculate · Apply).
- **Dados**: `/api/quality/signal`, `/api/quality/history`. Já reais.
- **Interações**: clique no eixo → `/quality/rules` ancorado; Snapshot ⌘S → **novo** `/api/quality/snapshot` (`touring memory store quality:snapshot:…`).

### 5.3 `/quality/rules` — Editor TOML
- **Alvo** (`hifi-quality-rules` com pele elite): grid `1.4fr 1fr`: editor TOML com gutter de linhas e syntax tokens (.c/.k/.s/.n via spans) | cards de rules (dot pass/fail, severity pill) + painel de violações (card neg para deny).
- **Dados**: `/api/quality/rules/evaluate` (existe). **Novo**: GET/PUT `/api/quality/rules` (ler/salvar o TOML).
- **Interações**: avaliar agora; presets; salvar ⌘S (PUT); dry-run.

### 5.4 `/quality/diff` — Comparação de snapshots
- **Alvo** (`hifi-quality-diff` elite): picker panel 5 colunas (prev | Δ grande central | curr | divisor | ações swap/histórico); RadarChart overlay (prev violet dashed, curr accent solid, **setas de delta** entre vértices com marker SVG); delta table com knob triplo (marker prev violet + fill + marker curr accent); raw counters diff.
- **Dados**: `/api/quality/diff` (existe), `/api/quality/history` (picker).

### 5.5 `/federation` — Atlas cross-workspace
- **Alvo** (`hifi-federation` elite): header com aggregate μ/σ; 3 painéis (histograma bottleneck 5 barras | extremos worst/best | scatter de distribuição 240×80 com mean line dashed); tabela 7 colunas (signal com mini bar inline, worst row com tint neg).
- **Dados**: `/api/quality/federation` (existe; parse robusto Wave 3).

### 5.6 `/health` — Sinais vitais
- **Alvo** (`hifi-health` elite): 4 quick metrics no hero (daemon uptime, sessões, jobs, P50); **pools 4-grid** (Tokio/Rayon/Spawn-blocking/Semaphore com utilização — knob 6px); grid 3 colunas: gate counters + AreaChart latência (P50 violet/P95 warn dashed) | drift 4 métricas + alert box | OTLP/DB status list.
- **Dados**: `/api/health`, `/api/gate-metrics` (existem). **Novo**: `/api/learning/status` (`touring learning status`) para drift; pools via `/api/status`.

### 5.7 `/orphans`
- **Alvo** (`hifi-orphans` + elite-pack §8f): KPI (total/high-conf/LOC potencial/ganho); tabela 7 colunas (símbolo+dot conf, crate·kind, reason, idade, conf colorida, suggest pill, menu); right-rail: POR QUE É ÓRFÃO + sugestão de inline (code preview) + impacto (LOC/orphans/blast/quality) + ações.
- **Dados**: `/api/orphans` (existe). **Novo**: `/api/wiring/suggest?symbol=` (`touring wiring suggest`).
- **Interações**: download JSON (existe), filtros por suggest/conf, whitelist local (localStorage).

### 5.8 `/wiring` — Módulos & cadeias (mantém papel atual, pele elite)
- **Alvo**: tabela de módulos (integration_score com ProgressTrack, orphan_count, pub_symbols) + link "Impact →" por símbolo para a página nova `/wiring/impact`.
- **Dados**: `/api/wiring/modules` (existe).

### 5.9 `/workspace` — Atlas (ver §8, redesign dedicado)

### 5.10 `/chains` — Workflow Chain
- **Alvo** (`hifi-elite-chain`): KPI strip (selected/downstream/particles/cycles); SVG 6 lanes verticais (x: 130→1140) com headers NUCLEUS→EXTERNAL, edges Bézier cúbicas, blast cone do selecionado, partículas, wave ribbon; transport (⏮ ▸ ⏭, scrubber, speed 0.5–2×); hover card; right-rail: wave propagation, direção FAN-OUT/FAN-IN, depth slider, top wavefront.
- **Dados**: `/api/viz/workspace` (nodes+edges reais — derivar shells por grau) + `/api/wiring/chains`. O SVG atual de /chains evolui para este layout.

### 5.11 `/search` — Busca fundida
- **Alvo** (elite-pack §8e + `hifi-search`): search box grande (input 24–30px, glow accent-hair, ⌘K hint); pills de fonte (tudo/símbolos/código/memory/path); timing line (tantivy Xms · fused Yms); resultados agrupados por fonte com ícones ({} código ◧ memory / path); preview pane com snippet + ações (Wiring impact, Atlas focar).
- **Dados**: `/api/search` (existe, tantivy). **Novo**: `/api/memory/recall?q=` para a fonte memory (`touring memory recall`).

### 5.12 `/memory` — Palácio
- **Alvo** (`hifi-elite-memory`): KPI (vectors/drawers/diary/recall@10); breadcrumb palace_path colorido por nível; grid `1.1fr 1fr`: **IsoPalace** (wings→rooms→closets→drawer expandido com gaveta) | diary timeline com marcadores AAAK (P/R/L/W/E círculos 22px com glow); right-rail: ANN recall (input + results com score bar) + Pensieve/AutoSave.
- **Dados**: `/api/memory`, `/api/memory/stats` (existem). **Novo**: `/api/memory/recall?q=`. Palace hierarchy: derivar de namespaces das keys de memória (`wing:room:…` se presente; senão agrupar por prefixo de key — honesto, sem inventar).
- **Honestidade**: se a hierarquia real não tiver 4 níveis, o IsoPalace agrupa pelos níveis existentes e o rótulo diz "derived from key prefixes".

### 5.13 `/hooks` — Pipeline live
- **Alvo** (`hifi-hooks-live` elite): ribbon 60s (EventRibbon 1080×80, 4 lanes, playhead); tabela 18 hooks (hook/event pill/P50/P95/signals/flags, sort por latência); right-rail: histograma de latência (AreaChart com P50/P95 dashed) + hook selecionado (payload JSON colorido).
- **Dados**: `/api/gate-metrics` (142 counters + `*_latency{p50,p90,p99}` — já reais). Ribbon: `/api/events`.

### 5.14 `/plans` — Board VGP
- **Alvo** (`hifi-elite-plans`): KPI (active/in flight/auto-run/pending gate); tabela de plans (stage dot, progress, impact, ETA); tasks do plan selecionado (status circles ✓●⚠○, depends, CC); right-rail: PipelineStages VGP vertical + impact metrics.
- **Dados**: `/api/decompose`, `/api/decompose/ready`, `/api/decompose/templates` (JSON real desde 12/06). Tasks: **novo** `/api/decompose/task?id=` (`touring decompose get <id>`).

### 5.15 `/sessions` — Histórico rastreável
- **Alvo** (`hifi-sessions` elite, o artboard mais rico): grid `312px 1fr 320px`: lista de sessões (cards com status dot, dur/xch/Δq, mini sparkline) | detail (header + **EventRibbon** de hooks + tool calls 4 cards + files tocados + AAAK) | right-rail (quality drift sparkline, RL rewards, ações Resume/Export); transport bar de replay (fase 2 — apenas se houver fonte de eventos por sessão).
- **Dados**: `/api/sessions` (existe — 20 sessões com objective/task_type/timestamps). Detail: **novo** `/api/sessions/{id}` (`touring session assess <id>` -j). RL: `/api/learning/status`.
- **Honestidade**: ribbon de hooks por sessão só com dados reais (`session assess`); caso a CLI não exponha, a região mostra os campos disponíveis e omite o ribbon.

### 5.16 `/cognitive` — Camada preditiva
- **Alvo** (`hifi-cognitive` elite): header "Camada Preditiva"; grid 1.4fr 1fr: painel MCTS (tree SVG + best action + UCB decomp + stats) | GoT (graph + version selector); linha 2: Pensieve (AreaChart) | Pheromone (weighted edges) | SessionPredictor (bars).
- **Dados**: `/api/cognitive`, `/api/cognitive/engines` (existem). Probabilidades do predictor: usar campos reais de `cognitive engines`; o que não existir, omitir painel (sem fake).

### 5.17 `/settings`
- **Alvo** (`hifi-settings` elite): nav própria de seções (220px); tema (5 swatches `el-swatch` com preview de bandas de cor — Obsidian/Daylight ativos, demais `disabled` honesto); densidade/tipografia (sliders); comportamentos (refresh interval do RefreshBus, depth BFS default, partículas on/off); flags informativas (read-only do `/api/status`); rotas/links (existente); About.
- **Dados**: localStorage p/ preferências de UI; `/api/status` para flags.

---

## 6. Páginas novas (gap do zip — criar)

### 6.1 `/mcp` — MCP Tools executor ★ prioridade
- **Layout** (`hifi-elite-mcp`): KPI (tools/calls 24h/workers/selected); grid `280px 1fr`: catálogo (search + categorias com dot colorido + counts) | detail (título mono dual-color `touring_`+nome accent, tags Stable, params form com slider depth + format toggles, botões **Dry-run**/**Execute**, output JSON + equivalência CLI); right-rail: job queue + histórico + macros.
- **Dados (novos endpoints)**:
  - `GET /api/mcp/tools` — catálogo: shellar `touring` para listar (ou manifesto estático do crate `touring-server` — 22 tools curados pós-refactor W1-W4; fonte: `tools_status.rs`/manifest).
  - `POST /api/mcp/call {tool, args, dry_run}` — executar via CLI equivalente (cada tool curado tem comando CLI 1:1; **só whitelisted**, nunca shell arbitrário).
  - `GET /api/jobs` — `touring jobs list -j` (L7-B), se disponível.
- **Segurança**: whitelist server-side de tools→argv; validação de args por schema; dry-run default.

### 6.2 `/wiring/impact` — BFS Blast Radius ★ prioridade
- **Layout** (`hifi-elite-wiring` + `hifi-wiring-impact`): KPI (direct d=1 / wavefront d=2 / transitive / cycles touched); canvas SVG **DepthRings** (5 elipses ry=0.78rx, nucleus 34px, nodes por anel com glow no wavefront, orphan dashed, partículas outward); transport (depth scrubber d=1..5, play, speed); right-rail: nucleus card + BFS por depth (5 knobs) + top wavefront + filtros + comando CLI.
- **Dados (novo endpoint)**: `GET /api/wiring/impact?symbol=X&depth=N` — `touring wiring impact <symbol> --depth N` (comando TIER 3 real, shape `{direct_consumers, max_depth, consumers}`).
- **Entrada**: input de símbolo com autocomplete via `/api/search`; links chegando de /orphans, /search, /workspace.

### 6.3 `/speculate` — Diff especulativo
- **Layout** (`hifi-elite-speculate`): KPI (composite cur→pred, VGP stage, lift, risk); PipelineStages VGP horizontal; grid `380px 460px 1fr`: proposals (cards com conf bar) | RadarChart 6 camadas overlay (cur violet dashed → pred accent, pesos exibidos) | per-layer table; bottom: code diff (+/− tint) + impact prediction 8 métricas.
- **Dados (novo endpoint)**: `GET /api/speculate?file=&symbol=` — `touring shadow validate`/`touring suggest` (mapear shape real na implementação; expor camadas que a CLI retornar). Proposals: `touring wiring suggest --top N -j`.
- **Honestidade**: se a CLI não fornecer todas as 6 camadas, renderizar as reais e rotular o painel com as camadas disponíveis.

### 6.4 ⌘K Command Palette (overlay global) ★ prioridade
- **Layout** (`hifi-elite-command`): modal central 640px (search 30px com glow), pills de categoria, resultados agrupados (Páginas / Símbolos / Memória / Comandos read-only), preview com equivalência CLI; kbd hints (↑↓ ↵ esc).
- **Dados**: `/api/search` (símbolos) + `/api/memory/recall` + rotas estáticas (client-side). **Execução**: apenas navegação e comandos read-only whitelisted (mesma whitelist do /mcp).
- **Implementação**: componente global no `EliteShell`; listener `keydown` ⌘K/Ctrl+K; focus trap; `el-kbd` styling.

### 6.5 `/inspector` — Tri-pane (fase posterior, W6)
- **Layout** (`hifi-elite-tripane`): tabs de domínio; `280px 1fr 320px`: library (counts por domínio + filtros) | EventRibbon 5 lanes + symbol table | inspector (scored signals + ações + history); transport.
- **Dados**: composição de endpoints existentes (`search`, `gate-metrics`, `events`, `wiring/impact`). Só entra quando os primitivos estiverem maduros — é a página que mais reusa.

---

## 7. Mapa de APIs

### 7.1 Existentes (24) — inalteradas
Ver §2.1. Todas shellam CLI com `shell_touring_value`, erros JSON honestos (Wave 3: status codes 404/502/500 + body `{"error":…}`).

### 7.2 Novas (9, todas mapeadas a CLI real — zero invenção)

| Endpoint | Comando touring | Página |
|---|---|---|
| `GET /api/suggest` | `touring wiring suggest --top 3 -j` + `touring wiring cycles` | dashboard next-moves |
| `GET /api/memory/recall?q=` | `touring memory recall "<q>"` | memory, search, ⌘K |
| `GET /api/learning/status` | `touring learning status` | health (drift), sessions (RL) |
| `GET /api/wiring/impact?symbol=&depth=` | `touring wiring impact <s> --depth N` | wiring/impact, orphans |
| `GET /api/wiring/suggest?symbol=` | `touring wiring suggest <s>` | orphans rail |
| `GET /api/decompose/task?id=` | `touring decompose get <id>` | plans detail |
| `GET /api/sessions/{id}` | `touring session assess <id>` | sessions detail |
| `GET/PUT /api/quality/rules` | read/write do TOML de rules | quality/rules |
| `GET /api/mcp/tools` + `POST /api/mcp/call` | manifesto curado + whitelist argv | mcp, ⌘K |

Validação na implementação: **cada shape capturado por execução real antes de codar o parser** (lição F-01/Wave 4: o clap-derive removeu `-j` de vários subcomandos — capturar sem `-j` primeiro).

---

## 8. Workspace Graph — redesign do Atlas

### 8.1 Problemas atuais
(a) visual desarmônico com o resto (painéis `nd-*`/`ws-*` próprios); (b) **sem fallback quando WebGL indisponível** (canvas vazio — observado no Chrome MCP e em GPUs bloqueadas); (c) toolbar/inspector não-elite.

### 8.2 Alvo (síntese `hifi-elite-atlas` + `hifi-atelier` + WF2)
- **Shell**: EliteShell com rail 360px. Hero: "Forty-five crates, one sphere." (display, count real).
- **Canvas 3D**: manter 3d-force-graph (interop preservado verbatim — 9 externs, 8 Effects, ResizeObserver). Recolorir via aspect lens: **Pauling** (shell/depth), **Quality**, **Cognitive**, **Safety**, **Centrality** — 5 lentes dos artboards; cores SEMPRE dos tokens.
- **Fallback 2D obrigatório**: detectar WebGL (`canvas.getContext('webgl2'||'webgl')`); se ausente → render SVG 2D determinístico das Pauling shells (elipses ry=0.78rx + top-N nodes por grau + edges curvos Bézier quadráticas — geometria do `hifi-elite-atlas`), com banner discreto "2D fallback · WebGL unavailable".
- **Left layers panel** (220px no grid interno): shells checkboxes (counts reais por profundidade), comunidades (Louvain se disponível no payload; senão clusters por crate), cycles list.
- **Right rail**: inspector do nó (dossier atual re-skinned: quality/cognitive/fan-in/out reais), aspect lens segmented, top crates, minimap 2D (SVG 160×80 com viewport rect).
- **Chain view**: botão "WORKFLOW CHAIN" alterna para `/chains` (mesma seleção via query param `?focus=`).

### 8.3 Dados
`/api/viz/workspace` (2.169 nodes/170.710 edges reais). Derivações client-side com `Memo`: grau → shell; top-N para fallback 2D (cap 120 nodes); clusters por prefixo de crate.

---

## 9. Plano de migração CSS

### 9.1 Estrutura alvo (substitui o monólito 6.327L)

```
styles/
  tokens.css        (~160L)  :root dark/light, type scale, motion vars
  elite.css         (~420L)  catálogo .el-* fechado (§3.3 + extensões)
  charts.css        (~180L)  classes dos primitivos SVG (spark, radar, ribbon, rings, iso)
  pages.css         (~400L)  apenas grids específicos por página (prefixo .pg-<rota>-)
```
**Meta: ≤ 2.500 linhas totais** (hoje 6.327). Os 20 prefixos morrem; grep-gate no CI da migração: `grep -cE '^\.(ws|qd|qr|pl|ql|hud|st|wir|hk|chn|db|nd|srch|mem|stage|cg|hlt|fed|workflow)-' == 0` ao final.

### 9.2 Estratégia (sem big-bang)
1. **W1** introduz `tokens.css` + `elite.css` novos **ao lado** do main.css (cascade: novos depois).
2. Cada página migrada troca dialeto→`.el-*`/`.pg-*` e **deleta** seu bloco antigo do main.css na mesma wave.
3. Última wave: main.css morre; trunk aponta para os 4 arquivos (ou concatena no build).
4. Gate por página: screenshot diff manual + contrato de classes (script existente da Wave 4, atualizado para o catálogo fechado).

---

## 10. Acessibilidade, performance, theming

- **A11y**: foco visível (outline accent-hair 2px); navegação por teclado na palette e tabelas (↑↓ ↵); `aria-label` em botões-ícone; contraste AA verificado para fg-3 sobre surface (a1a1aa/111114 = 4.6:1 ✓); `prefers-reduced-motion` desliga partículas/pulse.
- **Perf**: `Memo` para reshapes; `<For>` keyed; cap de nodes no fallback 2D (120) e no ribbon (200 eventos); WASM — verificar peso pós-refactor (meta: não crescer >5% vs 11 MB; remover Fraunces do font preload corta ~80 KB).
- **Theming**: dark default; light completo via `[data-theme]` (§3.1); swatches extras em /settings marcados "soon" desabilitados (honesto).

---

## 11. Fases de implementação (DAG)

```
W1 Foundation ──► W2 Editorial ──► W3 Instrumentos ──► W4 Novas páginas ──► W5 Atlas ──► W6 Polimento
```

| Wave | Escopo | Entregáveis | Gate |
|---|---|---|---|
| **W1 Foundation** | tokens.css + elite.css; EliteShell + Titlebar + Breadcrumb + Sidebar elite; Chrome v2; primitivos `Sparkline/AreaChart/RadarChart/ProgressTrack`; WorkspaceCtx + RefreshBus; ⌘K palette esqueleto (navegação) | shell global aplicado a 1 página piloto (/quality) | cargo check + trunk + 25 lib tests + contrato CSS + Chrome prova |
| **W2 Editorial** | /dashboard overview editorial; /federation; /health (+ `/api/learning/status`); /settings | 4 páginas elite + 1 endpoint | idem + curl shapes |
| **W3 Instrumentos** | EventRibbon + PipelineStages + DepthRings + IsoPalace; /hooks; /sessions (+ `/api/sessions/{id}`); /plans (+ task endpoint); /memory (+ recall); /quality/rules (+ GET/PUT); /quality/diff overlay; /orphans; /search; /wiring | 10 páginas + 4 endpoints | idem |
| **W4 Novas** | /mcp (+ tools/call/jobs, whitelist) ; /wiring/impact (+ endpoint); /speculate (+ endpoint); ⌘K fontes completas | 3 rotas novas + 4 endpoints | idem + segurança whitelist testada |
| **W5 Atlas** | /workspace redesign + fallback 2D + lenses; /chains elite integrado | 2 páginas | prova com e sem WebGL |
| **W6 Polimento** | /inspector tripane; kill main.css; varredura A11y/motion; screenshots finais de todas as 22 rotas | gate final | grep-gate prefixos = 0; e2e doctor |

Paralelismo: dentro de W2/W3, páginas são independentes → engineers paralelos (padrão 6-engineers da Wave 4: contratos de componente verbatim + arquivos exclusivos + CSS staging).

---

## 12. Riscos e mitigações

| Risco | Sev | Mitigação |
|---|---|---|
| Shapes CLI divergem do esperado (clap-derive) | ALTA | Capturar shape por execução ANTES do parser (lição F-01); erros JSON honestos |
| `/api/mcp/call` vira shell arbitrário | ALTA | Whitelist server-side tool→argv fixa; args validados por schema; dry-run default; nunca interpolar string crua |
| prettyplease mangleia `view!` nos generators | MÉDIA | `--no-format` / staging restore (gotcha registrado na Wave 4) |
| Regressão visual durante migração incremental | MÉDIA | Cascade nova depois da antiga; deleção do bloco antigo só no gate da página |
| WASM cresce com primitivos novos | BAIXA | Primitivos são SVG puro (sem deps novas); medir dist por wave |
| Dados ausentes para artboards ricos (palace 4 níveis, replay) | MÉDIA | Política de honestidade: renderizar o que a CLI dá, rotular derivações, omitir painéis sem fonte — **nunca** mockar |

---

## 13. Evidências e referências

- Exploração: `/tmp/touring-ux/` (zip extraído) · relatórios de extração `/tmp/touring-ux/REPORT-elite.md` (14 artboards) e `REPORT-hifi2.md` (15) + relatório hifi-1 (11) na sessão de 12/06/2026.
- Screenshot real do produto no zip: `uploads/Screenshot_2026-05-14_09-11-46.png` (o /workspace atual — ponto de partida do redesign).
- Estado atual: `crates/touring-bindings/src/web/{app.rs,routes/,components/,server/mod.rs,services/mod.rs}` + `crates/touring-web/public/assets/styles/main.css` (6.327L) — verificados 12/06/2026.
- Context7: `/leptos-rs/book` — LocalResource+Suspense+Suspend, provide_context/use_context, `<For>` keyed (citados em §4.4).
- Waves anteriores: Wave 3 (gaps de endpoint/erros honestos) e Wave 4 (page_chrome, 6 páginas novas, 6-engineers paralelos) — memórias `wave3:touring-web-gap-closure-2026-06-12`, `wave4:touring-web-premium-2026-06-12`.

---

_SPEC v1.0 — pronta para `taco-forge plan --quality high` por wave. Nenhuma linha desta SPEC autoriza dado fake: a régua é a mesma das Waves 3-4 — tudo conectado, tudo provado._
