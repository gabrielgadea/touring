---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W12"
name: "Per-Project Deployment"
phase: "F5-DEPLOYMENT"
depends_on:
  - W11
parallel_with: []
status: "DISCOVERY_DONE (2026-05-23) — gap analysis complete. init.rs (942L) + migrate.rs (1331L) JÁ EXISTEM mas com semântica DIFERENTE (profile preset / DB consolidation). W12 deve POTENCIALIZAR (REGRA #0), não substituir. Blueprint persisted at discovery:w12-per-project-deployment-gap:2026-05-23."
created: "2026-05-11"
cila: "L4"
rust_changes: "ADDITIVE"
estimated_days: "15-20"
checkpoint: "touring_premium_W12_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W12.py"
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
# W12: Per-Project Deployment

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F5-DEPLOYMENT
> **Contribuição para resultado final**: Saída do path global. Cada projeto isola knowledge/memory/learning. Múltiplas versões de Touring coexistem. Cliente externo pode instalar via curl install.touring.dev | sh. Premium product ready.

---

## Contexto e Dependências

- **Depende de**: W11
- **Paralelo com**: Nenhuma
- **CILA**: `L4`
- **Mudanças Rust**: `ADDITIVE`
- **Estimativa**: 15-20 dias
- **Checkpoint**: `touring_premium_W12_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W12.py`

---

## Descrição

Implementar `touring init` + toolchain manager rustup-like em ~/.touring/toolchains/ + per-project .touring/ structure + daemon multi-instance (per-project socket) + hook dispatcher walk-up + `touring migrate --from-global` + external installer (install.touring.dev). Pilot em konverter + analise. Funciona para Gabriel internamente E para clientes externos.

---

## Efeitos no Sistema

- `touring init` CLI funcional
- `~/.touring/toolchains/<version>/` rustup-like layout
- `.touring/touring.toml` schema v1.0
- Daemon multi-instance (per-project socket)
- Hook dispatcher walk-up em ~/.claude/hooks/touring-hook
- `touring migrate --from-global` automatiza transição
- `install.touring.dev` script signed + SBOM
- Pilot konverter + analise rodando per-project
- Cross-platform: Linux + macOS (Windows W14)

---

## Discovery Updates (2026-05-23) — Existing Infrastructure Inventory

> **Origem**: Goal continuation `/Touring --ultrathink prossiga com a implementação`.
> Antes de escrever 1 LOC, mapear o que já existe — Code-First Gate (FIX-S4).

### Existing infrastructure (DO NOT REPLACE — REGRA #0 potentialize)

| Component | Path | Lines | Current purpose | W12 spec semantics |
|---|---|---|---|---|
| `init` CLI | `crates/touring-server/src/cli/init.rs` | 942 | TOML profile preset (`--profile`/`--list-profiles`/`--cc-setup`/`--rignore-audit`); writes `~/.claude/touring/profiles/<name>.toml` → `~/.claude/touring/touring.toml` | DIFFERENT — W12.1 needs per-project `.touring/{touring.toml,data/,bin/,hooks/}` scaffold |
| `migrate` CLI | `crates/touring-server/src/cli/migrate.rs` | 1331 | DB consolidation (8 legacy DBs → 3 consolidated domains) — `status`/`plan`/`run`/`validate`/`rollback` | DIFFERENT — W12.7 needs `migrate --from-global` moving `~/.claude/touring/` data into `.touring/data/` |

### Implications

1. **REGRA #0 mandate**: W12 implementations MUST extend (not replace) the existing init.rs and migrate.rs. Either:
   - **Option A**: Add `--per-project` flag to existing subcommand (in-place extension)
   - **Option B** (recommended): Add NEW subcommands `init-project` and `migrate-from-global` (cleaner separation; existing `init`/`migrate` keep their semantics)

2. **Context7 rustup pattern adapted** (queried 2026-05-23):
   - `TOURING_HOME` env var, default `~/.touring/`
   - `~/.touring/toolchains/<version>/{bin,lib,share,meta.toml}` layout
   - `~/.touring/default` + `~/.touring/config.toml`
   - Per-project pin: `.touring/touring.toml` (channel + components + targets) — direct mirror of `rust-toolchain.toml`
   - Override chain: CLI shorthand (`touring +0.31`) → `TOURING_TOOLCHAIN` env → `.touring/touring.toml` walk-up → default

3. **Foundational ordering** (W12.5 daemon multi-instance is BLOCKING):
   - **W12.1+W12.2+W12.4** (CLI scaffold + `~/.touring/` layout + layered config) = foundational, can be parallel
   - **W12.5** (daemon multi-instance per-project socket) = blocks W12.6, W12.7, W12.9, W12.10
   - **W12.6** (hook dispatcher walk-up shim) = WAIT for W12.1/W12.2 — without `.touring/bin/`, shim 100% falls through to global fallback (zero functional gain)
   - **W12.7** (migrate `--from-global`) = depends on W12.2 layout
   - **W12.8** (install.touring.dev) = depends on W13 publishing pipeline (artifact server, signing) — premature now

### Re-estimated effort

Original 15-20 days; existing scaffolding awareness saves ~1 day. Revised **14-19 days**. Critical path: W12.1 → W12.2 → W12.5 → W12.6 → W12.7 → W12.9/W12.10 pilots.

### Blueprint anchor

`touring memory recall "discovery:w12-per-project-deployment-gap:2026-05-23"` — full gap analysis with Context7 rustup pattern map + existing-file inventory + foundational sequencing.

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W12.1: Implement `touring init` CLI

