# Touring-Autopilot — Expansion Plan (External Workspaces)

> **Status**: SPEC v1.0 (constitutional draft) — companion document to `2026-04-25-touring-autopilot-master-plan.md`
> **Author**: TACO (Claude Opus 4.7) under Gabriel's authorization
> **Date**: 2026-04-25
> **Risk class**: HIGH (multi-tenant; privacy boundaries; cross-workspace state)
> **Authorization required**: explicit Gabriel opt-in per phase
> **Lineage**: this doc EXTENDS the autopilot master plan; nothing here contradicts the master, only adds external-workspace dimensions.

---

## 0. Sumário executivo

**Problema**: o autopilot master plan (v1.0) é monolítico em torno de `~/.claude/rust/`. Operacionalmente isso significa que o autopilot só ajuda Gabriel quando ele edita touring; quando Gabriel + Claude Code trabalham em `analise/`, `kazuba-cargo/`, `templates/holon-*/`, ou qualquer projeto novo — o autopilot é invisível.

**Tese**: se a abstração "external workspace" não for considerada DESDE A FUNDAÇÃO, 80% dos detectors (D01-D15) precisarão de rewrite quando autopilot expandir para fora. Cada um assume hoje: schemas SQLite touring, RustQualitySignals (Rust-only), `cargo deny` advisories, `diff_api_surfaces` (syn-only). Repensar pós-Phase H = 6 semanas de retrabalho.

**Solução** (precision 0.95): introduzir 4 abstrações desde Phase A do master plan:
1. **`WorkspaceProfile` trait** — substrato polyglot que cada detector consome em vez de `&HookRuntime` direto
2. **`WorkspaceCapability` enum** — detector declara o que precisa (`AstParsing`, `MutationTesting`, `WiringIndex`, ...); workspace declara o que oferece via THSF manifest
3. **Per-workspace policy + state isolation** — cada projeto tem sua `.autopilot/` dir (analogia LSP `scopeUri`)
4. **Hybrid daemon model** — single autopilot daemon serving N workspaces (Modo D) + per-workspace capabilities via THSF (Modo E)

**Princípio rector**: **One autopilot, N workspaces, zero cross-contamination by default.** Cada workspace é uma fronteira de privacidade, RL state, policy, e gotcha library. Cross-workspace transfer learning é opt-in declarativo.

**Custo de antecipar**: introduzir as 4 abstrações no master plan custa ~2 semanas extras em Phase A+B. Adiar até post-H custa ~6 semanas + risco de design débito permanente. ROI claro.

**Inspiração canônica** (Context7-verified, confidence 1.0): LSP `workspace/workspaceFolders` + `workspace/configuration` (scopeUri) + `workspace/didChangeWorkspaceFolders` — patterns maduros para multi-root tools. THSF (operacional 2026-04-23/24, Fases 1-8 completas) é o mecanismo de declaração de capabilities preservando autonomia per-projeto.

---

## 1. Linhagem (genealogia da decisão)

| Momento  | Decisão                                                                                              |
|----------|------------------------------------------------------------------------------------------------------|
| Original A1 (plano 2026-04-24)        | "Autonomous detect-propose loop" — escopo não definido                          |
| Master plan (2026-04-25 §0)           | Escopo = `~/.claude/rust/` (workspace touring) — assumido implícito             |
| **Expansion plan (este doc)**         | Escopo = **N workspaces concorrentes** com isolation por default                |
| Phase A do master plan **(antes)**    | Detectors recebem `&HookRuntime`                                                |
| Phase A do master plan **(depois)**   | Detectors recebem `&dyn WorkspaceProfile` que internamente delega a `HookRuntime` quando workspace = touring; ou a outras providers quando workspace = externo |

Esta mudança é **não-quebrante** — o master plan permanece válido. O expansion plan adiciona uma camada de indireção que torna detectors agnósticos a workspace type.

---

## 2. Cinco modos de operação possíveis (decisão arquitetural)

### Trade-off matrix

| Modo | Descrição                                                         | Daemon count | RL state    | Privacy isolation | THSF reuse | Recomendação |
|------|-------------------------------------------------------------------|--------------|-------------|-------------------|------------|--------------|
| A    | **Internal**: só `~/.claude/rust/`                                | 1            | global      | N/A               | none       | atual MVP    |
| B    | **Embedded**: 1 daemon embebido por target workspace              | N            | per-ws      | strong (process)  | none       | DESCARTADO — duplicação de daemon recursos |
| C    | **Project-native**: sidecar binary instalado per-project          | N            | per-ws      | strong (process)  | none       | DESCARTADO — Gabriel não quer instalação per-projeto |
| D    | **Cloud-shared**: 1 daemon serve N workspaces                     | 1            | per-ws      | application-level | optional   | base recomendada |
| E    | **THSF-integrated**: capabilities per-projeto via `.holon/`       | varies       | per-ws      | strong (manifest) | mandatory  | overlay sobre D |
| **D+E híbrido (RECOMENDADO)** | 1 daemon (D) + per-project capability declaration (E) | 1            | per-ws      | strong (manifest+app) | full       | **adotar**   |

### Por que híbrido D+E venceu (precision 0.85)

1. **D fornece economia**: 1 daemon = menos RAM, 1 socket, 1 actor pool, 1 lugar para upgrade.
2. **E fornece autonomy preservation**: cada projeto declara o que oferece via `.autopilot/manifest.toml` (extensão do `.holon/manifest.toml`). Projeto pode revogar `autonomy_guarantee=true` apagando `.autopilot/`.
3. **Privacy isolation por default**: workspace registry com per-uri state isolation; nenhuma RL/finding/memory cross-leaks sem `--share-with` flag explícito.
4. **THSF Fases 1-8 estão done** (verificado memória `MEMORY.md`): infra de discovery, capability handshake, CRDT logging, MCP server stdio. Reuso integral.

---

## 3. As 4 abstrações fundamentais (introduzidas em Phase A)

### 3.1 `WorkspaceProfile` trait

