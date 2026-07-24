# 50-Dim Quality Remediation Patterns — Canônico (REGRA #2)

**Data**: 2026-06-21 | **Sessão**: L2 — decisão arquitetural | **Autoridade**: Gabriel Gadea
**Origem**: GAP-1 W7 do plano `2026-06-14-touring-elite-harness-strategy.md` — decisão **Opção B (REDIRECIONAR)**.
**Complementa**: `~/.claude/rules/elite-50-quality.md` (keystone) · `~/.claude/rust/docs/2026-06-14-touring-elite-harness-strategy.md` §11 (STATUS) · `~/.claude/rules/taco-forge-canonical-workflows.md` (REGRA #2 perfect-edit).

---

## TL;DR

Auto-remediação por dimensão = **recipe canônica de 2 passos**:

```bash
# 1. IDENTIFICAR gap específico (com score, file:line, evidence)
touring-quality check --gate F{dim} --target <FILE> --format json

# 2. APLICAR fix via perfect-edit (canonical, REGRA #2)
taco-forge perfect-edit --path <FILE> \
  --operation ssr --pattern '<gap_regex>' --replacement '<fix_regex>'
# OU
taco-forge perfect-edit --path <FILE> \
  --operation assist --assist-kind <kind> --line <N>
# OU (rich generation)
taco-forge perfect-edit --path <FILE> \
  --operation free-form --content-from <source>

# 3. VALIDAR
touring-quality check --gate F{dim} --target <FILE>
cargo check -p <crate>   # se aplicável
```

**NÃO** há 50 subcommands `taco-forge perfect-quality-f{nn}-{slug}`. O plano original W7 propunha 50 stubs; decisão REVISADA em 2026-06-21 = redirecionar para `perfect-edit` + esta doc de patterns. Os 50 templates `.toml` em `taco-forge/generators/` viraram **documentação viva** (lidos por esta doc).

---

## Por que Opção B (REDIRECIONAR) em vez de Opção A (DESPachar 50 stubs)

| Critério | A (50 stubs) | **B (perfect-edit + patterns)** |
|----------|--------------|-------------------------------|
| LOC adicionadas | 1.500–2.500 | ~50 edits em `.md` + esta doc (~150L) |
| Code paths | 50 (cada um risk surface) | 0 (reusa perfect-edit estável) |
| Tempo | 2-3 sprints | 0.5 sprint |
| Help do `taco-forge --help` | poluído (50 entries) | limpo (reusa perfect-edit) |
| Manutenção | mudança global = 50 lugares | mudança global = 1 (perfect-edit) |
| Risco regressão | 50× chance de bug | 0× (reusa código testado) |
| Flexibilidade | fix hardcoded por dim | usuário adapta pattern on-the-fly |
| REGRA #14 (agentic paradigm) | parcial | **total** |
| REGRA #16 (CLAUDE.md hygiene: simplicity) | viola | **conforme** |

**Conclusão TACO**: Opção B é claramente superior em 9/10 critérios. Os 50 `.toml` viraram **documentação**, não código executável.

---

## Os 7 patterns canônicos de fix

Cada uma das 50 dims mapeia para **1-3** destes patterns. Selecione conforme o gap diagnosticado pelo passo 1.

### Pattern 1 — **Refactor extract function** (F1.1, F1.2)
```bash
# F1.1 (cyclomatic complexity): extrair função longa em helper
taco-forge perfect-edit --path src/foo.rs \
  --operation assist --assist-kind extract_function --line 23

# F1.2 (maintainability): renomear identificador curto
taco-forge perfect-edit --path src/foo.rs \
  --operation ssr --pattern '\bx\b' --replacement 'request_body'
```
**Quando**: gap é "função > 50 LOC" ou "id ≤ 2 chars".

### Pattern 2 — **Refactor shared trait** (F1.3, F1.4)
```bash
# F1.3 (duplication): extrair trait compartilhada
# NÃO é regex; requer análise semântica. Use:
taco-forge perfect-edit --path src/foo.rs \
  --operation free-form --content-from refactored.rs
# (depois touring ast find confirma eliminação do duplicado)
```
**Quando**: gap é "duplicação ≥ 3%".

### Pattern 3 — **Remove hardcoded value** (F2.4 secrets, F2.6 config debug)
```bash
# F2.4: secret hardcoded → env var
taco-forge perfect-edit --path src/auth.rs \
  --operation ssr \
  --pattern 'const API_KEY: &str = "[a-zA-Z0-9]{32}";' \
  --replacement 'fn api_key() -> String { std::env::var("API_KEY").expect("API_KEY set") }'

# F2.6: debug=true → release-safe
taco-forge perfect-edit --path src/config.rs \
  --operation ssr --pattern 'const DEBUG: bool = true;' \
  --replacement 'const DEBUG: bool = cfg!(debug_assertions);'
```
**Quando**: gap é "secret em source" / "config insegura".

### Pattern 4 — **Migration API substituta** (F4.3 deprecated)
```bash
# F4.3: substitui API deprecada por substituto canônico
taco-forge perfect-edit --path src/db.rs \
  --operation ssr \
  --pattern 'db\.query_raw\("SELECT \* FROM' \
  --replacement 'db.query("SELECT * FROM'
# (substituto indicado pela `note` do `#[deprecated(...)]`)
```
**Quando**: gap é "uso de API `#[deprecated]`".

### Pattern 5 — **Update manifest** (F4.5 pkg-mgmt, F2.5 dep CVEs)
```bash
# F4.5: remova deps não usadas
# (NÃO perfect-edit — usa cargo tree + cargo machete)
cargo machete                          # identifica unused deps
# edit manual do Cargo.toml remove linhas

# F2.5: bump dep com CVE
taco-forge perfect-edit --path Cargo.toml \
  --operation ssr \
  --pattern '^serde = "1\.0\.[0-9]+"' \
  --replacement 'serde = "1.0.200"'
# (versão específica do OSV.dev / RustSec advisory)
```
**Quando**: gap é "dep abandonada/EOL" / "dep com CVE".

### Pattern 6 — **Add test stub** (F3.1 coverage, F3.4 edge cases, F3.7 perf tests)
```bash
# F3.1: cria test stub para uncovered path
taco-forge perfect-create --path tests/foo_edge.rs \
  --intent "test for uncovered branch in src/foo.rs line 47" \
  --kind RustTest

# F3.7: cria script k6/Gatling
taco-forge perfect-create-script --path tests/load/scenario.js \
  --intent "k6 load test: 100 RPS for 30s, p99 < 200ms" \
  --kind LoadTest
```
**Quando**: gap é "linha X não coberta" / "sem load test".

### Pattern 7 — **Documentation generation** (F3.8 inline doc, F3.9 API doc, F3.10 arch doc, F3.11 README, F3.12 doc accuracy, F3.13 changelog, F4.7 CI/CD, F4.8 deploy, F4.9 IaC, F4.10 monitoring, F4.11 incident, F4.12 env)
```bash
# F3.9: cria OpenAPI spec para endpoints
taco-forge perfect-create --path docs/api/openapi.yaml \
  --intent "OpenAPI 3.1 spec for /v1/users endpoints" \
  --kind OpenAPISpec

# F3.10: cria ADR para decisão arquitetural
taco-forge perfect-create --path docs/adr/0007-cache-strategy.md \
  --intent "ADR: cache eviction LRU + bounded size" \
  --kind ArchitectureDecisionRecord

# F3.11: atualiza README com badges
taco-forge perfect-edit --path README.md \
  --operation free-form --content-from new_readme.md

# F4.7: cria GitHub Actions workflow
taco-forge perfect-create --path .github/workflows/ci.yml \
  --intent "ci: cargo build + test + clippy on push" \
  --kind GitHubActions

# F4.9: cria módulo Terraform
taco-forge perfect-create --path infra/main.tf \
  --intent "terraform module: S3 bucket versioning + lifecycle" \
  --kind TerraformModule

# F4.10: cria Prom exporter
taco-forge perfect-create --path src/exporter.rs \
  --intent "Prometheus exporter for touring-quality composite" \
  --kind RustModule
```
**Quando**: gap é "doc ausente" / "config ausente" / "infra ausente".

---

## Mapeamento dim → pattern (50 dims)

| Dim | Patterns aplicáveis | Comando típico |
|-----|---------------------|----------------|
| F1.1 Complexity | 1 (extract) | `perfect-edit --operation assist --assist-kind extract_function --line N` |
| F1.2 Maintainability | 1 (rename) | `perfect-edit --operation ssr --pattern '\bx\b' --replacement 'request_body'` |
| F1.3 Duplication | 2 (free-form) | `perfect-edit --operation free-form --content-from refactored.rs` |
| F1.4 SOLID | 2 (free-form) | idem |
| F1.5 Tech debt | 2 (free-form) | idem |
| F1.6 Error handling | 1 (rename) | `perfect-edit --operation ssr --pattern 'unwrap\(\)' --replacement 'expect(...)'` |
| F1.7 Boundaries | 2 (free-form) | `perfect-edit --operation free-form` + pub→pub(crate) |
| F1.8 Dep cycles | 2 (free-form) | `perfect-edit --operation free-form` + refactor |
| F1.9 API design | 2 (free-form) | `perfect-edit --operation free-form` |
| F1.10 Data model | 2 (free-form) | idem |
| F1.11 Patterns | 1, 2 | conforme gap |
| F1.12 Arch consistency | 2 (free-form) | idem |
| F2.1 OWASP | 3 (replace), 4 (migrate) | `perfect-edit --operation ssr` em sanitizers |
| F2.2 Input validation | 3 (replace) | idem |
| F2.3 AuthN/AuthZ | 3 (replace) | idem |
| F2.4 Secrets | 3 (remove hardcoded) | `perfect-edit --operation ssr` secret→env var |
| F2.5 Dep CVEs | 5 (manifest) | `perfect-edit --operation ssr` em Cargo.toml |
| F2.6 Config security | 3 (replace) | `perfect-edit --operation ssr` debug→release-safe |
| F2.7 DB perf | 1 (extract query) | `perfect-edit --operation ssr` + index hints |
| F2.8 Memory | 1 (extract alloc) | `perfect-edit --operation assist` |
| F2.9 Caching | 1 (extract cache wrapper) | `perfect-edit --operation free-form` |
| F2.10 I/O | 1 (extract I/O) | idem |
| F2.11 Concurrency | 1, 2 | idem |
| F2.12 Frontend perf | 7 (create lazy) | `perfect-create --kind LazyComponent` |
| F2.13 Scalability | 7 (create pool) | `perfect-create --kind ConnectionPool` |
| F3.1 Coverage | 6 (test stub) | `perfect-create --kind RustTest` |
| F3.2 Test quality | 6 (mutation test) | idem |
| F3.3 Test pyramid | 6 (E2E test) | `perfect-create --kind E2ETest` |
| F3.4 Edge cases | 6 (proptest) | `perfect-create --kind PropertyTest` |
| F3.5 Test maint | 6 (Testcontainer) | `perfect-create --kind TestContainerTest` |
| F3.6 Sec tests | 6 (ZAP test) | `perfect-create --kind SecurityTest` |
| F3.7 Perf tests | 6 (k6 script) | `perfect-create-script --kind LoadTest` |
| F3.8 Inline doc | 7 (doc generation) | `perfect-edit --operation free-form` |
| F3.9 API doc | 7 (OpenAPI) | `perfect-create --kind OpenAPISpec` |
| F3.10 Arch doc | 7 (ADR/C4) | `perfect-create --kind ArchitectureDecisionRecord` |
| F3.11 README | 7 (README) | `perfect-edit --operation free-form` |
| F3.12 Doc accuracy | 7 (doc sync) | `perfect-edit --operation free-form` |
| F3.13 Changelog | 7 (CHANGELOG entry) | `perfect-edit --operation free-form` |
| F4.1 Idioms | 1 (clippy fix) | `perfect-edit --operation ssr` |
| F4.2 Frameworks | 1 (extract) | idem |
| F4.3 Deprecated | 4 (migrate API) | `perfect-edit --operation ssr` |
| F4.4 Modernization | 1 (modern syntax) | idem |
| F4.5 Pkg mgmt | 5 (manifest) | `perfect-edit --operation ssr` em Cargo.toml |
| F4.6 Build config | 1, 7 | `perfect-edit --operation ssr` em Cargo.toml |
| F4.7 CI/CD | 7 (GitHub Actions) | `perfect-create --kind GitHubActions` |
| F4.8 Deploy | 7 (k8s manifest) | `perfect-create --kind KubernetesManifest` |
| F4.9 IaC | 7 (Terraform) | `perfect-create --kind TerraformModule` |
| F4.10 Monitoring | 7 (Prom exporter) | `perfect-create --kind PrometheusExporter` |
| F4.11 Incident | 7 (runbook) | `perfect-create --kind IncidentRunbook` |
| F4.12 Env | 7 (Vault policy) | `perfect-create --kind VaultPolicy` |

---

## Workflow canônico em 4 passos

```bash
# PASSO 1 — IDENTIFICAR (gap específico com file:line)
touring-quality check --gate F{dim} --target <FILE> --format json
# Output: {"value": 0.4, "status": "Fail", "evidence": "...", "suggestions": [...]}

# PASSO 2 — SELECIONAR pattern (consultar tabela acima)

# PASSO 3 — APLICAR (perfect-edit com shadow validate)
taco-forge perfect-edit --path <FILE> --operation <op> \
  --pattern '<gap_regex>' --replacement '<fix_regex>'
# (atomic snapshot + rollback; shadow validate ≥ 0.8)

# PASSO 4 — VALIDAR (score + compile)
touring-quality check --gate F{dim} --target <FILE>
cargo check -p <crate>  # se Rust
cargo test -p <crate>   # se tem tests
```

---

## Decisões correlatas

- **W8 CEG X0a..X0d**: RE-SCOPED (não duplica scoring). X0a Quality será um **step de pre-flight no CEG** que CONSOME o QualityReport do hook `touring-quality-block-all.sh` como input — não duplica scoring.
- **Hard rules REGRA #21/#22/#23**: CANCELADAS — enforcement via hook `touring-quality-block-all.sh` é superior.
- **MCP tools +10 qualidade**: CANCELADAS — motor real é o binário CLI standalone `touring-quality`; Curated Plan (06/06) reduziu 102 → 22 MCP.
- **Decision matrix C01-C12 → D01-D50**: camadas ortogonais — C-cats (task→cmd) e D-rules (per-dim) coexistem.

---

**Cross-references**:
- Plano original: `~/.claude/rust/docs/2026-06-14-touring-elite-harness-strategy.md` §11 (STATUS)
- Keystone: `~/.claude/rules/elite-50-quality.md`
- Canonical workflows: `~/.claude/rules/taco-forge-canonical-workflows.md` (REGRA #2 perfect-edit)
- 50 D-rules: `~/.claude/rules/quality/D{01..52}.md` (cada uma agora aponta para este doc)