**Descrição**: Subcommand em touring-server-cli. Cria .touring/ structure, gera touring.toml inferindo features do diretório atual (Cargo.toml = Rust; pyproject.toml = Python; etc.).

**Dias estimados**: 2.0

**DISCOVER obrigatório**:
  - touring tantivy search 'cargo new --lib'
  - touring memory recall 'touring init implementation'

**TDD RED** (escrever ANTES do código):
```python
def test_touring_init_creates_structure():
    """RED: touring init in tmp dir creates .touring/ tree."""
```

**Critério de validação**: touring init -> .touring/{touring.toml,data/,bin/,hooks/} exists.

---

### W12.2: Implement ~/.touring/ toolchain manager

**Descrição**: Layout completo: toolchains/<version>/{bin,lib,share,meta.toml}, default file, config.toml. Touring-update binary copies/links into structure.

**Dias estimados**: 3.0

**Critério de validação**: ls ~/.touring/toolchains/ shows ≥ 1 version; ~/.touring/default file aponta para versão válida.

---

### W12.3: Implement `touring update/toolchain/component`

**Descrição**: `touring update [version]`, `touring update --rollback`, `touring toolchain list/install/remove/default`, `touring component list/add/remove`.

**Dias estimados**: 2.0

**TDD RED** (escrever ANTES do código):
```python
def test_toolchain_install_rollback():
    """RED: install A, install B, rollback → back to A."""
```

**Critério de validação**: touring toolchain list mostra versões instaladas; rollback funciona.

---

### W12.4: Implement layered config loader

**Descrição**: Precedência: project (.touring/touring.toml) ← user (~/.touring/config.toml) ← system (/etc/touring/) ← hardcoded defaults. Validator via JSON schema.

**Dias estimados**: 1.0

**Critério de validação**: Config merge testado em 4 cenários; conflicts resolvidos.

---

### W12.5: Daemon multi-instance (per-project socket)

**Descrição**: Daemon spawn detecta .touring/touring.toml via walk-up. Socket fica em <project>/.touring/daemon.sock. Múltiplos daemons coexistem (1 por projeto). Estimated RSS ~92 MB/daemon.

**Dias estimados**: 2.0

**TDD RED** (escrever ANTES do código):
```python
def test_two_projects_two_daemons():
    """RED: cd projA + cd projB + touring status → 2 sockets."""
```

**Critério de validação**: lsof | grep daemon.sock retorna N sockets para N projetos abertos.

**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.

---

### W12.6: Hook dispatcher walk-up shim

**Descrição**: ~/.claude/hooks/touring-hook é shell shim que faz walk-up procurando .touring/bin/touring-hook. Fallback para ~/.touring/toolchains/<default>/bin/.

**Dias estimados**: 1.0

**Critério de validação**: Em projeto com .touring/, hook usa local binary. Fora, usa default toolchain.

---

### W12.7: Implement `touring migrate --from-global`

**Descrição**: Migra ~/.claude/touring/ → .touring/data/ no projeto atual. Copia symbols.db filtered, memory.db filtered, learning.db filtered. Gera touring.toml inferido.

**Dias estimados**: 2.0

**TDD RED** (escrever ANTES do código):
```python
def test_migrate_preserves_project_memory():
    """RED: lessons tagged for this project copy correctly."""
```

**Critério de validação**: touring memory recall em projeto migrado retorna lessons originais.

---

### W12.8: External installer (install.touring.dev)

**Descrição**: Bash script signed com sigstore. Detecta OS/arch. Downloads binary tarball + SBOM. Verifica SHA-256 + signature. Cria ~/.touring/ + symlinks. Env.sh.

**Dias estimados**: 1.5

**DISCOVER obrigatório**:
  - touring memory recall 'install.touring.dev'
  - context7: 'rustup-init.sh source code'

**Critério de validação**: curl https://install.touring.dev | sh -- --dry-run → imprime steps sem mutar disco.

---

### W12.9: Pilot konverter: install + validate workflows

**Descrição**: cd ~/projects/konverter && touring init && touring migrate --from-global && validate: touring status, doctor, ast meta, wiring orphans, generate.

**Dias estimados**: 1.0

**Critério de validação**: 5 workflows core funcionam em konverter via .touring/ local.

---

### W12.10: Pilot analise: install + validate

**Descrição**: Idem para ~/projects/analise/.

**Dias estimados**: 1.0

**Critério de validação**: 5 workflows core funcionam em analise via .touring/ local.

---

### W12.11: Documentation: getting-started + migration guide

**Descrição**: docs/guide/getting-started.md (5-min tutorial). docs/guide/migration.md (from global → per-project). docs/guide/external-client.md (curl install.touring.dev).

**Dias estimados**: 2.0

**Critério de validação**: 3 guides em docs/guide/; mdbook builds; cada guide ≥ 200 LOC.

---

### W12.12: Cross-platform testing (Linux + macOS)

**Descrição**: CI matrix: ubuntu-latest, macos-latest. Windows fica para W14 (distro packages). Validar install + init + migrate em ambos.

**Dias estimados**: 1.5

**Critério de validação**: GitHub Actions matrix 2/2 green.

---

## Gate de Saída

2 pilots rodando per-project; install.touring.dev funcional; backward compat --legacy-global preservado; cross-platform Linux+macOS green.

## Riscos Específicos

- Hook dispatcher walk-up bug pode quebrar CC em runtime → feature flag --legacy-global default ON em 0.x, OFF em 1.0
- Daemon multi-instance pode esgotar fds em workstation com 50+ projetos → documentar limite + auto-shutdown opt-in
- Migration tool pode corromper memory.db se filtering errar → backup automático antes de migrar

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
