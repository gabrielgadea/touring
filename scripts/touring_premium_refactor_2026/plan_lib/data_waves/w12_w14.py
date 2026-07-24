"""data_waves.w12_w14 — W12-W14: test debt, structure, dream.

Extracted from data_waves.py lines 1482-1904. Each ``_register_*`` helper
appends Wave instances to the shared ``WAVES`` list (in ``data_waves_pkg``).
"""
from __future__ import annotations

from . import WAVES
from ..dataclasses import Subtask, Wave

def _register_w12_w14() -> None:
    """W12-W14: Per-project deployment + publishing + commercial tiers."""

    # ─── W12 — PER-PROJECT DEPLOYMENT ─────────────────────────────────────────
    WAVES.append(Wave(
        id="W12",
        name="Per-Project Deployment",
        phase="F5-DEPLOYMENT",
        depends_on=["W11"],
        cila="L4",
        rust_changes="ADDITIVE",
        days_min=15,
        days_max=20,
        description=(
            "Implementar `touring init` + toolchain manager rustup-like em "
            "~/.touring/toolchains/ + per-project .touring/ structure + daemon "
            "multi-instance (per-project socket) + hook dispatcher walk-up + "
            "`touring migrate --from-global` + external installer "
            "(install.touring.dev). Pilot em konverter + analise. Funciona "
            "para Gabriel internamente E para clientes externos."
        ),
        contribution=(
            "Saída do path global. Cada projeto isola knowledge/memory/learning. "
            "Múltiplas versões de Touring coexistem. Cliente externo pode instalar "
            "via curl install.touring.dev | sh. Premium product ready."
        ),
        effects=[
            "`touring init` CLI funcional",
            "`~/.touring/toolchains/<version>/` rustup-like layout",
            "`.touring/touring.toml` schema v1.0",
            "Daemon multi-instance (per-project socket)",
            "Hook dispatcher walk-up em ~/.claude/hooks/touring-hook",
            "`touring migrate --from-global` automatiza transição",
            "`install.touring.dev` script signed + SBOM",
            "Pilot konverter + analise rodando per-project",
            "Cross-platform: Linux + macOS (Windows W14)",
        ],
        subtasks=[
            Subtask(
                id="W12.1", name="Implement `touring init` CLI",
                description=("Subcommand em touring-server-cli. Cria .touring/ "
                             "structure, gera touring.toml inferindo features "
                             "do diretório atual (Cargo.toml = Rust; pyproject.toml "
                             "= Python; etc.)."),
                days=2.0,
                discover=["touring tantivy search 'cargo new --lib'",
                          "touring memory recall 'touring init implementation'"],
                tdd_red=("def test_touring_init_creates_structure():\n"
                         "    \"\"\"RED: touring init in tmp dir creates .touring/ tree.\"\"\""),
                validation="touring init -> .touring/{touring.toml,data/,bin/,hooks/} exists.",
            ),
            Subtask(
                id="W12.2", name="Implement ~/.touring/ toolchain manager",
                description=("Layout completo: toolchains/<version>/{bin,lib,share,meta.toml}, "
                             "default file, config.toml. Touring-update binary copies/links "
                             "into structure."),
                days=3.0,
                validation="ls ~/.touring/toolchains/ shows ≥ 1 version; "
                           "~/.touring/default file aponta para versão válida.",
            ),
            Subtask(
                id="W12.3", name="Implement `touring update/toolchain/component`",
                description=("`touring update [version]`, `touring update --rollback`, "
                             "`touring toolchain list/install/remove/default`, "
                             "`touring component list/add/remove`."),
                days=2.0,
                tdd_red=("def test_toolchain_install_rollback():\n"
                         "    \"\"\"RED: install A, install B, rollback → back to A.\"\"\""),
                validation="touring toolchain list mostra versões instaladas; rollback funciona.",
            ),
            Subtask(
                id="W12.4", name="Implement layered config loader",
                description=("Precedência: project (.touring/touring.toml) ← user "
                             "(~/.touring/config.toml) ← system (/etc/touring/) ← "
                             "hardcoded defaults. Validator via JSON schema."),
                days=1.0,
                validation="Config merge testado em 4 cenários; conflicts resolvidos.",
            ),
            Subtask(
                id="W12.5", name="Daemon multi-instance (per-project socket)",
                description=("Daemon spawn detecta .touring/touring.toml via walk-up. "
                             "Socket fica em <project>/.touring/daemon.sock. "
                             "Múltiplos daemons coexistem (1 por projeto). "
                             "Estimated RSS ~92 MB/daemon."),
                days=2.0,
                tdd_red=("def test_two_projects_two_daemons():\n"
                         "    \"\"\"RED: cd projA + cd projB + touring status → 2 sockets.\"\"\""),
                validation="lsof | grep daemon.sock retorna N sockets para N projetos abertos.",
                blocking=True,
            ),
            Subtask(
                id="W12.6", name="Hook dispatcher walk-up shim",
                description=("~/.claude/hooks/touring-hook é shell shim que faz "
                             "walk-up procurando .touring/bin/touring-hook. "
                             "Fallback para ~/.touring/toolchains/<default>/bin/."),
                days=1.0,
                validation="Em projeto com .touring/, hook usa local binary. "
                           "Fora, usa default toolchain.",
            ),
            Subtask(
                id="W12.7", name="Implement `touring migrate --from-global`",
                description=("Migra ~/.claude/touring/ → .touring/data/ no projeto "
                             "atual. Copia symbols.db filtered, memory.db filtered, "
                             "learning.db filtered. Gera touring.toml inferido."),
                days=2.0,
                tdd_red=("def test_migrate_preserves_project_memory():\n"
                         "    \"\"\"RED: lessons tagged for this project copy correctly.\"\"\""),
                validation="touring memory recall em projeto migrado retorna lessons originais.",
            ),
            Subtask(
                id="W12.8", name="External installer (install.touring.dev)",
                description=("Bash script signed com sigstore. Detecta OS/arch. "
                             "Downloads binary tarball + SBOM. Verifica SHA-256 + "
                             "signature. Cria ~/.touring/ + symlinks. Env.sh."),
                days=1.5,
                discover=["touring memory recall 'install.touring.dev'",
                          "context7: 'rustup-init.sh source code'"],
                validation="curl https://install.touring.dev | sh -- --dry-run → "
                           "imprime steps sem mutar disco.",
            ),
            Subtask(
                id="W12.9", name="Pilot konverter: install + validate workflows",
                description=("cd ~/projects/konverter && touring init && touring migrate "
                             "--from-global && validate: touring status, doctor, ast meta, "
                             "wiring orphans, generate."),
                days=1.0,
                validation="5 workflows core funcionam em konverter via .touring/ local.",
            ),
            Subtask(
                id="W12.10", name="Pilot analise: install + validate",
                description="Idem para ~/projects/analise/.",
                days=1.0,
                validation="5 workflows core funcionam em analise via .touring/ local.",
            ),
            Subtask(
                id="W12.11", name="Documentation: getting-started + migration guide",
                description=("docs/guide/getting-started.md (5-min tutorial). "
                             "docs/guide/migration.md (from global → per-project). "
                             "docs/guide/external-client.md (curl install.touring.dev)."),
                days=2.0,
                validation="3 guides em docs/guide/; mdbook builds; cada guide ≥ 200 LOC.",
            ),
            Subtask(
                id="W12.12", name="Cross-platform testing (Linux + macOS)",
                description=("CI matrix: ubuntu-latest, macos-latest. Windows fica "
                             "para W14 (distro packages). Validar install + init + "
                             "migrate em ambos."),
                days=1.5,
                validation="GitHub Actions matrix 2/2 green.",
            ),
        ],
        gate=("2 pilots rodando per-project; install.touring.dev funcional; "
              "backward compat --legacy-global preservado; cross-platform Linux+macOS green."),
        risks=[
            "Hook dispatcher walk-up bug pode quebrar CC em runtime → "
            "feature flag --legacy-global default ON em 0.x, OFF em 1.0",
            "Daemon multi-instance pode esgotar fds em workstation com 50+ projetos → "
            "documentar limite + auto-shutdown opt-in",
            "Migration tool pode corromper memory.db se filtering errar → "
            "backup automático antes de migrar",
        ],
    ))

    # ─── W13 — PUBLISHING PIPELINE ────────────────────────────────────────────
    WAVES.append(Wave(
        id="W13",
        name="Publishing Pipeline",
        phase="F6-PUBLISHING",
        depends_on=["W12"],
        cila="L3",
        rust_changes="DOCS + CI",
        days_min=8,
        days_max=10,
        description=(
            "Toda infrastructure necessária para publicar releases premium: "
            "docs.rs completo, semver-check em CI, cargo-msrv, sigstore signing, "
            "SBOM (CycloneDX), telemetry privacy doc + opt-out UX, CHANGELOG "
            "per-crate via release-plz. Release candidate 1.0.0-rc.1 publicado "
            "no registry interno."
        ),
        contribution=(
            "Sem essa infraestrutura, releases são amador. Esta wave torna cada "
            "tag versionada reprodutível, auditável e segura."
        ),
        effects=[
            "README per crate (13 + 2 = 15 manifests)",
            "#![warn(missing_docs)] em todos os crates",
            "docs.rs build green para todas combinações de features",
            "semver-check + cargo-msrv em CI per-crate",
            "Sigstore signing pipeline (release tarballs)",
            "SBOM CycloneDX per release",
            "Telemetry privacy doc completo",
            "CHANGELOG.md per-crate via release-plz",
            "1.0.0-rc.1 publicado em registry interno",
        ],
        subtasks=[
            Subtask(
                id="W13.1", name="README per crate + #![warn(missing_docs)]",
                description=("Cada crate tem README.md com purpose + public API + "
                             "examples/ + MSRV + license + docs.rs link. "
                             "Strict missing_docs no lib.rs."),
                days=2.0,
                validation="cargo doc --workspace --no-deps --warnings-as-errors exit 0.",
            ),
            Subtask(
                id="W13.2", name="docs.rs build all features combinatorially",
                description=("[package.metadata.docs.rs] features = ['full']. "
                             "Configurar para gerar docs com todas features ativas. "
                             "Verificar build local com cargo +nightly doc."),
                days=1.0,
                validation="cargo +nightly doc --no-deps --all-features exit 0.",
            ),
            Subtask(
                id="W13.3", name="semver-check CI gate",
                description=("cargo-semver-checks em PR. Block merge se public API "
                             "tem breaking change sem version bump major."),
                days=0.5,
                validation="CI workflow runs cargo semver-checks check-release.",
            ),
            Subtask(
                id="W13.4", name="cargo-msrv verify per crate",
                description=("[package.rust-version] = '1.83'. cargo-msrv verifica "
                             "em CI. Fails se algum crate exige MSRV > 1.83."),
                days=0.5,
                validation="cargo msrv verify per crate exit 0.",
            ),
            Subtask(
                id="W13.5", name="Sigstore signing pipeline",
                description=("cosign sign-blob para cada release tarball. "
                             "Public key publicado em GitHub repo. "
                             "install.touring.dev verifica assinatura."),
                days=1.0,
                discover=["context7: 'cosign sign-blob'",
                          "touring memory recall 'sigstore signing'"],
                validation="cosign verify-blob --certificate-identity exit 0.",
            ),
            Subtask(
                id="W13.6", name="SBOM (CycloneDX) per release",
                description=("cargo-cyclonedx genera sbom.json por release. "
                             "Anexa ao GitHub Release. Listado em install.touring.dev."),
                days=1.0,
                discover=["cargo install cargo-cyclonedx"],
                validation="sbom.json válido CycloneDX 1.5 schema.",
            ),
            Subtask(
                id="W13.7", name="Telemetry privacy doc + opt-out UX",
                description=("docs/privacy.md detalha o que é coletado, por tier, "
                             "como opt-out. `touring config set telemetry.opt_in false`. "
                             "First-run prompt asks user."),
                days=1.0,
                validation="docs/privacy.md ≥ 200 LOC; opt-out testado.",
            ),
            Subtask(
                id="W13.8", name="CHANGELOG.md per crate (release-plz config)",
                description=("release-plz auto-generates CHANGELOG.md from "
                             "conventional commits. Per-crate version bumps."),
                days=1.0,
                validation="release-plz update --dry-run gera CHANGELOG diffs.",
            ),
            Subtask(
                id="W13.9", name="Release candidate 1.0.0-rc.1",
                description=("Tag v1.0.0-rc.1. CI publica em internal registry. "
                             "install.touring.dev oferece como --nightly initial."),
                days=1.0,
                validation="install.touring.dev installa 1.0.0-rc.1; touring --version mostra rc.1.",
            ),
        ],
        gate=("Release tooling funcional; RC1 published; docs.rs green; "
              "semver-check + msrv + sigstore + SBOM operacionais."),
        risks=[
            "docs.rs build pode falhar com all-features (deps conflict) → "
            "limitar features no [package.metadata.docs.rs]",
            "Sigstore Fulcio cert exige OIDC token em GHA → usar reusable workflow",
        ],
    ))

    # ─── W14 — PRODUCT TIERS & DISTRIBUTION ───────────────────────────────────
    WAVES.append(Wave(
        id="W14",
        name="Product Tiers & Distribution",
        phase="F7-PRODUCT",
        depends_on=["W13"],
        cila="L4",
        rust_changes="ADDITIVE + DISTRO",
        days_min=10,
        days_max=15,
        description=(
            "Wave FINAL. Tiers comerciais (free/standard/premium/enterprise) "
            "ativos via Cargo features + JWT ed25519 license. Telemetria tiered. "
            "Private registry support (enterprise). SSO scaffold (Okta/Google/GH). "
            "Audit log SIEM export. install.touring.dev + binary releases CI/CD "
            "+ distro packages (deb/rpm/brew/scoop) + Docker images. 1.0.0 GA."
        ),
        contribution=(
            "Touring deixa de ser ferramenta interna e vira produto premium "
            "comercializável. 4 tiers ativáveis; install one-liner para clientes; "
            "enterprise on-prem support."
        ),
        effects=[
            "Tiers como Cargo features (tier-free, tier-standard, tier-premium, tier-enterprise)",
            "JWT ed25519 license validation com 30-day offline grace",
            "Telemetria tiered (free/std ON, premium/ent OFF)",
            "Private registry support (enterprise opt-in)",
            "SSO scaffold (Okta/Google/GitHub)",
            "Audit log SIEM export (Splunk/Datadog/ELK)",
            "install.touring.dev script + CI/CD para binary releases",
            "Distro packages: deb (PPA), rpm (COPR), brew (tap), scoop bucket",
            "Docker images: alpine, debian-slim, distroless",
            "1.0.0 GA publicado",
        ],
        subtasks=[
            Subtask(
                id="W14.1", name="Tiers as Cargo features",
                description=("touring-server/Cargo.toml: [features] tier-free, "
                             "tier-standard (extends free), tier-premium (extends "
                             "standard), tier-enterprise (extends premium). Cada "
                             "tier ativa subset de features dos crates abaixo."),
                days=2.0,
                tdd_red=("def test_tier_free_binary_size():\n"
                         "    \"\"\"RED: tier-free binary > 50 MB FAILS budget.\"\"\""),
                validation="cargo build --release --no-default-features --features "
                           "tier-free exit 0; binary ≤ 50 MB.",
            ),
            Subtask(
                id="W14.2", name="License key system (JWT ed25519)",
                description=("crate touring-license (or in foundation/license). "
                             "ed25519 public key embedded no binary. JWT validation "
                             "offline. 30-day grace post-expiration."),
                days=2.0,
                discover=["context7: 'jsonwebtoken ed25519'"],
                tdd_red=("def test_expired_license_grace_30days():\n"
                         "    \"\"\"RED: expired JWT after 31 days FAILS.\"\"\""),
                validation="touring login com license key válida → tier ativado.",
            ),
            Subtask(
                id="W14.3", name="Telemetry tiered (free/std ON, premium/ent OFF)",
                description=("Telemetry initializer reads tier from license. "
                             "Premium+ skips OTel exporter setup. UI nag if "
                             "free user opts out (suggest premium)."),
                days=1.0,
                validation="touring status → telemetry status reflects tier.",
            ),
            Subtask(
                id="W14.4", name="Private registry support (enterprise)",
                description=("touring.toml: [enterprise] registry_url. Touring "
                             "registry sync mirrors crates.io para private mirror. "
                             "Cargo configurado via .cargo/config.toml."),
                days=2.0,
                validation="touring registry sync exit 0 com private URL.",
            ),
            Subtask(
                id="W14.5", name="SSO scaffold (Okta/Google/GitHub)",
                description=("OAuth2/OIDC flow. touring login --sso okta abre browser. "
                             "Token armazenado em ~/.touring/credentials. Refresh "
                             "automático. License key derived from SSO identity."),
                days=2.0,
                discover=["context7: 'openid connect rust'"],
                validation="touring login --sso github → browser opens → callback OK.",
            ),
            Subtask(
                id="W14.6", name="Audit log SIEM export (enterprise)",
                description=("touring.toml [enterprise] audit_log_path. JSONL output "
                             "compatible com Splunk HEC, Datadog Logs, ELK. "
                             "Each tool invocation logged."),
                days=1.5,
                validation="touring config set enterprise.audit_log_path /tmp/audit.jsonl; "
                           "tools logged.",
            ),
            Subtask(
                id="W14.7", name="Pricing + license validation flow",
                description=("Stripe webhook → license server gera JWT → email cliente. "
                             "Cliente: touring login → cola key → tier ativa. "
                             "Renovação automática via webhook."),
                days=1.5,
                validation="End-to-end: Stripe checkout → license email → "
                           "touring login → tier-premium ativa.",
            ),
            Subtask(
                id="W14.8", name="install.touring.dev + binary releases CI/CD",
                description=("GHA workflow build-release.yml: matrix linux/macos/win × "
                             "x86_64/aarch64. Cross compile via cargo-zigbuild. "
                             "Upload to GH Release. install.touring.dev pull from "
                             "GH Release. Sigstore signs each tarball."),
                days=2.0,
                validation="Push tag v1.0.0-rc.2 → workflow builds 6 targets → "
                           "binaries em GH Release + signed.",
            ),
            Subtask(
                id="W14.9", name="Distro packages (deb, rpm, brew, scoop)",
                description=("cargo-deb gera .deb; cargo-rpm gera .rpm; "
                             "homebrew tap repo; scoop bucket. PPA Ubuntu/Debian, "
                             "COPR Fedora/RHEL."),
                days=2.0,
                discover=["cargo install cargo-deb cargo-rpm"],
                validation="apt install touring (PPA) + brew install touring "
                           "(homebrew/touring/touring) funciona.",
            ),
            Subtask(
                id="W14.10", name="Docker images (alpine, debian-slim, distroless)",
                description=("Dockerfile.alpine, Dockerfile.debian-slim, Dockerfile.distroless. "
                             "Multi-arch build via buildx. Published to ghcr.io/touring/touring."),
                days=1.0,
                validation="docker pull ghcr.io/touring/touring:1.0.0-distroless exit 0; "
                           "size ≤ 60 MB.",
            ),
        ],
        gate=("1.0.0 GA published; install.touring.dev funcional; 4 tiers ativáveis "
              "via license key; SSO + audit + private registry operacionais para "
              "enterprise; 4 distro packages + 3 Docker images disponíveis."),
        risks=[
            "Stripe webhook retry on transient error → idempotent license issuance",
            "SSO provider down → graceful degrade to free tier + clear error msg",
            "Audit log volume em high-traffic enterprise → log rotation + sampling",
            "Distro package signing keys mgmt → use GitHub Actions OIDC + Vault",
            "Docker distroless lacks shell → debug via tini --once",
        ],
    ))


# Note: WAVES, TIERS, RISKS, KPIS data is appended via Edits to keep this
# foundation module readable and reviewable. Render and main() are below.


