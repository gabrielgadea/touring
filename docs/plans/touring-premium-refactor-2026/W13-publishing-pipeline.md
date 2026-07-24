---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W13"
name: "Publishing Pipeline"
phase: "F6-PUBLISHING"
depends_on:
  - W12
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L3"
rust_changes: "DOCS + CI"
estimated_days: "8-10"
checkpoint: "touring_premium_W13_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W13.py"
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
# W13: Publishing Pipeline

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F6-PUBLISHING
> **Contribuição para resultado final**: Sem essa infraestrutura, releases são amador. Esta wave torna cada tag versionada reprodutível, auditável e segura.

---

## Contexto e Dependências

- **Depende de**: W12
- **Paralelo com**: Nenhuma
- **CILA**: `L3`
- **Mudanças Rust**: `DOCS + CI`
- **Estimativa**: 8-10 dias
- **Checkpoint**: `touring_premium_W13_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W13.py`

---

## Descrição

Toda infrastructure necessária para publicar releases premium: docs.rs completo, semver-check em CI, cargo-msrv, sigstore signing, SBOM (CycloneDX), telemetry privacy doc + opt-out UX, CHANGELOG per-crate via release-plz. Release candidate 1.0.0-rc.1 publicado no registry interno.

---

## Efeitos no Sistema

- README per crate (13 + 2 = 15 manifests)
- #![warn(missing_docs)] em todos os crates
- docs.rs build green para todas combinações de features
- semver-check + cargo-msrv em CI per-crate
- Sigstore signing pipeline (release tarballs)
- SBOM CycloneDX per release
- Telemetry privacy doc completo
- CHANGELOG.md per-crate via release-plz
- 1.0.0-rc.1 publicado em registry interno

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W13.1: README per crate + #![warn(missing_docs)]

**Descrição**: Cada crate tem README.md com purpose + public API + examples/ + MSRV + license + docs.rs link. Strict missing_docs no lib.rs.

**Dias estimados**: 2.0

**Critério de validação**: cargo doc --workspace --no-deps --warnings-as-errors exit 0.

---

### W13.2: docs.rs build all features combinatorially

**Descrição**: [package.metadata.docs.rs] features = ['full']. Configurar para gerar docs com todas features ativas. Verificar build local com cargo +nightly doc.

**Dias estimados**: 1.0

**Critério de validação**: cargo +nightly doc --no-deps --all-features exit 0.

---

### W13.3: semver-check CI gate

**Descrição**: cargo-semver-checks em PR. Block merge se public API tem breaking change sem version bump major.

**Dias estimados**: 0.5

**Critério de validação**: CI workflow runs cargo semver-checks check-release.

---

### W13.4: cargo-msrv verify per crate

**Descrição**: [package.rust-version] = '1.83'. cargo-msrv verifica em CI. Fails se algum crate exige MSRV > 1.83.

**Dias estimados**: 0.5

**Critério de validação**: cargo msrv verify per crate exit 0.

---

### W13.5: Sigstore signing pipeline

**Descrição**: cosign sign-blob para cada release tarball. Public key publicado em GitHub repo. install.touring.dev verifica assinatura.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - context7: 'cosign sign-blob'
  - touring memory recall 'sigstore signing'

**Critério de validação**: cosign verify-blob --certificate-identity exit 0.

---

### W13.6: SBOM (CycloneDX) per release

**Descrição**: cargo-cyclonedx genera sbom.json por release. Anexa ao GitHub Release. Listado em install.touring.dev.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - cargo install cargo-cyclonedx

**Critério de validação**: sbom.json válido CycloneDX 1.5 schema.

---

### W13.7: Telemetry privacy doc + opt-out UX

**Descrição**: docs/privacy.md detalha o que é coletado, por tier, como opt-out. `touring config set telemetry.opt_in false`. First-run prompt asks user.

**Dias estimados**: 1.0

**Critério de validação**: docs/privacy.md ≥ 200 LOC; opt-out testado.

---

### W13.8: CHANGELOG.md per crate (release-plz config)

**Descrição**: release-plz auto-generates CHANGELOG.md from conventional commits. Per-crate version bumps.

**Dias estimados**: 1.0

**Critério de validação**: release-plz update --dry-run gera CHANGELOG diffs.

---

### W13.9: Release candidate 1.0.0-rc.1

**Descrição**: Tag v1.0.0-rc.1. CI publica em internal registry. install.touring.dev oferece como --nightly initial.

**Dias estimados**: 1.0

**Critério de validação**: install.touring.dev installa 1.0.0-rc.1; touring --version mostra rc.1.

---

## Gate de Saída

Release tooling funcional; RC1 published; docs.rs green; semver-check + msrv + sigstore + SBOM operacionais.

## Riscos Específicos

- docs.rs build pode falhar com all-features (deps conflict) → limitar features no [package.metadata.docs.rs]
- Sigstore Fulcio cert exige OIDC token em GHA → usar reusable workflow

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