```rust
// crates/touring-hooks/src/autopilot/workspace.rs
pub trait WorkspaceProfile: Send + Sync {
    /// Stable workspace identifier (URI form, mirroring LSP).
    fn uri(&self) -> &WorkspaceUri;

    /// Human-readable name (for logs + Gabriel-facing UI).
    fn name(&self) -> &str;

    /// Predominant language(s) — first is primary.
    fn languages(&self) -> &[Language];

    /// What the workspace can offer (negotiated at register time).
    fn capabilities(&self) -> &WorkspaceCapabilities;

    /// Per-workspace policy.toml + autonomy levels.
    fn policy(&self) -> &WorkspacePolicy;

    /// Workspace-scoped touring runtime (when capability available).
    /// Returns None when the workspace is not a touring-managed workspace.
    fn touring_runtime(&self) -> Option<&HookRuntime>;

    /// Workspace-scoped SQLite ledger path for autopilot state.
    fn autopilot_db_path(&self) -> &Path;

    /// Workspace-scoped cache root.
    fn cache_root(&self) -> &Path;
}

pub struct WorkspaceUri(pub String);   // file:///path/to/project — LSP-compatible

pub enum Language {
    Rust, Python, TypeScript, JavaScript, Go, C, Cpp, Java,
    Bash, Markdown, Yaml, Toml, Other(String),
}
```

**Two impls ship in MVP:**

- `TouringWorkspaceProfile` — primary case, wraps existing `HookRuntime`. Used for `~/.claude/rust/` and any other Rust workspace that has touring fully indexed.
- `GenericWorkspaceProfile` — fallback for any workspace. Limits to filesystem scanning + ast-grep + universal detectors (D15 todo.stagnant, D09 gotcha.match via universal rules, D16 format.drift via available formatters).

### 3.2 `WorkspaceCapability` enum (declarative)

```rust
pub enum WorkspaceCapability {
    /// touring-index symbol database operational
    SymbolIndex,
    /// touring-ast multi-language AST extraction
    AstParsing(Vec<Language>),
    /// touring-analysis quality scoring (Halstead, MI, CC)
    QualityScoring(Vec<Language>),
    /// touring-analysis wiring DB (orphan/cycle/blast)
    WiringGraph,
    /// cargo-mutants + touring mutation-test wrapper
    MutationTesting,
    /// touring health-delta singleton (per-workspace process)
    HealthDelta,
    /// gotcha rule library (Q3 YAML format)
    GotchaLibrary { rules_dir: PathBuf },
    /// language-specific formatter
    Formatter { language: Language, tool: String },
    /// any cargo-deny / pip-audit / npm audit equivalent
    SupplyChainAudit { tool: String },
    /// llvm-cov / pytest-cov / nyc — coverage data
    CoverageReporter { tool: String, threshold: f32 },
    /// any test runner (cargo nextest, pytest, jest, ...)
    TestRunner { tool: String },
}

pub struct WorkspaceCapabilities {
    inner: HashSet<WorkspaceCapability>,
}

impl WorkspaceCapabilities {
    pub fn has(&self, cap: &WorkspaceCapability) -> bool { ... }
}
```

Each detector declares its required capabilities at registration time:

```rust
impl AutopilotDetector for QualityRegressionDetector {
    fn required_capabilities(&self) -> Vec<WorkspaceCapability> {
        vec![
            WorkspaceCapability::AstParsing(vec![Language::Rust, Language::Python, Language::TypeScript]),
            WorkspaceCapability::QualityScoring(vec![Language::Rust, Language::Python, Language::TypeScript]),
            WorkspaceCapability::HealthDelta,
        ]
    }
}
```

A detector with unmet capabilities for a given workspace **silently skips** that workspace (no panic, no error). This is the "graceful degradation across heterogeneous workspaces" pattern — confidence 0.9 that this is the right default.

### 3.3 Per-workspace `.autopilot/` directory

Mirroring LSP `scopeUri` per-workspace configuration:

```
<workspace>/
├── .autopilot/
│   ├── manifest.toml          # capabilities + provided detectors (extends .holon/)
│   ├── policy.toml            # autonomy levels per category (cf. master §8)
│   ├── conventions.toml       # formatter/linter/test preferences (NEW — see §6)
│   ├── pinning.toml           # tool versions (rustfmt, prettyplease, black, ...)
│   ├── snoozes.json           # active suppression list
│   └── state/
│       ├── findings.db        # SQLite ledger (autopilot_findings, _proposals, ...)
│       ├── rl_weights.bin     # rkyv-serialized LinUCB state for THIS workspace
│       └── memory.db          # workspace-scoped touring memory (lessons, patterns)
```

`manifest.toml` extends THSF schema:

```toml
# Extends .holon/manifest.toml — autopilot section is optional
[holon.identity]
name = "kazuba-geo-engine"
version = "1.0.0"
autonomy_guarantee = true

[autopilot]                                    # NEW SECTION
schema_version = "1.0"
opt_in = true                                  # explicit — autopilot does nothing without this
managed_languages = ["python", "rust"]

[autopilot.capabilities]
ast_parsing = true                             # auto-detected; can override
quality_scoring = true
wiring_graph = false                           # this project doesn't have touring index
mutation_testing = false                       # cargo-mutants not relevant for Python
gotcha_library = ".autopilot/gotchas/"        # path to YAML rules

[autopilot.policy]
default_autonomy = "L0"                        # invisible until enable
quiet_hours = "22:00-08:00"
max_proposals_per_session = 3

[autopilot.policy.categories."quality.regression"]
autonomy = "L1"
max_per_session = 2

[autopilot.conventions]                        # NEW (cf. §6)
python_formatter = "ruff"                      # not "black"
typescript_formatter = "biome"                 # not "prettier"
test_runner = "pytest"
linter = "ruff"

[autopilot.privacy]
share_rl_with = []                             # no cross-workspace RL transfer
share_findings_with = []                       # findings stay local
```

### 3.4 Workspace registry

Daemon-side singleton mapping `WorkspaceUri → WorkspaceProfile`:

```rust
// crates/touring-hooks/src/autopilot/registry.rs
pub struct WorkspaceRegistry {
    profiles: DashMap<WorkspaceUri, Arc<dyn WorkspaceProfile>>,
}

impl WorkspaceRegistry {
    pub fn register(&self, profile: Arc<dyn WorkspaceProfile>) -> Result<()>;
    pub fn unregister(&self, uri: &WorkspaceUri) -> Result<()>;
    pub fn get(&self, uri: &WorkspaceUri) -> Option<Arc<dyn WorkspaceProfile>>;
    pub fn list(&self) -> Vec<WorkspaceUri>;

    /// LSP-equivalent: workspace/didChangeWorkspaceFolders
    pub fn handle_change(&self, added: Vec<WorkspaceUri>, removed: Vec<WorkspaceUri>);
}
```

CLI surface:

```bash
touring autopilot workspace register <path> [--manifest <path>]
touring autopilot workspace list [-j]
touring autopilot workspace unregister <uri>
touring autopilot workspace activate <uri>      # set as "current" for default scope
touring autopilot workspace info <uri>          # capabilities + policy + counters
```

---

## 4. Polyglot detector dispatch

Today every detector spec assumes Rust (master §5). The expansion forces every detector to dispatch by language.

### 4.1 The dispatch trait

```rust
pub trait LanguageQualityProvider: Send + Sync {
    fn supports(&self, language: Language) -> bool;
    fn compute_quality(&self, source: &str, lang: Language) -> Option<QualityScore>;
}

pub struct QualityProviderRegistry {
    providers: Vec<Arc<dyn LanguageQualityProvider>>,
}

impl QualityProviderRegistry {
    pub fn pick(&self, lang: Language) -> Option<&dyn LanguageQualityProvider> {
        self.providers.iter().find(|p| p.supports(lang)).map(|p| p.as_ref())
    }
}
```

**Three providers ship in MVP:**

| Provider                         | Languages                       | Backend                                              |
|----------------------------------|----------------------------------|------------------------------------------------------|
| `RustSynQualityProvider`         | Rust                            | `RustQualitySignals` (existing W7-8)                 |
| `TreeSitterQualityProvider`      | Python, TS, JS, Go, C, C++, Java| `touring-ast-polyglot` (existing W6 multi-lang)      |
| `GenericLineCountProvider`       | any (fallback)                  | LOC + cyclomatic estimate via brace counting (cheap) |

### 4.2 Per-detector dispatch example

```rust
impl AutopilotDetector for QualityRegressionDetector {
    fn scan(&self, profile: &dyn WorkspaceProfile, scope: ScanScope) -> Vec<Finding> {
        let files = scope.files_in(profile);
        let mut findings = Vec::new();
        for file in files {
            let lang = Language::from_extension(file.extension());
            let provider = match self.quality_registry.pick(lang) {
                Some(p) => p,
                None => continue,        // skip unsupported language — graceful
            };
            let source = match fs::read_to_string(&file).ok() { Some(s) => s, None => continue };
            let curr = match provider.compute_quality(&source, lang) { Some(q) => q, None => continue };
            // ... compare against baseline + emit Finding
        }
        findings
    }
}
```

This collapses 8 language-specific detectors into 1 dispatched detector. Massive code reduction + uniform UX.

### 4.3 Formatter dispatch (D16 polyglot extension)

```rust
pub trait LanguageFormatter: Send + Sync {
    fn supports(&self, language: Language) -> bool;
    fn format_in_memory(&self, source: &str) -> Result<String>;   // semantic-preserving
    fn version(&self) -> &str;                                     // for pinning
}

pub struct FormatterRegistry {
    formatters: Vec<Arc<dyn LanguageFormatter>>,
}
```

Implementations:

| Formatter                    | Language     | Backend                                          |
|------------------------------|--------------|--------------------------------------------------|
| `PrettypleaseRustFormatter`  | Rust         | `touring_ast::format_rust_code` (D16 MVP)        |
| `RuffPythonFormatter`        | Python       | `ruff format` subprocess (in-memory via stdin)   |
| `BlackPythonFormatter`       | Python       | `black -` subprocess                             |
| `BiomeTSFormatter`           | TS/JS        | `biome format` subprocess                        |
| `PrettierTSFormatter`        | TS/JS        | `prettier --stdin-filepath foo.ts` subprocess    |
| `GofmtGoFormatter`           | Go           | `gofmt` (built-in)                               |
| `ClangFormatCFormatter`      | C/C++        | `clang-format` subprocess                        |

Per-workspace `conventions.toml` selects which formatter to use when multiple are available:

```toml
[autopilot.conventions]
python_formatter = "ruff"    # not "black"
typescript_formatter = "biome"
```

---

## 5. Per-workspace RL isolation + transfer learning option

### 5.1 Isolation by default (privacy first)

The master plan (§9) describes anti-loop guarantees within ONE workspace. Multi-workspace forces explicit privacy:

- **`<workspace>/.autopilot/state/rl_weights.bin`** — LinUCB arm weights stored PER-WORKSPACE. Daemon loads on workspace register; saves on unregister + every 5min.
- **No cross-workspace reads by default** — bandit for `analise/` cannot influence bandit for `kazuba-cargo/`.
- **Per-workspace gotcha library** — `analise/.autopilot/gotchas/*.yaml` not visible from another workspace.
- **Per-workspace memory** — `analise/.autopilot/state/memory.db` is sandboxed.

### 5.2 Cold-start protocol (14-day L0-L1 init)

When a NEW workspace is registered:

1. **Day 0**: all categories pinned to **L0** (invisible). Detectors run, populate ledger, but no surfacing.
2. **Day 1-7**: `touring autopilot list --workspace <uri>` available (Gabriel can pull). Still no proactive surfacing.
3. **Day 7**: autopilot generates a **calibration report**: detected category frequency, confidence distribution, would-have-surfaced count.
4. **Day 14**: Gabriel can issue `touring autopilot workspace promote <uri>` to bump default autonomy from L0 to L1 globally for that workspace.

This avoids the "100 false positives in first session destroys trust forever" failure mode.

### 5.3 Optional cross-workspace transfer learning (opt-in)

