---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W14"
name: "Product Tiers & Distribution"
phase: "F7-PRODUCT"
depends_on:
  - W13
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L4"
rust_changes: "ADDITIVE + DISTRO"
estimated_days: "10-15"
checkpoint: "touring_premium_W14_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W14.py"
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
# W14: Product Tiers & Distribution

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F7-PRODUCT
> **Contribuição para resultado final**: Touring deixa de ser ferramenta interna e vira produto premium comercializável. 4 tiers ativáveis; install one-liner para clientes; enterprise on-prem support.

---

## Contexto e Dependências

- **Depende de**: W13
- **Paralelo com**: Nenhuma
- **CILA**: `L4`
- **Mudanças Rust**: `ADDITIVE + DISTRO`
- **Estimativa**: 10-15 dias
- **Checkpoint**: `touring_premium_W14_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W14.py`

---

## Descrição

Wave FINAL. Tiers comerciais (free/standard/premium/enterprise) ativos via Cargo features + JWT ed25519 license. Telemetria tiered. Private registry support (enterprise). SSO scaffold (Okta/Google/GH). Audit log SIEM export. install.touring.dev + binary releases CI/CD + distro packages (deb/rpm/brew/scoop) + Docker images. 1.0.0 GA.

---

## Efeitos no Sistema

- Tiers como Cargo features (tier-free, tier-standard, tier-premium, tier-enterprise)
- JWT ed25519 license validation com 30-day offline grace
- Telemetria tiered (free/std ON, premium/ent OFF)
- Private registry support (enterprise opt-in)
- SSO scaffold (Okta/Google/GitHub)
- Audit log SIEM export (Splunk/Datadog/ELK)
- install.touring.dev script + CI/CD para binary releases
- Distro packages: deb (PPA), rpm (COPR), brew (tap), scoop bucket
- Docker images: alpine, debian-slim, distroless
- 1.0.0 GA publicado

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W14.1: Tiers as Cargo features

**Descrição**: touring-server/Cargo.toml: [features] tier-free, tier-standard (extends free), tier-premium (extends standard), tier-enterprise (extends premium). Cada tier ativa subset de features dos crates abaixo.

**Dias estimados**: 2.0

**TDD RED** (escrever ANTES do código):
```python
def test_tier_free_binary_size():
    """RED: tier-free binary > 50 MB FAILS budget."""
```

**Critério de validação**: cargo build --release --no-default-features --features tier-free exit 0; binary ≤ 50 MB.

---

### W14.2: License key system (JWT ed25519)

**Descrição**: crate touring-license (or in foundation/license). ed25519 public key embedded no binary. JWT validation offline. 30-day grace post-expiration.

**Dias estimados**: 2.0

**DISCOVER obrigatório**:
  - context7: 'jsonwebtoken ed25519'

**TDD RED** (escrever ANTES do código):
```python
def test_expired_license_grace_30days():
    """RED: expired JWT after 31 days FAILS."""
```

**Critério de validação**: touring login com license key válida → tier ativado.

---

### W14.3: Telemetry tiered (free/std ON, premium/ent OFF)

**Descrição**: Telemetry initializer reads tier from license. Premium+ skips OTel exporter setup. UI nag if free user opts out (suggest premium).

**Dias estimados**: 1.0

**Critério de validação**: touring status → telemetry status reflects tier.

---

### W14.4: Private registry support (enterprise)

**Descrição**: touring.toml: [enterprise] registry_url. Touring registry sync mirrors crates.io para private mirror. Cargo configurado via .cargo/config.toml.

**Dias estimados**: 2.0

**Critério de validação**: touring registry sync exit 0 com private URL.

---

### W14.5: SSO scaffold (Okta/Google/GitHub)

**Descrição**: OAuth2/OIDC flow. touring login --sso okta abre browser. Token armazenado em ~/.touring/credentials. Refresh automático. License key derived from SSO identity.

**Dias estimados**: 2.0

**DISCOVER obrigatório**:
  - context7: 'openid connect rust'

**Critério de validação**: touring login --sso github → browser opens → callback OK.

---

### W14.6: Audit log SIEM export (enterprise)

**Descrição**: touring.toml [enterprise] audit_log_path. JSONL output compatible com Splunk HEC, Datadog Logs, ELK. Each tool invocation logged.

**Dias estimados**: 1.5

**Critério de validação**: touring config set enterprise.audit_log_path /tmp/audit.jsonl; tools logged.

---

### W14.7: Pricing + license validation flow

**Descrição**: Stripe webhook → license server gera JWT → email cliente. Cliente: touring login → cola key → tier ativa. Renovação automática via webhook.

**Dias estimados**: 1.5

**Critério de validação**: End-to-end: Stripe checkout → license email → touring login → tier-premium ativa.

---

### W14.8: install.touring.dev + binary releases CI/CD

**Descrição**: GHA workflow build-release.yml: matrix linux/macos/win × x86_64/aarch64. Cross compile via cargo-zigbuild. Upload to GH Release. install.touring.dev pull from GH Release. Sigstore signs each tarball.

**Dias estimados**: 2.0

**Critério de validação**: Push tag v1.0.0-rc.2 → workflow builds 6 targets → binaries em GH Release + signed.

---

### W14.9: Distro packages (deb, rpm, brew, scoop)

**Descrição**: cargo-deb gera .deb; cargo-rpm gera .rpm; homebrew tap repo; scoop bucket. PPA Ubuntu/Debian, COPR Fedora/RHEL.

**Dias estimados**: 2.0

**DISCOVER obrigatório**:
  - cargo install cargo-deb cargo-rpm

**Critério de validação**: apt install touring (PPA) + brew install touring (homebrew/touring/touring) funciona.

---

### W14.10: Docker images (alpine, debian-slim, distroless)

**Descrição**: Dockerfile.alpine, Dockerfile.debian-slim, Dockerfile.distroless. Multi-arch build via buildx. Published to ghcr.io/touring/touring.

**Dias estimados**: 1.0

**Critério de validação**: docker pull ghcr.io/touring/touring:1.0.0-distroless exit 0; size ≤ 60 MB.

---

## Gate de Saída

1.0.0 GA published; install.touring.dev funcional; 4 tiers ativáveis via license key; SSO + audit + private registry operacionais para enterprise; 4 distro packages + 3 Docker images disponíveis.

## Riscos Específicos

- Stripe webhook retry on transient error → idempotent license issuance
- SSO provider down → graceful degrade to free tier + clear error msg
- Audit log volume em high-traffic enterprise → log rotation + sampling
- Distro package signing keys mgmt → use GitHub Actions OIDC + Vault
- Docker distroless lacks shell → debug via tini --once

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