If Gabriel determines two workspaces share enough characteristics (e.g. both Python data science projects), he can opt-in:

```bash
touring autopilot workspace share-rl <source-uri> <target-uri> --categories quality.regression,complexity.spike
```

Mechanism: source workspace's LinUCB weights for those categories are linearly blended into target's weights with shrinkage factor 0.3 (don't dominate target's own learning). Audit trail in `autopilot_policy_changes`.

**Default = no sharing.** Opt-in is per-category, not per-workspace, to avoid leaking irrelevant signals.

---

## 6. Project conventions schema (`.autopilot/conventions.toml`)

This is the per-workspace customization Gabriel needs. Without it, autopilot would impose its preferences on every project.

### 6.1 Schema

```toml
[autopilot.conventions]
# Language-specific tooling preferences (autopilot dispatches accordingly)
rust_formatter = "prettyplease"          # vs "rustfmt"
python_formatter = "ruff"                # vs "black"
python_linter = "ruff"                   # vs "pylint" / "flake8"
typescript_formatter = "biome"           # vs "prettier"
typescript_linter = "biome"              # vs "eslint"
go_formatter = "gofmt"
c_formatter = "clang-format"

# Test runners (D04, D11)
rust_test_runner = "cargo nextest"       # vs "cargo test"
python_test_runner = "pytest"            # vs "unittest"
typescript_test_runner = "vitest"        # vs "jest"

# Coverage tools (D11)
rust_coverage = "cargo llvm-cov"
python_coverage = "pytest-cov"
typescript_coverage = "c8"

# Mutation testing (D04 — only Rust today, more later)
mutation_testing_tool = "cargo-mutants"

# Supply-chain audit (D10)
rust_supply_chain = "cargo deny"
python_supply_chain = "pip-audit"
typescript_supply_chain = "npm audit"

# Doc style (D06 — future)
rust_doc_style = "rustdoc"
python_doc_style = "google"              # vs "numpy" / "rest"
typescript_doc_style = "tsdoc"           # vs "jsdoc"

# Quality thresholds (overrides global defaults — master §6)
[autopilot.conventions.thresholds]
quality_regression_drop_threshold = 0.10  # workspace tolerates more drift
mutation_kill_rate_minimum = 60.0         # different from default 50
complexity_cc_max = 20                    # higher than default 15

# Categories to disable for THIS workspace (overrides defaults)
[autopilot.conventions.disabled_categories]
"docs.drift" = true                       # this project intentionally has terse docs
"todo.stagnant" = false                   # we keep TODO list intentionally stagnant
```

### 6.2 Inheritance + override semantics

1. **Defaults** in `~/.autopilot/global-conventions.toml` (Gabriel's user-wide preferences).
2. **Workspace** overrides via `<workspace>/.autopilot/conventions.toml`.
3. **Session** overrides via `--convention key=value` CLI flag (one-shot).

Resolution order: session > workspace > global > built-in defaults.

---

## 7. Hard Rule #11 externalization

Master plan §1 has Hard Rule #11 baked in: "autopilot NUNCA invoca git". This is Gabriel-specific to `~/.claude/rust/`. In external projects:

- Other developers may use git normally.
- Autopilot proposals can suggest git operations (e.g. "run `git diff main` before applying this refactor").

### 7.1 Per-workspace policy

```toml
[autopilot.policy.rules]
forbid_git = false                # default for external projects
prefer_touring_memory = true      # still prefer touring memory for history when available

# For ~/.claude/rust/ specifically:
# .autopilot/policy.toml has:
#   forbid_git = true
#   prefer_touring_memory = true
```

### 7.2 Validation step in proposer

When generating a proposal, the proposer checks `policy.rules.forbid_git`. If `true` AND `suggested_action` references git → auto-reject the proposal (do not surface). If `false` → allow.

This preserves Hard Rule #11 in the touring workspace while liberating autopilot for general use.

---

## 8. THSF integration deep-dive (Modo E)

### 8.1 Why THSF is the right substrate

THSF (Fases 1-8 entregues, 2026-04-23/24) provides:
- `.holon/manifest.toml` for declarative capability discovery
- `holon discover <root>` walks filesystem finding all manifests
- `holon invoke <name> <capability> <args>` calls the right adapter
- CRDT-logged invocations + `autonomy_guarantee=true` invariant
- Multi-transport (cli, capnp, wasm)

Autopilot extends, doesn't replace.

### 8.2 New THSF capability namespace: `autopilot.*`

Each project can declare custom autopilot extensions:

```toml
# kazuba-geo-engine/.autopilot/manifest.toml
# (extends THSF .holon/manifest.toml)

[autopilot.offers."detector.geo_data_consistency"]
schema = "schemas/geo-detector.json"
adapter = "cli"
adapter_cmd = ".autopilot/adapters/geo-data-consistency.py"
description = "Detects inconsistencies in geographic data files (CRS, projections, geometry validity)"

[autopilot.offers."fixer.geo_reproject_to_wgs84"]
schema = "schemas/geo-fixer.json"
adapter = "cli"
adapter_cmd = ".autopilot/adapters/reproject-wgs84.py"
description = "Reprojects geometry columns to WGS84 (EPSG:4326)"
side_effects = "filesystem"   # P2 directive — confirm before applying
```

Autopilot daemon, when scanning `kazuba-geo-engine`, discovers these custom detectors via THSF and integrates them into the standard pipeline:
- Custom detector findings flow through the same triage / confidence / proposer
- Custom fixer can be invoked as auto-fix candidate by speculator
- Same RL loop applies (Gabriel can reject "geo_reproject_to_wgs84" suggestions)

### 8.3 Bidirectional flow (autopilot ↔ THSF)

| Direction                              | Mechanism                                                                                             |
|----------------------------------------|--------------------------------------------------------------------------------------------------------|
| Project → Autopilot (declares offers)  | `.autopilot/manifest.toml` `[autopilot.offers.*]` discovered at workspace register                     |
| Autopilot → Project (consumes capabilities) | `holon invoke <name> autopilot.<offer> <args>` via existing THSF transport                       |
| Autopilot → Gabriel (surfaces findings)| MCP tool `mcp__autopilot__list_workspaces`, `mcp__autopilot__list_findings --uri <ws>`                 |
| Gabriel → Autopilot (decides)          | CLI `touring autopilot decide <id>` — workspace inferred from cwd or `--workspace <uri>`               |

### 8.4 THSF P2 (`--confirm`) integration

THSF Wave P2 added `holon invoke --confirm` for side-effect-having capabilities. Autopilot speculator MUST honor this: when a fixer is invoked in shadow validation, pass `--confirm=False` (no prompts in shadow); when applying for real (L4+), prompt according to `side_effects` declaration.

---

## 9. LSP-inspired patterns (Context7-verified)

Adopting LSP shapes wherever possible — they are mature and battle-tested across IDE ecosystems.

### 9.1 `WorkspaceFolder` shape

```rust
pub struct WorkspaceFolder {
    pub uri: WorkspaceUri,    // file:///absolute/path
    pub name: String,         // human label
}
```

Autopilot adopts this shape for `WorkspaceProfile.uri()` + `name()`.

### 9.2 `workspace/didChangeWorkspaceFolders` notification

When Gabriel runs `touring autopilot workspace register <new-path>` or `unregister <uri>`, the daemon broadcasts an internal event:

```rust
pub enum WorkspaceFoldersChange {
    Added(Vec<WorkspaceFolder>),
    Removed(Vec<WorkspaceFolder>),
}
```

Internal subscribers: `WorkspaceRegistry`, `DetectorScheduler`, `RLBridge`. Each reacts (load/persist state, schedule scans, etc.). MCP server emits a corresponding push notification to Claude Code so it can re-fetch the active workspace list.

### 9.3 `workspace/configuration` with `scopeUri`

```rust
pub struct ConfigurationItem {
    pub scope_uri: Option<WorkspaceUri>,
    pub section: String,                   // e.g. "autopilot.policy"
}

pub struct ConfigurationParams {
    pub items: Vec<ConfigurationItem>,
}
```

When a detector needs a config value, it requests via `WorkspaceProfile.config().get("autopilot.conventions.python_formatter")` — implementation reads from layered conventions.toml.

### 9.4 Capability negotiation at workspace register

Mirroring LSP `initialize` capabilities exchange:

```rust
pub struct WorkspaceRegistrationParams {
    pub uri: WorkspaceUri,
    pub manifest_path: Option<PathBuf>,
    pub announced_capabilities: WorkspaceCapabilities,
}

pub struct WorkspaceRegistrationResponse {
    pub negotiated_capabilities: WorkspaceCapabilities,
    pub active_detectors: Vec<DetectorId>,
    pub effective_policy: WorkspacePolicy,
}
```

Daemon reconciles `announced_capabilities` (from manifest) with what it can actually offer (some capabilities require optional features at compile time). Returns the intersection.

---

## 10. Claude Code integration patterns

### 10.1 Workspace inference from `cwd`

Claude Code sessions have a `cwd`. Autopilot uses this to infer the active workspace:

```
cwd = /home/gabrielgadea/projects/analise/scripts/process_analysis/
↑
walk up looking for nearest .autopilot/manifest.toml or .holon/manifest.toml or Cargo.toml
↑
match against WorkspaceRegistry — if found, that's the active workspace
↑
if not registered, prompt Gabriel: "auto-register this workspace?" (one-time)
```

### 10.2 MCP tools per-workspace

```
mcp__autopilot__list_workspaces
mcp__autopilot__active_workspace          # what's the cwd-inferred workspace?
mcp__autopilot__list_findings { uri? }
mcp__autopilot__show_finding { id }
mcp__autopilot__decide { id, decision }
mcp__autopilot__metrics { uri? }
mcp__autopilot__policy { uri }
```

If `uri` is omitted on `list_findings` etc., uses the cwd-inferred active workspace.

### 10.3 additionalContext digest per-workspace

`instructions-loaded` hook injects digest of P0+P1 findings ONLY for the cwd-inferred active workspace. Avoids cross-workspace noise.

Example digest:

```
[TOURING-AUTOPILOT — workspace: analise (Python)]
- 2 P1 quality.regression findings in scripts/process_analysis/phase_4_validation.py (TDG dropped A→C+)
- 1 P0 supply_chain.advisory: vulnerability in fastapi <0.110 (CVE-2026-XXXX)
Run `touring autopilot list` to review.
```

### 10.4 TACO compatibility

Autopilot in external workspace MUST NOT interfere with TACO orchestration:
- Autopilot L0-L3 NEVER edits files
- L4+ goes through TACO Phase 5 (touring-engineer agent) using normal locks
- `instructions-loaded` injection is read-only (never blocks Claude Code)
- Per-workspace policy supersedes global policy when conflict

---

## 11. Scope graduation S0-S5

Mirroring autonomy levels L0-L5, but for SCOPE OF OPERATION:

| Scope | Description                                                          | Risk  | Requires |
|-------|----------------------------------------------------------------------|-------|----------|
| **S0**| Self only (`~/.claude/rust/` touring workspace)                      | LOW   | Phase A-H of master plan |
| **S1**| Single external workspace (manual register + reload)                 | LOW   | Phase J (this doc)       |
| **S2**| Multi-workspace (registry-based; up to N concurrent)                 | MEDIUM| Phase K                  |
| **S3**| Cross-workspace RL with explicit `--share-with` consent              | MEDIUM| Phase L                  |
| **S4**| Cloud sync (multi-host via libp2p — THSF Fase 7 reactivated)         | HIGH  | Future (gated on Gabriel multi-host need) |
| **S5**| Marketplace (community-contributed detectors/fixers via THSF capn)   | HIGH  | Far future, requires moderation pipeline |

Default scope at MVP (master plan Phase H complete): **S0**. Promotion to S1+ requires Gabriel explicit opt-in per workspace via `touring autopilot workspace register`.

---

## 12. Roadmap Phase J-L (post-master-plan Phase H)

Each phase independent of subsequent. Gabriel approves phase-by-phase. ETAs assume master plan Phase H is complete.

### Phase J — External workspace foundation — **L size, 1.5 semanas**

**Goal**: introduce the 4 abstractions (§3) NON-DISRUPTIVELY. All existing detectors continue working on `~/.claude/rust/`. Only the public API changes (each detector now receives `&dyn WorkspaceProfile` instead of `&HookRuntime`).

Sub-deliverables:
- J1: `WorkspaceProfile` trait + `TouringWorkspaceProfile` impl (all existing master plan code wrapped)
- J2: `WorkspaceCapability` enum + auto-detection from filesystem
- J3: `WorkspaceRegistry` + CLI `touring autopilot workspace register/list/unregister/info`
- J4: Per-workspace SQLite schema migration from monolithic to per-workspace ledger
- J5: Refactor 3 existing MVP detectors (D02, D08, D15) to use `WorkspaceProfile`
- J6: 10+ unit tests for registry + capability negotiation

**Acceptance**: `touring autopilot workspace register /home/gabrielgadea/.claude/rust` works; existing scans return identical findings to pre-J world.

### Phase K — First external workspace + polyglot detectors — **XL size, 2 semanas**

**Goal**: enable `touring autopilot workspace register /home/gabrielgadea/projects/analise` (Python). Implement `LanguageQualityProvider` dispatch. D01 + D12 + D15 + D16 work for Python.

Sub-deliverables:
- K1: `LanguageQualityProvider` trait + `TreeSitterQualityProvider` impl
- K2: D01 quality.regression — polyglot dispatch
- K3: D12 complexity.spike — polyglot dispatch
- K4: D15 todo.stagnant — already polyglot via tree-sitter, verify
- K5: D16 format.drift — polyglot via `LanguageFormatter` trait + ruff/black/biome impls
- K6: `.autopilot/conventions.toml` schema + loader
- K7: `.autopilot/manifest.toml` schema (extends THSF)
- K8: Cold-start protocol (14-day L0-L1 init mode)
- K9: 30+ tests including Python/TypeScript synthetic fixtures

**Acceptance**: register `analise/`, run `touring autopilot scan --workspace analise/`, get findings for Python files; conventions.toml respected; no false positives leaking from `~/.claude/rust/` workspace.

### Phase L — Multi-workspace + privacy boundaries — **L size, 1 semana**

**Goal**: 3+ workspaces concurrently. Per-workspace RL state isolation enforced. `share-rl` opt-in implemented but not promoted.

Sub-deliverables:
- L1: Per-workspace `rl_weights.bin` persistence (rkyv)
- L2: `touring autopilot workspace share-rl <src> <tgt> --categories <list>` (audit-logged)
- L3: Privacy invariant test: scan workspace A → reject findings → assert workspace B's bandit unchanged
- L4: Cross-workspace dashboard: `touring autopilot status --all`
- L5: workspace/didChangeWorkspaceFolders LSP-style notification (internal pub-sub)
- L6: 20+ tests for isolation + share-rl + dashboard

**Acceptance**: 3 workspaces registered (`touring`, `analise`, `kazuba-cargo`); per-workspace metrics distinct; share-rl audit trail present in policy_changes table.

### Phase M (future, gated) — THSF custom detector integration

**Goal**: `[autopilot.offers.*]` in `.holon/manifest.toml` discovered + invoked as detectors/fixers. Project-specific custom autopilot extensions.

Sub-deliverables:
- M1: THSF discovery walker in workspace register
- M2: Custom detector adapter (subprocess via THSF, JSON-in/JSON-out)
- M3: Custom fixer adapter with side-effects honoring (`--confirm` from P2)
- M4: Pilot: `kazuba-geo-engine` ships 1 custom detector + 1 custom fixer

**Acceptance**: register `kazuba-geo-engine`, pilot detector fires on synthetic GeoJSON file with bad CRS, proposes fixer, applied via L4 (after Gabriel approval).

### Phase N+ (far future, gated)

- S4: Cloud sync via libp2p (THSF Fase 7 reactivation)
- S5: Marketplace + community moderation pipeline
- Anomaly detection via cognitive engine (MCTS shadow rollouts to predict regression before it happens)

---

## 13. Risks (HIGH stakes — multi-tenant)

| ID  | Risk                                                                                | Prob   | Impact   | Mitigation                                                                                            |
|-----|-------------------------------------------------------------------------------------|--------|----------|-------------------------------------------------------------------------------------------------------|
| **X1**  | Cross-workspace RL contamination leaks confidential signal                       | MEDIUM | CRITICAL | Per-workspace state isolation by default; `share-rl` is opt-in per-category; audit trail              |
| **X2**  | Workspace registry DB bloats — 100s of legacy registered workspaces             | LOW    | LOW      | Auto-purge on `state=last_active < 90d`; CLI `touring autopilot workspace gc`                          |
| **X3**  | Polyglot dispatch picks wrong language (e.g. .h is C or C++?)                   | MEDIUM | LOW      | Conventions.toml override; ambiguity defaults to most permissive provider; no panic                   |
| **X4**  | `.autopilot/manifest.toml` injection attacks (malicious capability adapters)    | LOW    | CRITICAL | THSF `autonomy_guarantee=true` invariant + side_effects declaration mandatory + sandbox via WASM     |
| **X5**  | Workspace cwd inference picks wrong workspace (nested .autopilot/)              | MEDIUM | MEDIUM   | Walk-up strategy + closest-match + Gabriel confirmation prompt for ambiguous cases                    |
| **X6**  | Daemon overload with 10+ workspaces all scanning concurrently                    | MEDIUM | MEDIUM   | Per-workspace actor + GranularityBandit budget allocation across workspaces (not just within)         |
| **X7**  | Conventions.toml drift between Gabriel's intent and reality                     | LOW    | LOW      | `touring autopilot policy --suggest-tune --workspace <uri>` quarterly                                  |
| **X8**  | Cold-start L0-L1 mode forgotten — Gabriel never promotes; autopilot stays mute  | MEDIUM | LOW      | Day-14 PushNotification asking "promote workspace X to L1?"                                            |
| **X9**  | Per-workspace RL never converges (insufficient signal)                          | MEDIUM | MEDIUM   | Detector dropout: workspaces with < 50 decisions in 60d auto-demote to S0 (passive)                   |
| **X10** | Hard Rule #11 leaks from touring workspace into external                        | LOW    | HIGH     | `forbid_git` flag is policy.toml-scoped; default false externally; explicit opt-in to inherit         |
| **X11** | THSF custom detector returns malformed JSON → autopilot crashes                  | MEDIUM | MEDIUM   | catch_unwind around adapter calls; demote category 1 level on parse failure; gotcha logged           |
| **X12** | Conventions tool not installed (e.g. `ruff` missing) — silent skip vs. error?   | MEDIUM | LOW      | Skip silently in scan; surface as "advisory: install ruff to enable Python format detection" digest  |
| **X13** | Gabriel registers same workspace twice (different paths, e.g. via symlink)      | LOW    | LOW      | URI canonicalize via `fs::canonicalize`; reject duplicate with "already registered as <uri>"          |
| **X14** | Polyglot quality scores not comparable across languages → confusing dashboards  | MEDIUM | LOW      | Per-language scale calibration; UI shows raw + percentile within language                              |
| **X15** | Workspace discovery via cwd is racy in multi-Claude-Code-window scenarios       | LOW    | LOW      | Workspace inference cached per-session-id, re-evaluated on cwd change                                  |

---

## 14. Migration path (autopilot v1.0 internal → v2.0 external)

If the master plan ships first as v1.0 (single-workspace internal) and the expansion plan lands later as v2.0 (multi-workspace), the migration must be **non-disruptive**:

### Step 1: Add abstractions without breaking existing API

```rust
// v1.0 detector signature (master plan):
fn scan(&self, rt: &HookRuntime, scope: ScanScope) -> Vec<Finding>;

// v2.0 detector signature (expansion):
fn scan(&self, profile: &dyn WorkspaceProfile, scope: ScanScope) -> Vec<Finding>;

// MIGRATION: TouringWorkspaceProfile.touring_runtime() returns Some(&rt),
// existing detector internals call profile.touring_runtime().unwrap() during transition.
// New polyglot detectors use profile.* methods directly.
```

### Step 2: SQLite schema migration

v1.0 ledger lives in workspace knowledge_db. v2.0 moves to `<workspace>/.autopilot/state/findings.db`. Migration runs on first daemon boot post-upgrade:

```
~/.claude/rust/.claude/touring/symbols.db
  → autopilot_findings, autopilot_proposals, ...
  → COPY rows to ~/.claude/rust/.autopilot/state/findings.db
  → DROP from old location
```

Migration is idempotent + atomic + Gabriel-approval-gated (manual confirm, not automatic).

### Step 3: Default policy promotion

Existing `~/.claude/rust/` autopilot policy is preserved. New workspaces start at L0 globally. Gabriel must explicitly `touring autopilot workspace promote` each new one.

### Step 4: Backward compatibility window

For 30 days, both `touring autopilot list` (no workspace = touring default) and `touring autopilot list --workspace <uri>` work. After 30 days, no-arg form requires `--workspace .` to make workspace explicit. Documented in changelog.

---

## 15. Open research questions

1. **Workspace ID format**: file:// URI (LSP-compatible) vs. UUID (stable across moves)? Recommendation: file:// URI for human-readability, with fs::canonicalize.

2. **Per-workspace daemon vs. single daemon**: master plan §X assumes single daemon. With 10+ workspaces, does per-workspace daemon make sense? Recommendation: stay single until measured contention.

3. **THSF custom detector RL**: when project ships a custom detector, do its rewards flow to project-local RL or global RL? Recommendation: project-local; project ships its own bandit configuration optionally.

4. **Conventions.toml schema versioning**: how to handle schema evolution? Recommendation: `schema_version = "1.0"` field + auto-migrate on load.

5. **Workspace "team" mode**: multiple developers, each with their own autopilot policy? Recommendation: deferred — Gabriel-single-user is current scope.

6. **Memory transfer when project moves disk locations**: if Gabriel mv's `analise/` to a new path, autopilot state should follow. Recommendation: include `original_uri` + `current_uri` in registry; offer migrate command.

7. **Performance budget across workspaces**: is single-workspace 60s scan budget the same when serving 5 workspaces concurrently? Recommendation: P95 scan latency < 5s per workspace, daemon CPU < 50% under steady state.

8. **Autopilot self-bootstrap**: can autopilot recommend installing `ruff` when missing for Python workspace? Recommendation: yes, but only as Hint severity proposal, never auto-install.

---

## 16. Anti-patterns to avoid (multi-workspace specific)

1. **DON'T** assume a single global RL state — per-workspace isolation is invariant.
2. **DON'T** show findings from workspace A to user when working in workspace B (cwd-inference must be tight).
3. **DON'T** allow detectors to bypass `WorkspaceProfile` — direct `HookRuntime` access is forbidden.
4. **DON'T** mix file paths from different workspaces in the same proposal markdown.
5. **DON'T** assume Hard Rule #11 in non-touring workspaces.
6. **DON'T** auto-install missing tools (`ruff`, `black`, etc.) — Gabriel decides what tools live where.
7. **DON'T** persist cross-workspace data without explicit `--share-with` consent.
8. **DON'T** leak prettyplease-version-pinning across workspaces (each project pins its own).
9. **DON'T** silently degrade when `.autopilot/manifest.toml` is missing — explicit "workspace not opt-in" message.
10. **DON'T** trust THSF custom detector output without sandboxing (X4 risk).

---

## 17. Implementation file layout (delta over master plan)

```
crates/touring-hooks/src/autopilot/
├── (existing master plan files: detector.rs, finding.rs, ...)
├── workspace.rs                  # NEW — WorkspaceProfile trait + impls
├── workspace_registry.rs         # NEW — DashMap registry + LSP-style change events
├── workspace_uri.rs              # NEW — file:// URI canonicalization
├── conventions.rs                # NEW — .autopilot/conventions.toml loader
├── language_provider.rs          # NEW — LanguageQualityProvider + FormatterRegistry
├── thsf_bridge.rs                # NEW — discovery of [autopilot.offers.*] in manifests
├── detectors/
│   ├── (existing master plan: d01..d16)
│   └── ...                       # No new detectors in expansion; existing ones become polyglot
└── tests/
    ├── multi_workspace_e2e.rs    # NEW — Phase L acceptance
    ├── polyglot_quality.rs       # NEW — Phase K acceptance
    ├── thsf_custom_detector.rs   # NEW — Phase M acceptance
    └── privacy_isolation.rs      # NEW — Phase L cross-workspace RL leak detection

crates/touring-server/src/cli/autopilot.rs   # ADDITIONS — workspace subcommands

docs/autopilot/
├── (existing: playbook.md, detectors/D{NN}.md)
├── workspace-registry.md         # NEW — operational guide
├── conventions-schema.md         # NEW — full reference
└── thsf-integration.md           # NEW — custom detector authoring guide
```

**Estimated total LOC delta**: ~2.500 LOC over master plan baseline (workspace abstractions + polyglot providers + tests).

---

## 18. Decision matrix: por que cada abstração agora?

| Abstraction          | Could ship in v2.0 instead of v1.0?                                                |
|----------------------|------------------------------------------------------------------------------------|
| `WorkspaceProfile`   | NO — every detector uses it; deferring requires rewriting all 16 detectors        |
| `WorkspaceCapability`| NO — graceful degradation pattern depends on it; without it, polyglot is brittle  |
| `.autopilot/` dir    | YES technically (could ship monolithic ledger first), but migration is harder later|
| THSF integration     | YES — Phase M can wait                                                             |

Recommendation: ship `WorkspaceProfile` + `WorkspaceCapability` IN PHASE A of master plan (extra ~3-5 days). Defer `.autopilot/` per-workspace dir + THSF integration to Phase J+ (post-Phase H of master plan).

---

## 19. Authorization matrix (this expansion plan)

| Phase | Authorization needed                                  | Granted? |
|-------|-------------------------------------------------------|----------|
| Spec  | "Aprovar este expansion plan v1.0 como referência"   | Pending  |
| Pre-A | "Inserir WorkspaceProfile/Capability EM Phase A do master plan (não como Phase J separada)" | Pending — recommended |
| J     | "Phase J — external workspace foundation"             | Pending  |
| K     | "Phase K — polyglot + first external workspace"       | Pending  |
| L     | "Phase L — multi-workspace + privacy"                 | Pending  |
| M     | "Phase M — THSF custom detectors"                     | Pending  |
| N+    | S4/S5 — far future, separate proposals                | Deferred |

---

## 20. Compliance with master plan principles

This expansion preserves master plan's 10 founding principles (§1.1):

| Master principle                              | How expansion preserves                                                              |
|-----------------------------------------------|---------------------------------------------------------------------------------------|
| 1. HUMAN-IN-THE-LOOP por padrão               | Every workspace registration is Gabriel-explicit; promotion is explicit              |
| 2. EVIDÊNCIA antes de PROPOSTA                | Per-workspace evidence (workspace-scoped paths, capabilities, conventions cited)     |
| 3. FALSO POSITIVO É BUG                       | Per-workspace FP rate measured; auto-demote per-workspace per-category                |
| 4. GRADIENTE DE AUTONOMIA, NÃO INTERRUPTOR    | L0-L5 + S0-S5 (orthogonal axes: autonomy × scope)                                    |
| 5. DEDUPLICATION é INVARIANTE                 | Dedup is per-workspace; no cross-workspace finding fusion                            |
| 6. REUTILIZA, NÃO REINVENTA                    | THSF, LSP patterns, prettyplease, ruff — all reused, none reinvented                  |
| 7. OBSERVABILIDADE FIRST                       | Per-workspace counters in gate-metrics namespace                                      |
| 8. HARD RULE #11 INVIOLÁVEL (touring ws only) | Externalized via per-workspace policy; default forbid_git=false outside touring       |
| 9. REVERSIBILIDADE TOTAL                       | Per-workspace pre-state snapshots in `<workspace>/.autopilot/state/snapshots/`        |
| 10. DECAY DE CONFIANÇA                         | Same — 7-day decay per-workspace                                                       |

---

## 21. Glossary (additions over master plan)

| Term                       | Definition                                                                                  |
|----------------------------|---------------------------------------------------------------------------------------------|
| WorkspaceProfile           | Trait abstracting workspace-scoped operations (URI, capabilities, policy, runtime, paths)   |
| WorkspaceCapability        | Enum declaring what a workspace can offer (AstParsing, MutationTesting, ...)                |
| WorkspaceUri               | LSP-compatible `file:///path` canonicalized via fs::canonicalize                            |
| LanguageQualityProvider    | Trait dispatching quality scoring per Language                                              |
| LanguageFormatter          | Trait dispatching formatting per Language                                                   |
| Conventions                | Per-workspace tooling preferences in `.autopilot/conventions.toml`                          |
| Cold-start protocol        | 14-day L0-L1 init mode for newly-registered workspaces                                      |
| Cross-workspace RL          | Optional opt-in transfer learning between workspaces with similar characteristics           |
| THSF (Touring Holonic Symbiosis Framework) | Acoplamento temporário entre projetos autônomos via `.holon/manifest.toml`     |
| Scope graduation S0-S5     | Orthogonal to autonomy L0-L5; controls workspace breadth (self → multi → cloud)             |

---

## 22. Addenda (post-spec changes)

_Empty. Append entries as Gabriel approves deviations._

---

**End of expansion plan v1.0**

_Next action_: present this document alongside the master plan to Gabriel for review. Recommendation: approve both Spec line of master plan AND **Pre-A** of expansion (`WorkspaceProfile` + `WorkspaceCapability` EM Phase A do master plan) before Phase A coding begins. This adds ~3-5 days to Phase A but saves ~6 weeks of Phase J retrofit.
